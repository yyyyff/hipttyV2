use hiptty_adapter::ForumClient;
use hiptty_core::{
    AdapterResult, Credentials, SessionInfo, ThreadDetail, ThreadList, SECURITY_QUESTIONS,
};
use hiptty_image::ImageKind;
use tokio::sync::mpsc;

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
            }
        }
    });
}
