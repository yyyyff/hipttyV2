use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use hiptty_core::{
    AdapterResult, Credentials, PostAction, PostResult, PrePostInfo, SearchQuery, SessionInfo,
    SimpleList, ThreadDetail, ThreadList, UserInfo,
};
use reqwest_cookie_store::CookieStoreMutex;

use crate::auth;
use crate::client::ForumClient;
use crate::fixture::{self, FixtureDump};
use crate::http::urls::ForumUrls;
use crate::http::HttpClient;
use crate::parser::{simple_list, thread_detail, thread_list, user_info};
use crate::session::{self, clear_cookie_store, load_cookie_store, save_cookie_store};

pub struct DiscuzClient {
    http: HttpClient,
    urls: ForumUrls,
    session_path: PathBuf,
    cookie_store: Arc<CookieStoreMutex>,
}

impl DiscuzClient {
    pub fn new(config_dir: Option<&std::path::Path>, profile: &str) -> AdapterResult<Self> {
        session::validate_profile(profile)?;
        let config_dir = session::config_dir(config_dir)?;
        session::migrate_legacy_session(&config_dir, profile)?;
        let session_path = session::session_path(&config_dir, profile);
        let cookie_store = load_cookie_store(&session_path)?;
        let http = HttpClient::new(Arc::clone(&cookie_store))?;

        Ok(Self {
            http,
            urls: ForumUrls::default_4d4y(),
            session_path,
            cookie_store,
        })
    }

    fn persist_session(&self) -> AdapterResult<()> {
        save_cookie_store(&self.cookie_store, &self.session_path)
    }
}

#[async_trait]
impl ForumClient for DiscuzClient {
    async fn login(&self, credentials: Credentials) -> AdapterResult<SessionInfo> {
        let info = auth::login(&self.http, &self.urls, &credentials).await?;
        self.persist_session()?;
        Ok(info)
    }

    async fn logout(&self) -> AdapterResult<()> {
        clear_cookie_store(&self.cookie_store)?;
        // Persist empty jar so a restart cannot revive the previous session.
        self.persist_session()?;
        // Best-effort remove the file; empty jar is already safe if this fails.
        let _ = std::fs::remove_file(&self.session_path);
        Ok(())
    }

    async fn session_status(&self) -> AdapterResult<SessionInfo> {
        auth::session_info(&self.http, &self.urls).await
    }

    async fn forum_threads(&self, fid: u32, page: u32) -> AdapterResult<ThreadList> {
        let url = self.urls.thread_list(fid, page);
        let html = self.http.get_text(&url).await?;
        // List max_page comes from forum `div.pages` chrome only.
        // Never fall back to per-thread `ThreadSummary.max_page` (reply pages).
        thread_list::parse(&html, page, &self.urls)
    }

    async fn thread_detail(&self, tid: &str, page: u32) -> AdapterResult<ThreadDetail> {
        let url = self.urls.thread_detail(tid, page);
        let html = self.http.get_text(&url).await?;
        thread_detail::parse(&html, tid, &self.urls)
    }

    async fn thread_at_post(&self, tid: &str, pid: &str) -> AdapterResult<ThreadDetail> {
        let url = self.urls.thread_at_post(tid, pid);
        let html = self.http.get_text(&url).await?;
        thread_detail::parse(&html, tid, &self.urls)
    }

    async fn thread_last_page(&self, tid: &str) -> AdapterResult<ThreadDetail> {
        let url = self.urls.thread_last_page(tid);
        let html = self.http.get_text(&url).await?;
        thread_detail::parse(&html, tid, &self.urls)
    }

    async fn search(&self, query: SearchQuery) -> AdapterResult<SimpleList> {
        let url = self.urls.search(&query);
        let html = self.http.get_text(&url).await?;
        if query.fulltext {
            simple_list::parse_search_fulltext(&html, query.page, &self.urls)
        } else {
            simple_list::parse_search_title(&html, query.page, &self.urls)
        }
    }

    async fn my_threads(&self, page: u32) -> AdapterResult<SimpleList> {
        let url = self.urls.my_threads(page);
        let html = self.http.get_text(&url).await?;
        simple_list::parse_my_threads(&html, page)
    }

    async fn my_replies(&self, page: u32) -> AdapterResult<SimpleList> {
        let url = self.urls.my_replies(page);
        let html = self.http.get_text(&url).await?;
        simple_list::parse_my_replies(&html, page)
    }

