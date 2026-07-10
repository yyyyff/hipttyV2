use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use hiptty_core::{AdapterError, ErrorCode, PostAction};
use hiptty_widgets::{ComposerFocus, LoginField};
use ratatui_textarea::{Input as TextareaInput, Key as TextareaKey};
use tokio::sync::mpsc;

use hiptty_image::thread_avatar_job;

use crate::app::{App, DetailLoadIntent, DetailState, Overlay, Page};
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
    // Raw mode: Ctrl+C is a key event, not SIGINT — handle before CONTROL is filtered out.
    if key.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'))
    {
        app.quit = true;
        return;
    }
    if app.toast.is_some()
        && matches!(key.code, KeyCode::Esc | KeyCode::Enter)
        && app.overlay == Overlay::None
        && app.confirm_delete.is_none()
        && app.composer.is_none()
    {
        app.dismiss_toast();
        return;
    }
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
                app.overlay = Overlay::None;
                crate::handlers::switch_feed_forum(app, fid, worker_tx);
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
        KeyCode::Char('[') => {
            let fid = crate::handlers::cycle_default_forum_tab(app, -1);
            crate::handlers::switch_feed_forum(app, fid, worker_tx);
        }
        KeyCode::Char(']') => {
            let fid = crate::handlers::cycle_default_forum_tab(app, 1);
            crate::handlers::switch_feed_forum(app, fid, worker_tx);
        }
        KeyCode::Char('f') => {
            app.overlay = Overlay::ForumPicker;
            app.forum_picker_scroll = 0;
            let entries = app.forum_picker_fids();
            app.forum_picker_selected = entries
                .iter()
                .position(|&fid| fid == app.feed.fid)
                .unwrap_or(0);
        }
        KeyCode::Char('g') => {
            app.feed.selected = 0;
            app.feed.scroll_lines = 0;
        }
        KeyCode::Char('G') if !app.feed.threads.is_empty() => {
            app.feed.selected = app.feed.threads.len() - 1;
            app.sync_feed_scroll();
            maybe_load_more(app, worker_tx);
        }
        KeyCode::Char('r') => refresh_feed(app, worker_tx),
        KeyCode::Char('n') => open_new_thread(app, worker_tx),
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
            // Note: global Esc/b usually intercepts via navigate_back() before this arm.
            // Pin cleanup is guaranteed by draw() when page != ThreadDetail.
            app.page = Page::ThreadFeed;
            app.detail.loading = false;
            app.detail.loading_more = false;
            app.detail.pending_intent = None;
            app.detail.pending_request_id = 0;
            app.detail.error = None;
            if let Some(cache) = app.images_mut() {
                cache.clear_pinned();
            }
        }
        KeyCode::Char('j') | KeyCode::Down => {
            if app.detail.detail.is_some() {
                let viewport = app.scroll_viewport_height();
                let selected = app.detail.selected;
                let scroll_top = app.detail.scroll_top;
                if let Some((sel, scroll)) = app.detail_layout().map(|layout| {
                    let (sel, scroll) = layout.detail_step_down(selected, scroll_top, viewport);
                    (sel, layout.clamp_scroll_top(scroll, viewport))
                }) {
                    app.detail.selected = sel;
                    app.detail.scroll_top = scroll;
                }
                maybe_load_more_detail(app, worker_tx);
                prefetch_detail_viewport_images(app, worker_tx);
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if app.detail.detail.is_some() {
                let viewport = app.scroll_viewport_height();
                let selected = app.detail.selected;
                let scroll_top = app.detail.scroll_top;
                if let Some((sel, scroll)) = app.detail_layout().map(|layout| {
                    let (sel, scroll) = layout.detail_step_up(selected, scroll_top, viewport);
                    (sel, layout.clamp_scroll_top(scroll, viewport))
                }) {
                    app.detail.selected = sel;
                    app.detail.scroll_top = scroll;
                }
                maybe_load_more_detail(app, worker_tx);
                prefetch_detail_viewport_images(app, worker_tx);
            }
        }
        KeyCode::PageDown => {
            if app.detail.detail.is_some() {
                let viewport = app.scroll_viewport_height();
                let scroll_top = app.detail.scroll_top;
                if let Some((scroll, sel)) = app.detail_layout().map(|layout| {
                    let scroll = layout.page_scroll_top(scroll_top, 1, viewport);
                    (scroll, layout.first_visible(scroll))
                }) {
                    app.detail.scroll_top = scroll;
                    app.detail.selected = sel;
                }
                maybe_load_more_detail(app, worker_tx);
                prefetch_detail_viewport_images(app, worker_tx);
            }
        }
        KeyCode::PageUp => {
            if app.detail.detail.is_some() {
                let viewport = app.scroll_viewport_height();
                let scroll_top = app.detail.scroll_top;
                if let Some((scroll, sel)) = app.detail_layout().map(|layout| {
                    let scroll = layout.page_scroll_top(scroll_top, -1, viewport);
                    (scroll, layout.first_visible(scroll))
                }) {
                    app.detail.scroll_top = scroll;
                    app.detail.selected = sel;
                }
                maybe_load_more_detail(app, worker_tx);
                prefetch_detail_viewport_images(app, worker_tx);
            }
        }
        KeyCode::Char('g') => {
            request_thread_detail(app, worker_tx, 1, DetailLoadIntent::ReplaceTop);
        }
        KeyCode::Char('G') => {
            if let Some(d) = app.detail.detail.as_ref() {
                let last = d.last_page.max(1);
                // Buffer already ends at the last page → scroll only (no replace/flicker).
                // If we only hold the last page (after a prior G), upward scroll will prepend.
                if d.page >= last {
                    app.scroll_detail_to_bottom();
                } else if last > 0 {
                    request_thread_detail(app, worker_tx, last, DetailLoadIntent::ReplaceBottom);
                }
            }
        }
        KeyCode::Char('r') => open_detail_reply(app),
        // q / e / d deferred: need floor-scoped actions (own posts only / quote target).
        _ => {}
    }
}

