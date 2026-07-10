//! Background work for ForumClient I/O.
//!
//! Four lanes (dispatcher never blocks on long work except logout barrier):
//! 1. **Read scopes** — latest-wins; new request aborts the previous handle in the same scope.
//! 2. **Session/Write lane** — serial; never cancelled (auth, posts, PM, upload, logout).
//! 3. **CheckUnread** — independent background task with `session_epoch`.
//! 4. **FetchImage** — concurrent spawns (unchanged).
//!
//! HTTP layer has no automatic retries for POST/upload; this worker also never retries writes.

use std::sync::Arc;

use hiptty_adapter::ForumClient;
use hiptty_core::SearchQuery;
use hiptty_core::{
    AdapterResult, Credentials, PostAction, PostResult, PrePostInfo, SessionInfo, ThreadDetail,
    ThreadList, SECURITY_QUESTIONS,
};
use hiptty_image::ImageKind;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::list_page::ListPageKind;

/// Cancel scope for foreground reads. Same-scope replacements abort the prior task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ReadScope {
    Feed,
    Detail,
    SimpleList,
    PmThread,
    Blacklist,
    ComposerPrepare,
}

const READ_SCOPE_COUNT: usize = 6;

impl ReadScope {
    fn index(self) -> usize {
        match self {
            Self::Feed => 0,
            Self::Detail => 1,
            Self::SimpleList => 2,
            Self::PmThread => 3,
            Self::Blacklist => 4,
            Self::ComposerPrepare => 5,
        }
    }
}

#[derive(Debug)]
pub enum WorkerRequest {
    CheckSession {
        /// Monotonic auth operation id; only the latest result applies in the UI.
        auth_op_id: u64,
    },
    AutoLogin {
        creds: StoredCreds,
        auth_op_id: u64,
    },
    ManualLogin {
        username: String,
        password: String,
        security_index: usize,
        security_answer: String,
        auth_op_id: u64,
    },
    LoadThreads {
        fid: u32,
        page: u32,
        request_id: u64,
    },
    LoadThreadDetail {
        tid: String,
        page: u32,
        request_id: u64,
    },
    FetchImage {
        url: String,
        kind: ImageKind,
        /// Session epoch captured when the fetch was dispatched; stale after logout/login.
        session_epoch: u64,
    },
    PreparePost {
        action: PostAction,
        request_id: u64,
    },
    SubmitPost {
        action: PostAction,
        content: String,
        subject: Option<String>,
        delete: bool,
    },
    UploadComposerImage {
        action: PostAction,
        path: String,
    },
    LoadSimpleList {
        kind: ListPageKind,
        page: u32,
        fid: u32,
        query: String,
        search_id: Option<String>,
        request_id: u64,
    },
    LoadPmThread {
        uid: String,
        request_id: u64,
    },
    SendPm {
        uid: String,
        content: String,
    },
    PmDelete {
        uid: String,
    },
    LoadThreadAtPost {
        tid: String,
        pid: String,
        request_id: u64,
    },
    CheckUnread {
        session_epoch: u64,
    },
    LoadBlacklist {
        request_id: u64,
    },
    /// Clear in-memory cookies and persist empty session (local logout).
    Logout {
        auth_op_id: u64,
    },
}

#[derive(Debug, Clone)]
pub struct StoredCreds {
    pub username: String,
    pub password_md5: String,
    pub security_question: String,
    pub security_answer: String,
}

#[derive(Debug)]
pub enum WorkerResponse {
    Session {
        auth_op_id: u64,
        info: SessionInfo,
    },
    LoginResult {
        auth_op_id: u64,
        manual: bool,
        result: AdapterResult<SessionInfo>,
        username: String,
        password_plain: Option<String>,
    },
    ThreadsLoaded {
        fid: u32,
        page: u32,
        request_id: u64,
        result: AdapterResult<ThreadList>,
    },
    ThreadDetailLoaded {
        tid: String,
        page: u32,
        request_id: u64,
        result: AdapterResult<ThreadDetail>,
    },
    ImageFetched {
        url: String,
        kind: ImageKind,
        session_epoch: u64,
        result: AdapterResult<Vec<u8>>,
    },
    PrePostReady {
        action: PostAction,
        request_id: u64,
        result: AdapterResult<PrePostInfo>,
    },
    PostSubmitted {
        action: PostAction,
        delete: bool,
        result: AdapterResult<PostResult>,
    },
    ComposerImageUploaded {
        path: String,
        result: AdapterResult<String>,
    },
    SimpleListLoaded {
        kind: ListPageKind,
        request_id: u64,
        result: AdapterResult<hiptty_core::SimpleList>,
    },
    PmThreadLoaded {
        uid: String,
        request_id: u64,
        result: AdapterResult<hiptty_core::SimpleList>,
    },
    PmSent {
        uid: String,
        result: AdapterResult<()>,
    },
    PmDeleted {
        uid: String,
        result: AdapterResult<()>,
    },
    ThreadAtPostLoaded {
        tid: String,
        request_id: u64,
        result: AdapterResult<ThreadDetail>,
    },
    UnreadChecked {
        session_epoch: u64,
        /// `None` when the network check failed; UI must not clear existing unread flags.
        has_pm: Option<bool>,
        has_notifications: Option<bool>,
    },
    BlacklistLoaded {
        request_id: u64,
        result: AdapterResult<Vec<String>>,
    },
    LoggedOut {
        auth_op_id: u64,
        result: AdapterResult<()>,
    },
}

