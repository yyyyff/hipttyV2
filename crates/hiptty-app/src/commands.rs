use hiptty_core::forum_name;

use crate::app::{App, Overlay, Page};
use crate::handlers::request_list_page;
use crate::list_page::ListPageKind;
use crate::nav::navigate_to;
use crate::worker::WorkerRequest;
use tokio::sync::mpsc;

pub fn execute_command(app: &mut App, raw: &str, worker_tx: &mpsc::UnboundedSender<WorkerRequest>) {
    let input = raw.trim();
    if input.is_empty() {
        return;
    }
    let parts: Vec<&str> = input.split_whitespace().collect();
    match parts[0] {
        "exit" | "quit" => {
            app.quit = true;
        }
        "q" => {
            app.overlay = Overlay::None;
            crate::nav::navigate_back(app);
        }
        "login" => {
            app.overlay = Overlay::None;
            app.page = Page::Login;
        }
        "logout" => {
            let _ = crate::config::clear_credentials(&app.credentials_path);
            app.session.logged_in = false;
            app.session.username = None;
            app.page = Page::Login;
            app.set_toast("已登出", false);
        }
        "pm" => open_page(app, Page::PmList, worker_tx),
        "notif" | "notifications" => open_page(app, Page::Notifications, worker_tx),
        "search" if parts.len() >= 2 => {
            app.list_page.search_query = parts[1..].join(" ");
            open_page(app, Page::Search, worker_tx);
        }
        _ => app.set_toast(format!("未知命令: {input}"), true),
    }
    app.overlay = Overlay::None;
}

fn open_page(app: &mut App, page: Page, worker_tx: &mpsc::UnboundedSender<WorkerRequest>) {
    let kind = match page {
        Page::PmList => ListPageKind::PmList,
        Page::Notifications => ListPageKind::Notifications,
        Page::Search => ListPageKind::Search,
        Page::MyThreads => ListPageKind::MyThreads,
        Page::MyReplies => ListPageKind::MyReplies,
        Page::Favorites => ListPageKind::Favorites,
        _ => return,
    };
    navigate_to(app, page);
    request_list_page(app, worker_tx, kind, 1);
}

pub fn command_hints() -> &'static str {
    ":exit :q :login :logout :pm :notif :search <词>"
}

pub fn settings_row_label(app: &App, row: usize) -> String {
    match row {
        0 => format!(
            "默认版块 1   [{}]",
            forum_name(app.settings.default_forums[0]).unwrap_or("?")
        ),
        1 => format!(
            "默认版块 2   [{}]",
            forum_name(app.settings.default_forums[1]).unwrap_or("?")
        ),
        2 => format!(
            "默认版块 3   [{}]",
            forum_name(app.settings.default_forums[2]).unwrap_or("?")
        ),
        3 => format!("黑名单       [{} 人]", app.blacklist_count),
        _ => String::new(),
    }
}
