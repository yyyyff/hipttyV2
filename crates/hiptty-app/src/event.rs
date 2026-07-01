use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use hiptty_core::{AdapterError, ErrorCode};
use hiptty_widgets::LoginField;
use tokio::sync::mpsc;

use crate::app::{App, FeedState, Overlay, Page};
use crate::worker::{StoredCreds, WorkerRequest, WorkerResponse};

pub fn handle_key(app: &mut App, key: KeyEvent, worker_tx: &mpsc::UnboundedSender<WorkerRequest>) {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return;
    }

    match app.overlay {
        Overlay::ForumPicker => handle_forum_picker_key(app, key, worker_tx),
        Overlay::None => match app.page {
            Page::Login => handle_login_key(app, key, worker_tx),
            Page::ThreadFeed => handle_feed_key(app, key, worker_tx),
        },
    }
}

fn handle_forum_picker_key(
    app: &mut App,
    key: KeyEvent,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
) {
    let entries = app.forum_picker_fids();
    match key.code {
        KeyCode::Esc => app.overlay = Overlay::None,
        KeyCode::Char('j') | KeyCode::Down => {
            if app.forum_picker_selected + 1 < entries.len() {
                app.forum_picker_selected += 1;
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.forum_picker_selected = app.forum_picker_selected.saturating_sub(1);
        }
        KeyCode::Enter => {
            if let Some(&fid) = entries.get(app.forum_picker_selected) {
                app.feed = FeedState::new(fid);
                app.overlay = Overlay::None;
                request_threads(app, worker_tx, 1);
            }
        }
        _ => {}
    }
}

fn handle_login_key(
    app: &mut App,
    key: KeyEvent,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
) {
    match key.code {
        KeyCode::Esc => app.quit = true,
        KeyCode::Tab | KeyCode::BackTab => cycle_login_focus(app, key.code == KeyCode::BackTab),
        KeyCode::Char('h') | KeyCode::Left if app.login.focused == LoginField::SecurityQuestion => {
            app.login.security_index = app.login.security_index.saturating_sub(1);
        }
        KeyCode::Char('l') | KeyCode::Right
            if app.login.focused == LoginField::SecurityQuestion =>
        {
            if app.login.security_index + 1 < hiptty_core::SECURITY_QUESTIONS.len() {
                app.login.security_index += 1;
            }
        }
        KeyCode::Enter => {
            if app.login.focused == LoginField::Submit {
                submit_login(app, worker_tx);
            } else {
                cycle_login_focus(app, false);
            }
        }
        KeyCode::Backspace => backspace_login(app),
        KeyCode::Char(c) => append_login(app, c),
        _ => {}
    }
}

fn cycle_login_focus(app: &mut App, reverse: bool) {
    let fields = [
        LoginField::Username,
        LoginField::Password,
        LoginField::SecurityQuestion,
        LoginField::SecurityAnswer,
        LoginField::Submit,
    ];
    let pos = fields
        .iter()
        .position(|f| *f == app.login.focused)
        .unwrap_or(0);
    let next = if reverse {
        pos.checked_sub(1).unwrap_or(fields.len() - 1)
    } else {
        (pos + 1) % fields.len()
    };
    app.login.focused = fields[next];
}

fn append_login(app: &mut App, c: char) {
    match app.login.focused {
        LoginField::Username => app.login.username.push(c),
        LoginField::Password => app.login.password.push(c),
        LoginField::SecurityAnswer if app.login.security_index > 0 => {
            app.login.security_answer.push(c);
        }
        _ => {}
    }
}

fn backspace_login(app: &mut App) {
    match app.login.focused {
        LoginField::Username => {
            app.login.username.pop();
        }
        LoginField::Password => {
            app.login.password.pop();
        }
        LoginField::SecurityAnswer => {
            app.login.security_answer.pop();
        }
        _ => {}
    }
}

fn submit_login(app: &mut App, worker_tx: &mpsc::UnboundedSender<WorkerRequest>) {
    if app.login.loading {
        return;
    }
    if app.login.username.is_empty() || app.login.password.is_empty() {
        app.login.error = Some("请填写用户名和密码".into());
        return;
    }
    if app.login.security_index > 0 && app.login.security_answer.is_empty() {
        app.login.error = Some("请填写安全问题答案".into());
        return;
    }
    app.login.loading = true;
    app.login.error = None;
    let _ = worker_tx.send(WorkerRequest::ManualLogin {
        username: app.login.username.clone(),
        password: app.login.password.clone(),
        security_index: app.login.security_index,
        security_answer: app.login.security_answer.clone(),
    });
}

fn handle_feed_key(app: &mut App, key: KeyEvent, worker_tx: &mpsc::UnboundedSender<WorkerRequest>) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('b') => app.quit = true,
        KeyCode::Char('j') | KeyCode::Down => {
            if app.feed.selected + 1 < app.feed.threads.len() {
                app.feed.selected += 1;
                maybe_load_more(app, worker_tx);
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.feed.selected = app.feed.selected.saturating_sub(1);
        }
        KeyCode::Char('f') => {
            app.overlay = Overlay::ForumPicker;
            let entries = app.forum_picker_fids();
            app.forum_picker_selected = entries
                .iter()
                .position(|&fid| fid == app.feed.fid)
                .unwrap_or(0);
        }
        KeyCode::Char('g') => {
            app.feed.selected = 0;
            app.feed.scroll = 0;
        }
        KeyCode::Char('G') if !app.feed.threads.is_empty() => {
            app.feed.selected = app.feed.threads.len() - 1;
        }
        _ => {}
    }
}

