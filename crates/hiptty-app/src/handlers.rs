use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use hiptty_core::{list_item_to_thread_summary, AdapterError, ErrorCode, ListItem, ThreadSummary};
use hiptty_widgets::MAIN_MENU_ITEMS;
use tokio::sync::mpsc;

use crate::app::{App, DetailLoadIntent, DetailState, FeedState, Overlay, Page};
use crate::commands::{execute_command, tab_complete_command};
use crate::composer::{ComposerKind, ComposerState};
use crate::list_page::ListPageKind;
use crate::nav::{navigate_back, navigate_to};
use crate::worker::{WorkerRequest, WorkerResponse};

pub fn handle_global_key(
    app: &mut App,
    key: KeyEvent,
    _worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
) -> bool {
    if app.composer.is_some() || app.confirm_delete.is_some() {
        return false;
    }
    if app.overlay != Overlay::None {
        return false;
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return false;
    }
    match key.code {
        KeyCode::Char(':') => {
            app.overlay_state.command_input.clear();
            app.overlay_state.command_cursor = 0;
            app.overlay = Overlay::CommandBar;
            true
        }
        KeyCode::Esc => {
            if app.page == Page::ThreadFeed || !navigate_back(app) {
                app.overlay_state.main_menu_selected = 0;
                app.overlay = Overlay::MainMenu;
            }
            true
        }
        KeyCode::Char('/') if matches!(app.page, Page::ThreadFeed | Page::Search) => {
            app.overlay_state.search_input = app.list_page.search_query.clone();
            app.overlay = Overlay::SearchPrompt;
            true
        }
        KeyCode::Char('b')
            if !matches!(app.page, Page::ThreadFeed | Page::Login | Page::Startup) =>
        {
            navigate_back(app);
            true
        }
        _ => false,
    }
}

pub fn handle_overlay_key(
    app: &mut App,
    key: KeyEvent,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
) {
    match app.overlay {
        Overlay::MainMenu => handle_main_menu_key(app, key, worker_tx),
        Overlay::Settings => handle_settings_key(app, key, worker_tx),
        Overlay::SearchPrompt => handle_search_prompt_key(app, key, worker_tx),
        Overlay::CommandBar => handle_command_bar_key(app, key, worker_tx),
        Overlay::ForumPicker | Overlay::None => {}
    }
}

