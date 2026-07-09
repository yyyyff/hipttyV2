use std::time::{Duration, Instant};

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use hiptty_widgets::{
    apply_scroll_delta, clamp_scroll_top, clamp_thread_scroll_lines, floor_index_at_line,
    item_index_at_row, list_content_lines, max_scroll_lines, ScrollBar, ScrollBarArrows,
    ITEM_HEIGHT, PM_ITEM_HEIGHT, SIMPLE_ITEM_HEIGHT, WHEEL_LINES,
};
use ratatui::layout::Rect;
use tokio::sync::mpsc;

use crate::app::{App, MouseClickState, Overlay, Page};
use crate::event::{
    maybe_load_more, maybe_load_more_detail, open_thread_detail, prefetch_detail_viewport_images,
};
use crate::handlers::{
    activate_main_menu_item, activate_settings_row, maybe_load_more_list, open_list_selection,
    switch_feed_forum,
};
use crate::list_page::ListPageKind;
use crate::nav::navigate_to;
use crate::worker::WorkerRequest;

const DOUBLE_CLICK: Duration = Duration::from_millis(450);

pub fn handle_mouse(
    app: &mut App,
    event: MouseEvent,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
) {
    if app.overlay == Overlay::MainMenu {
        handle_main_menu_mouse(app, event, worker_tx);
        return;
    }
    if app.overlay == Overlay::ForumPicker {
        handle_forum_picker_mouse(app, event, worker_tx);
        return;
    }
    if app.overlay == Overlay::Settings {
        handle_settings_mouse(app, event);
        return;
    }
    if app.overlay != Overlay::None {
        return;
    }

    update_forum_tab_hover(app, event);

    if let Some(action) = title_bar_action(app, event) {
        apply_title_action(app, action, worker_tx);
        return;
    }

    if let Some(offset) = scrollbar_action(app, event, current_scroll_offset(app)) {
        apply_scroll_offset(app, offset, worker_tx);
        return;
    }

    if matches!(event.kind, MouseEventKind::Moved) {
        update_selection_from_pointer(app, event.row, event.column, worker_tx);
        return;
    }

    if matches!(event.kind, MouseEventKind::Down(MouseButton::Left)) {
        handle_content_click(app, event.row, event.column, worker_tx);
    }
}

#[derive(Debug, Clone, Copy)]
enum TitleAction {
    Notifications,
    PmList,
    ForumTab(usize),
}

fn handle_forum_picker_mouse(
    app: &mut App,
    event: MouseEvent,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
) {
    update_forum_tab_hover(app, event);
    if let Some(index) = forum_picker_index_at(app, event.column, event.row) {
        app.forum_picker_selected = index;
        if matches!(event.kind, MouseEventKind::Down(MouseButton::Left)) {
            if let Some(&fid) = app.forum_picker_fids().get(index) {
                app.overlay = Overlay::None;
                switch_feed_forum(app, fid, worker_tx);
            }
        }
    }
}

fn forum_picker_index_at(app: &App, column: u16, row: u16) -> Option<usize> {
    app.forum_picker_hits
        .iter()
        .find(|hit| point_in(column, row, hit.area))
        .map(|hit| hit.entry_index)
}

fn update_forum_tab_hover(app: &mut App, event: MouseEvent) {
    if app.page != Page::ThreadFeed {
        app.forum_tab_hover = None;
        return;
    }
    app.forum_tab_hover = None;
    for (i, tab) in app.title_bar_hits.forum_tabs.tabs.iter().enumerate() {
        if let Some(r) = tab {
            if point_in(event.column, event.row, *r) {
                app.forum_tab_hover = Some(i);
                return;
            }
        }
    }
}

fn handle_settings_mouse(app: &mut App, event: MouseEvent) {
    if let Some(index) = settings_index_at(app, event.column, event.row) {
        app.overlay_state.settings_selected = index;
        if matches!(event.kind, MouseEventKind::Down(MouseButton::Left)) {
            activate_settings_row(app, index);
        }
    }
}

fn settings_index_at(app: &App, column: u16, row: u16) -> Option<usize> {
    app.settings_hits
        .iter()
        .enumerate()
        .find(|(_, hit)| point_in(column, row, **hit))
        .map(|(i, _)| i)
}