fn maybe_load_more(app: &mut App, worker_tx: &mpsc::UnboundedSender<WorkerRequest>) {
    if app.feed.loading {
        return;
    }
    let remaining = app.feed.threads.len().saturating_sub(app.feed.selected);
    if remaining <= 5 && app.feed.page < app.feed.max_page {
        request_threads(app, worker_tx, app.feed.page + 1);
    }
}

fn request_threads(app: &mut App, worker_tx: &mpsc::UnboundedSender<WorkerRequest>, page: u32) {
    if page == 1 {
        app.feed.threads.clear();
        app.feed.selected = 0;
        app.feed.scroll = 0;
    }
    app.feed.loading = true;
    let _ = worker_tx.send(WorkerRequest::LoadThreads {
        fid: app.feed.fid,
        page,
    });
}

pub fn handle_worker_response(
    app: &mut App,
    response: WorkerResponse,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
) {
    match response {
        WorkerResponse::Session(info) => {
            if !app.startup_done {
                app.startup_done = true;
                if info.logged_in {
                    app.session = info;
                    app.page = Page::ThreadFeed;
                    request_threads(app, worker_tx, 1);
                } else if let Some(creds) = crate::config::load_credentials(&app.credentials_path) {
                    app.prefill_login(&creds);
                    let _ = worker_tx.send(WorkerRequest::AutoLogin(StoredCreds {
                        username: creds.username,
                        password_md5: creds.password_md5,
                        security_question: creds.security_question,
                        security_answer: creds.security_answer,
                    }));
                } else {
                    app.page = Page::Login;
                }
            }
        }
        WorkerResponse::LoginResult {
            manual,
            result,
            username,
            password_plain,
        } => match result {
            Ok(info) => {
                app.session = info;
                if manual {
                    let plain = password_plain.unwrap_or_else(|| app.login.password.clone());
                    app.on_login_success(username, &plain);
                    request_threads(app, worker_tx, 1);
                } else {
                    app.session.logged_in = true;
                    app.session.username = Some(username);
                    app.page = Page::ThreadFeed;
                    request_threads(app, worker_tx, 1);
                }
            }
            Err(err) => {
                app.login.loading = false;
                if manual {
                    app.login.error = Some(error_message(&err));
                } else {
                    app.toast = Some(format!("自动登录失败: {}", error_message(&err)));
                    app.page = Page::Login;
                }
            }
        },
        WorkerResponse::ThreadsLoaded { fid, page, result } => {
            if app.feed.fid != fid {
                return;
            }
            app.feed.loading = false;
            match result {
                Ok(list) => {
                    if page == 1 {
                        app.feed.threads = list.threads;
                    } else {
                        app.feed.threads.extend(list.threads);
                    }
                    app.feed.page = list.page;
                    app.feed.max_page = list.max_page;
                    app.feed.error = None;
                }
                Err(err) => {
                    if is_auth_required(&err) {
                        try_auto_relogin(app, worker_tx);
                    } else {
                        app.feed.error = Some(error_message(&err));
                        app.toast = Some(error_message(&err));
                    }
                }
            }
        }
    }
}

fn try_auto_relogin(app: &mut App, worker_tx: &mpsc::UnboundedSender<WorkerRequest>) {
    if let Some(creds) = crate::config::load_credentials(&app.credentials_path) {
        app.session.logged_in = false;
        let _ = worker_tx.send(WorkerRequest::AutoLogin(StoredCreds {
            username: creds.username,
            password_md5: creds.password_md5,
            security_question: creds.security_question,
            security_answer: creds.security_answer,
        }));
    } else {
        app.page = Page::Login;
        app.toast = Some("登录已过期，请重新登录".into());
    }
}

fn is_auth_required(err: &AdapterError) -> bool {
    matches!(err.code(), ErrorCode::AuthRequired | ErrorCode::AuthFailed)
}

fn error_message(err: &AdapterError) -> String {
    err.to_string()
}

pub fn startup(_app: &mut App, worker_tx: &mpsc::UnboundedSender<WorkerRequest>) {
    let _ = worker_tx.send(WorkerRequest::CheckSession);
}
