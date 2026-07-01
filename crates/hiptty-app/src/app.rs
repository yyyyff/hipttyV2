use hiptty_core::{
    forum_name, processed_password, AppSettings, SessionInfo, StoredCredentials, ThreadSummary,
    SECURITY_QUESTIONS,
};
use hiptty_render::Palette;
use hiptty_widgets::{forum_picker_entries, LoginField};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Login,
    ThreadFeed,
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

#[derive(Debug, Clone)]
pub struct App {
    pub page: Page,
    pub overlay: Overlay,
    pub settings: AppSettings,
    pub session: SessionInfo,
    pub login: LoginState,
    pub feed: FeedState,
    pub forum_picker_selected: usize,
    pub tick: u64,
    pub toast: Option<String>,
    pub quit: bool,
    pub profile: String,
    pub config_dir: std::path::PathBuf,
    pub credentials_path: std::path::PathBuf,
    pub settings_path: std::path::PathBuf,
    pub startup_done: bool,
}

impl App {
    pub fn new(settings: AppSettings, config_dir: std::path::PathBuf, profile: String) -> Self {
        let credentials_path = crate::config::credentials_path(&config_dir, &profile);
        let settings_path = crate::config::settings_path(&config_dir);
        let default_fid = settings.default_forums[0];
        Self {
            page: Page::Login,
            overlay: Overlay::None,
            settings,
            session: SessionInfo {
                logged_in: false,
                username: None,
                uid: None,
            },
            login: LoginState::default(),
            feed: FeedState::new(default_fid),
            forum_picker_selected: 0,
            tick: 0,
            toast: None,
            quit: false,
            profile,
            config_dir,
            credentials_path,
            settings_path,
            startup_done: false,
        }
    }

    pub fn palette(&self) -> Palette {
        Palette::for_theme(self.settings.theme)
    }

    pub fn breadcrumb(&self) -> String {
        match self.page {
            Page::Login => String::new(),
            Page::ThreadFeed => forum_name(self.feed.fid).unwrap_or("Forum").to_string(),
        }
    }

    pub fn status_hints(&self) -> &'static str {
        match self.overlay {
            Overlay::ForumPicker => "j/k  Enter  Esc",
            Overlay::None => match self.page {
                Page::Login => "Tab 切换 · Esc 退出",
                Page::ThreadFeed => "j/k ↑↓  Enter  r  n  f  /  b",
            },
        }
    }

    pub fn forum_picker_fids(&self) -> Vec<u32> {
        forum_picker_entries(&self.settings.default_forums)
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
