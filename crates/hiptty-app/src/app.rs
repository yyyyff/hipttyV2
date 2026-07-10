use hiptty_core::{
    forum_name, processed_password, AppSettings, SessionInfo, StoredCredentials, ThreadDetail,
    ThreadSummary, SECURITY_QUESTIONS,
};
use hiptty_image::{AvatarDiskCache, FetchRequest, ImageCache};
use hiptty_render::{format_count, Palette};
use hiptty_widgets::{
    forum_picker_entries, loading_status_label, page_status_label, snap_scroll_to_item,
    thread_list_capacity, FloorLayout, KeyHint, LoginField, ScrollBarInteraction, ScrollChrome,
    TitleBarHits, SCROLLBAR_COLS, TOAST_ERROR_TICKS, TOAST_SUCCESS_TICKS,
};
use ratatui_image::picker::Picker;
use tokio::sync::mpsc;

use crate::composer::{ComposerState, ConfirmDeleteState};
use crate::list_page::{ListPageState, PmThreadState};
use crate::nav::NavStack;
use crate::worker::WorkerRequest;

/// Snapshot of UI geometry that affects Kitty placement positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GraphicsLayoutKey {
    page: Page,
    overlay: Overlay,
    detail_scroll: u32,
    feed_scroll: u16,
    list_scroll: u16,
    pm_scroll: u16,
    viewport_w: u16,
    viewport_h: u16,
}