fn handle_main_menu_mouse(
    app: &mut App,
    event: MouseEvent,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
) {
    if let Some(index) = main_menu_index_at(app, event.column, event.row) {
        app.overlay_state.main_menu_selected = index;
        if matches!(event.kind, MouseEventKind::Down(MouseButton::Left)) {
            activate_main_menu_item(app, index, worker_tx);
        }
    }
}

fn main_menu_index_at(app: &App, column: u16, row: u16) -> Option<usize> {
    app.main_menu_hits
        .iter()
        .enumerate()
        .find(|(_, hit)| point_in(column, row, **hit))
        .map(|(i, _)| i)
}

fn title_bar_action(app: &App, event: MouseEvent) -> Option<TitleAction> {
    if !point_in(event.column, event.row, app.title_bar_area) {
        return None;
    }
    if let Some(r) = app.title_bar_hits.notifications {
        if point_in(event.column, event.row, r) {
            return Some(TitleAction::Notifications);
        }
    }
    if let Some(r) = app.title_bar_hits.pm {
        if point_in(event.column, event.row, r) {
            return Some(TitleAction::PmList);
        }
    }
    if app.page == Page::ThreadFeed
        && matches!(event.kind, MouseEventKind::Down(MouseButton::Left))
    {
        if let Some(index) = app.forum_tab_hover {
            return Some(TitleAction::ForumTab(index));
        }
    }
    None
}

fn apply_title_action(
    app: &mut App,
    action: TitleAction,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
) {
    use crate::handlers::request_list_page;
    match action {
        TitleAction::Notifications => {
            navigate_to(app, Page::Notifications);
            request_list_page(app, worker_tx, ListPageKind::Notifications, 1);
        }
        TitleAction::PmList => {
            navigate_to(app, Page::PmList);
            request_list_page(app, worker_tx, ListPageKind::PmList, 1);
        }
        TitleAction::ForumTab(index) => {
            if let Some(&fid) = app.settings.default_forums.get(index) {
                app.overlay = Overlay::None;
                switch_feed_forum(app, fid, worker_tx);
            }
        }
    }
}

fn current_scroll_offset(app: &App) -> u16 {
    match app.page {
        Page::ThreadFeed => app.feed.scroll_lines,
        Page::ThreadDetail => app.detail.scroll_top,
        Page::PmList
        | Page::Notifications
        | Page::Search
        | Page::MyThreads
        | Page::MyReplies
        | Page::Favorites => app.list_page.scroll_lines,
        Page::PmThread => app.pm_thread.scroll_lines,
        _ => 0,
    }
}

fn scrollbar_action(app: &mut App, event: MouseEvent, current_offset: u16) -> Option<u16> {
    let chrome = app.scroll_chrome?;

    match event.kind {
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
            return wheel_over_content(event, chrome, current_offset);
        }
        MouseEventKind::Down(MouseButton::Left)
        | MouseEventKind::Drag(MouseButton::Left)
        | MouseEventKind::Up(MouseButton::Left) => {
            if !chrome.shown || !point_in(event.column, event.row, chrome.bar) {
                return None;
            }
        }
        _ => return None,
    }

    if !chrome.shown {
        return None;
    }

    let scrollbar = ScrollBar::vertical(chrome.lengths())
        .arrows(ScrollBarArrows::None)
        .offset(current_offset as usize)
        .scroll_step(1);

    scrollbar
        .handle_mouse_event(chrome.bar, event, &mut app.scrollbar_interaction)
        .map(|cmd| chrome.apply_command(cmd))
}

fn wheel_over_content(
    event: MouseEvent,
    chrome: hiptty_widgets::ScrollChrome,
    current_offset: u16,
) -> Option<u16> {
    let delta = match event.kind {
        MouseEventKind::ScrollUp => -WHEEL_LINES,
        MouseEventKind::ScrollDown => WHEEL_LINES,
        _ => return None,
    };
    if !point_in(event.column, event.row, chrome.content) {
        return None;
    }
    Some(apply_scroll_delta(
        current_offset,
        delta,
        chrome.max_offset(),
    ))
}

