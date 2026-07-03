use std::path::Path;

use async_trait::async_trait;
use hiptty_core::{
    AdapterResult, Credentials, PostAction, PostResult, PrePostInfo, SearchQuery, SessionInfo,
    SimpleList, ThreadDetail, ThreadList, UserInfo,
};

/// Async API surface for 4d4y Discuz forum operations.
///
/// CLI and future TUI both consume this trait; implementations must not depend on ratatui.
#[async_trait]
pub trait ForumClient: Send + Sync {
    // --- Auth ---

    async fn login(&self, credentials: Credentials) -> AdapterResult<SessionInfo>;
    async fn logout(&self) -> AdapterResult<()>;
    async fn session_status(&self) -> AdapterResult<SessionInfo>;

    // --- Read ---

    async fn forum_threads(&self, fid: u32, page: u32) -> AdapterResult<ThreadList>;
    async fn thread_detail(&self, tid: &str, page: u32) -> AdapterResult<ThreadDetail>;
    async fn thread_at_post(&self, tid: &str, pid: &str) -> AdapterResult<ThreadDetail>;
    async fn thread_last_page(&self, tid: &str) -> AdapterResult<ThreadDetail>;
    async fn search(&self, query: SearchQuery) -> AdapterResult<SimpleList>;
    async fn my_threads(&self, page: u32) -> AdapterResult<SimpleList>;
    async fn my_replies(&self, page: u32) -> AdapterResult<SimpleList>;
    async fn favorites(&self, page: u32) -> AdapterResult<SimpleList>;
    async fn attention(&self, page: u32) -> AdapterResult<SimpleList>;
    async fn pm_list(&self) -> AdapterResult<SimpleList>;
    async fn pm_new_list(&self) -> AdapterResult<SimpleList>;
    async fn pm_thread(&self, uid: &str) -> AdapterResult<SimpleList>;
    async fn notifications(&self) -> AdapterResult<SimpleList>;
    async fn user_info(&self, uid: &str) -> AdapterResult<UserInfo>;
    async fn blacklist(&self) -> AdapterResult<Vec<String>>;
    async fn new_posts(&self, search_id: Option<&str>, page: u32) -> AdapterResult<SimpleList>;
    async fn check_new_pm(&self) -> AdapterResult<bool>;

    // --- Write (no vote_poll) ---

    async fn prepare_post(&self, action: PostAction) -> AdapterResult<PrePostInfo>;
    async fn post(
        &self,
        action: PostAction,
        content: &str,
        subject: Option<&str>,
        delete: bool,
    ) -> AdapterResult<PostResult>;
    async fn send_pm(&self, uid: &str, content: &str) -> AdapterResult<()>;
    async fn pm_delete(&self, uid: &str) -> AdapterResult<()>;
    async fn favorite_add(&self, tid: &str) -> AdapterResult<()>;
    async fn favorite_remove(&self, tid: &str) -> AdapterResult<()>;
    async fn blacklist_add(&self, username: &str) -> AdapterResult<()>;
    async fn blacklist_remove(&self, username: &str) -> AdapterResult<()>;
    async fn upload_image(
        &self,
        action: PostAction,
        data: &[u8],
        filename: &str,
    ) -> AdapterResult<String>;

    // --- Admin / dev ---

    async fn dump_fixture(
        &self,
        url: &str,
        output: Option<&Path>,
    ) -> AdapterResult<crate::fixture::FixtureDump>;

    /// Fetch raw bytes (e.g. avatars, smilies, inline images). Uses session cookies.
    async fn fetch_url(&self, url: &str) -> AdapterResult<Vec<u8>>;
}