pub fn open_thread_detail(
    app: &mut App,
    thread: &hiptty_core::ThreadSummary,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
) {
    app.detail = DetailState::from_summary(thread, app.feed.fid);
    app.page = Page::ThreadDetail;
    request_thread_detail(app, worker_tx, 1, DetailLoadIntent::ReplaceTop);
}

pub fn request_thread_detail(
    app: &mut App,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
    page: u32,
    intent: DetailLoadIntent,
) {
    if app.detail.tid.is_empty() {
        return;
    }
    match intent {
        DetailLoadIntent::ReplaceTop | DetailLoadIntent::ReplaceBottom => {
            app.detail.loading = true;
            app.detail.loading_more = false;
        }
        DetailLoadIntent::AppendPreserve | DetailLoadIntent::PrependPreserve => {
            app.detail.loading_more = true;
        }
    }
    let request_id = app.detail.allocate_request_id();
    app.detail.pending_intent = Some(intent);
    app.detail.error = None;
    let _ = worker_tx.send(WorkerRequest::LoadThreadDetail {
        tid: app.detail.tid.clone(),
        page,
        request_id,
    });
}

pub fn maybe_load_more_detail(app: &mut App, worker_tx: &mpsc::UnboundedSender<WorkerRequest>) {
    if app.detail.loading || app.detail.loading_more {
        return;
    }
    let page_hi = app.detail.detail.as_ref().map(|d| d.page);
    let last_page = app.detail.detail.as_ref().map(|d| d.last_page);
    let post_count = app
        .detail
        .detail
        .as_ref()
        .map(|d| d.posts.len())
        .unwrap_or(0);
    let (Some(page_hi), Some(last_page)) = (page_hi, last_page) else {
        return;
    };
    if post_count == 0 {
        return;
    }
    let viewport = app.scroll_viewport_height();
    let scroll_top = app.detail.scroll_top;
    let Some(layout) = app.detail_layout() else {
        return;
    };
    let first_visible = layout.first_visible(scroll_top);
    let last_visible = layout.last_visible(scroll_top, viewport);

    // Near top and missing earlier pages (typical after `G` only loaded the last page).
    let page_lo = app.detail.loaded_page_lo.max(1);
    if page_lo > 1 && (scroll_top == 0 || first_visible <= 2) {
        request_thread_detail(
            app,
            worker_tx,
            page_lo - 1,
            DetailLoadIntent::PrependPreserve,
        );
        return;
    }

    // Near bottom and missing later pages.
    if page_hi < last_page {
        let remaining_floors = post_count.saturating_sub(last_visible + 1);
        if remaining_floors <= 2 {
            request_thread_detail(
                app,
                worker_tx,
                page_hi + 1,
                DetailLoadIntent::AppendPreserve,
            );
        }
    }
}