fn apply_scroll_offset(
    app: &mut App,
    offset: u16,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
) {
    match app.page {
        Page::ThreadFeed => {
            let max = list_max_offset(app);
            app.feed.scroll_lines = clamp_thread_scroll_lines(offset, max);
        }
        Page::ThreadDetail => {
            if let Some(detail) = app.detail.detail.as_ref() {
                let width = app.content_width();
                let viewport = app.scroll_viewport_height();
                let palette = app.palette();
                let posts = detail.posts.as_slice();
                app.detail.scroll_top = {
                    let images = app.images();
                    clamp_scroll_top(offset, posts, width, viewport, palette, images)
                };
                maybe_load_more_detail(app, worker_tx);
                prefetch_detail_viewport_images(app, worker_tx);
            }
        }
        Page::PmList
        | Page::Notifications
        | Page::Search
        | Page::MyThreads
        | Page::MyReplies
        | Page::Favorites => {
            let item_h = list_item_height(app.page);
            let max = max_scroll_lines(
                list_content_lines(app.list_page.items.len(), item_h),
                app.scroll_viewport_height(),
            );
            app.list_page.scroll_lines = clamp_thread_scroll_lines(offset, max);
            if let Some(kind) = app.list_page.kind {
                maybe_load_more_list(app, worker_tx, kind);
            }
        }
        Page::PmThread => {
            let max = max_scroll_lines(
                list_content_lines(app.pm_thread.messages.len(), PM_ITEM_HEIGHT),
                app.scroll_viewport_height(),
            );
            app.pm_thread.scroll_lines = clamp_thread_scroll_lines(offset, max);
        }
        _ => {}
    }
}

fn update_selection_from_pointer(
    app: &mut App,
    row: u16,
    column: u16,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
) {
    match app.page {
        Page::ThreadFeed => update_feed_selection_from_pointer(app, row, column, worker_tx),
        Page::ThreadDetail => update_detail_selection_from_pointer(app, row, column),
        Page::PmList
        | Page::Notifications
        | Page::Search
        | Page::MyThreads
        | Page::MyReplies
        | Page::Favorites => {
            update_list_page_selection_from_pointer(app, row, column, worker_tx);
        }
        Page::PmThread => update_pm_thread_selection_from_pointer(app, row, column),
        _ => {}
    }
}

fn update_detail_selection_from_pointer(app: &mut App, row: u16, column: u16) {
    let Some(rel_y) = content_relative_y(app, row, column) else {
        return;
    };
    let Some(detail) = app.detail.detail.as_ref() else {
        return;
    };
    let line = app.detail.scroll_top.saturating_add(rel_y);
    let width = app.content_width();
    let palette = app.palette();
    let idx = {
        let images = app.images();
        floor_index_at_line(line, &detail.posts, width, palette, images)
    };
    if idx >= detail.posts.len() || idx == app.detail.selected {
        return;
    }
    app.detail.selected = idx;
}

fn update_feed_selection_from_pointer(
    app: &mut App,
    row: u16,
    column: u16,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
) {
    let Some(rel_y) = content_relative_y(app, row, column) else {
        return;
    };
    let idx = item_index_at_row(rel_y, app.feed.scroll_lines, ITEM_HEIGHT);
    if idx >= app.feed.threads.len() || idx == app.feed.selected {
        return;
    }
    app.feed.selected = idx;
    app.sync_feed_scroll();
    maybe_load_more(app, worker_tx);
}

fn update_list_page_selection_from_pointer(
    app: &mut App,
    row: u16,
    column: u16,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
) {
    let Some(rel_y) = content_relative_y(app, row, column) else {
        return;
    };
    let item_h = list_item_height(app.page);
    let idx = item_index_at_row(rel_y, app.list_page.scroll_lines, item_h);
    if idx >= app.list_page.items.len() || idx == app.list_page.selected {
        return;
    }
    app.list_page.selected = idx;
    app.sync_list_scroll();
    if let Some(kind) = app.list_page.kind {
        maybe_load_more_list(app, worker_tx, kind);
    }
}

