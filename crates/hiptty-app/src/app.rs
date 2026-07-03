use hiptty_core::{
    forum_name, processed_password, AppSettings, SessionInfo, StoredCredentials, ThreadDetail,
    ThreadSummary, SECURITY_QUESTIONS,
};
use hiptty_render::{format_count, Palette};
use hiptty_image::{AvatarDiskCache, FetchRequest, ImageCache};
use hiptty_widgets::{
    clamp_scroll_top, ensure_scroll_top, ensure_thread_scroll, forum_picker_entries,
    thread_list_capacity, LoginField,
};
use ratatui_image::picker::Picker;
use tokio::sync::mpsc;

use crate::composer::{ComposerState, ConfirmDeleteState};
use crate::worker::WorkerRequest;

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    None,
    ForumPicker,
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
    pub scroll: usize,
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
            scroll: 0,
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
    pub tick: u64,
    pub viewport_width: u16,
    pub viewport_height: u16,
    pub toast: Option<String>,
    pub composer: Option<ComposerState>,
    pub confirm_delete: Option<ConfirmDeleteState>,
    pub quit: bool,
    pub profile: String,
    pub config_dir: std::path::PathBuf,
    pub credentials_path: std::path::PathBuf,
    pub settings_path: std::path::PathBuf,
    pub startup_done: bool,
    pub image_cache: Option<ImageCache>,
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
            tick: 0,
            viewport_width: 80,
            viewport_height: 24,
            toast: None,
            composer: None,
            confirm_delete: None,
            quit: false,
            profile,
            config_dir,
            credentials_path,
            settings_path,
            startup_done: false,
            image_cache: None,
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
        Palette::for_theme(self.settings.theme)
    }

    pub fn breadcrumb(&self) -> String {
        match self.page {
            Page::Startup | Page::Login => String::new(),
            Page::ThreadFeed => forum_name(self.feed.fid).unwrap_or("Forum").to_string(),
            Page::ThreadDetail => self.detail_breadcrumb(),
        }
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
            Overlay::None => match self.page {
                Page::Startup => "Esc 退出",
                Page::Login => "Tab/↑↓ 切换 · Enter 确认 · Esc 退出",
                Page::ThreadFeed => {
                    "j/k ↑↓  Enter 打开  r 回复  n 新帖  f 切换版块  / 搜索  b 返回"
                }
                Page::ThreadDetail => {
                    "j/k ↑↓  PgUp/Dn  r 回复  q 引用  e 编辑  d 删除  g/G 首页/末页  b 返回"
                }
            },
        }
    }

    pub fn forum_picker_fids(&self) -> Vec<u32> {
        forum_picker_entries(&self.settings.default_forums)
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
        let capacity = self.feed_list_capacity();
        self.feed.scroll = ensure_thread_scroll(self.feed.selected, self.feed.scroll, capacity);
    }

    pub fn detail_breadcrumb(&self) -> String {
        let forum = self
            .detail
            .fid
            .and_then(forum_name)
            .unwrap_or("Forum");
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
        let viewport = self.main_content_height();
        let palette = self.palette();
        let images = self.images();
        self.detail.scroll_top = clamp_scroll_top(
            ensure_scroll_top(
                self.detail.selected,
                self.detail.scroll_top,
                &detail.posts,
                self.viewport_width,
                viewport,
                palette,
                images,
            ),
            &detail.posts,
            self.viewport_width,
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
