use hiptty_core::{
    forum_name, processed_password, AppSettings, SessionInfo, StoredCredentials, ThreadDetail,
    ThreadSummary, SECURITY_QUESTIONS,
};
use hiptty_image::{AvatarDiskCache, FetchRequest, ImageCache};
use hiptty_render::{format_count, Palette};
use hiptty_widgets::{
    clamp_scroll_top, ensure_scroll_top, first_visible_floor, floor_offsets, forum_picker_entries,
    snap_scroll_to_item, thread_list_capacity, LoginField, ScrollBarInteraction, ScrollChrome,
    TitleBarHits, TOAST_ERROR_TICKS, TOAST_SUCCESS_TICKS, SCROLLBAR_COLS,
};
use ratatui_image::picker::Picker;
use tokio::sync::mpsc;

use crate::composer::{ComposerState, ConfirmDeleteState};
use crate::list_page::{ListPageState, PmThreadState};
use crate::nav::NavStack;
use crate::worker::WorkerRequest;

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

/// How the next `ThreadDetailLoaded` response should merge into state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailFetchMode {
    /// Replace `posts` (open thread, `g`, or `G`).
    Replace,
    /// Append `posts` when scrolling near the end of the current page.
    Append,
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
    pub command_input: String,
    pub search_input: String,
}

#[derive(Debug, Clone, Default)]
pub struct UnreadState {
    pub has_pm: bool,
    pub has_notifications: bool,
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
    pub scroll_top: u16,
    pub loading: bool,
    pub loading_more: bool,
    pub pending_fetch: Option<DetailFetchMode>,
    pub error: Option<String>,
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
            pending_fetch: None,
            error: None,
        }
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
    /// Updated each frame for mouse hit-testing and scrollbar interaction.
    pub scroll_chrome: Option<ScrollChrome>,
    pub scrollbar_interaction: ScrollBarInteraction,
    pub title_bar_area: ratatui::layout::Rect,
    pub title_bar_hits: TitleBarHits,
    pub main_menu_hits: Vec<ratatui::layout::Rect>,
    pub settings_hits: Vec<ratatui::layout::Rect>,
    pub last_click: Option<MouseClickState>,
}

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
                pending_fetch: None,
                error: None,
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

    pub fn dispatch_image_fetches(
        &self,
        requests: impl IntoIterator<Item = FetchRequest>,
        worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
    ) {
        for req in requests {
            let _ = worker_tx.send(WorkerRequest::FetchImage {
                url: req.url,
                kind: req.kind,
            });
        }
    }

    pub fn palette(&self) -> Palette {
        Palette::default()
    }

    pub fn dims_background(&self) -> bool {
        self.overlay != Overlay::None || self.confirm_delete.is_some()
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
        let expired = self
            .toast
            .as_ref()
            .is_some_and(|toast| self.tick >= toast.started_at.saturating_add(toast.duration_ticks));
        if expired {
            self.toast = None;
        }
    }

    pub fn dismiss_toast(&mut self) {
        self.toast = None;
    }

    pub fn status_hints(&self) -> &'static str {
        if self.confirm_delete.is_some() {
            return "y 确认  n/Esc 取消";
        }
        if self.composer.is_some() {
            return "Ctrl+S 发送  Esc 取消  Ctrl+I 插图";
        }
        match self.overlay {
            Overlay::ForumPicker => "j/k  Enter  Esc",
            Overlay::MainMenu => "",
            Overlay::Settings => "j/k  Enter  Esc",
            Overlay::SearchPrompt => "Enter 搜索  Esc 取消",
            Overlay::CommandBar => "Enter 执行  Esc 取消",
            Overlay::None => match self.page {
                Page::Startup => "Esc 退出",
                Page::Login => "Tab/↑↓ 切换 · Enter 确认 · Esc 退出",
                Page::ThreadFeed => {
                    "j/k ↑↓  Enter 打开  [ ] 版块  f 更多  r 回复  n 新帖  / 搜索  Esc 菜单  : 命令"
                }
                Page::ThreadDetail => {
                    "j/k ↑↓  PgUp/Dn  r 回复  q 引用  e 编辑  d 删除  g/G 首页/末页  b 返回"
                }
                Page::PmList => "j/k  Enter 打开  d 删除  b 返回  Esc 菜单",
                Page::PmThread => "r 回复  d 删除对话  j/k 滚动  b 返回",
                Page::Notifications => "j/k  Enter 跳转  b 返回  Esc 菜单",
                Page::Search => "j/k  Enter 打开  / 新搜索  b 返回  Esc 菜单",
                Page::MyThreads | Page::MyReplies | Page::Favorites => {
                    "j/k  Enter 打开  b 返回  Esc 菜单"
                }
            },
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

    /// Keep the top visible floor anchored after layout changes (append, image measure).
    pub fn preserve_detail_scroll(&mut self) {
        let Some(detail) = &self.detail.detail else {
            return;
        };
        if detail.posts.is_empty() {
            return;
        }
        let viewport = self.scroll_viewport_height();
        let width = self.content_width();
        let palette = self.palette();
        let images = self.images();
        let anchor = first_visible_floor(
            self.detail.scroll_top,
            &detail.posts,
            width,
            palette,
            images,
        );
        let offsets = floor_offsets(&detail.posts, width, palette, images);
        let anchored = offsets.get(anchor).copied().unwrap_or(0);
        self.detail.scroll_top =
            clamp_scroll_top(anchored, &detail.posts, width, viewport, palette, images);
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
        let Some(detail) = &self.detail.detail else {
            return;
        };
        if detail.posts.is_empty() {
            return;
        }
        let viewport = self.scroll_viewport_height();
        let width = self.content_width();
        let palette = self.palette();
        let images = self.images();
        self.detail.scroll_top = clamp_scroll_top(
            ensure_scroll_top(
                self.detail.selected,
                self.detail.scroll_top,
                &detail.posts,
                width,
                viewport,
                palette,
                images,
            ),
            &detail.posts,
            width,
            viewport,
            palette,
            images,
        );
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
