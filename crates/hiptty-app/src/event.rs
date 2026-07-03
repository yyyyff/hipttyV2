use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui_textarea::{Input as TextareaInput, Key as TextareaKey};
use hiptty_core::{AdapterError, ErrorCode, PostAction};
use hiptty_widgets::{ComposerFocus, LoginField};
use tokio::sync::mpsc;

use hiptty_image::thread_avatar_job;

use crate::app::{App, DetailFetchMode, DetailState, FeedState, Overlay, Page};
use crate::composer::{
    delete_label, delete_post_action, edit_body, edit_header, edit_post_action, new_thread_action,
    quote_header, quote_post_action, reply_thread_action, ComposerKind, ComposerState,
    ConfirmDeleteState,
};
use crate::handlers::{
    handle_global_key, handle_list_page_key, handle_overlay_key, handle_pm_thread_key,
};
use crate::worker::{StoredCreds, WorkerRequest, WorkerResponse};

pub fn handle_key(app: &mut App, key: KeyEvent, worker_tx: &mpsc::UnboundedSender<WorkerRequest>) {
    if app.confirm_delete.is_some() {
        handle_confirm_delete_key(app, key, worker_tx);
        return;
    }
    if app.composer.is_some() {
        handle_composer_key(app, key, worker_tx);
        return;
    }
    if app.overlay != Overlay::None && app.overlay != Overlay::ForumPicker {
        handle_overlay_key(app, key, worker_tx);
        return;
    }
    if handle_global_key(app, key, worker_tx) {
        return;
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return;
    }

    match app.overlay {
        Overlay::ForumPicker => handle_forum_picker_key(app, key, worker_tx),
        Overlay::None => match app.page {
            Page::Startup => handle_startup_key(app, key),
            Page::Login => handle_login_key(app, key, worker_tx),
            Page::ThreadFeed => handle_feed_key(app, key, worker_tx),
            Page::ThreadDetail => handle_detail_key(app, key, worker_tx),
            Page::PmList | Page::Notifications => handle_list_page_key(app, key, worker_tx),
            Page::Search | Page::MyThreads | Page::MyReplies | Page::Favorites => {
                handle_list_page_key(app, key, worker_tx)
            }
            Page::PmThread => handle_pm_thread_key(app, key, worker_tx),
        },
        _ => {}
    }
}

