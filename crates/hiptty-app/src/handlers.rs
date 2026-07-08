use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use hiptty_core::{list_item_to_thread_summary, AdapterError, ErrorCode, ListItem, ThreadSummary};
use hiptty_widgets::MAIN_MENU_ITEMS;
use tokio::sync::mpsc;

use crate::app::{App, DetailFetchMode, DetailState, FeedState, Overlay, Page};
use crate::commands::execute_command;
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
            app.overlay = Overlay::CommandBar;
            true
        }
        KeyCode::Esc => {
            if app.page == Page::ThreadFeed {
                app.overlay_state.main_menu_selected = 0;
                app.overlay = Overlay::MainMenu;
            } else if !navigate_back(app) {
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
            let _ = worker_tx.send(WorkerRequest::LoadBlacklist);
        }
        6 => app.quit = true,
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
        KeyCode::Enter => match app.overlay_state.settings_selected {
            0..=2 => {
                let idx = app.overlay_state.settings_selected;
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
        },
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
    match key.code {
        KeyCode::Esc => app.overlay = Overlay::None,
        KeyCode::Enter => {
            let input = app.overlay_state.command_input.clone();
            execute_command(app, &input, worker_tx);
        }
        KeyCode::Backspace => {
            app.overlay_state.command_input.pop();
        }
        KeyCode::Char(c) => app.overlay_state.command_input.push(c),
        _ => {}
    }
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
            app.composer.as_mut().map(|c| c.pm_uid = Some(uid));
        }
        KeyCode::Char('d') => {
            let uid = app.pm_thread.peer_uid.clone();
            let _ = worker_tx.send(WorkerRequest::PmDelete { uid });
            app.set_toast("正在删除对话", false);
        }
        _ => {}
    }
}

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
    let _ = worker_tx.send(WorkerRequest::LoadSimpleList {
        kind,
        page,
        fid: app.feed.fid,
        query: app.list_page.search_query.clone(),
        search_id: app.list_page.search_id.clone(),
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
        request_list_page(app, worker_tx, kind, app.list_page.page + 1);
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
                let _ = worker_tx.send(WorkerRequest::LoadPmThread { uid });
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
                pending_fetch: None,
                error: None,
            };
            navigate_to(app, Page::ThreadDetail);
            let _ = worker_tx.send(WorkerRequest::LoadThreadAtPost { tid, pid });
            return;
        }
    }
    if item.tid.is_some() {
        open_thread_summary(app, worker_tx, list_item_to_thread_summary(item), None);
    } else if let Some(uid) = item.uid.clone() {
        navigate_to(app, Page::PmThread);
        let name = item.author.clone().unwrap_or_else(|| uid.clone());
        app.pm_thread.reset(uid.clone(), name);
        let _ = worker_tx.send(WorkerRequest::LoadPmThread { uid });
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
                pending_fetch: None,
                error: None,
            };
            navigate_to(app, Page::ThreadDetail);
            let _ = worker_tx.send(WorkerRequest::LoadThreadAtPost { tid, pid });
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
    crate::event::request_thread_detail(app, worker_tx, 1, DetailFetchMode::Replace);
}

pub fn handle_list_response(
    app: &mut App,
    kind: ListPageKind,
    result: Result<hiptty_core::SimpleList, AdapterError>,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
) {
    if app.list_page.kind != Some(kind) {
        return;
    }
    app.list_page.loading = false;
    match result {
        Ok(list) => {
            if list.page <= 1 {
                app.list_page.items = list.items;
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
        WorkerResponse::SimpleListLoaded { kind, result } => {
            handle_list_response(app, *kind, result.clone(), worker_tx);
            true
        }
        WorkerResponse::PmThreadLoaded { uid, result } => {
            if app.pm_thread.peer_uid != *uid {
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
                    if is_auth_required(&err) {
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
                        let _ = worker_tx.send(WorkerRequest::LoadPmThread { uid });
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
            has_pm,
            has_notifications,
        } => {
            app.unread.has_pm = *has_pm;
            app.unread.has_notifications = *has_notifications;
            true
        }
        WorkerResponse::ThreadAtPostLoaded { tid, result } => {
            if app.detail.tid != *tid {
                return true;
            }
            app.detail.loading = false;
            match result {
                Ok(detail) => {
                    app.detail.title = detail.title.clone();
                    if let Some(fid) = detail.fid {
                        app.detail.fid = Some(fid);
                    }
                    app.detail.detail = Some(detail.clone());
                    app.detail.error = None;
                    crate::event::prefetch_detail_images(app, worker_tx);
                    app.sync_detail_scroll();
                }
                Err(err) => {
                    if is_auth_required(&err) {
                        crate::event::try_auto_relogin(app, worker_tx);
                    } else {
                        app.detail.error = Some(err.to_string());
                        app.set_toast(err.to_string(), true);
                    }
                }
            }
            true
        }
        WorkerResponse::BlacklistLoaded { result } => {
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

pub fn switch_feed_forum(app: &mut App, fid: u32, worker_tx: &mpsc::UnboundedSender<WorkerRequest>) {
    if app.feed.fid == fid {
        return;
    }
    app.feed = FeedState::new(fid);
    app.feed.loading = true;
    let _ = worker_tx.send(WorkerRequest::LoadThreads { fid, page: 1 });
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
