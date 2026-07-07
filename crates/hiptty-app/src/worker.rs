use hiptty_adapter::ForumClient;
use hiptty_core::SearchQuery;
use hiptty_core::{
    AdapterResult, Credentials, PostAction, PostResult, PrePostInfo, SessionInfo, ThreadDetail,
    ThreadList, SECURITY_QUESTIONS,
};
use hiptty_image::ImageKind;
use tokio::sync::mpsc;

use crate::list_page::ListPageKind;

#[derive(Debug)]
pub enum WorkerRequest {
    CheckSession,
    AutoLogin(StoredCreds),
    ManualLogin {
        username: String,
        password: String,
        security_index: usize,
        security_answer: String,
    },
    LoadThreads {
        fid: u32,
        page: u32,
    },
    LoadThreadDetail {
        tid: String,
        page: u32,
    },
    FetchImage {
        url: String,
        kind: ImageKind,
    },
    PreparePost {
        action: PostAction,
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
    },
    LoadPmThread {
        uid: String,
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
    },
    CheckUnread,
    LoadBlacklist,
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
    Session(SessionInfo),
    LoginResult {
        manual: bool,
        result: AdapterResult<SessionInfo>,
        username: String,
        password_plain: Option<String>,
    },
    ThreadsLoaded {
        fid: u32,
        page: u32,
        result: AdapterResult<ThreadList>,
    },
    ThreadDetailLoaded {
        tid: String,
        page: u32,
        result: AdapterResult<ThreadDetail>,
    },
    ImageFetched {
        url: String,
        kind: ImageKind,
        result: AdapterResult<Vec<u8>>,
    },
    PrePostReady {
        action: PostAction,
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
        result: AdapterResult<hiptty_core::SimpleList>,
    },
    PmThreadLoaded {
        uid: String,
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
        result: AdapterResult<ThreadDetail>,
    },
    UnreadChecked {
        has_pm: bool,
        has_notifications: bool,
    },
    BlacklistLoaded {
        result: AdapterResult<Vec<String>>,
    },
}

pub fn spawn_worker<C: ForumClient + 'static>(
    client: C,
    mut rx: mpsc::UnboundedReceiver<WorkerRequest>,
    tx: mpsc::UnboundedSender<WorkerResponse>,
) {
    tokio::spawn(async move {
        while let Some(req) = rx.recv().await {
            match req {
                WorkerRequest::CheckSession => {
                    let result = client.session_status().await;
                    let info = result.unwrap_or(SessionInfo {
                        logged_in: false,
                        username: None,
                        uid: None,
                    });
                    let _ = tx.send(WorkerResponse::Session(info));
                }
                WorkerRequest::AutoLogin(creds) => {
                    let credentials = Credentials {
                        username: creds.username.clone(),
                        password: creds.password_md5,
                        security_question: Some(creds.security_question),
                        security_answer: Some(creds.security_answer),
                    };
                    let result = client.login(credentials).await;
                    let _ = tx.send(WorkerResponse::LoginResult {
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
                        manual: true,
                        result,
                        username,
                        password_plain: Some(password_plain),
                    });
                }
                WorkerRequest::LoadThreads { fid, page } => {
                    let result = client.forum_threads(fid, page).await;
                    let _ = tx.send(WorkerResponse::ThreadsLoaded { fid, page, result });
                }
                WorkerRequest::LoadThreadDetail { tid, page } => {
                    let result = client.thread_detail(&tid, page).await;
                    let _ = tx.send(WorkerResponse::ThreadDetailLoaded { tid, page, result });
                }
                WorkerRequest::FetchImage { url, kind } => {
                    let result = client.fetch_url(&url).await;
                    let _ = tx.send(WorkerResponse::ImageFetched { url, kind, result });
                }
                WorkerRequest::PreparePost { action } => {
                    let result = client.prepare_post(action.clone()).await;
                    let _ = tx.send(WorkerResponse::PrePostReady { action, result });
                }
                WorkerRequest::SubmitPost {
                    action,
                    content,
                    subject,
                    delete,
                } => {
                    let result = client
                        .post(action.clone(), &content, subject.as_deref(), delete)
                        .await;
                    let _ = tx.send(WorkerResponse::PostSubmitted {
                        action,
                        delete,
                        result,
                    });
                }
                WorkerRequest::LoadSimpleList {
                    kind,
                    page,
                    fid,
                    query,
                    search_id,
                } => {
                    let result = async {
                        match kind {
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
                        }
                    }
                    .await;
                    let _ = tx.send(WorkerResponse::SimpleListLoaded { kind, result });
                }
                WorkerRequest::LoadPmThread { uid } => {
                    let result = client.pm_thread(&uid).await;
                    let _ = tx.send(WorkerResponse::PmThreadLoaded { uid, result });
                }
                WorkerRequest::SendPm { uid, content } => {
                    let result = client.send_pm(&uid, &content).await;
                    let _ = tx.send(WorkerResponse::PmSent { uid, result });
                }
                WorkerRequest::PmDelete { uid } => {
                    let result = client.pm_delete(&uid).await;
                    let _ = tx.send(WorkerResponse::PmDeleted { uid, result });
                }
                WorkerRequest::LoadThreadAtPost { tid, pid } => {
                    let result = client.thread_at_post(&tid, &pid).await;
                    let _ = tx.send(WorkerResponse::ThreadAtPostLoaded { tid, result });
                }
                WorkerRequest::CheckUnread => {
                    let has_pm = client.check_new_pm().await.unwrap_or(false);
                    let has_notifications = client
                        .notifications()
                        .await
                        .map(|list| list.items.iter().any(|i| i.is_new))
                        .unwrap_or(false);
                    let _ = tx.send(WorkerResponse::UnreadChecked {
                        has_pm,
                        has_notifications,
                    });
                }
                WorkerRequest::LoadBlacklist => {
                    let result = client.blacklist().await;
                    let _ = tx.send(WorkerResponse::BlacklistLoaded { result });
                }
                WorkerRequest::UploadComposerImage { action, path } => {
                    let result = async {
                        let bytes = std::fs::read(&path).map_err(|e| {
                            hiptty_core::AdapterError::InvalidInput(format!(
                                "cannot read {}: {e}",
                                path
                            ))
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
            }
        }
    });
}