fn handle_startup_key(app: &mut App, key: KeyEvent) {
    if matches!(key.code, KeyCode::Esc) {
        app.quit = true;
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
        KeyCode::Tab | KeyCode::Down => cycle_login_focus(app, false),
        KeyCode::BackTab | KeyCode::Up => cycle_login_focus(app, true),
        KeyCode::Char('h') | KeyCode::Left if app.login.focused == LoginField::SecurityQuestion => {
            app.login.security_index = app.login.security_index.saturating_sub(1);
            normalize_login_focus(app);
        }
        KeyCode::Char('l') | KeyCode::Right
            if app.login.focused == LoginField::SecurityQuestion =>
        {
            if app.login.security_index + 1 < hiptty_core::SECURITY_QUESTIONS.len() {
                app.login.security_index += 1;
            }
            normalize_login_focus(app);
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

fn login_focus_order(app: &App) -> Vec<LoginField> {
    let mut fields = vec![
        LoginField::Username,
        LoginField::Password,
        LoginField::SecurityQuestion,
    ];
    if app.login.security_index > 0 {
        fields.push(LoginField::SecurityAnswer);
    }
    fields.push(LoginField::Submit);
    fields
}

fn normalize_login_focus(app: &mut App) {
    if app.login.security_index == 0 && app.login.focused == LoginField::SecurityAnswer {
        app.login.focused = LoginField::SecurityQuestion;
    }
}

fn cycle_login_focus(app: &mut App, reverse: bool) {
    let fields = login_focus_order(app);
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
                app.sync_feed_scroll();
                maybe_load_more(app, worker_tx);
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if app.feed.selected > 0 {
                app.feed.selected -= 1;
                app.sync_feed_scroll();
            }
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
            app.sync_feed_scroll();
            maybe_load_more(app, worker_tx);
        }
        KeyCode::Char('r') => {
            if let Some(thread) = app.feed.threads.get(app.feed.selected).cloned() {
                open_feed_reply(app, &thread.tid);
            }
        }
        KeyCode::Char('n') => open_new_thread(app),
        KeyCode::Enter => {
            let selected = app.feed.selected;
            if let Some(thread) = app.feed.threads.get(selected).cloned() {
                open_thread_detail(app, &thread, worker_tx);
            }
        }
        _ => {}
    }
}

fn handle_detail_key(
    app: &mut App,
    key: KeyEvent,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('b') => {
            app.page = Page::ThreadFeed;
            app.detail.loading = false;
            app.detail.loading_more = false;
            app.detail.pending_fetch = None;
            app.detail.error = None;
        }
        KeyCode::Char('j') | KeyCode::Down => {
            if let Some(posts) = app.detail.detail.as_ref().map(|d| d.posts.as_slice()) {
                let palette = app.palette();
                let viewport = app.main_content_height();
                let (selected, scroll_top) = {
                    let images = app.images();
                    hiptty_widgets::detail_step_down(
                        app.detail.selected,
                        app.detail.scroll_top,
                        posts,
                        app.viewport_width,
                        viewport,
                        palette,
                        images,
                    )
                };
                app.detail.selected = selected;
                app.detail.scroll_top = hiptty_widgets::clamp_scroll_top(
                    scroll_top,
                    posts,
                    app.viewport_width,
                    viewport,
                    palette,
                    app.images(),
                );
                maybe_load_more_detail(app, worker_tx);
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if let Some(posts) = app.detail.detail.as_ref().map(|d| d.posts.as_slice()) {
                let palette = app.palette();
                let viewport = app.main_content_height();
                let (selected, scroll_top) = {
                    let images = app.images();
                    hiptty_widgets::detail_step_up(
                        app.detail.selected,
                        app.detail.scroll_top,
                        posts,
                        app.viewport_width,
                        viewport,
                        palette,
                        images,
                    )
                };
                app.detail.selected = selected;
                app.detail.scroll_top = hiptty_widgets::clamp_scroll_top(
                    scroll_top,
                    posts,
                    app.viewport_width,
                    viewport,
                    palette,
                    app.images(),
                );
            }
        }
        KeyCode::PageDown => {
            if let Some(posts) = app.detail.detail.as_ref().map(|d| d.posts.as_slice()) {
                let palette = app.palette();
                let viewport = app.main_content_height();
                app.detail.scroll_top = {
                    let images = app.images();
                    hiptty_widgets::page_scroll_top(
                        app.detail.scroll_top,
                        1,
                        posts,
                        app.viewport_width,
                        viewport,
                        palette,
                        images,
                    )
                };
                app.detail.selected = hiptty_widgets::first_visible_floor(
                    app.detail.scroll_top,
                    posts,
                    app.viewport_width,
                    palette,
                    app.images(),
                );
                maybe_load_more_detail(app, worker_tx);
            }
        }
        KeyCode::PageUp => {
            if let Some(posts) = app.detail.detail.as_ref().map(|d| d.posts.as_slice()) {
                let palette = app.palette();
                let viewport = app.main_content_height();
                app.detail.scroll_top = {
                    let images = app.images();
                    hiptty_widgets::page_scroll_top(
                        app.detail.scroll_top,
                        -1,
                        posts,
                        app.viewport_width,
                        viewport,
                        palette,
                        images,
                    )
                };
                app.detail.selected = hiptty_widgets::first_visible_floor(
                    app.detail.scroll_top,
                    posts,
                    app.viewport_width,
                    palette,
                    app.images(),
                );
            }
        }
        KeyCode::Char('g') => {
            request_thread_detail(app, worker_tx, 1, DetailFetchMode::Replace);
        }
        KeyCode::Char('G') => {
            if let Some(last) = app.detail.detail.as_ref().map(|d| d.last_page) {
                if last > 0 {
                    request_thread_detail(app, worker_tx, last, DetailFetchMode::Replace);
                }
            }
        }
        KeyCode::Char('r') => open_detail_reply(app),
        KeyCode::Char('q') => open_detail_quote(app, worker_tx),
        KeyCode::Char('e') => open_detail_edit(app, worker_tx),
        KeyCode::Char('d') => open_detail_delete_confirm(app),
        _ => {}
    }
}

fn open_thread_detail(
    app: &mut App,
    thread: &hiptty_core::ThreadSummary,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
) {
    app.detail = DetailState::from_summary(thread, app.feed.fid);
    app.page = Page::ThreadDetail;
    request_thread_detail(app, worker_tx, 1, DetailFetchMode::Replace);
}

pub fn request_thread_detail(
    app: &mut App,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
    page: u32,
    mode: DetailFetchMode,
) {
    if app.detail.tid.is_empty() {
        return;
    }
    if mode == DetailFetchMode::Replace {
        if page == 1 {
            app.detail.selected = 0;
            app.detail.scroll_top = 0;
        }
        app.detail.loading = true;
        app.detail.loading_more = false;
    } else {
        app.detail.loading_more = true;
    }
    app.detail.pending_fetch = Some(mode);
    app.detail.error = None;
    let _ = worker_tx.send(WorkerRequest::LoadThreadDetail {
        tid: app.detail.tid.clone(),
        page,
    });
}

fn maybe_load_more_detail(app: &mut App, worker_tx: &mpsc::UnboundedSender<WorkerRequest>) {
    if app.detail.loading || app.detail.loading_more {
        return;
    }
    let Some(detail) = app.detail.detail.as_ref() else {
        return;
    };
    if detail.page >= detail.last_page || detail.posts.is_empty() {
        return;
    }
    let viewport = app.main_content_height();
    let palette = app.palette();
    let images = app.images();
    let last_visible = hiptty_widgets::last_visible_floor(
        app.detail.scroll_top,
        &detail.posts,
        app.viewport_width,
        viewport,
        palette,
        images,
    );
    let remaining_floors = detail.posts.len().saturating_sub(last_visible + 1);
    if remaining_floors > 2 {
        return;
    }
    request_thread_detail(app, worker_tx, detail.page + 1, DetailFetchMode::Append);
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
    if crate::handlers::handle_worker_extensions(app, &response, worker_tx) {
        return;
    }
    match response {
        WorkerResponse::Session(info) => {
            if !app.startup_done {
                app.startup_done = true;
                if info.logged_in {
                    app.session = info;
                    app.page = Page::ThreadFeed;
                    request_threads(app, worker_tx, 1);
                    let _ = worker_tx.send(WorkerRequest::CheckUnread);
                } else if let Some(creds) = crate::config::load_credentials(&app.credentials_path) {
                    app.prefill_login(&creds);
                    app.login.loading = true;
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
                    let _ = worker_tx.send(WorkerRequest::CheckUnread);
                } else {
                    app.session.logged_in = true;
                    app.session.username = Some(username);
                    app.login.loading = false;
                    app.page = Page::ThreadFeed;
                    request_threads(app, worker_tx, 1);
                    let _ = worker_tx.send(WorkerRequest::CheckUnread);
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
        WorkerResponse::ThreadDetailLoaded { tid, page: _, result } => {
            if app.detail.tid != tid {
                return;
            }
            let mode = app.detail.pending_fetch.take().unwrap_or(DetailFetchMode::Replace);
            app.detail.loading = false;
            app.detail.loading_more = false;
            match result {
                Ok(incoming) => {
                    app.detail.title = incoming.title.clone();
                    if let Some(fid) = incoming.fid {
                        app.detail.fid = Some(fid);
                    }
                    let appended_posts = match mode {
                        DetailFetchMode::Append => {
                            if let Some(existing) = app.detail.detail.as_mut() {
                                let new_posts = incoming.posts.clone();
                                existing.posts.extend(new_posts.clone());
                                existing.page = incoming.page;
                                existing.last_page = incoming.last_page;
                                new_posts
                            } else {
                                app.detail.detail = Some(incoming);
                                Vec::new()
                            }
                        }
                        DetailFetchMode::Replace => {
                            let all_posts = incoming.posts.clone();
                            app.detail.detail = Some(incoming);
                            all_posts
                        }
                    };
                    app.detail.error = None;
                    if mode == DetailFetchMode::Append && !appended_posts.is_empty() {
                        prefetch_detail_posts(app, &appended_posts, worker_tx);
                    } else {
                        prefetch_detail_images(app, worker_tx);
                    }
                    app.sync_detail_scroll();
                }
                Err(err) => {
                    if is_auth_required(&err) {
                        try_auto_relogin(app, worker_tx);
                    } else {
                        app.detail.error = Some(error_message(&err));
                        app.toast = Some(error_message(&err));
                    }
                }
            }
        }
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
                    prefetch_feed_avatars(app, worker_tx);
                    app.sync_feed_scroll();
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
        WorkerResponse::PrePostReady { action, result } => {
            if app
                .composer
                .as_ref()
                .is_none_or(|composer| composer.action != action)
            {
                return;
            }
            let fallback = if app
                .composer
                .as_ref()
                .is_some_and(|composer| composer.kind == ComposerKind::Edit)
            {
                app.selected_post().map(edit_body)
            } else {
                None
            };
            let Some(composer) = app.composer.as_mut() else {
                return;
            };
            match result {
                Ok(info) => {
                    composer.apply_prepare(info.quote_text, info.subject, fallback);
                    composer.error = None;
                }
                Err(err) => {
                    if is_auth_required(&err) {
                        app.composer = None;
                        try_auto_relogin(app, worker_tx);
                    } else {
                        composer.preparing = false;
                        composer.error = Some(error_message(&err));
                    }
                }
            }
        }
        WorkerResponse::PostSubmitted {
            action,
            delete,
            result,
        } => {
            if delete {
                if let Some(confirm) = app.confirm_delete.as_mut() {
                    if confirm.action == action {
                        confirm.submitting = false;
                    }
                }
            } else if let Some(composer) = app.composer.as_mut() {
                if composer.action == action {
                    composer.submitting = false;
                }
            }
            match result {
                Ok(post_result) if post_result.success => {
                    let new_subject = app.composer.as_ref().map(|c| c.subject.clone());
                    app.composer = None;
                    app.confirm_delete = None;
                    app.toast = Some(post_result.message.clone());
                    apply_post_success(
                        app,
                        &action,
                        delete,
                        post_result,
                        worker_tx,
                        new_subject,
                    );
                }
                Ok(post_result) => {
                    app.toast = Some(post_result.message);
                }
                Err(err) => {
                    if is_auth_required(&err) {
                        app.composer = None;
                        app.confirm_delete = None;
                        try_auto_relogin(app, worker_tx);
                    } else if matches!(err.code(), ErrorCode::RateLimit) {
                        app.toast = Some(format_rate_limit(&err));
                    } else {
                        let message = error_message(&err);
                        if delete {
                            if let Some(confirm) = app.confirm_delete.as_mut() {
                                confirm.submitting = false;
                            }
                            app.toast = Some(message);
                        } else if let Some(composer) = app.composer.as_mut() {
                            composer.error = Some(message);
                        } else {
                            app.toast = Some(message);
                        }
                    }
                }
            }
        }
        WorkerResponse::ComposerImageUploaded { path, result } => {
            let Some(composer) = app.composer.as_mut() else {
                return;
            };
            composer.submitting = false;
            match result {
                Ok(image_id) => {
                    let tag = format!("[attachimg]{image_id}[/attachimg]");
                    composer.textarea.insert_str(&tag);
                    composer.image_path = None;
                    composer.focus = ComposerFocus::Body;
                    composer.error = None;
                }
                Err(err) => {
                    composer.error = Some(format!("上传失败 ({path}): {}", error_message(&err)));
                }
            }
        }
        WorkerResponse::ImageFetched { url, kind, result } => {
            let outcome = match result {
                Ok(bytes) => hiptty_image::FetchOutcome::Ok(bytes),
                Err(err) if err.code() == hiptty_core::ErrorCode::NotFound => {
                    hiptty_image::FetchOutcome::NotFound
                }
                Err(_) => hiptty_image::FetchOutcome::Failed,
            };
            if let Some(cache) = app.images_mut() {
                cache.apply_fetch(url, kind, outcome);
            }
            if app.page == crate::app::Page::ThreadDetail {
                app.sync_detail_scroll();
            }
        }
        WorkerResponse::SimpleListLoaded { .. }
        | WorkerResponse::PmThreadLoaded { .. }
        | WorkerResponse::PmSent { .. }
        | WorkerResponse::PmDeleted { .. }
        | WorkerResponse::ThreadAtPostLoaded { .. }
        | WorkerResponse::UnreadChecked { .. }
        | WorkerResponse::BlacklistLoaded { .. } => {}
    }
}

pub fn prefetch_detail_images(app: &mut App, worker_tx: &mpsc::UnboundedSender<WorkerRequest>) {
    let posts: Vec<_> = app
        .detail
        .detail
        .as_ref()
        .map(|detail| detail.posts.clone())
        .unwrap_or_default();
    prefetch_detail_posts(app, &posts, worker_tx);
}

fn prefetch_detail_posts(
    app: &mut App,
    posts: &[hiptty_core::Post],
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
) {
    let width = app.viewport_width.saturating_sub(1);
    let mut jobs = Vec::new();
    if let Some(cache) = app.images_mut() {
        for post in posts {
            jobs.extend(hiptty_image::prefetch_post(cache, post, width));
        }
    }
    app.dispatch_image_fetches(jobs, worker_tx);
}

fn prefetch_feed_avatars(app: &mut App, worker_tx: &mpsc::UnboundedSender<WorkerRequest>) {
    let jobs: Vec<_> = app
        .feed
        .threads
        .iter()
        .filter_map(thread_avatar_job)
        .collect();
    enqueue_image_jobs(app, jobs, worker_tx);
}

pub fn enqueue_image_jobs(
    app: &mut App,
    jobs: Vec<hiptty_image::FetchRequest>,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
) {
    let mut pending = Vec::new();
    if let Some(cache) = app.images_mut() {
        for job in jobs {
            if cache.request(job.url.clone(), job.kind) {
                pending.push(job);
            }
        }
    }
    app.dispatch_image_fetches(pending, worker_tx);
}

pub fn try_auto_relogin(app: &mut App, worker_tx: &mpsc::UnboundedSender<WorkerRequest>) {
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

fn handle_composer_key(
    app: &mut App,
    key: KeyEvent,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    if ctrl {
        match key.code {
            KeyCode::Char('s') | KeyCode::Char('S') => submit_composer(app, worker_tx),
            KeyCode::Char('i') | KeyCode::Char('I') => start_image_path_input(app),
            _ => {}
        }
        return;
    }

    let Some(composer) = app.composer.as_mut() else {
        return;
    };

    if composer.preparing || composer.submitting {
        if matches!(key.code, KeyCode::Esc) {
            app.composer = None;
        }
        return;
    }

    if composer.image_path.is_some() {
        handle_image_path_key(app, key, worker_tx);
        return;
    }

    match key.code {
        KeyCode::Esc => app.composer = None,
        KeyCode::Tab | KeyCode::BackTab => cycle_composer_focus(app, key.code == KeyCode::BackTab),
        KeyCode::Backspace if composer.focus == ComposerFocus::Subject => {
            composer.subject.pop();
        }
        KeyCode::Char(c) if composer.focus == ComposerFocus::Subject => {
            composer.subject.push(c);
        }
        _ if composer.focus == ComposerFocus::Body => {
            composer.textarea.input(textarea_input(key));
        }
        _ => {}
    }
}

fn handle_image_path_key(
    app: &mut App,
    key: KeyEvent,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
) {
    let Some(composer) = app.composer.as_mut() else {
        return;
    };
    match key.code {
        KeyCode::Esc => {
            composer.image_path = None;
            composer.focus = ComposerFocus::Body;
            composer.error = None;
        }
        KeyCode::Enter => {
            let path = composer.image_path.clone().unwrap_or_default();
            if path.trim().is_empty() {
                composer.error = Some("请输入图片路径".into());
                return;
            }
            composer.submitting = true;
            composer.error = None;
            let action = composer.action.clone();
            let _ = worker_tx.send(WorkerRequest::UploadComposerImage { action, path });
        }
        KeyCode::Backspace => {
            if let Some(path) = composer.image_path.as_mut() {
                path.pop();
            }
        }
        KeyCode::Char(c) => {
            if let Some(path) = composer.image_path.as_mut() {
                path.push(c);
            }
        }
        _ => {}
    }
}

fn cycle_composer_focus(app: &mut App, reverse: bool) {
    let Some(composer) = app.composer.as_mut() else {
        return;
    };
    if !composer.show_subject {
        composer.focus = ComposerFocus::Body;
        return;
    }
    composer.focus = match (composer.focus, reverse) {
        (ComposerFocus::Subject, false) => ComposerFocus::Body,
        (ComposerFocus::Body, false) => ComposerFocus::Subject,
        (ComposerFocus::Body, true) => ComposerFocus::Subject,
        (ComposerFocus::Subject, true) => ComposerFocus::Body,
        (ComposerFocus::ImagePath, _,) => ComposerFocus::Body,
    };
}

fn start_image_path_input(app: &mut App) {
    let Some(composer) = app.composer.as_mut() else {
        return;
    };
    composer.image_path = Some(String::new());
    composer.focus = ComposerFocus::ImagePath;
    composer.error = None;
}

fn submit_composer(app: &mut App, worker_tx: &mpsc::UnboundedSender<WorkerRequest>) {
    let Some(composer) = app.composer.as_mut() else {
        return;
    };
    if composer.preparing || composer.submitting {
        return;
    }
    let content = composer.body_text();
    if content.trim().is_empty() {
        composer.error = Some("正文不能为空".into());
        return;
    }
    if composer.show_subject && composer.subject.trim().is_empty() {
        composer.error = Some("标题不能为空".into());
        return;
    }
    composer.submitting = true;
    composer.error = None;
    if composer.kind == ComposerKind::PmReply {
        if let Some(uid) = composer.pm_uid.clone() {
            let _ = worker_tx.send(WorkerRequest::SendPm { uid, content });
        }
        return;
    }
    let subject = if composer.show_subject {
        Some(composer.subject.clone())
    } else {
        None
    };
    let _ = worker_tx.send(WorkerRequest::SubmitPost {
        action: composer.action.clone(),
        content,
        subject,
        delete: false,
    });
}

fn handle_confirm_delete_key(
    app: &mut App,
    key: KeyEvent,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
) {
    let Some(confirm) = app.confirm_delete.as_ref() else {
        return;
    };
    if confirm.submitting {
        if matches!(key.code, KeyCode::Esc) {
            app.confirm_delete = None;
        }
        return;
    }
    match key.code {
        KeyCode::Esc | KeyCode::Char('n') => app.confirm_delete = None,
        KeyCode::Char('y') => {
            let action = confirm.action.clone();
            if let Some(confirm) = app.confirm_delete.as_mut() {
                confirm.submitting = true;
            }
            let _ = worker_tx.send(WorkerRequest::SubmitPost {
                action,
                content: String::new(),
                subject: None,
                delete: true,
            });
        }
        _ => {}
    }
}

fn open_feed_reply(app: &mut App, tid: &str) {
    let action = reply_thread_action(tid);
    app.composer = Some(ComposerState::open(
        ComposerKind::Reply,
        action,
        "回复".into(),
        String::new(),
        None,
    ));
}

fn open_new_thread(app: &mut App) {
    let fid = app.feed.fid;
    let forum = hiptty_core::forum_name(fid).unwrap_or("Forum");
    let action = new_thread_action(fid);
    app.composer = Some(ComposerState::open(
        ComposerKind::NewThread,
        action,
        format!("新帖 · {forum}"),
        String::new(),
        Some(String::new()),
    ));
}

fn open_detail_reply(app: &mut App) {
    let tid = app.detail.tid.clone();
    let action = reply_thread_action(&tid);
    app.composer = Some(ComposerState::open(
        ComposerKind::Reply,
        action,
        "回复".into(),
        String::new(),
        None,
    ));
}

fn open_detail_quote(app: &mut App, worker_tx: &mpsc::UnboundedSender<WorkerRequest>) {
    let Some(post) = app.selected_post().cloned() else {
        app.toast = Some("请先选择要引用的楼层".into());
        return;
    };
    let tid = app.detail.tid.clone();
    let action = quote_post_action(&tid, &post.pid);
    app.composer = Some(ComposerState::preparing(
        ComposerKind::Quote,
        action.clone(),
        quote_header(&post),
    ));
    let _ = worker_tx.send(WorkerRequest::PreparePost { action });
}

fn open_detail_edit(app: &mut App, worker_tx: &mpsc::UnboundedSender<WorkerRequest>) {
    let Some(post) = app.selected_post().cloned() else {
        app.toast = Some("请先选择要编辑的楼层".into());
        return;
    };
    let tid = app.detail.tid.clone();
    let fid = app.detail.fid.unwrap_or(app.feed.fid);
    let action = edit_post_action(&tid, &post.pid, fid, post.page);
    let body = edit_body(&post);
    let subject = app.detail.title.clone();
    app.composer = Some(ComposerState::open(
        ComposerKind::Edit,
        action.clone(),
        edit_header(&post),
        body,
        Some(subject),
    ));
    let _ = worker_tx.send(WorkerRequest::PreparePost { action });
}

fn open_detail_delete_confirm(app: &mut App) {
    let Some(post) = app.selected_post().cloned() else {
        app.toast = Some("请先选择要删除的楼层".into());
        return;
    };
    let tid = app.detail.tid.clone();
    let fid = app.detail.fid.unwrap_or(app.feed.fid);
    let action = delete_post_action(&tid, &post.pid, fid);
    app.confirm_delete = Some(ConfirmDeleteState {
        action,
        label: delete_label(&post),
        submitting: false,
    });
}

fn apply_post_success(
    app: &mut App,
    action: &PostAction,
    delete: bool,
    result: hiptty_core::PostResult,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
    new_subject: Option<String>,
) {
    if delete {
        app.page = Page::ThreadFeed;
        request_threads(app, worker_tx, 1);
        return;
    }
    match action {
        PostAction::NewThread { .. } => {
            if let Some(tid) = result.tid {
                app.page = Page::ThreadDetail;
                app.detail = DetailState {
                    tid: tid.clone(),
                    fid: Some(app.feed.fid),
                    title: new_subject.unwrap_or_default(),
                    reply_count: None,
                    view_count: None,
                    detail: result.detail,
                    selected: 0,
                    scroll_top: 0,
                    loading: false,
                    loading_more: false,
                    pending_fetch: None,
                    error: None,
                };
                if app.detail.detail.is_none() {
                    request_thread_detail(app, worker_tx, 1, DetailFetchMode::Replace);
                } else {
                    prefetch_detail_images(app, worker_tx);
                    app.sync_detail_scroll();
                }
            }
        }
        PostAction::ReplyThread { .. }
        | PostAction::ReplyPost { .. }
        | PostAction::QuotePost { .. }
        | PostAction::EditPost { .. } => {
            if let Some(detail) = result.detail {
                app.detail.detail = Some(detail);
                app.detail.loading = false;
                app.detail.loading_more = false;
                app.detail.pending_fetch = None;
                app.detail.error = None;
                prefetch_detail_images(app, worker_tx);
                app.sync_detail_scroll();
            } else if app.page == Page::ThreadDetail {
                request_thread_detail(app, worker_tx, 1, DetailFetchMode::Replace);
            }
        }
        _ => {}
    }
}

fn textarea_input(key: KeyEvent) -> TextareaInput {
    if key.kind == KeyEventKind::Release {
        return TextareaInput::default();
    }
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    if key.code == KeyCode::BackTab {
        return TextareaInput {
            key: TextareaKey::Tab,
            ctrl,
            alt,
            shift: true,
        };
    }
    let key_code = match key.code {
        KeyCode::Char(c) => TextareaKey::Char(c),
        KeyCode::Backspace => TextareaKey::Backspace,
        KeyCode::Enter => TextareaKey::Enter,
        KeyCode::Left => TextareaKey::Left,
        KeyCode::Right => TextareaKey::Right,
        KeyCode::Up => TextareaKey::Up,
        KeyCode::Down => TextareaKey::Down,
        KeyCode::Tab => TextareaKey::Tab,
        KeyCode::Delete => TextareaKey::Delete,
        KeyCode::Home => TextareaKey::Home,
        KeyCode::End => TextareaKey::End,
        KeyCode::PageUp => TextareaKey::PageUp,
        KeyCode::PageDown => TextareaKey::PageDown,
        KeyCode::Esc => TextareaKey::Esc,
        KeyCode::F(n) => TextareaKey::F(n),
        _ => TextareaKey::Null,
    };
    TextareaInput {
        key: key_code,
        ctrl,
        alt,
        shift,
    }
}

fn format_rate_limit(err: &AdapterError) -> String {
    let msg = err.to_string();
    if let Some(rest) = msg.strip_prefix("wait ") {
        if let Some(secs) = rest.split_whitespace().next() {
            return format!("发帖过快，请 {secs} 秒后重试");
        }
    }
    format!("发帖过快: {msg}")
}