fn handle_main_menu_key(
    app: &mut App,
    key: KeyEvent,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
) {
    match key.code {
        KeyCode::Esc => app.overlay = Overlay::None,
        KeyCode::Char('j') | KeyCode::Down => {
            if app.overlay_state.main_menu_selected + 1 < MAIN_MENU_ITEMS.len() {
                app.overlay_state.main_menu_selected += 1;
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.overlay_state.main_menu_selected =
                app.overlay_state.main_menu_selected.saturating_sub(1);
        }
        KeyCode::Enter => {
            let selected = app.overlay_state.main_menu_selected;
            activate_main_menu_item(app, selected, worker_tx);
        }
        _ => {}
    }
}

pub fn activate_main_menu_item(
    app: &mut App,
    selected: usize,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
) {
    app.overlay = Overlay::None;
    match selected {
        0 => {
            navigate_to(app, Page::PmList);
            request_list_page(app, worker_tx, ListPageKind::PmList, 1);
        }
        1 => {
            navigate_to(app, Page::Notifications);
            request_list_page(app, worker_tx, ListPageKind::Notifications, 1);
        }
        2 => {
            navigate_to(app, Page::MyThreads);
            request_list_page(app, worker_tx, ListPageKind::MyThreads, 1);
        }
        3 => {
            navigate_to(app, Page::MyReplies);
            request_list_page(app, worker_tx, ListPageKind::MyReplies, 1);
        }
        4 => {
            navigate_to(app, Page::Favorites);
            request_list_page(app, worker_tx, ListPageKind::Favorites, 1);
        }
        5 => {
            app.overlay_state.settings_selected = 0;
            app.overlay = Overlay::Settings;
            let request_id = app.allocate_worker_request_id();
            app.blacklist_pending_request_id = request_id;
            let _ = worker_tx.send(WorkerRequest::LoadBlacklist { request_id });
        }
        6 => app.quit = true,
        _ => {}
    }
}

pub fn activate_settings_row(app: &mut App, idx: usize) {
    match idx {
        0..=2 => {
            let forums: Vec<u32> = hiptty_core::FORUMS.iter().map(|forum| forum.id).collect();
            let current = app.settings.default_forums[idx];
            let next = forums
                .iter()
                .position(|&f| f == current)
                .map(|i| forums[(i + 1) % forums.len()])
                .unwrap_or(forums[0]);
            app.settings.default_forums[idx] = next;
            let _ = crate::config::save_settings(&app.settings_path, &app.settings);
            app.set_toast("默认版块已更新", false);
        }
        3 => app.set_toast("黑名单管理将在后续版本提供", false),
        _ => {}
    }
}

fn handle_settings_key(
    app: &mut App,
    key: KeyEvent,
    _worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
) {
    const ROWS: usize = 4;
    match key.code {
        KeyCode::Esc => app.overlay = Overlay::None,
        KeyCode::Char('j') | KeyCode::Down => {
            if app.overlay_state.settings_selected + 1 < ROWS {
                app.overlay_state.settings_selected += 1;
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.overlay_state.settings_selected =
                app.overlay_state.settings_selected.saturating_sub(1);
        }
        KeyCode::Enter => {
            activate_settings_row(app, app.overlay_state.settings_selected);
        }
        _ => {}
    }
}

fn handle_search_prompt_key(
    app: &mut App,
    key: KeyEvent,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
) {
    match key.code {
        KeyCode::Esc => app.overlay = Overlay::None,
        KeyCode::Enter => {
            let query = app.overlay_state.search_input.trim().to_string();
            app.overlay = Overlay::None;
            if query.is_empty() {
                app.set_toast("请输入搜索词", true);
                return;
            }
            app.list_page.search_query = query;
            navigate_to(app, Page::Search);
            request_list_page(app, worker_tx, ListPageKind::Search, 1);
        }
        KeyCode::Backspace => {
            app.overlay_state.search_input.pop();
        }
        KeyCode::Char(c) => app.overlay_state.search_input.push(c),
        _ => {}
    }
}

fn handle_command_bar_key(
    app: &mut App,
    key: KeyEvent,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
) {
    // Ctrl+U clears the line (readline / vim cmdline habit).
    if key.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(key.code, KeyCode::Char('u') | KeyCode::Char('U'))
    {
        app.overlay_state.command_input.clear();
        app.overlay_state.command_cursor = 0;
        return;
    }

    match key.code {
        KeyCode::Esc => {
            app.overlay = Overlay::None;
            app.overlay_state.command_input.clear();
            app.overlay_state.command_cursor = 0;
        }
        KeyCode::Enter => {
            let input = app.overlay_state.command_input.clone();
            execute_command(app, &input, worker_tx);
        }
        KeyCode::Tab => {
            let page = app.page;
            let input = &mut app.overlay_state.command_input;
            let cursor = &mut app.overlay_state.command_cursor;
            tab_complete_command(input, cursor, page);
        }
        KeyCode::Backspace => command_backspace(app),
        KeyCode::Delete => command_delete(app),
        KeyCode::Left => {
            let cur = app.overlay_state.command_cursor;
            app.overlay_state.command_cursor =
                prev_char_boundary(&app.overlay_state.command_input, cur);
        }
        KeyCode::Right => {
            let cur = app.overlay_state.command_cursor;
            app.overlay_state.command_cursor =
                next_char_boundary(&app.overlay_state.command_input, cur);
        }
        KeyCode::Home => app.overlay_state.command_cursor = 0,
        KeyCode::End => app.overlay_state.command_cursor = app.overlay_state.command_input.len(),
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            command_insert(app, c);
        }
        _ => {}
    }
}

fn command_insert(app: &mut App, c: char) {
    let cur = app
        .overlay_state
        .command_cursor
        .min(app.overlay_state.command_input.len());
    app.overlay_state.command_input.insert(cur, c);
    app.overlay_state.command_cursor = cur + c.len_utf8();
}

fn command_backspace(app: &mut App) {
    let cur = app.overlay_state.command_cursor;
    if cur == 0 {
        return;
    }
    let prev = prev_char_boundary(&app.overlay_state.command_input, cur);
    app.overlay_state.command_input.drain(prev..cur);
    app.overlay_state.command_cursor = prev;
}

fn command_delete(app: &mut App) {
    let cur = app.overlay_state.command_cursor;
    if cur >= app.overlay_state.command_input.len() {
        return;
    }
    let next = next_char_boundary(&app.overlay_state.command_input, cur);
    app.overlay_state.command_input.drain(cur..next);
}

fn prev_char_boundary(s: &str, idx: usize) -> usize {
    if idx == 0 {
        return 0;
    }
    let mut i = idx.min(s.len()) - 1;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn next_char_boundary(s: &str, idx: usize) -> usize {
    if idx >= s.len() {
        return s.len();
    }
    let mut i = idx + 1;
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

pub fn handle_list_page_key(
    app: &mut App,
    key: KeyEvent,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
) {
    let kind = app.list_page.kind.unwrap_or(ListPageKind::Search);
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            if app.list_page.selected + 1 < app.list_page.items.len() {
                app.list_page.selected += 1;
                app.sync_list_scroll();
                maybe_load_more_list(app, worker_tx, kind);
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if app.list_page.selected > 0 {
                app.list_page.selected -= 1;
                app.sync_list_scroll();
            }
        }
        KeyCode::Enter => open_list_selection(app, worker_tx),
        KeyCode::Char('r') => refresh_list_page(app, worker_tx, kind),
        KeyCode::Char('d') if kind == ListPageKind::PmList => {
            if let Some(item) = app.list_page.items.get(app.list_page.selected).cloned() {
                if let Some(uid) = item.uid {
                    let _ = worker_tx.send(WorkerRequest::PmDelete { uid: uid.clone() });
                    app.set_toast(
                        format!("正在删除与 {} 的对话", item.author.unwrap_or_default()),
                        false,
                    );
                }
            }
        }
        _ => {}
    }
}

pub fn handle_pm_thread_key(
    app: &mut App,
    key: KeyEvent,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
) {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            if app.pm_thread.selected + 1 < app.pm_thread.messages.len() {
                app.pm_thread.selected += 1;
                app.sync_pm_scroll();
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if app.pm_thread.selected > 0 {
                app.pm_thread.selected -= 1;
                app.sync_pm_scroll();
            }
        }
        KeyCode::Char('r') => {
            let uid = app.pm_thread.peer_uid.clone();
            let name = app.pm_thread.peer_name.clone();
            app.composer = Some(ComposerState::open(
                ComposerKind::PmReply,
                hiptty_core::PostAction::ReplyThread { tid: String::new() },
                format!("私信 @{name}"),
                String::new(),
                None,
            ));
            if let Some(c) = app.composer.as_mut() {
                c.pm_uid = Some(uid);
            }
        }
        KeyCode::Char('d') => {
            let uid = app.pm_thread.peer_uid.clone();
            let _ = worker_tx.send(WorkerRequest::PmDelete { uid });
            app.set_toast("正在删除对话", false);
        }
        _ => {}
    }
}

/// First open / full replace for a list page (clears items; keeps `search_query` text).
pub fn request_list_page(
    app: &mut App,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
    kind: ListPageKind,
    page: u32,
) {
    app.list_page.reset_for(kind);
    app.list_page.loading = true;
    if page == 1 {
        app.list_page.selected = 0;
        app.list_page.scroll_lines = 0;
    }
    app.list_page.page = page;
    // After reset_for, search_id is None (fresh search / page open).
    dispatch_list_load(app, worker_tx, kind, page, None);
}

/// Load next page while keeping existing rows, selection, scroll, and search_id.
pub fn request_list_page_append(
    app: &mut App,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
    kind: ListPageKind,
    page: u32,
) {
    app.list_page.kind = Some(kind);
    app.list_page.loading = true;
    app.list_page.error = None;
    dispatch_list_load(app, worker_tx, kind, page, app.list_page.search_id.clone());
}

fn dispatch_list_load(
    app: &mut App,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
    kind: ListPageKind,
    page: u32,
    search_id: Option<String>,
) {
    let request_id = app.allocate_worker_request_id();
    app.list_page.pending_request_id = request_id;
    let _ = worker_tx.send(WorkerRequest::LoadSimpleList {
        kind,
        page,
        fid: app.feed.fid,
        query: app.list_page.search_query.clone(),
        search_id,
        request_id,
    });
}

/// Force-refresh current list page 1 without blanking existing rows.
pub fn refresh_list_page(
    app: &mut App,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
    kind: ListPageKind,
) {
    if app.list_page.loading {
        return;
    }
    app.list_page.kind = Some(kind);
    app.list_page.loading = true;
    app.list_page.error = None;
    // Page-1 replace; keep selection until response arrives.
    let request_id = app.allocate_worker_request_id();
    app.list_page.pending_request_id = request_id;
    let _ = worker_tx.send(WorkerRequest::LoadSimpleList {
        kind,
        page: 1,
        fid: app.feed.fid,
        query: app.list_page.search_query.clone(),
        search_id: None,
        request_id,
    });
}

pub fn request_pm_thread(
    app: &mut App,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
    uid: String,
) {
    let request_id = app.allocate_worker_request_id();
    app.pm_thread.pending_request_id = request_id;
    app.pm_thread.loading = true;
    let _ = worker_tx.send(WorkerRequest::LoadPmThread { uid, request_id });
}

pub fn request_thread_at_post(
    app: &mut App,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
    tid: String,
    pid: String,
) {
    let request_id = app.allocate_worker_request_id();
    app.detail.pending_request_id = request_id;
    app.detail.loading = true;
    app.detail.loading_more = false;
    app.detail.pending_intent = None;
    app.detail.error = None;
    let _ = worker_tx.send(WorkerRequest::LoadThreadAtPost {
        tid,
        pid,
        request_id,
    });
}

pub fn maybe_load_more_list(
    app: &mut App,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
    kind: ListPageKind,
) {
    if app.list_page.loading || kind == ListPageKind::PmList || kind == ListPageKind::Notifications
    {
        return;
    }
    let remaining = app
        .list_page
        .items
        .len()
        .saturating_sub(app.list_page.selected);
    if remaining <= 5 && app.list_page.page < app.list_page.max_page {
        request_list_page_append(app, worker_tx, kind, app.list_page.page + 1);
    }
}

pub fn open_list_selection(app: &mut App, worker_tx: &mpsc::UnboundedSender<WorkerRequest>) {
    let Some(item) = app.list_page.items.get(app.list_page.selected).cloned() else {
        return;
    };
    let kind = app.list_page.kind;
    match app.page {
        Page::PmList => {
            if let Some(uid) = item.uid.clone() {
                let name = item.author.unwrap_or_else(|| uid.clone());
                navigate_to(app, Page::PmThread);
                app.pm_thread.reset(uid.clone(), name);
                request_pm_thread(app, worker_tx, uid);
            }
        }
        Page::Notifications => open_notification_target(app, worker_tx, &item),
        Page::Search | Page::MyThreads | Page::MyReplies | Page::Favorites => {
            open_thread_from_item(app, worker_tx, &item, kind);
        }
        _ => {}
    }
}

fn open_notification_target(
    app: &mut App,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
    item: &ListItem,
) {
    if let (Some(tid), Some(pid)) = (item.tid.clone(), item.pid.clone()) {
        if !pid.is_empty() {
            app.detail = DetailState {
                tid: tid.clone(),
                fid: None,
                title: item.title.clone().unwrap_or_default(),
                reply_count: None,
                view_count: None,
                detail: None,
                selected: 0,
                scroll_top: 0,
                loading: true,
                loading_more: false,
                pending_intent: None,
                pending_request_id: 0,
                loaded_page_lo: 1,
                error: None,
                layout_revision: 0,
                layout: None,
            };
            navigate_to(app, Page::ThreadDetail);
            request_thread_at_post(app, worker_tx, tid, pid);
            return;
        }
    }
    if item.tid.is_some() {
        open_thread_summary(app, worker_tx, list_item_to_thread_summary(item), None);
    } else if let Some(uid) = item.uid.clone() {
        navigate_to(app, Page::PmThread);
        let name = item.author.clone().unwrap_or_else(|| uid.clone());
        app.pm_thread.reset(uid.clone(), name);
        request_pm_thread(app, worker_tx, uid);
    }
}

fn open_thread_from_item(
    app: &mut App,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
    item: &ListItem,
    kind: Option<ListPageKind>,
) {
    if let Some(pid) = item.pid.clone().filter(|p| !p.is_empty()) {
        if let Some(tid) = item.tid.clone().filter(|t| !t.is_empty()) {
            app.detail = DetailState {
                tid: tid.clone(),
                fid: None,
                title: item.title.clone().unwrap_or_default(),
                reply_count: None,
                view_count: None,
                detail: None,
                selected: 0,
                scroll_top: 0,
                loading: true,
                loading_more: false,
                pending_intent: None,
                pending_request_id: 0,
                loaded_page_lo: 1,
                error: None,
                layout_revision: 0,
                layout: None,
            };
            navigate_to(app, Page::ThreadDetail);
            request_thread_at_post(app, worker_tx, tid, pid);
            return;
        }
    }
    let fid = item
        .forum
        .as_ref()
        .and_then(|f| {
            hiptty_core::FORUMS
                .iter()
                .find(|forum| forum.name == f)
                .map(|f| f.id)
        })
        .or_else(|| {
            if kind == Some(ListPageKind::Search) {
                Some(app.feed.fid)
            } else {
                None
            }
        });
    open_thread_summary(app, worker_tx, list_item_to_thread_summary(item), fid);
}

fn open_thread_summary(
    app: &mut App,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
    thread: ThreadSummary,
    fid: Option<u32>,
) {
    let fid = fid.unwrap_or(app.feed.fid);
    app.detail = DetailState::from_summary(&thread, fid);
    navigate_to(app, Page::ThreadDetail);
    crate::event::request_thread_detail(app, worker_tx, 1, DetailLoadIntent::ReplaceTop);
}

pub fn handle_list_response(
    app: &mut App,
    kind: ListPageKind,
    request_id: u64,
    result: Result<hiptty_core::SimpleList, AdapterError>,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
) {
    if app.list_page.kind != Some(kind) {
        return;
    }
    if request_id != app.list_page.pending_request_id {
        return;
    }
    app.list_page.loading = false;
    match result {
        Ok(list) => {
            if list.page <= 1 {
                app.list_page.items = list.items;
                if !app.list_page.items.is_empty() {
                    app.list_page.selected = app
                        .list_page
                        .selected
                        .min(app.list_page.items.len().saturating_sub(1));
                } else {
                    app.list_page.selected = 0;
                    app.list_page.scroll_lines = 0;
                }
            } else {
                app.list_page.items.extend(list.items);
            }
            app.list_page.page = list.page;
            app.list_page.max_page = list.max_page;
            if list.search_id.is_some() {
                app.list_page.search_id = list.search_id;
            }
            app.list_page.error = None;
            app.sync_list_scroll();
            if kind == ListPageKind::PmList {
                prefetch_list_avatars(app, worker_tx);
            }
        }
        Err(err) => {
            if is_auth_required(&err) {
                crate::event::try_auto_relogin(app, worker_tx);
            } else if app.list_page.items.is_empty() {
                app.list_page.error = Some(err.to_string());
            }
            app.set_toast(err.to_string(), true);
        }
    }
}

pub fn handle_worker_extensions(
    app: &mut App,
    response: &WorkerResponse,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
) -> bool {
    match response {
        WorkerResponse::SimpleListLoaded {
            kind,
            request_id,
            result,
        } => {
            handle_list_response(app, *kind, *request_id, result.clone(), worker_tx);
            true
        }
        WorkerResponse::PmThreadLoaded {
            uid,
            request_id,
            result,
        } => {
            if app.pm_thread.peer_uid != *uid {
                return true;
            }
            if *request_id != app.pm_thread.pending_request_id {
                return true;
            }
            app.pm_thread.loading = false;
            match result {
                Ok(list) => {
                    app.pm_thread.messages = list.items.clone();
                    app.pm_thread.error = None;
                    app.sync_pm_scroll();
                }
                Err(err) => {
                    if is_auth_required(err) {
                        crate::event::try_auto_relogin(app, worker_tx);
                    } else {
                        app.pm_thread.error = Some(err.to_string());
                        app.set_toast(err.to_string(), true);
                    }
                }
            }
            true
        }
        WorkerResponse::PmSent { uid, result } => {
            let uid = uid.clone();
            let result = result.clone();
            if app.composer.is_some() {
                if let Some(composer) = app.composer.as_mut() {
                    composer.submitting = false;
                }
            }
            match result {
                Ok(()) => {
                    app.composer = None;
                    app.set_toast("发送成功", false);
                    if app.page == Page::PmThread && app.pm_thread.peer_uid == uid {
                        request_pm_thread(app, worker_tx, uid);
                    }
                }
                Err(err) => {
                    if let Some(composer) = app.composer.as_mut() {
                        composer.error = Some(err.to_string());
                    } else {
                        app.set_toast(err.to_string(), true);
                    }
                }
            }
            true
        }
        WorkerResponse::PmDeleted { uid, result } => {
            match result {
                Ok(()) => {
                    app.set_toast("删除成功", false);
                    if app.page == Page::PmThread && app.pm_thread.peer_uid == *uid {
                        navigate_back(app);
                    } else if app.page == Page::PmList {
                        request_list_page(app, worker_tx, ListPageKind::PmList, 1);
                    }
                }
                Err(err) => app.set_toast(err.to_string(), true),
            }
            true
        }
        WorkerResponse::UnreadChecked {
            session_epoch,
            has_pm,
            has_notifications,
        } => {
            if *session_epoch != app.session_epoch {
                // Stale session (logout/login). in_flight was cleared on epoch bump if needed.
                return true;
            }
            app.unread.check_in_flight = false;
            // Network errors yield None — keep previous unread flags.
            if let Some(v) = has_pm {
                app.unread.has_pm = *v;
            }
            if let Some(v) = has_notifications {
                app.unread.has_notifications = *v;
            }
            true
        }
        WorkerResponse::ThreadAtPostLoaded {
            tid,
            request_id,
            result,
        } => {
            if app.detail.tid != *tid {
                return true;
            }
            if *request_id != app.detail.pending_request_id {
                return true;
            }
            app.detail.loading = false;
            match result {
                Ok(detail) => {
                    app.detail.title = detail.title.clone();
                    if let Some(fid) = detail.fid {
                        app.detail.fid = Some(fid);
                    }
                    let page = detail.page.max(1);
                    app.detail.detail = Some(detail.clone());
                    app.detail.loaded_page_lo = page;
                    app.detail.error = None;
                    app.invalidate_detail_layout();
                    app.clamp_detail_selected();
                    crate::event::prefetch_detail_images(app, worker_tx);
                    app.sync_detail_scroll();
                }
                Err(err) => {
                    if is_auth_required(err) {
                        crate::event::try_auto_relogin(app, worker_tx);
                    } else {
                        app.detail.error = Some(err.to_string());
                        app.set_toast(err.to_string(), true);
                    }
                }
            }
            true
        }
        WorkerResponse::BlacklistLoaded { request_id, result } => {
            if *request_id != app.blacklist_pending_request_id {
                return true;
            }
            match result {
                Ok(list) => app.blacklist_count = list.len(),
                Err(_) => app.blacklist_count = 0,
            }
            true
        }
        _ => false,
    }
}

fn prefetch_list_avatars(app: &mut App, worker_tx: &mpsc::UnboundedSender<WorkerRequest>) {
    let jobs: Vec<_> = app
        .list_page
        .items
        .iter()
        .filter_map(|item| {
            item.avatar_url
                .as_ref()
                .map(|url| hiptty_image::FetchRequest {
                    url: url.clone(),
                    kind: hiptty_image::ImageKind::Avatar,
                })
        })
        .collect();
    crate::event::enqueue_image_jobs(app, jobs, worker_tx);
}

fn is_auth_required(err: &AdapterError) -> bool {
    matches!(err.code(), ErrorCode::AuthRequired | ErrorCode::AuthFailed)
}

pub fn switch_feed_forum(
    app: &mut App,
    fid: u32,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
) {
    if app.feed.fid == fid {
        return;
    }
    app.feed = FeedState::new(fid);
    app.feed.loading = true;
    let request_id = app.allocate_worker_request_id();
    app.feed.pending_request_id = request_id;
    let _ = worker_tx.send(WorkerRequest::LoadThreads {
        fid,
        page: 1,
        request_id,
    });
}

pub fn cycle_default_forum_tab(app: &App, delta: i32) -> u32 {
    let forums = &app.settings.default_forums;
    let len = forums.len() as i32;
    let idx = forums
        .iter()
        .position(|&fid| fid == app.feed.fid)
        .map(|i| i as i32)
        .unwrap_or(-1);
    let next = if idx < 0 {
        if delta > 0 {
            0
        } else {
            (len - 1) as usize
        }
    } else {
        (idx + delta).rem_euclid(len) as usize
    };
    forums[next]
}

#[cfg(test)]
mod tests {
    use super::*;
    use hiptty_core::{AdapterError, AppSettings, ThreadList};
    use std::path::PathBuf;
    use tokio::sync::mpsc;

    fn test_app() -> App {
        App::new(
            AppSettings::default(),
            PathBuf::from("/tmp/hiptty-test"),
            "default".into(),
        )
    }

    #[test]
    fn stale_threads_response_does_not_clear_loading() {
        let mut app = test_app();
        app.feed.fid = 2;
        app.feed.loading = true;
        app.feed.pending_request_id = 10;
        let (tx, _rx) = mpsc::unbounded_channel();
        crate::event::handle_worker_response(
            &mut app,
            WorkerResponse::ThreadsLoaded {
                fid: 2,
                page: 1,
                request_id: 9,
                result: Ok(ThreadList {
                    threads: Vec::new(),
                    page: 1,
                    max_page: 1,
                    uid_hint: None,
                }),
            },
            &tx,
        );
        assert!(app.feed.loading, "stale id must not clear loading");
        assert!(app.toast.is_none());
    }

    #[test]
    fn matching_threads_response_clears_loading() {
        let mut app = test_app();
        app.feed.fid = 2;
        app.feed.loading = true;
        app.feed.pending_request_id = 10;
        let (tx, _rx) = mpsc::unbounded_channel();
        crate::event::handle_worker_response(
            &mut app,
            WorkerResponse::ThreadsLoaded {
                fid: 2,
                page: 1,
                request_id: 10,
                result: Ok(ThreadList {
                    threads: Vec::new(),
                    page: 1,
                    max_page: 1,
                    uid_hint: None,
                }),
            },
            &tx,
        );
        assert!(!app.feed.loading);
    }

    #[test]
    fn stale_unread_epoch_does_not_update_flags() {
        let mut app = test_app();
        app.session_epoch = 2;
        app.unread.has_pm = false;
        app.unread.check_in_flight = true;
        let (tx, _rx) = mpsc::unbounded_channel();
        assert!(handle_worker_extensions(
            &mut app,
            &WorkerResponse::UnreadChecked {
                session_epoch: 1,
                has_pm: Some(true),
                has_notifications: Some(true),
            },
            &tx,
        ));
        assert!(!app.unread.has_pm);
        // Stale epoch must not free a newer in-flight check incorrectly —
        // in_flight was for epoch 2 path only if still set; after epoch mismatch we leave alone.
        // Here check_in_flight stays true only if we didn't clear — which is intentional for
        // concurrent new checks. After bump_session_epoch it is cleared. Simulate stale only:
        assert!(app.unread.check_in_flight);
    }

    #[test]
    fn list_stale_request_id_ignored() {
        let mut app = test_app();
        app.list_page.kind = Some(ListPageKind::PmList);
        app.list_page.loading = true;
        app.list_page.pending_request_id = 5;
        let (tx, _rx) = mpsc::unbounded_channel();
        handle_list_response(
            &mut app,
            ListPageKind::PmList,
            4,
            Err(AdapterError::Network("request timed out".into())),
            &tx,
        );
        assert!(app.list_page.loading);
        assert!(app.toast.is_none());
    }

    #[test]
    fn list_append_preserves_items() {
        use hiptty_core::ListItem;
        let mut app = test_app();
        app.list_page.kind = Some(ListPageKind::MyThreads);
        app.list_page.items = vec![ListItem {
            tid: Some("1".into()),
            pid: None,
            uid: None,
            title: Some("a".into()),
            author: None,
            avatar_url: None,
            forum: None,
            time: None,
            info: None,
            is_new: false,
        }];
        app.list_page.selected = 0;
        app.list_page.scroll_lines = 2;
        app.list_page.page = 1;
        app.list_page.max_page = 3;
        app.list_page.search_id = Some("sid".into());
        let (tx, mut rx) = mpsc::unbounded_channel();
        request_list_page_append(&mut app, &tx, ListPageKind::MyThreads, 2);
        assert_eq!(app.list_page.items.len(), 1, "append must not clear items");
        assert_eq!(app.list_page.selected, 0);
        assert_eq!(app.list_page.scroll_lines, 2);
        assert_eq!(app.list_page.search_id.as_deref(), Some("sid"));
        assert!(app.list_page.loading);
        match rx.try_recv().expect("request sent") {
            WorkerRequest::LoadSimpleList {
                page,
                search_id,
                request_id,
                ..
            } => {
                assert_eq!(page, 2);
                assert_eq!(search_id.as_deref(), Some("sid"));
                assert_eq!(request_id, app.list_page.pending_request_id);
            }
            other => panic!("unexpected request: {other:?}"),
        }
    }

    #[test]
    fn login_success_clears_password_and_security_answer() {
        let mut app = test_app();
        app.login.username = "alice".into();
        app.login.password = "s3cret".into();
        app.login.security_index = 1;
        app.login.security_answer = "pet".into();
        app.nav_stack.push(Page::PmList);
        app.detail.tid = "old".into();
        app.detail.title = "stale".into();
        app.on_login_success("alice".into(), "s3cret");
        assert!(app.login.password.is_empty());
        assert!(app.login.security_answer.is_empty());
        assert!(app.nav_stack.pop().is_none());
        assert!(app.detail.tid.is_empty());
        assert_eq!(app.page, Page::ThreadFeed);
        assert!(app.session.logged_in);
    }

    #[test]
    fn stale_login_result_ignored_after_logout_op() {
        // AutoLogin(1) → Logout(2): ignore LoginResult(1).
        let mut app = test_app();
        app.page = Page::Login;
        app.auth_in_flight = true;
        app.latest_auth_op_id = 2;
        app.pending_logout_op_id = Some(2);
        app.startup_done = true;
        let (tx, _rx) = mpsc::unbounded_channel();
        crate::event::handle_worker_response(
            &mut app,
            WorkerResponse::LoginResult {
                auth_op_id: 1,
                manual: false,
                result: Ok(hiptty_core::SessionInfo {
                    logged_in: true,
                    username: Some("alice".into()),
                    uid: Some("1".into()),
                }),
                username: "alice".into(),
                password_plain: None,
            },
            &tx,
        );
        assert!(!app.session.logged_in);
        assert_eq!(app.page, Page::Login);
        assert!(
            app.auth_in_flight,
            "only matching auth response clears flag"
        );
    }

    #[test]
    fn stale_session_ignored_after_logout_op() {
        // CheckSession(1) → Logout(2): ignore Session(1).
        let mut app = test_app();
        app.page = Page::Login;
        app.startup_done = false;
        app.latest_auth_op_id = 2;
        app.pending_logout_op_id = Some(2);
        let (tx, _rx) = mpsc::unbounded_channel();
        crate::event::handle_worker_response(
            &mut app,
            WorkerResponse::Session {
                auth_op_id: 1,
                info: hiptty_core::SessionInfo {
                    logged_in: true,
                    username: Some("alice".into()),
                    uid: Some("1".into()),
                },
            },
            &tx,
        );
        assert!(!app.session.logged_in);
        assert!(!app.startup_done);
        assert_eq!(app.page, Page::Login);
    }

    #[test]
    fn stale_logged_out_ignored_after_manual_login() {
        // Logout(1) → ManualLogin(2): ignore LoggedOut(1); stay logged in after LoginResult(2).
        let mut app = test_app();
        app.page = Page::Login;
        app.login.password = "pw".into();
        app.login.username = "alice".into();
        app.latest_auth_op_id = 2;
        app.pending_logout_op_id = Some(1); // leftover from logout
        app.auth_in_flight = true;
        app.logout_local_error = Some("should not toast".into());
        let (tx, mut rx) = mpsc::unbounded_channel();
        crate::event::handle_worker_response(
            &mut app,
            WorkerResponse::LoggedOut {
                auth_op_id: 1,
                result: Ok(()),
            },
            &tx,
        );
        assert!(app.auth_in_flight);
        assert_eq!(app.pending_logout_op_id, Some(1));
        assert!(app.toast.is_none());

        crate::event::handle_worker_response(
            &mut app,
            WorkerResponse::LoginResult {
                auth_op_id: 2,
                manual: true,
                result: Ok(hiptty_core::SessionInfo {
                    logged_in: true,
                    username: Some("alice".into()),
                    uid: Some("1".into()),
                }),
                username: "alice".into(),
                password_plain: Some("pw".into()),
            },
            &tx,
        );
        assert!(app.session.logged_in);
        assert_eq!(app.page, Page::ThreadFeed);
        assert!(!app.auth_in_flight);
        assert!(app.pending_logout_op_id.is_none());
        // Login success issues Feed load.
        let _ = rx.try_recv();
    }

    #[test]
    fn logout_clears_nav_detail_pm_and_secrets() {
        let mut app = test_app();
        app.session.logged_in = true;
        app.session.username = Some("alice".into());
        app.page = Page::ThreadDetail;
        app.nav_stack.push(Page::ThreadFeed);
        app.nav_stack.push(Page::PmList);
        app.detail.tid = "tid99".into();
        app.detail.title = "secret thread".into();
        app.pm_thread.peer_uid = "42".into();
        app.pm_thread.peer_name = "bob".into();
        app.list_page.kind = Some(ListPageKind::Favorites);
        app.login.password = "still-here".into();
        app.login.security_answer = "ans".into();
        app.composer = Some(crate::composer::ComposerState::open(
            crate::composer::ComposerKind::Reply,
            hiptty_core::PostAction::ReplyThread { tid: "1".into() },
            "reply".into(),
            String::new(),
            None,
        ));

        app.reset_session_ui_for_logout();

        assert_eq!(app.page, Page::Login);
        assert!(!app.session.logged_in);
        assert!(app.nav_stack.pop().is_none());
        assert!(app.detail.tid.is_empty());
        assert!(app.pm_thread.peer_uid.is_empty());
        assert!(app.list_page.kind.is_none());
        assert!(app.composer.is_none());
        assert!(app.login.password.is_empty());
        assert!(app.login.security_answer.is_empty());
        // :q must not restore old session pages.
        assert!(!crate::nav::navigate_back(&mut app));
        assert_eq!(app.page, Page::Login);
    }
}