    async fn favorites(&self, page: u32) -> AdapterResult<SimpleList> {
        let url = self.urls.favorites("favorites", page);
        let html = self.http.get_text(&url).await?;
        simple_list::parse_favorites(&html, page)
    }

    async fn attention(&self, page: u32) -> AdapterResult<SimpleList> {
        let url = self.urls.favorites("attention", page);
        let html = self.http.get_text(&url).await?;
        simple_list::parse_favorites(&html, page)
    }

    async fn pm_list(&self) -> AdapterResult<SimpleList> {
        let url = self.urls.pm_list();
        let html = self.http.get_text(&url).await?;
        simple_list::parse_pm_list(&html, &self.urls)
    }

    async fn pm_new_list(&self) -> AdapterResult<SimpleList> {
        let url = self.urls.pm_new();
        let html = self.http.get_text(&url).await?;
        simple_list::parse_pm_new_list(&html, &self.urls)
    }

    async fn pm_thread(&self, uid: &str) -> AdapterResult<SimpleList> {
        let url = self.urls.pm_thread(uid);
        let html = self.http.get_text(&url).await?;
        simple_list::parse_pm_thread(&html, &self.urls)
    }

    async fn notifications(&self) -> AdapterResult<SimpleList> {
        let url = self.urls.notifications();
        let html = self.http.get_text(&url).await?;
        simple_list::parse_notifications(&html, &self.urls)
    }

    async fn user_info(&self, uid: &str) -> AdapterResult<UserInfo> {
        let url = self.urls.user_info(uid);
        let html = self.http.get_text(&url).await?;
        user_info::parse(&html, &self.urls)
    }

    async fn blacklist(&self) -> AdapterResult<Vec<String>> {
        let url = self.urls.blacklist();
        let html = self.http.get_text(&url).await?;
        user_info::parse_blacklist(&html)
    }

    async fn new_posts(&self, search_id: Option<&str>, page: u32) -> AdapterResult<SimpleList> {
        let url = self.urls.new_posts(search_id, page);
        let html = self.http.get_text(&url).await?;
        simple_list::parse_search_title(&html, page, &self.urls)
    }

    async fn check_new_pm(&self) -> AdapterResult<bool> {
        let url = self.urls.pm_check_new();
        let body = self.http.get_text(&url).await?;
        simple_list::parse_check_new_pm(&body)
    }

    async fn prepare_post(&self, action: PostAction) -> AdapterResult<PrePostInfo> {
        crate::write::prepare_post(&self.http, &self.urls, action).await
    }

    async fn post(
        &self,
        action: PostAction,
        content: &str,
        subject: Option<&str>,
        delete: bool,
    ) -> AdapterResult<PostResult> {
        crate::write::post(&self.http, &self.urls, action, content, subject, delete).await
    }

    async fn send_pm(&self, uid: &str, content: &str) -> AdapterResult<()> {
        crate::write::send_pm(&self.http, &self.urls, uid, content).await
    }

    async fn pm_delete(&self, uid: &str) -> AdapterResult<()> {
        crate::write::pm_delete(&self.http, &self.urls, uid).await
    }

    async fn favorite_add(&self, tid: &str) -> AdapterResult<()> {
        crate::write::favorite_add(&self.http, &self.urls, tid).await
    }

    async fn favorite_remove(&self, tid: &str) -> AdapterResult<()> {
        crate::write::favorite_remove(&self.http, &self.urls, tid).await
    }

    async fn blacklist_add(&self, username: &str) -> AdapterResult<()> {
        crate::write::blacklist_add(&self.http, &self.urls, username).await
    }

    async fn blacklist_remove(&self, username: &str) -> AdapterResult<()> {
        crate::write::blacklist_remove(&self.http, &self.urls, username).await
    }

    async fn upload_image(
        &self,
        action: PostAction,
        data: &[u8],
        filename: &str,
    ) -> AdapterResult<String> {
        crate::write::upload_image(&self.http, &self.urls, action, data, filename).await
    }

    async fn dump_fixture(&self, url: &str, output: Option<&Path>) -> AdapterResult<FixtureDump> {
        fixture::dump_fixture(&self.http, &self.urls.base, url, output).await
    }

    async fn fetch_url(&self, url: &str) -> AdapterResult<Vec<u8>> {
        self.http.get_bytes(url).await
    }
}