pub fn maybe_load_more(app: &mut App, worker_tx: &mpsc::UnboundedSender<WorkerRequest>) {
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
        app.feed.scroll_lines = 0;
    }
    app.feed.loading = true;
    app.feed.error = None;
    let _ = worker_tx.send(WorkerRequest::LoadThreads {
        fid: app.feed.fid,
        page,
    });
}

/// Force-refresh feed page 1 without blanking the list (loading shows in status bar).
pub fn refresh_feed(app: &mut App, worker_tx: &mpsc::UnboundedSender<WorkerRequest>) {
    if app.feed.loading {
        return;
    }
    app.feed.loading = true;
    app.feed.error = None;
    let _ = worker_tx.send(WorkerRequest::LoadThreads {
        fid: app.feed.fid,
        page: 1,
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
                    app.set_toast(format!("自动登录失败: {}", error_message(&err)), true);
                    app.page = Page::Login;
                }
            }
        },
        WorkerResponse::ThreadDetailLoaded {
            tid,
            page: _,
            request_id,
            result,
        } => {
            if app.detail.tid != tid {
                return;
            }
            // Drop stale responses from earlier g/G / append requests.
            if request_id != app.detail.pending_request_id {
                return;
            }
            let intent = app.detail.pending_intent.take().unwrap_or({
                if app.detail.loading_more {
                    DetailLoadIntent::AppendPreserve
                } else {
                    DetailLoadIntent::ReplaceTop
                }
            });
            app.detail.loading = false;
            app.detail.loading_more = false;
            match result {
                Ok(incoming) => {
                    app.detail.title = incoming.title.clone();
                    if let Some(fid) = incoming.fid {
                        app.detail.fid = Some(fid);
                    }
                    let page = incoming.page.max(1);
                    let mut prepended_count = 0usize;
                    let edge_posts = match intent {
                        DetailLoadIntent::AppendPreserve => {
                            if let Some(existing) = app.detail.detail.as_mut() {
                                let new_posts = incoming.posts.clone();
                                existing.posts.extend(new_posts.clone());
                                existing.page = incoming.page;
                                existing.last_page = incoming.last_page;
                                // loaded_page_lo unchanged
                                new_posts
                            } else {
                                app.detail.loaded_page_lo = page;
                                app.detail.detail = Some(incoming);
                                Vec::new()
                            }
                        }
                        DetailLoadIntent::PrependPreserve => {
                            if let Some(existing) = app.detail.detail.as_mut() {
                                let new_posts = incoming.posts.clone();
                                prepended_count = new_posts.len();
                                let mut merged = new_posts.clone();
                                merged.append(&mut existing.posts);
                                existing.posts = merged;
                                // Keep highest loaded page (`existing.page`); advance lo.
                                existing.last_page = incoming.last_page;
                                app.detail.loaded_page_lo = page;
                                new_posts
                            } else {
                                app.detail.loaded_page_lo = page;
                                app.detail.detail = Some(incoming);
                                Vec::new()
                            }
                        }
                        DetailLoadIntent::ReplaceTop | DetailLoadIntent::ReplaceBottom => {
                            app.detail.loaded_page_lo = page;
                            let all_posts = incoming.posts.clone();
                            app.detail.detail = Some(incoming);
                            all_posts
                        }
                    };
                    app.detail.error = None;
                    match intent {
                        DetailLoadIntent::AppendPreserve => {
                            if !edge_posts.is_empty() {
                                prefetch_detail_posts(app, &edge_posts, worker_tx);
                            }
                            // preserve_detail_scroll invalidates layout (revision++) and rebuilds.
                            app.preserve_detail_scroll();
                        }
                        DetailLoadIntent::PrependPreserve => {
                            // Shift scroll/selection by the height/count of prepended floors so the
                            // viewport stays on the same content the user was reading.
                            let old_scroll = app.detail.scroll_top;
                            let old_selected = app.detail.selected;
                            app.detail.invalidate_layout();
                            let shift = app
                                .detail_layout()
                                .map(|l| l.offset(prepended_count))
                                .unwrap_or(0);
                            app.detail.scroll_top = old_scroll.saturating_add(shift);
                            app.detail.selected = old_selected.saturating_add(prepended_count);
                            app.clamp_detail_selected();
                            if !edge_posts.is_empty() {
                                prefetch_detail_posts(app, &edge_posts, worker_tx);
                            }
                            // Further earlier pages load on the next upward scroll (avoid cascade).
                        }
                        DetailLoadIntent::ReplaceTop => {
                            // Posts replaced — revision must change even when post count matches.
                            app.detail.invalidate_layout();
                            // g / open / refresh page 1 jump to top; mid-page refresh keeps selection.
                            let page = app.detail.detail.as_ref().map(|d| d.page).unwrap_or(1);
                            if page <= 1 {
                                app.detail.selected = 0;
                                app.detail.scroll_top = 0;
                            }
                            app.clamp_detail_selected();
                            prefetch_detail_images(app, worker_tx);
                            app.sync_detail_scroll();
                        }
                        DetailLoadIntent::ReplaceBottom => {
                            app.detail.invalidate_layout();
                            prefetch_detail_images(app, worker_tx);
                            app.scroll_detail_to_bottom();
                        }
                    }
                }
                Err(err) => {
                    if is_auth_required(&err) {
                        try_auto_relogin(app, worker_tx);
                    } else {
                        app.detail.error = Some(error_message(&err));
                        app.set_toast(error_message(&err), true);
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
                        if !app.feed.threads.is_empty() {
                            app.feed.selected = app
                                .feed
                                .selected
                                .min(app.feed.threads.len().saturating_sub(1));
                        } else {
                            app.feed.selected = 0;
                            app.feed.scroll_lines = 0;
                        }
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
                        app.set_toast(error_message(&err), true);
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
                    composer.apply_prepare(info, fallback);
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
                    app.set_toast(post_result.message.clone(), false);
                    apply_post_success(app, &action, delete, post_result, worker_tx, new_subject);
                }
                Ok(post_result) => {
                    app.set_toast(post_result.message, true);
                }
                Err(err) => {
                    if is_auth_required(&err) {
                        app.composer = None;
                        app.confirm_delete = None;
                        try_auto_relogin(app, worker_tx);
                    } else if matches!(err.code(), ErrorCode::RateLimit) {
                        app.set_toast(format_rate_limit(&err), true);
                    } else {
                        let message = error_message(&err);
                        if delete {
                            if let Some(confirm) = app.confirm_delete.as_mut() {
                                confirm.submitting = false;
                            }
                            app.set_toast(message, true);
                        } else if let Some(composer) = app.composer.as_mut() {
                            composer.error = Some(message);
                        } else {
                            app.set_toast(message, true);
                        }
                    }
                }
            }
        }
        WorkerResponse::LoggedOut { result } => {
            // UI already on Login; only toast "已登出" when local + worker cleanup both OK.
            let pending = app.logout_pending;
            app.logout_pending = false;
            let local_err = app.logout_local_error.take();
            if !pending {
                // Spurious / late response — only surface hard worker failures.
                if let Err(err) = result {
                    app.set_toast(format!("登出清理失败: {err}"), true);
                }
                return;
            }
            match (result, local_err) {
                (Ok(()), None) => app.set_toast("已登出", false),
                (Ok(()), Some(local)) => {
                    // Worker OK but credentials may remain — keep the error, no success cover.
                    app.set_toast(local, true);
                }
                (Err(err), None) => {
                    app.set_toast(format!("登出清理失败: {err}"), true);
                }
                (Err(err), Some(local)) => {
                    app.set_toast(format!("{local}；会话清理也失败: {err}"), true);
                }
            }
        }
        WorkerResponse::ComposerImageUploaded { .. } => {
            // Image insert path removed from TUI; ignore late upload responses.
        }
        WorkerResponse::ImageFetched {
            url,
            kind,
            session_epoch,
            result,
        } => {
            // Always free a concurrency slot; drop payload if the session turned over.
            if session_epoch != app.session_epoch {
                app.on_image_fetch_finished(worker_tx);
                return;
            }
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
            // Height only changes when decode finishes (cache.poll in draw); do not yank
            // scroll here. Drain the in-flight image queue for the next fetches.
            app.on_image_fetch_finished(worker_tx);
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

/// Floors before/after the visible range to warm for smooth scroll.
const DETAIL_IMAGE_PREFETCH_PAD: usize = 1;

/// Prefetch detail images near the current viewport (not the whole page).
pub fn prefetch_detail_images(app: &mut App, worker_tx: &mpsc::UnboundedSender<WorkerRequest>) {
    prefetch_detail_viewport_images(app, worker_tx);
}

/// Prefetch only floors in `[first - pad, last + pad]` for current scroll.
pub fn prefetch_detail_viewport_images(
    app: &mut App,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
) {
    let post_count = app
        .detail
        .detail
        .as_ref()
        .map(|d| d.posts.len())
        .unwrap_or(0);
    if post_count == 0 {
        // Empty detail must not keep a previous thread's pin set.
        if let Some(cache) = app.images_mut() {
            cache.clear_pinned();
        }
        return;
    }
    let width = app.content_width();
    let viewport = app.scroll_viewport_height();
    let scroll = app.detail.scroll_top;
    let (first, last) = {
        let Some(layout) = app.detail_layout() else {
            return;
        };
        (
            layout.first_visible(scroll),
            layout.last_visible(scroll, viewport),
        )
    };
    let start = first.saturating_sub(DETAIL_IMAGE_PREFETCH_PAD);
    let end = last
        .saturating_add(DETAIL_IMAGE_PREFETCH_PAD)
        .min(post_count.saturating_sub(1));
    // Only clone the visible pad range — full-list clone was O(n) string copy on every scroll.
    let slice: Vec<_> = app
        .detail
        .detail
        .as_ref()
        .map(|d| d.posts[start..=end].to_vec())
        .unwrap_or_default();
    let mut jobs = Vec::new();
    if let Some(cache) = app.images_mut() {
        jobs.extend(hiptty_image::prefetch_posts_range(
            cache,
            &slice,
            0,
            slice.len().saturating_sub(1),
            width,
        ));
    }
    app.dispatch_image_fetches(jobs, worker_tx);
}

/// Prefetch images for newly appended floors only (still pad around viewport when possible).
fn prefetch_detail_posts(
    app: &mut App,
    posts: &[hiptty_core::Post],
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
) {
    if posts.is_empty() {
        return;
    }
    // After append, warm the new posts (they sit at the end) via the viewport path if
    // the user is near the bottom; also request jobs for the appended slice so lazy-load
    // append does not wait for another scroll.
    let width = app.content_width();
    let mut jobs = Vec::new();
    if let Some(cache) = app.images_mut() {
        for post in posts {
            jobs.extend(hiptty_image::prefetch_post(cache, post, width));
        }
    }
    app.dispatch_image_fetches(jobs, worker_tx);
    prefetch_detail_viewport_images(app, worker_tx);
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
        app.set_toast("登录已过期，请重新登录", true);
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
    // Submit shortcuts. Terminals disagree on Ctrl+Enter encoding:
    // - Enter + CONTROL (kitty/modern)
    // - Char('\n') + CONTROL (LF)
    // - Char('j') + CONTROL (Ctrl+J == LF historically)
    // Ctrl+S kept as a reliable fallback.
    if is_composer_submit_key(key) {
        submit_composer(app, worker_tx);
        return;
    }

    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    if ctrl {
        // Swallow other Ctrl combos so they don't type into the body.
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

    match key.code {
        KeyCode::Esc => app.composer = None,
        KeyCode::Tab | KeyCode::BackTab if composer.show_subject || composer.need_type_ui() => {
            cycle_composer_focus(app, key.code == KeyCode::BackTab);
        }
        KeyCode::Left
        | KeyCode::Right
        | KeyCode::Char('h')
        | KeyCode::Char('l')
        | KeyCode::Char('j')
        | KeyCode::Char('k')
            if composer.focus == ComposerFocus::Type =>
        {
            let delta = match key.code {
                KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('k') => -1,
                _ => 1,
            };
            composer.cycle_type(delta);
        }
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

fn cycle_composer_focus(app: &mut App, reverse: bool) {
    let Some(composer) = app.composer.as_mut() else {
        return;
    };
    let order = composer.focus_order();
    if order.len() <= 1 {
        composer.focus = ComposerFocus::Body;
        return;
    }
    let cur = order.iter().position(|&f| f == composer.focus).unwrap_or(0);
    let next = if reverse {
        cur.checked_sub(1).unwrap_or(order.len() - 1)
    } else {
        (cur + 1) % order.len()
    };
    composer.focus = order[next];
}

fn is_composer_submit_key(key: KeyEvent) -> bool {
    let mods = key.modifiers;
    // Ignore pure Shift; require Control (optionally with Shift/Alt as some TEs do).
    if !mods.contains(KeyModifiers::CONTROL) {
        return false;
    }
    matches!(
        key.code,
        KeyCode::Enter
            | KeyCode::Char('\n')
            | KeyCode::Char('\r')
            | KeyCode::Char('j')
            | KeyCode::Char('J')
            | KeyCode::Char('s')
            | KeyCode::Char('S')
    )
}

fn submit_composer(app: &mut App, worker_tx: &mpsc::UnboundedSender<WorkerRequest>) {
    let Some(composer) = app.composer.as_mut() else {
        return;
    };
    if composer.preparing || composer.submitting {
        return;
    }
    if !composer.type_selected_ok() {
        composer.error = Some("该版发帖必须指定分类".into());
        composer.focus = ComposerFocus::Type;
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
    composer.sync_type_into_action();
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

fn open_new_thread(app: &mut App, worker_tx: &mpsc::UnboundedSender<WorkerRequest>) {
    let fid = app.feed.fid;
    let forum = hiptty_core::forum_name(fid).unwrap_or("Forum");
    let action = new_thread_action(fid);
    app.composer = Some(ComposerState::preparing(
        ComposerKind::NewThread,
        action.clone(),
        format!("新帖 · {forum}"),
    ));
    // Ensure subject field is available while preparing.
    if let Some(composer) = app.composer.as_mut() {
        composer.show_subject = true;
        composer.subject.clear();
    }
    let _ = worker_tx.send(WorkerRequest::PreparePost { action });
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

/// Floor-scoped quote/edit/delete — kept for a later contextual UI; not bound globally.
#[allow(dead_code)]
fn open_detail_quote(app: &mut App, worker_tx: &mpsc::UnboundedSender<WorkerRequest>) {
    let Some(post) = app.selected_post().cloned() else {
        app.set_toast("请先选择要引用的楼层", true);
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

#[allow(dead_code)]
fn open_detail_edit(app: &mut App, worker_tx: &mpsc::UnboundedSender<WorkerRequest>) {
    let Some(post) = app.selected_post().cloned() else {
        app.set_toast("请先选择要编辑的楼层", true);
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

#[allow(dead_code)]
fn open_detail_delete_confirm(app: &mut App) {
    let Some(post) = app.selected_post().cloned() else {
        app.set_toast("请先选择要删除的楼层", true);
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
                let loaded_page = result.detail.as_ref().map(|d| d.page.max(1)).unwrap_or(1);
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
                    pending_intent: None,
                    pending_request_id: 0,
                    next_request_id: 1,
                    loaded_page_lo: loaded_page,
                    error: None,
                    layout_revision: 0,
                    layout: None,
                };
                if app.detail.detail.is_none() {
                    request_thread_detail(app, worker_tx, 1, DetailLoadIntent::ReplaceTop);
                } else {
                    app.detail.invalidate_layout();
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
                let page = detail.page.max(1);
                app.detail.detail = Some(detail);
                app.detail.loaded_page_lo = page;
                app.detail.loading = false;
                app.detail.loading_more = false;
                app.detail.pending_intent = None;
                app.detail.pending_request_id = 0;
                app.detail.error = None;
                app.invalidate_detail_layout();
                prefetch_detail_images(app, worker_tx);
                app.sync_detail_scroll();
            } else if app.page == Page::ThreadDetail {
                request_thread_detail(app, worker_tx, 1, DetailLoadIntent::ReplaceTop);
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