pub fn spawn_worker<C: ForumClient + 'static>(
    client: C,
    mut rx: mpsc::UnboundedReceiver<WorkerRequest>,
    tx: mpsc::UnboundedSender<WorkerResponse>,
) {
    let client = Arc::new(client);
    tokio::spawn(async move {
        let (write_tx, mut write_rx) = mpsc::unbounded_channel::<WorkerRequest>();
        // Write task signals when cookie-mutating auth ops finish so the dispatcher can
        // release the session barrier. Deferred reads are always discarded (UI re-requests
        // Feed/unread after successful login; fail/logout must not resume stale work).
        let (auth_done_tx, mut auth_done_rx) = mpsc::unbounded_channel::<()>();
        let write_client = Arc::clone(&client);
        let write_resp = tx.clone();
        tokio::spawn(async move {
            while let Some(req) = write_rx.recv().await {
                let auth_barrier_op = is_auth_barrier_op(&req);
                handle_session_write(write_client.as_ref(), req, &write_resp).await;
                if auth_barrier_op {
                    let _ = auth_done_tx.send(());
                }
            }
        });

        let mut read_handles: [Option<JoinHandle<()>>; READ_SCOPE_COUNT] = Default::default();
        let mut unread_handle: Option<JoinHandle<()>> = None;
        let mut unread_epoch: Option<u64> = None;
        // Nested AutoLogin/Logout while one is still on the write lane.
        let mut auth_barrier_depth: u32 = 0;
        // Latest-wins buffer while barrier is up; discarded when barrier fully lifts.
        let mut pending_reads: [Option<WorkerRequest>; READ_SCOPE_COUNT] = Default::default();
        let mut pending_unread: Option<u64> = None;

        loop {
            tokio::select! {
                biased;
                done = auth_done_rx.recv() => {
                    if done.is_none() {
                        break;
                    }
                    auth_barrier_depth = auth_barrier_depth.saturating_sub(1);
                    if auth_barrier_depth == 0 {
                        // Always discard: success path UI issues a fresh Feed/unread request;
                        // fail/logout must not re-fire AuthRequired loops or old-session pages.
                        discard_deferred_work(&mut pending_reads, &mut pending_unread);
                    }
                }
                req = rx.recv() => {
                    let Some(req) = req else { break; };
                    match req {
                        WorkerRequest::FetchImage {
                            url,
                            kind,
                            session_epoch,
                        } => {
                            // Bare client — not part of the session cookie barrier.
                            let client = Arc::clone(&client);
                            let tx = tx.clone();
                            tokio::spawn(async move {
                                let result = client.fetch_url(&url).await;
                                let _ = tx.send(WorkerResponse::ImageFetched {
                                    url,
                                    kind,
                                    session_epoch,
                                    result,
                                });
                            });
                        }

                        WorkerRequest::CheckUnread { session_epoch } => {
                            if auth_barrier_depth > 0 {
                                pending_unread = Some(session_epoch);
                                continue;
                            }
                            start_or_replace_unread(
                                &mut unread_handle,
                                &mut unread_epoch,
                                session_epoch,
                                Arc::clone(&client),
                                tx.clone(),
                            )
                            .await;
                        }

                        // Full session barrier: abort in-flight reads; park new ones for discard.
                        req @ (WorkerRequest::AutoLogin { .. }
                        | WorkerRequest::ManualLogin { .. }
                        | WorkerRequest::Logout { .. }) => {
                            abort_session_reads(&mut read_handles, &mut unread_handle).await;
                            unread_epoch = None;
                            auth_barrier_depth = auth_barrier_depth.saturating_add(1);
                            let _ = write_tx.send(req);
                        }

                        req if is_session_write(&req) => {
                            let _ = write_tx.send(req);
                        }

                        req => {
                            if let Some(scope) = read_scope_of(&req) {
                                if auth_barrier_depth > 0 {
                                    pending_reads[scope.index()] = Some(req);
                                    continue;
                                }
                                replace_read(
                                    &mut read_handles,
                                    scope,
                                    Arc::clone(&client),
                                    tx.clone(),
                                    req,
                                )
                                .await;
                            }
                        }
                    }
                }
            }
        }
    });
}

fn is_auth_barrier_op(req: &WorkerRequest) -> bool {
    matches!(
        req,
        WorkerRequest::AutoLogin { .. }
            | WorkerRequest::ManualLogin { .. }
            | WorkerRequest::Logout { .. }
    )
}

fn is_session_write(req: &WorkerRequest) -> bool {
    matches!(
        req,
        WorkerRequest::CheckSession { .. }
            | WorkerRequest::AutoLogin { .. }
            | WorkerRequest::ManualLogin { .. }
            | WorkerRequest::SubmitPost { .. }
            | WorkerRequest::SendPm { .. }
            | WorkerRequest::PmDelete { .. }
            | WorkerRequest::UploadComposerImage { .. }
            | WorkerRequest::Logout { .. }
    )
}