fn update_pm_thread_selection_from_pointer(app: &mut App, row: u16, column: u16) {
    let Some(rel_y) = content_relative_y(app, row, column) else {
        return;
    };
    let idx = item_index_at_row(rel_y, app.pm_thread.scroll_lines, PM_ITEM_HEIGHT);
    if idx >= app.pm_thread.messages.len() || idx == app.pm_thread.selected {
        return;
    }
    app.pm_thread.selected = idx;
    app.sync_pm_scroll();
}

fn content_relative_y(app: &App, row: u16, column: u16) -> Option<u16> {
    let chrome = app.scroll_chrome?;
    if !point_in(column, row, chrome.content) {
        return None;
    }
    Some(row.saturating_sub(chrome.content.y))
}

fn list_max_offset(app: &App) -> u16 {
    max_scroll_lines(
        list_content_lines(app.feed.threads.len(), ITEM_HEIGHT),
        app.scroll_viewport_height(),
    )
}

fn list_item_height(page: Page) -> u16 {
    match page {
        Page::PmList | Page::Notifications => SIMPLE_ITEM_HEIGHT,
        _ => ITEM_HEIGHT,
    }
}

fn handle_content_click(
    app: &mut App,
    row: u16,
    column: u16,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
) {
    let Some(chrome) = app.scroll_chrome else {
        return;
    };
    if !point_in(column, row, chrome.content) {
        return;
    }

    // Detail: scroll only via wheel/scrollbar; clicks do nothing.
    if app.page == Page::ThreadDetail {
        return;
    }

    let rel_y = row.saturating_sub(chrome.content.y);
    let double = is_double_click(app, column, row);

    match app.page {
        Page::ThreadFeed => {
            let idx = item_index_at_row(rel_y, app.feed.scroll_lines, ITEM_HEIGHT);
            if idx >= app.feed.threads.len() {
                return;
            }
            app.feed.selected = idx;
            app.sync_feed_scroll();
            if double {
                if let Some(thread) = app.feed.threads.get(idx).cloned() {
                    open_thread_detail(app, &thread, worker_tx);
                }
            }
        }
        Page::PmList | Page::Notifications => {
            let idx = item_index_at_row(rel_y, app.list_page.scroll_lines, SIMPLE_ITEM_HEIGHT);
            if idx >= app.list_page.items.len() {
                return;
            }
            app.list_page.selected = idx;
            app.sync_list_scroll();
            if double {
                open_list_selection(app, worker_tx);
            }
        }
        Page::Search | Page::MyThreads | Page::MyReplies | Page::Favorites => {
            let idx = item_index_at_row(rel_y, app.list_page.scroll_lines, ITEM_HEIGHT);
            if idx >= app.list_page.items.len() {
                return;
            }
            app.list_page.selected = idx;
            app.sync_list_scroll();
            if double {
                open_list_selection(app, worker_tx);
            }
        }
        Page::PmThread => {
            let idx = item_index_at_row(rel_y, app.pm_thread.scroll_lines, PM_ITEM_HEIGHT);
            if idx < app.pm_thread.messages.len() {
                app.pm_thread.selected = idx;
                app.sync_pm_scroll();
            }
        }
        _ => {}
    }

    record_click(app, column, row);
}

fn is_double_click(app: &App, column: u16, row: u16) -> bool {
    let Some(prev) = app.last_click else {
        return false;
    };
    prev.page == app.page
        && prev.column == column
        && prev.row == row
        && prev.at.elapsed() <= DOUBLE_CLICK
}

fn record_click(app: &mut App, column: u16, row: u16) {
    app.last_click = Some(MouseClickState {
        at: Instant::now(),
        column,
        row,
        page: app.page,
    });
}

fn point_in(column: u16, row: u16, area: Rect) -> bool {
    column >= area.x
        && column < area.x.saturating_add(area.width)
        && row >= area.y
        && row < area.y.saturating_add(area.height)
}

pub fn install_scroll_chrome(
    app: &mut App,
    full_content: Rect,
    content_len: u16,
    offset: u16,
) -> Rect {
    let (content, bar) = hiptty_widgets::split_content_scrollbar(full_content);
    let viewport = content.height;
    let shown = content_len > viewport && bar.width > 0;
    app.scroll_chrome = Some(hiptty_widgets::ScrollChrome {
        content,
        bar,
        content_len,
        viewport_len: viewport,
        offset,
        shown,
    });
    content
}
