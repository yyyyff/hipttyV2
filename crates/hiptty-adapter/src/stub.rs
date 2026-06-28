use std::path::Path;

use async_trait::async_trait;
use hiptty_core::{
    AdapterError, AdapterResult, Credentials, PostAction, PostResult, PrePostInfo, SearchQuery,
    SessionInfo, SimpleList, ThreadDetail, ThreadList, UserInfo,
};

use crate::client::ForumClient;

/// Placeholder client — every method returns `NotImplemented` until PR2+.
#[derive(Debug, Default, Clone)]
pub struct StubForumClient;

impl StubForumClient {
    fn not_impl(method: &str) -> AdapterError {
        AdapterError::NotImplemented(method.to_string())
    }
}

#[async_trait]
impl ForumClient for StubForumClient {
    async fn login(&self, _credentials: Credentials) -> AdapterResult<SessionInfo> {
        Err(Self::not_impl("login"))
    }

    async fn logout(&self) -> AdapterResult<()> {
        Err(Self::not_impl("logout"))
    }

    async fn session_status(&self) -> AdapterResult<SessionInfo> {
        Err(Self::not_impl("session_status"))
    }

    async fn forum_threads(&self, _fid: u32, _page: u32) -> AdapterResult<ThreadList> {
        Err(Self::not_impl("forum_threads"))
    }

    async fn thread_detail(&self, _tid: &str, _page: u32) -> AdapterResult<ThreadDetail> {
        Err(Self::not_impl("thread_detail"))
    }

    async fn thread_at_post(&self, _tid: &str, _pid: &str) -> AdapterResult<ThreadDetail> {
        Err(Self::not_impl("thread_at_post"))
    }

    async fn thread_last_page(&self, _tid: &str) -> AdapterResult<ThreadDetail> {
        Err(Self::not_impl("thread_last_page"))
    }

    async fn search(&self, _query: SearchQuery) -> AdapterResult<SimpleList> {
        Err(Self::not_impl("search"))
    }

    async fn my_threads(&self, _page: u32) -> AdapterResult<SimpleList> {
        Err(Self::not_impl("my_threads"))
    }

    async fn my_replies(&self, _page: u32) -> AdapterResult<SimpleList> {
        Err(Self::not_impl("my_replies"))
    }

    async fn favorites(&self, _page: u32) -> AdapterResult<SimpleList> {
        Err(Self::not_impl("favorites"))
    }

    async fn attention(&self, _page: u32) -> AdapterResult<SimpleList> {
        Err(Self::not_impl("attention"))
    }

    async fn pm_list(&self) -> AdapterResult<SimpleList> {
        Err(Self::not_impl("pm_list"))
    }

    async fn pm_new_list(&self) -> AdapterResult<SimpleList> {
        Err(Self::not_impl("pm_new_list"))
    }

    async fn pm_thread(&self, _uid: &str) -> AdapterResult<SimpleList> {
        Err(Self::not_impl("pm_thread"))
    }

    async fn notifications(&self) -> AdapterResult<SimpleList> {
        Err(Self::not_impl("notifications"))
    }

    async fn user_info(&self, _uid: &str) -> AdapterResult<UserInfo> {
        Err(Self::not_impl("user_info"))
    }

    async fn blacklist(&self) -> AdapterResult<Vec<String>> {
        Err(Self::not_impl("blacklist"))
    }

    async fn new_posts(&self, _search_id: Option<&str>, _page: u32) -> AdapterResult<SimpleList> {
        Err(Self::not_impl("new_posts"))
    }

    async fn check_new_pm(&self) -> AdapterResult<bool> {
        Err(Self::not_impl("check_new_pm"))
    }

    async fn prepare_post(&self, _action: PostAction) -> AdapterResult<PrePostInfo> {
        Err(Self::not_impl("prepare_post"))
    }

    async fn post(
        &self,
        _action: PostAction,
        _content: &str,
        _subject: Option<&str>,
        _delete: bool,
    ) -> AdapterResult<PostResult> {
        Err(Self::not_impl("post"))
    }

    async fn send_pm(&self, _uid: &str, _content: &str) -> AdapterResult<()> {
        Err(Self::not_impl("send_pm"))
    }

    async fn pm_delete(&self, _uid: &str) -> AdapterResult<()> {
        Err(Self::not_impl("pm_delete"))
    }

    async fn favorite_add(&self, _tid: &str) -> AdapterResult<()> {
        Err(Self::not_impl("favorite_add"))
    }

    async fn favorite_remove(&self, _tid: &str) -> AdapterResult<()> {
        Err(Self::not_impl("favorite_remove"))
    }

    async fn blacklist_add(&self, _username: &str) -> AdapterResult<()> {
        Err(Self::not_impl("blacklist_add"))
    }

    async fn blacklist_remove(&self, _username: &str) -> AdapterResult<()> {
        Err(Self::not_impl("blacklist_remove"))
    }

    async fn upload_image(
        &self,
        _action: PostAction,
        _data: &[u8],
        _filename: &str,
    ) -> AdapterResult<String> {
        Err(Self::not_impl("upload_image"))
    }

    async fn dump_fixture(
        &self,
        _url: &str,
        _output: Option<&Path>,
    ) -> AdapterResult<crate::fixture::FixtureDump> {
        Err(Self::not_impl("dump_fixture"))
    }
}