fn read_scope_of(req: &WorkerRequest) -> Option<ReadScope> {
    match req {
        WorkerRequest::LoadThreads { .. } => Some(ReadScope::Feed),
        WorkerRequest::LoadThreadDetail { .. } | WorkerRequest::LoadThreadAtPost { .. } => {
            Some(ReadScope::Detail)
        }
        WorkerRequest::LoadSimpleList { .. } => Some(ReadScope::SimpleList),
        WorkerRequest::LoadPmThread { .. } => Some(ReadScope::PmThread),
        WorkerRequest::LoadBlacklist { .. } => Some(ReadScope::Blacklist),
        WorkerRequest::PreparePost { .. } => Some(ReadScope::ComposerPrepare),
        _ => None,
    }
}

async fn replace_read<C: ForumClient + 'static>(
    handles: &mut [Option<JoinHandle<()>>; READ_SCOPE_COUNT],
    scope: ReadScope,
    client: Arc<C>,
    tx: mpsc::UnboundedSender<WorkerResponse>,
    req: WorkerRequest,
) {
    let idx = scope.index();
    if let Some(prev) = handles[idx].take() {
        prev.abort();
        // Join so session-client work cannot race cookie clear after logout abort path.
        let _ = prev.await;
    }
    handles[idx] = Some(tokio::spawn(async move {
        handle_read(client.as_ref(), req, &tx).await;
    }));
}

async fn abort_and_join_all(handles: &mut [Option<JoinHandle<()>>; READ_SCOPE_COUNT]) {
    for slot in handles.iter_mut() {
        if let Some(h) = slot.take() {
            h.abort();
            let _ = h.await;
        }
    }
}

/// Cancel all cancelable session-client work (reads + unread). Images use bare client — not here.
async fn abort_session_reads(
    read_handles: &mut [Option<JoinHandle<()>>; READ_SCOPE_COUNT],
    unread_handle: &mut Option<JoinHandle<()>>,
) {
    abort_and_join_all(read_handles).await;
    if let Some(h) = unread_handle.take() {
        h.abort();
        let _ = h.await;
    }
}

async fn start_or_replace_unread<C: ForumClient + 'static>(
    unread_handle: &mut Option<JoinHandle<()>>,
    unread_epoch: &mut Option<u64>,
    session_epoch: u64,
    client: Arc<C>,
    tx: mpsc::UnboundedSender<WorkerResponse>,
) {
    // Same epoch: UI already coalesces; drop if still in flight.
    // New epoch: must abort the old task or check_in_flight sticks.
    if let Some(h) = unread_handle.take() {
        if !h.is_finished() {
            if *unread_epoch == Some(session_epoch) {
                *unread_handle = Some(h);
                return;
            }
            h.abort();
            let _ = h.await;
        }
    }
    *unread_epoch = Some(session_epoch);
    *unread_handle = Some(tokio::spawn(async move {
        let (pm_result, notif_result) = tokio::join!(client.check_new_pm(), client.notifications());
        let has_pm = pm_result.ok();
        let has_notifications = notif_result
            .ok()
            .map(|list| list.items.iter().any(|i| i.is_new));
        let _ = tx.send(WorkerResponse::UnreadChecked {
            session_epoch,
            has_pm,
            has_notifications,
        });
    }));
}

fn discard_deferred_work(
    pending_reads: &mut [Option<WorkerRequest>; READ_SCOPE_COUNT],
    pending_unread: &mut Option<u64>,
) {
    for slot in pending_reads.iter_mut() {
        *slot = None;
    }
    *pending_unread = None;
}

async fn handle_read<C: ForumClient + ?Sized>(
    client: &C,
    req: WorkerRequest,
    tx: &mpsc::UnboundedSender<WorkerResponse>,
) {
    match req {
        WorkerRequest::LoadThreads {
            fid,
            page,
            request_id,
        } => {
            let result = client.forum_threads(fid, page).await;
            let _ = tx.send(WorkerResponse::ThreadsLoaded {
                fid,
                page,
                request_id,
                result,
            });
        }
        WorkerRequest::LoadThreadDetail {
            tid,
            page,
            request_id,
        } => {
            let result = client.thread_detail(&tid, page).await;
            let _ = tx.send(WorkerResponse::ThreadDetailLoaded {
                tid,
                page,
                request_id,
                result,
            });
        }
        WorkerRequest::LoadThreadAtPost {
            tid,
            pid,
            request_id,
        } => {
            let result = client.thread_at_post(&tid, &pid).await;
            let _ = tx.send(WorkerResponse::ThreadAtPostLoaded {
                tid,
                request_id,
                result,
            });
        }
        WorkerRequest::LoadSimpleList {
            kind,
            page,
            fid,
            query,
            search_id,
            request_id,
        } => {
            let result = match kind {
                ListPageKind::PmList => client.pm_list().await,
                ListPageKind::Notifications => client.notifications().await,
                ListPageKind::Search => {
                    let mut q = SearchQuery::new(query);
                    q.fid = Some(fid.to_string());
                    q.page = page.max(1);
                    if let Some(id) = search_id {
                        client.new_posts(Some(&id), page.max(1)).await
                    } else {
                        client.search(q).await
                    }
                }
                ListPageKind::MyThreads => client.my_threads(page.max(1)).await,
                ListPageKind::MyReplies => client.my_replies(page.max(1)).await,
                ListPageKind::Favorites => client.favorites(page.max(1)).await,
            };
            let _ = tx.send(WorkerResponse::SimpleListLoaded {
                kind,
                request_id,
                result,
            });
        }
        WorkerRequest::LoadPmThread { uid, request_id } => {
            let result = client.pm_thread(&uid).await;
            let _ = tx.send(WorkerResponse::PmThreadLoaded {
                uid,
                request_id,
                result,
            });
        }
        WorkerRequest::LoadBlacklist { request_id } => {
            let result = client.blacklist().await;
            let _ = tx.send(WorkerResponse::BlacklistLoaded { request_id, result });
        }
        WorkerRequest::PreparePost { action, request_id } => {
            let result = client.prepare_post(action.clone()).await;
            let _ = tx.send(WorkerResponse::PrePostReady {
                action,
                request_id,
                result,
            });
        }
        _ => {}
    }
}