impl GraphicsLayoutKey {
    fn from_app(app: &App) -> Self {
        Self {
            page: app.page,
            overlay: app.overlay,
            detail_scroll: app.detail.scroll_top,
            feed_scroll: app.feed.scroll_lines,
            list_scroll: app.list_page.scroll_lines,
            pm_scroll: app.pm_thread.scroll_lines,
            viewport_w: app.viewport_width,
            viewport_h: app.viewport_height,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MouseClickState {
    pub at: std::time::Instant,
    pub column: u16,
    pub row: u16,
    pub page: Page,
}

fn list_item_height(page: Page) -> u16 {
    match page {
        Page::PmList | Page::Notifications => hiptty_widgets::SIMPLE_ITEM_HEIGHT,
        _ => hiptty_widgets::ITEM_HEIGHT,
    }
}

/// How the next `ThreadDetailLoaded` response should merge into state and place the viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailLoadIntent {
    /// Replace posts and jump to the first floor (open thread, `g`, refresh page 1).
    ReplaceTop,
    /// Replace posts and jump to the last floor / max scroll (`G`).
    ReplaceBottom,
    /// Append next page posts and preserve the mid-floor scroll anchor (near-end auto load).
    AppendPreserve,
    /// Prepend previous page posts and keep the viewport on the same floors (near-top after `G`).
    PrependPreserve,
}

/// Backward-compatible alias used by a few call sites during the intent migration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailFetchMode {
    Replace,
    Append,
}

impl From<DetailFetchMode> for DetailLoadIntent {
    fn from(mode: DetailFetchMode) -> Self {
        match mode {
            DetailFetchMode::Replace => Self::ReplaceTop,
            DetailFetchMode::Append => Self::AppendPreserve,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    /// Session check / auto-login in progress.
    Startup,
    Login,
    ThreadFeed,
    ThreadDetail,
    PmList,
    PmThread,
    Notifications,
    Search,
    MyThreads,
    MyReplies,
    Favorites,
}

#[derive(Debug, Clone)]
pub struct ToastState {
    pub message: String,
    pub is_error: bool,
    pub started_at: u64,
    pub duration_ticks: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    None,
    ForumPicker,
    MainMenu,
    Settings,
    SearchPrompt,
    CommandBar,
}

#[derive(Debug, Clone, Default)]
pub struct OverlayState {
    pub main_menu_selected: usize,
    pub settings_selected: usize,
    /// Command-mode buffer (without the leading `:`).
    pub command_input: String,
    /// Byte offset of the caret inside [`command_input`].
    pub command_cursor: usize,
    pub search_input: String,
}

#[derive(Debug, Clone, Default)]
pub struct UnreadState {
    pub has_pm: bool,
    pub has_notifications: bool,
    /// True while a `CheckUnread` request is queued or running in the worker.
    /// Prevents unbounded pile-up when the network is slow/offline.
    pub check_in_flight: bool,
}

#[derive(Debug, Clone)]
pub struct LoginState {
    pub username: String,
    pub password: String,
    pub security_index: usize,
    pub security_answer: String,
    pub focused: LoginField,
    pub error: Option<String>,
    pub loading: bool,
}

impl Default for LoginState {
    fn default() -> Self {
        Self {
            username: String::new(),
            password: String::new(),
            security_index: 0,
            security_answer: String::new(),
            focused: LoginField::Username,
            error: None,
            loading: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FeedState {
    pub fid: u32,
    pub threads: Vec<ThreadSummary>,
    pub selected: usize,
    pub scroll_lines: u16,
    pub page: u32,
    pub max_page: u32,
    pub loading: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DetailState {
    pub tid: String,
    pub fid: Option<u32>,
    pub title: String,
    pub reply_count: Option<String>,
    pub view_count: Option<String>,
    pub detail: Option<ThreadDetail>,
    pub selected: usize,
    pub scroll_top: u32,
    pub loading: bool,
    pub loading_more: bool,
    /// Intent for the in-flight detail request (paired with [`Self::pending_request_id`]).
    pub pending_intent: Option<DetailLoadIntent>,
    /// Monotonic id of the latest detail request; stale responses are dropped.
    pub pending_request_id: u64,
    /// Next request id to allocate.
    pub next_request_id: u64,
    /// Lowest page number currently held in `detail.posts` (for upward prepend after `G`).
    /// Highest is `detail.page`.
    pub loaded_page_lo: u32,
    pub error: Option<String>,
    /// Bumped when posts change or layout must rebuild (image heights, etc.).
    pub layout_revision: u64,
    /// Cached floor heights/offsets. Matched by `width` + [`Self::layout_revision`].
    pub layout: Option<FloorLayout>,
}

impl DetailState {
    pub fn from_summary(thread: &ThreadSummary, fid: u32) -> Self {
        Self {
            tid: thread.tid.clone(),
            fid: Some(fid),
            title: thread.title.clone(),
            reply_count: thread.reply_count.clone(),
            view_count: thread.view_count.clone(),
            detail: None,
            selected: 0,
            scroll_top: 0,
            loading: false,
            loading_more: false,
            pending_intent: None,
            pending_request_id: 0,
            next_request_id: 1,
            loaded_page_lo: 1,
            error: None,
            layout_revision: 0,
            layout: None,
        }
    }

    pub fn invalidate_layout(&mut self) {
        self.layout_revision = self.layout_revision.wrapping_add(1);
        self.layout = None;
    }

    pub fn allocate_request_id(&mut self) -> u64 {
        let id = self.next_request_id;
        self.next_request_id = self.next_request_id.wrapping_add(1).max(1);
        self.pending_request_id = id;
        id
    }
}

impl FeedState {
    pub fn new(fid: u32) -> Self {
        Self {
            fid,
            threads: Vec::new(),
            selected: 0,
            scroll_lines: 0,
            page: 0,
            max_page: 0,
            loading: false,
            error: None,
        }
    }
}

#[derive(Debug)]
pub struct App {
    pub page: Page,
    pub overlay: Overlay,
    pub settings: AppSettings,
    pub session: SessionInfo,
    pub login: LoginState,
    pub feed: FeedState,
    pub detail: DetailState,
    pub forum_picker_selected: usize,
    pub forum_picker_scroll: usize,
    pub forum_picker_hits: Vec<hiptty_widgets::ForumPickerHit>,
    pub forum_tab_hover: Option<usize>,
    pub tick: u64,
    pub viewport_width: u16,
    pub viewport_height: u16,
    pub toast: Option<ToastState>,
    pub composer: Option<ComposerState>,
    pub confirm_delete: Option<ConfirmDeleteState>,
    pub nav_stack: NavStack,
    pub list_page: ListPageState,
    pub pm_thread: PmThreadState,
    pub overlay_state: OverlayState,
    pub unread: UnreadState,
    pub blacklist_count: usize,
    pub quit: bool,
    pub profile: String,
    pub config_dir: std::path::PathBuf,
    pub credentials_path: std::path::PathBuf,
    pub settings_path: std::path::PathBuf,
    pub startup_done: bool,
    pub image_cache: Option<ImageCache>,
    /// HTTP image fetches waiting to be dispatched (viewport lazy load + concurrency cap).
    pub image_fetch_queue: std::collections::VecDeque<FetchRequest>,
    /// In-flight `FetchImage` worker requests.
    pub image_fetches_in_flight: usize,
    /// Bumped on login/logout so in-flight image responses cannot apply to a new session.
    pub session_epoch: u64,
    /// Waiting for worker `LoggedOut` to confirm cookie/session cleanup.
    pub logout_pending: bool,
    /// Local credential-clear error retained so worker success cannot paint over it.
    pub logout_local_error: Option<String>,
    /// Next frame should emit Kitty placement deletes (scroll / layout / image ready).
    pub graphics_dirty: bool,
    /// Last geometry key for which we already issued a placement clear.
    last_graphics_layout: Option<GraphicsLayoutKey>,
    /// Updated each frame for mouse hit-testing and scrollbar interaction.
    pub scroll_chrome: Option<ScrollChrome>,
    pub scrollbar_interaction: ScrollBarInteraction,
    pub title_bar_area: ratatui::layout::Rect,
    pub title_bar_hits: TitleBarHits,
    pub main_menu_hits: Vec<ratatui::layout::Rect>,
    pub settings_hits: Vec<ratatui::layout::Rect>,
    pub last_click: Option<MouseClickState>,
}

/// Max concurrent image HTTP fetches (decode is separate, multi-threaded in ImageCache).
pub const IMAGE_FETCH_CONCURRENCY: usize = 3;

impl App {
    pub fn new(settings: AppSettings, config_dir: std::path::PathBuf, profile: String) -> Self {
        let credentials_path = crate::config::credentials_path(&config_dir, &profile);
        let settings_path = crate::config::settings_path(&config_dir);
        let default_fid = settings.default_forums[0];
        Self {
            page: Page::Startup,
            overlay: Overlay::None,
            settings,
            session: SessionInfo {
                logged_in: false,
                username: None,
                uid: None,
            },
            login: LoginState::default(),
            feed: FeedState::new(default_fid),
            detail: DetailState {
                tid: String::new(),
                fid: None,
                title: String::new(),
                reply_count: None,
                view_count: None,
                detail: None,
                selected: 0,
                scroll_top: 0,
                loading: false,
                loading_more: false,
                pending_intent: None,
                pending_request_id: 0,
                next_request_id: 1,
                loaded_page_lo: 1,
                error: None,
                layout_revision: 0,
                layout: None,
            },
            forum_picker_selected: 0,
            forum_picker_scroll: 0,
            forum_picker_hits: Vec::new(),
            forum_tab_hover: None,
            tick: 0,
            viewport_width: 80,
            viewport_height: 24,
            toast: None,
            composer: None,
            confirm_delete: None,
            nav_stack: NavStack::default(),
            list_page: ListPageState::default(),
            pm_thread: PmThreadState::default(),
            overlay_state: OverlayState::default(),
            unread: UnreadState::default(),
            blacklist_count: 0,
            quit: false,
            profile,
            config_dir,
            credentials_path,
            settings_path,
            startup_done: false,
            image_cache: None,
            image_fetch_queue: std::collections::VecDeque::new(),
            image_fetches_in_flight: 0,
            session_epoch: 0,
            logout_pending: false,
            logout_local_error: None,
            graphics_dirty: true,
            last_graphics_layout: None,
            scroll_chrome: None,
            scrollbar_interaction: ScrollBarInteraction::new(),
            title_bar_area: ratatui::layout::Rect::default(),
            title_bar_hits: TitleBarHits::default(),
            main_menu_hits: Vec::new(),
            settings_hits: Vec::new(),
            last_click: None,
        }
    }

    /// Initialize the image cache once. `picker` must come from a single
    /// `Picker::from_query_stdio()` call (terminal protocol is probed at most once).
    pub fn init_images(&mut self, picker: Picker) {
        if self.image_cache.is_some() {
            return;
        }
        let avatar_disk = AvatarDiskCache::new(&self.config_dir).ok();
        self.image_cache = Some(ImageCache::new(picker, avatar_disk));
    }

    pub fn images(&self) -> Option<&ImageCache> {
        self.image_cache.as_ref()
    }

    pub fn images_mut(&mut self) -> Option<&mut ImageCache> {
        self.image_cache.as_mut()
    }

    /// Mark that Kitty/Sixel geometry may have moved; next draw clears placements on WT.
    pub fn mark_graphics_dirty(&mut self) {
        self.graphics_dirty = true;
    }

    /// Consume the dirty flag for this frame (true on first paint after a graphics move).
    pub fn take_graphics_dirty(&mut self) -> bool {
        let dirty = self.graphics_dirty;
        self.graphics_dirty = false;
        dirty
    }

    /// If page/scroll/viewport changed since last clear, mark graphics dirty.
    pub fn note_graphics_layout(&mut self) {
        let key = GraphicsLayoutKey::from_app(self);
        if self.last_graphics_layout != Some(key) {
            self.graphics_dirty = true;
            self.last_graphics_layout = Some(key);
        }
    }

    /// Enqueue image HTTP jobs and dispatch up to [`IMAGE_FETCH_CONCURRENCY`] at a time.
    pub fn dispatch_image_fetches(
        &mut self,
        requests: impl IntoIterator<Item = FetchRequest>,
        worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
    ) {
        for req in requests {
            // Dedupe against queue (same url already waiting).
            if self
                .image_fetch_queue
                .iter()
                .any(|q| q.url == req.url && q.kind == req.kind)
            {
                continue;
            }
            self.image_fetch_queue.push_back(req);
        }
        self.pump_image_fetch_queue(worker_tx);
    }

    pub fn on_image_fetch_finished(&mut self, worker_tx: &mpsc::UnboundedSender<WorkerRequest>) {
        self.image_fetches_in_flight = self.image_fetches_in_flight.saturating_sub(1);
        self.pump_image_fetch_queue(worker_tx);
    }

    fn pump_image_fetch_queue(&mut self, worker_tx: &mpsc::UnboundedSender<WorkerRequest>) {
        while self.image_fetches_in_flight < IMAGE_FETCH_CONCURRENCY {
            let Some(req) = self.image_fetch_queue.pop_front() else {
                break;
            };
            if worker_tx
                .send(WorkerRequest::FetchImage {
                    url: req.url,
                    kind: req.kind,
                    session_epoch: self.session_epoch,
                })
                .is_ok()
            {
                self.image_fetches_in_flight += 1;
            } else {
                break;
            }
        }
    }

    /// Invalidate in-flight image work after logout/login (cookie jar may have changed).
    pub fn bump_session_epoch(&mut self) {
        self.session_epoch = self.session_epoch.wrapping_add(1);
        self.image_fetch_queue.clear();
        // In-flight responses still decrement `image_fetches_in_flight` when they arrive.
    }

    pub fn palette(&self) -> Palette {
        Palette::default()
    }

    pub fn dims_background(&self) -> bool {
        if self.confirm_delete.is_some() {
            return true;
        }
        // Command bar replaces the status line only — keep content undimmed (vim-style).
        !matches!(self.overlay, Overlay::None | Overlay::CommandBar)
    }

    pub fn content_palette(&self) -> Palette {
        if self.dims_background() {
            self.palette().dimmed()
        } else {
            self.palette()
        }
    }

    pub fn breadcrumb(&self) -> String {
        match self.page {
            Page::Startup | Page::Login => String::new(),
            Page::ThreadFeed => String::new(),
            Page::ThreadDetail => self.detail_breadcrumb(),
            Page::PmList => "私信".into(),
            Page::PmThread => format!("私信 · {}", self.pm_thread.peer_name),
            Page::Notifications => "通知".into(),
            Page::Search => {
                if self.list_page.search_query.is_empty() {
                    "搜索".into()
                } else {
                    format!("搜索: \"{}\"", self.list_page.search_query)
                }
            }
            Page::MyThreads => "我的帖子".into(),
            Page::MyReplies => "我的回复".into(),
            Page::Favorites => "我的收藏".into(),
        }
    }

    pub fn set_toast(&mut self, message: impl Into<String>, is_error: bool) {
        let duration_ticks = if is_error {
            TOAST_ERROR_TICKS
        } else {
            TOAST_SUCCESS_TICKS
        };
        self.toast = Some(ToastState {
            message: message.into(),
            is_error,
            started_at: self.tick,
            duration_ticks,
        });
    }

    pub fn poll_toast(&mut self) {
        let expired = self.toast.as_ref().is_some_and(|toast| {
            self.tick >= toast.started_at.saturating_add(toast.duration_ticks)
        });
        if expired {
            self.toast = None;
        }
    }

    pub fn dismiss_toast(&mut self) {
        self.toast = None;
    }

    /// Left-cluster shortcuts for the status bar (priority 0 = keep first when narrow).
    pub fn status_hints(&self) -> Vec<KeyHint> {
        if self.confirm_delete.is_some() {
            return vec![
                KeyHint::new("y", "确认", 0),
                KeyHint::new("n/Esc", "取消", 0),
            ];
        }
        if let Some(composer) = &self.composer {
            if composer.submitting || composer.preparing {
                return vec![KeyHint::new("Esc", "取消", 0)];
            }
            let mut hints = vec![
                KeyHint::new("C-Enter/S", "发送", 0),
                KeyHint::new("Esc", "取消", 0),
            ];
            if composer.need_type_ui() || composer.show_subject {
                hints.push(KeyHint::new("Tab", "切换", 1));
            }
            if composer.need_type_ui() {
                hints.push(KeyHint::new("←→", "分类", 1));
            }
            return hints;
        }
        match self.overlay {
            Overlay::ForumPicker => vec![
                KeyHint::new("j/k", "选择", 0),
                KeyHint::new("Enter", "确认", 0),
                KeyHint::new("Esc", "关闭", 0),
            ],
            Overlay::MainMenu => vec![
                KeyHint::new("j/k", "移动", 0),
                KeyHint::new("Enter", "确认", 0),
                KeyHint::new("Esc", "关闭", 0),
            ],
            Overlay::Settings => vec![
                KeyHint::new("j/k", "移动", 0),
                KeyHint::new("Enter", "修改", 0),
                KeyHint::new("Esc", "关闭", 0),
            ],
            Overlay::SearchPrompt => vec![
                KeyHint::new("Enter", "搜索", 0),
                KeyHint::new("Esc", "取消", 0),
            ],
            Overlay::CommandBar => vec![
                KeyHint::new("Enter", "执行", 0),
                KeyHint::new("Esc", "取消", 0),
            ],
            Overlay::None => match self.page {
                Page::Startup => vec![KeyHint::new("Esc", "退出", 0)],
                Page::Login => vec![
                    KeyHint::new("Tab/↑↓", "切换", 0),
                    KeyHint::new("Enter", "确认", 0),
                    KeyHint::new("Esc", "退出", 1),
                ],
                Page::ThreadFeed => vec![
                    KeyHint::new("j/k", "导航", 0),
                    KeyHint::new("Enter", "打开", 0),
                    KeyHint::new("r", "刷新", 1),
                    KeyHint::new("n", "新帖", 1),
                    KeyHint::new("/", "搜索", 1),
                    KeyHint::new("[]", "版块", 2),
                    KeyHint::new("f", "更多", 2),
                    KeyHint::new("Esc", "菜单", 1),
                    KeyHint::new(":", "命令", 2),
                ],
                Page::ThreadDetail => vec![
                    KeyHint::new("j/k", "滚动", 0),
                    KeyHint::new("r", "回复", 0),
                    KeyHint::new("b", "返回", 0),
                    KeyHint::new("PgUp/Dn", "翻页", 1),
                    KeyHint::new("g/G", "首/末", 2),
                ],
                Page::PmList => vec![
                    KeyHint::new("j/k", "导航", 0),
                    KeyHint::new("Enter", "打开", 0),
                    KeyHint::new("r", "刷新", 1),
                    KeyHint::new("d", "删除", 1),
                    KeyHint::new("b", "返回", 0),
                    KeyHint::new("Esc", "菜单", 1),
                ],
                Page::PmThread => vec![
                    KeyHint::new("j/k", "滚动", 0),
                    KeyHint::new("r", "回复", 0),
                    KeyHint::new("d", "删除", 1),
                    KeyHint::new("b", "返回", 0),
                ],
                Page::Notifications => vec![
                    KeyHint::new("j/k", "导航", 0),
                    KeyHint::new("Enter", "跳转", 0),
                    KeyHint::new("r", "刷新", 1),
                    KeyHint::new("b", "返回", 0),
                    KeyHint::new("Esc", "菜单", 1),
                ],
                Page::Search => vec![
                    KeyHint::new("j/k", "导航", 0),
                    KeyHint::new("Enter", "打开", 0),
                    KeyHint::new("r", "刷新", 1),
                    KeyHint::new("/", "新搜索", 1),
                    KeyHint::new("b", "返回", 0),
                    KeyHint::new("Esc", "菜单", 1),
                ],
                Page::MyThreads | Page::MyReplies | Page::Favorites => vec![
                    KeyHint::new("j/k", "导航", 0),
                    KeyHint::new("Enter", "打开", 0),
                    KeyHint::new("r", "刷新", 1),
                    KeyHint::new("b", "返回", 0),
                    KeyHint::new("Esc", "菜单", 1),
                ],
            },
        }
    }

    /// Right-side status: loading animation and/or page indicator.
    pub fn status_right(&self) -> Option<String> {
        if self.confirm_delete.is_some() {
            return None;
        }
        if let Some(composer) = &self.composer {
            // post() includes resolve_inline_images (compress/upload) then submit.
            if composer.submitting {
                return Some(format!("发送中{}", loading_dots(self.tick)));
            }
            if composer.preparing {
                return Some(format!("准备中{}", loading_dots(self.tick)));
            }
            return None;
        }
        if self.overlay == Overlay::CommandBar {
            // Suggestions are width-aware and built in `paint_status_bar`.
            return None;
        }
        if matches!(
            self.overlay,
            Overlay::MainMenu | Overlay::Settings | Overlay::SearchPrompt | Overlay::ForumPicker
        ) {
            return None;
        }

        let loading = self.page_is_loading();
        let page = self.page_status();

        match (loading, page) {
            (true, Some(p)) => Some(format!("{} · {p}", loading_status_label(self.tick))),
            (true, None) => Some(loading_status_label(self.tick)),
            (false, Some(p)) => Some(p),
            (false, None) => None,
        }
    }

    fn page_is_loading(&self) -> bool {
        match self.page {
            Page::ThreadFeed => self.feed.loading,
            Page::ThreadDetail => self.detail.loading || self.detail.loading_more,
            Page::PmThread => self.pm_thread.loading,
            Page::PmList
            | Page::Notifications
            | Page::Search
            | Page::MyThreads
            | Page::MyReplies
            | Page::Favorites => self.list_page.loading,
            Page::Startup | Page::Login => self.login.loading,
        }
    }

    fn page_status(&self) -> Option<String> {
        match self.page {
            Page::ThreadFeed => page_status_label(self.feed.page, self.feed.max_page),
            Page::ThreadDetail => self
                .detail
                .detail
                .as_ref()
                .and_then(|d| page_status_label(d.page, d.last_page)),
            Page::PmList
            | Page::Notifications
            | Page::Search
            | Page::MyThreads
            | Page::MyReplies
            | Page::Favorites => page_status_label(self.list_page.page, self.list_page.max_page),
            _ => None,
        }
    }

    /// When command bar is open, return the current input for inline status rendering.
    pub fn status_command_input(&self) -> Option<&str> {
        if self.overlay == Overlay::CommandBar {
            Some(self.overlay_state.command_input.as_str())
        } else {
            None
        }
    }

    pub fn status_command_cursor(&self) -> Option<usize> {
        if self.overlay == Overlay::CommandBar {
            Some(
                self.overlay_state
                    .command_cursor
                    .min(self.overlay_state.command_input.len()),
            )
        } else {
            None
        }
    }

    pub fn sync_list_scroll(&mut self) {
        let item_h = list_item_height(self.page);
        let viewport = self.scroll_viewport_height();
        self.list_page.scroll_lines = snap_scroll_to_item(
            self.list_page.selected,
            self.list_page.scroll_lines,
            viewport,
            item_h,
        );
    }

    pub fn sync_pm_scroll(&mut self) {
        let item_h = hiptty_widgets::PM_ITEM_HEIGHT;
        let viewport = self.scroll_viewport_height();
        self.pm_thread.scroll_lines = snap_scroll_to_item(
            self.pm_thread.selected,
            self.pm_thread.scroll_lines,
            viewport,
            item_h,
        );
    }

    pub fn forum_picker_fids(&self) -> Vec<u32> {
        forum_picker_entries(self.feed.fid, &self.settings.default_forums)
    }

    /// Main content height below the 2-row title, two rules, and status bar.
    pub fn main_content_height(&self) -> u16 {
        self.viewport_height.saturating_sub(5)
    }

    pub fn feed_content_height(&self) -> u16 {
        self.main_content_height()
    }

    pub fn feed_list_capacity(&self) -> usize {
        thread_list_capacity(self.feed_content_height())
    }

    pub fn sync_feed_scroll(&mut self) {
        let item_h = hiptty_widgets::ITEM_HEIGHT;
        let viewport = self.scroll_viewport_height();
        self.feed.scroll_lines =
            snap_scroll_to_item(self.feed.selected, self.feed.scroll_lines, viewport, item_h);
    }

    /// Drop cached floor layout (posts / image heights changed).
    pub fn invalidate_detail_layout(&mut self) {
        self.detail.invalidate_layout();
    }

    /// Ensure [`DetailState::layout`] matches current posts, content width, revision, and image heights.
    pub fn ensure_detail_layout(&mut self) {
        let width = self.content_width();
        let revision = self.detail.layout_revision;
        let post_count = self
            .detail
            .detail
            .as_ref()
            .map(|d| d.posts.len())
            .unwrap_or(0);
        if post_count == 0 {
            self.detail.layout = None;
            return;
        }
        if self
            .detail
            .layout
            .as_ref()
            .is_some_and(|l| l.matches(width, revision))
        {
            return;
        }
        let palette = self.palette();
        let images = self.images();
        let posts = self
            .detail
            .detail
            .as_ref()
            .map(|d| d.posts.as_slice())
            .unwrap_or(&[]);
        self.detail.layout = Some(FloorLayout::build(posts, width, palette, images, revision));
    }

    /// Select the last floor and scroll to the document bottom.
    pub fn scroll_detail_to_bottom(&mut self) {
        let viewport = self.scroll_viewport_height();
        let n = self
            .detail
            .detail
            .as_ref()
            .map(|d| d.posts.len())
            .unwrap_or(0);
        if n == 0 {
            self.detail.selected = 0;
            self.detail.scroll_top = 0;
            return;
        }
        self.detail.selected = n - 1;
        let Some(layout) = self.detail_layout() else {
            return;
        };
        self.detail.scroll_top = layout.max_scroll(viewport);
    }

    /// Clamp selected index into the current posts range.
    pub fn clamp_detail_selected(&mut self) {
        let n = self
            .detail
            .detail
            .as_ref()
            .map(|d| d.posts.len())
            .unwrap_or(0);
        if n == 0 {
            self.detail.selected = 0;
        } else {
            self.detail.selected = self.detail.selected.min(n - 1);
        }
    }

    /// Borrow the cached layout, rebuilding if stale.
    pub fn detail_layout(&mut self) -> Option<&FloorLayout> {
        self.ensure_detail_layout();
        self.detail.layout.as_ref()
    }

    /// Keep the mid-floor scroll position after layout reflows (append, image decode).
    ///
    /// Anchors `(first_visible_floor, offset_within_floor)` rather than snapping to the floor
    /// top — otherwise j/k or wheel mid-floor jumps back when images become Ready.
    pub fn preserve_detail_scroll(&mut self) {
        // Posts may have grown; rebuild once, then re-clamp the same document line anchor.
        self.invalidate_detail_layout();
        let Some(layout) = self.detail_layout().cloned() else {
            return;
        };
        let viewport = self.scroll_viewport_height();
        let anchor = layout.capture_scroll_anchor(self.detail.scroll_top);
        self.detail.scroll_top = layout.restore_scroll_anchor(anchor, viewport);
    }

    /// Capture scroll anchor using *current* image heights (call before cache.poll).
    pub fn capture_detail_scroll_anchor(&mut self) -> Option<hiptty_widgets::DetailScrollAnchor> {
        let scroll_top = self.detail.scroll_top;
        let layout = self.detail_layout()?;
        Some(layout.capture_scroll_anchor(scroll_top))
    }

    /// Restore a previously captured anchor after heights changed.
    pub fn restore_detail_scroll_anchor(&mut self, anchor: hiptty_widgets::DetailScrollAnchor) {
        self.invalidate_detail_layout();
        let viewport = self.scroll_viewport_height();
        let Some(layout) = self.detail_layout() else {
            return;
        };
        let next = layout.restore_scroll_anchor(anchor, viewport);
        self.detail.scroll_top = next;
    }

    /// Content width below title/status, reserving the scrollbar column when possible.
    pub fn content_width(&self) -> u16 {
        if self.viewport_width > SCROLLBAR_COLS {
            self.viewport_width.saturating_sub(SCROLLBAR_COLS)
        } else {
            self.viewport_width
        }
    }

    pub fn scroll_viewport_height(&self) -> u16 {
        self.scroll_chrome
            .map(|c| c.viewport_len)
            .unwrap_or_else(|| self.main_content_height())
    }

    pub fn detail_breadcrumb(&self) -> String {
        let forum = self.detail.fid.and_then(forum_name).unwrap_or("Forum");
        let title = hiptty_render::display_title(&self.detail.title);
        format!("{forum} > {title}")
    }

    pub fn detail_title_counts(&self) -> Option<String> {
        let mut parts = Vec::new();
        if let Some(replies) = format_count(self.detail.reply_count.as_deref()) {
            parts.push(format!("\u{f27a} {replies}"));
        }
        if let Some(views) = format_count(self.detail.view_count.as_deref()) {
            parts.push(format!("\u{f06e} {views}"));
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join("  "))
        }
    }

    pub fn sync_detail_scroll(&mut self) {
        let viewport = self.scroll_viewport_height();
        let selected = self.detail.selected;
        let scroll_top = self.detail.scroll_top;
        // Width change must rebuild; ensure_detail_layout checks width/count.
        let Some(layout) = self.detail_layout() else {
            return;
        };
        let next = layout.ensure_scroll_top(selected, scroll_top, viewport);
        let next = layout.clamp_scroll_top(next, viewport);
        self.detail.scroll_top = next;
    }

    pub fn startup_message(&self) -> &'static str {
        if self.login.loading {
            "正在登录..."
        } else {
            "正在连接..."
        }
    }

    pub fn on_login_success(&mut self, username: String, password_plain: &str) {
        let qid = SECURITY_QUESTIONS[self.login.security_index].0;
        let answer = if self.login.security_index == 0 {
            String::new()
        } else {
            self.login.security_answer.clone()
        };
        let stored = StoredCredentials {
            username: username.clone(),
            password_md5: processed_password(password_plain),
            security_question: qid.to_string(),
            security_answer: answer,
        };
        let _ = crate::config::save_credentials(&self.credentials_path, &stored);
        self.bump_session_epoch();
        self.session.username = Some(username);
        self.session.logged_in = true;
        self.login.loading = false;
        self.login.error = None;
        self.page = Page::ThreadFeed;
        self.feed = FeedState::new(self.settings.default_forums[0]);
    }

    pub fn selected_post(&self) -> Option<&hiptty_core::Post> {
        let detail = self.detail.detail.as_ref()?;
        detail.posts.get(self.detail.selected)
    }

    pub fn prefill_login(&mut self, creds: &StoredCredentials) {
        self.login.username = creds.username.clone();
        self.login.password.clear();
        self.login.security_answer = creds.security_answer.clone();
        self.login.security_index = SECURITY_QUESTIONS
            .iter()
            .position(|(id, _)| *id == creds.security_question)
            .unwrap_or(0);
    }
}

fn loading_dots(tick: u64) -> &'static str {
    match (tick / 3) % 4 {
        0 => "",
        1 => ".",
        2 => "..",
        _ => "...",
    }
}