async fn handle_session_write<C: ForumClient + ?Sized>(
    client: &C,
    req: WorkerRequest,
    tx: &mpsc::UnboundedSender<WorkerResponse>,
) {
    match req {
        WorkerRequest::CheckSession { auth_op_id } => {
            let result = client.session_status().await;
            let info = result.unwrap_or(SessionInfo {
                logged_in: false,
                username: None,
                uid: None,
            });
            let _ = tx.send(WorkerResponse::Session { auth_op_id, info });
        }
        WorkerRequest::AutoLogin { creds, auth_op_id } => {
            let credentials = Credentials {
                username: creds.username.clone(),
                password: creds.password_md5,
                security_question: Some(creds.security_question),
                security_answer: Some(creds.security_answer),
            };
            let result = client.login(credentials).await;
            let _ = tx.send(WorkerResponse::LoginResult {
                auth_op_id,
                manual: false,
                result,
                username: creds.username,
                password_plain: None,
            });
        }
        WorkerRequest::ManualLogin {
            username,
            password,
            security_index,
            security_answer,
            auth_op_id,
        } => {
            let password_plain = password.clone();
            let qid = SECURITY_QUESTIONS[security_index].0;
            let credentials = Credentials {
                username: username.clone(),
                password,
                security_question: Some(qid.to_string()),
                security_answer: Some(security_answer),
            };
            let result = client.login(credentials).await;
            let _ = tx.send(WorkerResponse::LoginResult {
                auth_op_id,
                manual: true,
                result,
                username,
                password_plain: Some(password_plain),
            });
        }
        WorkerRequest::SubmitPost {
            action,
            content,
            subject,
            delete,
        } => {
            // Never auto-retry: server may have already accepted the write.
            let result = client
                .post(action.clone(), &content, subject.as_deref(), delete)
                .await;
            let _ = tx.send(WorkerResponse::PostSubmitted {
                action,
                delete,
                result,
            });
        }
        WorkerRequest::SendPm { uid, content } => {
            let result = client.send_pm(&uid, &content).await;
            let _ = tx.send(WorkerResponse::PmSent { uid, result });
        }
        WorkerRequest::PmDelete { uid } => {
            let result = client.pm_delete(&uid).await;
            let _ = tx.send(WorkerResponse::PmDeleted { uid, result });
        }
        WorkerRequest::UploadComposerImage { action, path } => {
            let result = async {
                let bytes = std::fs::read(&path).map_err(|e| {
                    hiptty_core::AdapterError::InvalidInput(format!("cannot read {}: {e}", path))
                })?;
                let filename = std::path::Path::new(&path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("image.jpg");
                client.upload_image(action, &bytes, filename).await
            }
            .await;
            let _ = tx.send(WorkerResponse::ComposerImageUploaded { path, result });
        }
        WorkerRequest::Logout { auth_op_id } => {
            let result = client.logout().await;
            let _ = tx.send(WorkerResponse::LoggedOut { auth_op_id, result });
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use hiptty_core::{
        AdapterError, AdapterResult, Credentials, PostAction, PostResult, PrePostInfo, SearchQuery,
        SessionInfo, SimpleList, ThreadDetail, ThreadList, UserInfo,
    };
    use std::path::Path;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Mutex;
    use tokio::sync::Notify;

    /// Blocks selected methods until `release` is notified; records call order.
    struct FakeClient {
        block_forum: Arc<Notify>,
        release_forum: Arc<Notify>,
        block_unread: Arc<Notify>,
        release_unread: Arc<Notify>,
        block_write: Arc<Notify>,
        release_write: Arc<Notify>,
        block_login: Arc<Notify>,
        release_login: Arc<Notify>,
        block_logout: Arc<Notify>,
        release_logout: Arc<Notify>,
        forum_starts: Arc<Mutex<Vec<(u32, u64)>>>,
        write_log: Arc<Mutex<Vec<&'static str>>>,
        write_active: Arc<AtomicU32>,
        write_max_active: Arc<AtomicU32>,
        login_starts: Arc<AtomicU32>,
        login_should_fail: Arc<std::sync::atomic::AtomicBool>,
    }

    impl FakeClient {
        fn new() -> Self {
            Self {
                block_forum: Arc::new(Notify::new()),
                release_forum: Arc::new(Notify::new()),
                block_unread: Arc::new(Notify::new()),
                release_unread: Arc::new(Notify::new()),
                block_write: Arc::new(Notify::new()),
                release_write: Arc::new(Notify::new()),
                block_login: Arc::new(Notify::new()),
                release_login: Arc::new(Notify::new()),
                block_logout: Arc::new(Notify::new()),
                release_logout: Arc::new(Notify::new()),
                forum_starts: Arc::new(Mutex::new(Vec::new())),
                write_log: Arc::new(Mutex::new(Vec::new())),
                write_active: Arc::new(AtomicU32::new(0)),
                write_max_active: Arc::new(AtomicU32::new(0)),
                login_starts: Arc::new(AtomicU32::new(0)),
                login_should_fail: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            }
        }
    }

    #[async_trait]
    impl ForumClient for FakeClient {
        async fn login(&self, _credentials: Credentials) -> AdapterResult<SessionInfo> {
            self.login_starts.fetch_add(1, Ordering::SeqCst);
            self.block_login.notify_one();
            self.release_login.notified().await;
            self.write_log.lock().unwrap().push("login");
            if self.login_should_fail.load(Ordering::SeqCst) {
                return Err(AdapterError::AuthFailed("bad creds".into()));
            }
            Ok(SessionInfo {
                logged_in: true,
                username: Some("u".into()),
                uid: Some("1".into()),
            })
        }
        async fn logout(&self) -> AdapterResult<()> {
            self.block_logout.notify_one();
            self.release_logout.notified().await;
            self.write_log.lock().unwrap().push("logout");
            Ok(())
        }
        async fn session_status(&self) -> AdapterResult<SessionInfo> {
            Ok(SessionInfo {
                logged_in: false,
                username: None,
                uid: None,
            })
        }
        async fn forum_threads(&self, fid: u32, _page: u32) -> AdapterResult<ThreadList> {
            self.forum_starts.lock().unwrap().push((fid, 0));
            self.block_forum.notify_one();
            self.release_forum.notified().await;
            Ok(ThreadList {
                threads: Vec::new(),
                page: 1,
                max_page: 1,
                uid_hint: Some(fid.to_string()),
            })
        }
        async fn thread_detail(&self, _tid: &str, _page: u32) -> AdapterResult<ThreadDetail> {
            Err(AdapterError::NotImplemented("thread_detail".into()))
        }
        async fn thread_at_post(&self, _tid: &str, _pid: &str) -> AdapterResult<ThreadDetail> {
            Err(AdapterError::NotImplemented("thread_at_post".into()))
        }
        async fn thread_last_page(&self, _tid: &str) -> AdapterResult<ThreadDetail> {
            Err(AdapterError::NotImplemented("thread_last_page".into()))
        }
        async fn search(&self, _query: SearchQuery) -> AdapterResult<SimpleList> {
            Err(AdapterError::NotImplemented("search".into()))
        }
        async fn my_threads(&self, _page: u32) -> AdapterResult<SimpleList> {
            Err(AdapterError::NotImplemented("my_threads".into()))
        }
        async fn my_replies(&self, _page: u32) -> AdapterResult<SimpleList> {
            Err(AdapterError::NotImplemented("my_replies".into()))
        }
        async fn favorites(&self, _page: u32) -> AdapterResult<SimpleList> {
            Err(AdapterError::NotImplemented("favorites".into()))
        }
        async fn attention(&self, _page: u32) -> AdapterResult<SimpleList> {
            Err(AdapterError::NotImplemented("attention".into()))
        }
        async fn pm_list(&self) -> AdapterResult<SimpleList> {
            Err(AdapterError::NotImplemented("pm_list".into()))
        }
        async fn pm_new_list(&self) -> AdapterResult<SimpleList> {
            Err(AdapterError::NotImplemented("pm_new_list".into()))
        }
        async fn pm_thread(&self, _uid: &str) -> AdapterResult<SimpleList> {
            Err(AdapterError::NotImplemented("pm_thread".into()))
        }
        async fn notifications(&self) -> AdapterResult<SimpleList> {
            self.block_unread.notify_one();
            self.release_unread.notified().await;
            Ok(SimpleList {
                page: 1,
                max_page: 1,
                search_id: None,
                items: Vec::new(),
            })
        }
        async fn user_info(&self, _uid: &str) -> AdapterResult<UserInfo> {
            Err(AdapterError::NotImplemented("user_info".into()))
        }
        async fn blacklist(&self) -> AdapterResult<Vec<String>> {
            Ok(Vec::new())
        }
        async fn new_posts(
            &self,
            _search_id: Option<&str>,
            _page: u32,
        ) -> AdapterResult<SimpleList> {
            Err(AdapterError::NotImplemented("new_posts".into()))
        }
        async fn check_new_pm(&self) -> AdapterResult<bool> {
            self.block_unread.notify_one();
            self.release_unread.notified().await;
            Ok(false)
        }
        async fn prepare_post(&self, _action: PostAction) -> AdapterResult<PrePostInfo> {
            Err(AdapterError::NotImplemented("prepare_post".into()))
        }
        async fn post(
            &self,
            _action: PostAction,
            _content: &str,
            _subject: Option<&str>,
            _delete: bool,
        ) -> AdapterResult<PostResult> {
            let n = self.write_active.fetch_add(1, Ordering::SeqCst) + 1;
            self.write_max_active.fetch_max(n, Ordering::SeqCst);
            self.write_log.lock().unwrap().push("post");
            self.block_write.notify_one();
            self.release_write.notified().await;
            self.write_active.fetch_sub(1, Ordering::SeqCst);
            Ok(PostResult {
                success: true,
                message: "ok".into(),
                tid: None,
                floor: None,
                detail: None,
            })
        }
        async fn send_pm(&self, _uid: &str, _content: &str) -> AdapterResult<()> {
            let n = self.write_active.fetch_add(1, Ordering::SeqCst) + 1;
            self.write_max_active.fetch_max(n, Ordering::SeqCst);
            self.write_log.lock().unwrap().push("send_pm");
            self.block_write.notify_one();
            self.release_write.notified().await;
            self.write_active.fetch_sub(1, Ordering::SeqCst);
            Ok(())
        }
        async fn pm_delete(&self, _uid: &str) -> AdapterResult<()> {
            self.write_log.lock().unwrap().push("pm_delete");
            Ok(())
        }
        async fn favorite_add(&self, _tid: &str) -> AdapterResult<()> {
            Ok(())
        }
        async fn favorite_remove(&self, _tid: &str) -> AdapterResult<()> {
            Ok(())
        }
        async fn blacklist_add(&self, _username: &str) -> AdapterResult<()> {
            Ok(())
        }
        async fn blacklist_remove(&self, _username: &str) -> AdapterResult<()> {
            Ok(())
        }
        async fn upload_image(
            &self,
            _action: PostAction,
            _data: &[u8],
            _filename: &str,
        ) -> AdapterResult<String> {
            Err(AdapterError::NotImplemented("upload_image".into()))
        }
        async fn dump_fixture(
            &self,
            _url: &str,
            _output: Option<&Path>,
        ) -> AdapterResult<hiptty_adapter::FixtureDump> {
            Err(AdapterError::NotImplemented("dump_fixture".into()))
        }
        async fn fetch_url(&self, _url: &str) -> AdapterResult<Vec<u8>> {
            Ok(Vec::new())
        }
    }

    #[tokio::test]
    async fn feed_latest_wins_starts_immediately() {
        let fake = FakeClient::new();
        let block = Arc::clone(&fake.block_forum);
        let release = Arc::clone(&fake.release_forum);
        let starts = Arc::clone(&fake.forum_starts);

        let (req_tx, req_rx) = mpsc::unbounded_channel();
        let (resp_tx, mut resp_rx) = mpsc::unbounded_channel();
        spawn_worker(fake, req_rx, resp_tx);

        req_tx
            .send(WorkerRequest::LoadThreads {
                fid: 1,
                page: 1,
                request_id: 1,
            })
            .unwrap();
        // Wait until A has entered forum_threads.
        tokio::time::timeout(Duration::from_secs(2), block.notified())
            .await
            .expect("A should start");

        req_tx
            .send(WorkerRequest::LoadThreads {
                fid: 2,
                page: 1,
                request_id: 2,
            })
            .unwrap();
        req_tx
            .send(WorkerRequest::LoadThreads {
                fid: 3,
                page: 1,
                request_id: 3,
            })
            .unwrap();

        // C should start without waiting for A/B timeouts — only need C's start signal.
        // A was aborted; release any waiter that survived.
        release.notify_waiters();
        tokio::time::timeout(Duration::from_millis(500), async {
            loop {
                let n = starts.lock().unwrap().len();
                if n >= 2 {
                    // At least A and a replacement (B or C) started.
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("replacement feed should start promptly");

        // Drain: release and collect last response for fid 3.
        release.notify_waiters();
        let mut saw_c = false;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while tokio::time::Instant::now() < deadline {
            release.notify_waiters();
            match tokio::time::timeout(Duration::from_millis(50), resp_rx.recv()).await {
                Ok(Some(WorkerResponse::ThreadsLoaded {
                    fid,
                    request_id,
                    result: Ok(_),
                    ..
                })) => {
                    if fid == 3 && request_id == 3 {
                        saw_c = true;
                        break;
                    }
                }
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => {}
            }
        }
        assert!(saw_c, "latest feed C should complete and respond");
    }

    #[tokio::test]
    async fn unread_does_not_block_feed() {
        let fake = FakeClient::new();
        let unread_block = Arc::clone(&fake.block_unread);
        let forum_block = Arc::clone(&fake.block_forum);
        let forum_release = Arc::clone(&fake.release_forum);
        let unread_release = Arc::clone(&fake.release_unread);

        let (req_tx, req_rx) = mpsc::unbounded_channel();
        let (resp_tx, _resp_rx) = mpsc::unbounded_channel();
        spawn_worker(fake, req_rx, resp_tx);

        req_tx
            .send(WorkerRequest::CheckUnread { session_epoch: 1 })
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), unread_block.notified())
            .await
            .expect("unread should start");

        req_tx
            .send(WorkerRequest::LoadThreads {
                fid: 9,
                page: 1,
                request_id: 1,
            })
            .unwrap();
        tokio::time::timeout(Duration::from_millis(500), forum_block.notified())
            .await
            .expect("feed should start while unread blocked");

        forum_release.notify_waiters();
        unread_release.notify_waiters();
    }

    #[tokio::test]
    async fn unread_does_not_block_write() {
        let fake = FakeClient::new();
        let unread_block = Arc::clone(&fake.block_unread);
        let write_block = Arc::clone(&fake.block_write);
        let write_release = Arc::clone(&fake.release_write);
        let unread_release = Arc::clone(&fake.release_unread);

        let (req_tx, req_rx) = mpsc::unbounded_channel();
        let (resp_tx, _resp_rx) = mpsc::unbounded_channel();
        spawn_worker(fake, req_rx, resp_tx);

        req_tx
            .send(WorkerRequest::CheckUnread { session_epoch: 1 })
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), unread_block.notified())
            .await
            .expect("unread starts");

        req_tx
            .send(WorkerRequest::SendPm {
                uid: "1".into(),
                content: "hi".into(),
            })
            .unwrap();
        tokio::time::timeout(Duration::from_millis(500), write_block.notified())
            .await
            .expect("write should start while unread blocked");

        write_release.notify_waiters();
        unread_release.notify_waiters();
    }

    #[tokio::test]
    async fn writes_are_serial() {
        let fake = FakeClient::new();
        let write_block = Arc::clone(&fake.block_write);
        let write_release = Arc::clone(&fake.release_write);
        let max_active = Arc::clone(&fake.write_max_active);
        let log = Arc::clone(&fake.write_log);

        let (req_tx, req_rx) = mpsc::unbounded_channel();
        let (resp_tx, mut resp_rx) = mpsc::unbounded_channel();
        spawn_worker(fake, req_rx, resp_tx);

        for _ in 0..3 {
            req_tx
                .send(WorkerRequest::SendPm {
                    uid: "1".into(),
                    content: "x".into(),
                })
                .unwrap();
        }

        for _ in 0..3 {
            tokio::time::timeout(Duration::from_secs(2), write_block.notified())
                .await
                .expect("write step");
            write_release.notify_one();
            let _ = tokio::time::timeout(Duration::from_secs(1), resp_rx.recv()).await;
        }

        assert_eq!(max_active.load(Ordering::SeqCst), 1);
        assert_eq!(log.lock().unwrap().as_slice(), &["send_pm"; 3]);
    }

    use std::time::Duration;

    #[tokio::test]
    async fn check_unread_new_epoch_aborts_old() {
        let fake = FakeClient::new();
        let unread_block = Arc::clone(&fake.block_unread);
        let unread_release = Arc::clone(&fake.release_unread);

        let (req_tx, req_rx) = mpsc::unbounded_channel();
        let (resp_tx, mut resp_rx) = mpsc::unbounded_channel();
        spawn_worker(fake, req_rx, resp_tx);

        req_tx
            .send(WorkerRequest::CheckUnread { session_epoch: 1 })
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), unread_block.notified())
            .await
            .expect("epoch 1 unread starts");

        // Epoch 2 must replace, not drop.
        req_tx
            .send(WorkerRequest::CheckUnread { session_epoch: 2 })
            .unwrap();
        tokio::time::timeout(Duration::from_millis(500), unread_block.notified())
            .await
            .expect("epoch 2 unread must start after aborting epoch 1");

        unread_release.notify_waiters();
        let mut saw_epoch2 = false;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while tokio::time::Instant::now() < deadline {
            unread_release.notify_waiters();
            if let Ok(Some(WorkerResponse::UnreadChecked { session_epoch, .. })) =
                tokio::time::timeout(Duration::from_millis(50), resp_rx.recv()).await
            {
                if session_epoch == 2 {
                    saw_epoch2 = true;
                    break;
                }
            }
        }
        assert!(saw_epoch2, "epoch 2 unread response must arrive");
    }

    #[tokio::test]
    async fn load_threads_held_and_discarded_after_autologin_success() {
        let fake = FakeClient::new();
        let login_block = Arc::clone(&fake.block_login);
        let login_release = Arc::clone(&fake.release_login);
        let forum_starts = Arc::clone(&fake.forum_starts);

        let (req_tx, req_rx) = mpsc::unbounded_channel();
        let (resp_tx, mut resp_rx) = mpsc::unbounded_channel();
        spawn_worker(fake, req_rx, resp_tx);

        req_tx
            .send(WorkerRequest::AutoLogin {
                creds: StoredCreds {
                    username: "u".into(),
                    password_md5: "x".into(),
                    security_question: "0".into(),
                    security_answer: String::new(),
                },
                auth_op_id: 1,
            })
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), login_block.notified())
            .await
            .expect("login starts");

        req_tx
            .send(WorkerRequest::LoadThreads {
                fid: 7,
                page: 1,
                request_id: 42,
            })
            .unwrap();
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert!(
            forum_starts.lock().unwrap().is_empty(),
            "LoadThreads must not start during AutoLogin barrier"
        );

        login_release.notify_waiters();
        // Success still discards deferred reads — UI issues a fresh Feed after LoginResult.
        let deadline = tokio::time::Instant::now() + Duration::from_millis(300);
        while tokio::time::Instant::now() < deadline {
            tokio::task::yield_now().await;
        }
        assert!(
            forum_starts.lock().unwrap().is_empty(),
            "deferred LoadThreads must be discarded after auth barrier"
        );
        // Login result still arrives.
        let mut saw_login = false;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while tokio::time::Instant::now() < deadline {
            if let Ok(Some(WorkerResponse::LoginResult { result: Ok(_), .. })) =
                tokio::time::timeout(Duration::from_millis(50), resp_rx.recv()).await
            {
                saw_login = true;
                break;
            }
        }
        assert!(saw_login);
    }

    #[tokio::test]
    async fn deferred_reads_discarded_after_autologin_failure() {
        let fake = FakeClient::new();
        fake.login_should_fail.store(true, Ordering::SeqCst);
        let login_block = Arc::clone(&fake.block_login);
        let login_release = Arc::clone(&fake.release_login);
        let forum_starts = Arc::clone(&fake.forum_starts);

        let (req_tx, req_rx) = mpsc::unbounded_channel();
        let (resp_tx, mut resp_rx) = mpsc::unbounded_channel();
        spawn_worker(fake, req_rx, resp_tx);

        req_tx
            .send(WorkerRequest::AutoLogin {
                creds: StoredCreds {
                    username: "u".into(),
                    password_md5: "x".into(),
                    security_question: "0".into(),
                    security_answer: String::new(),
                },
                auth_op_id: 1,
            })
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), login_block.notified())
            .await
            .expect("login starts");

        req_tx
            .send(WorkerRequest::LoadThreads {
                fid: 9,
                page: 1,
                request_id: 1,
            })
            .unwrap();
        login_release.notify_waiters();

        let deadline = tokio::time::Instant::now() + Duration::from_millis(300);
        while tokio::time::Instant::now() < deadline {
            tokio::task::yield_now().await;
        }
        assert!(
            forum_starts.lock().unwrap().is_empty(),
            "failed login must not resume deferred LoadThreads"
        );

        let mut saw_fail = false;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while tokio::time::Instant::now() < deadline {
            if let Ok(Some(WorkerResponse::LoginResult { result: Err(_), .. })) =
                tokio::time::timeout(Duration::from_millis(50), resp_rx.recv()).await
            {
                saw_fail = true;
                break;
            }
        }
        assert!(saw_fail);
    }

    #[tokio::test]
    async fn deferred_reads_discarded_after_logout() {
        let fake = FakeClient::new();
        let logout_block = Arc::clone(&fake.block_logout);
        let logout_release = Arc::clone(&fake.release_logout);
        let forum_starts = Arc::clone(&fake.forum_starts);

        let (req_tx, req_rx) = mpsc::unbounded_channel();
        let (resp_tx, mut resp_rx) = mpsc::unbounded_channel();
        spawn_worker(fake, req_rx, resp_tx);

        req_tx
            .send(WorkerRequest::Logout { auth_op_id: 1 })
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), logout_block.notified())
            .await
            .expect("logout starts");

        req_tx
            .send(WorkerRequest::LoadThreads {
                fid: 3,
                page: 1,
                request_id: 1,
            })
            .unwrap();
        logout_release.notify_waiters();

        let deadline = tokio::time::Instant::now() + Duration::from_millis(300);
        while tokio::time::Instant::now() < deadline {
            tokio::task::yield_now().await;
        }
        assert!(
            forum_starts.lock().unwrap().is_empty(),
            "logout must not resume deferred LoadThreads"
        );

        let mut saw_logout = false;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while tokio::time::Instant::now() < deadline {
            if let Ok(Some(WorkerResponse::LoggedOut { .. })) =
                tokio::time::timeout(Duration::from_millis(50), resp_rx.recv()).await
            {
                saw_logout = true;
                break;
            }
        }
        assert!(saw_logout);
    }

    #[tokio::test]
    async fn autologin_aborts_blocking_feed_first() {
        let fake = FakeClient::new();
        let forum_block = Arc::clone(&fake.block_forum);
        let login_block = Arc::clone(&fake.block_login);
        let forum_release = Arc::clone(&fake.release_forum);
        let login_release = Arc::clone(&fake.release_login);
        let login_starts = Arc::clone(&fake.login_starts);

        let (req_tx, req_rx) = mpsc::unbounded_channel();
        let (resp_tx, mut resp_rx) = mpsc::unbounded_channel();
        spawn_worker(fake, req_rx, resp_tx);

        req_tx
            .send(WorkerRequest::LoadThreads {
                fid: 1,
                page: 1,
                request_id: 1,
            })
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), forum_block.notified())
            .await
            .expect("feed blocks");

        req_tx
            .send(WorkerRequest::AutoLogin {
                creds: StoredCreds {
                    username: "u".into(),
                    password_md5: "x".into(),
                    security_question: "0".into(),
                    security_answer: String::new(),
                },
                auth_op_id: 1,
            })
            .unwrap();

        // Login must start promptly even while feed was blocked — barrier aborts the read.
        tokio::time::timeout(Duration::from_millis(500), login_block.notified())
            .await
            .expect("AutoLogin must start after aborting reads");
        assert_eq!(login_starts.load(Ordering::SeqCst), 1);

        login_release.notify_waiters();
        forum_release.notify_waiters();

        let mut saw_login = false;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while tokio::time::Instant::now() < deadline {
            if let Ok(Some(WorkerResponse::LoginResult { result: Ok(_), .. })) =
                tokio::time::timeout(Duration::from_millis(50), resp_rx.recv()).await
            {
                saw_login = true;
                break;
            }
        }
        assert!(saw_login);
    }
}
