use hiptty_core::forum_name;
use hiptty_render::str_width;

use crate::app::{App, Overlay, Page};
use crate::composer::{
    quote_header, quote_post_action, reply_floor_header, reply_post_action, reply_thread_action,
    ComposerKind, ComposerState,
};
use crate::event::refresh_feed;
use crate::handlers::{refresh_list_page, request_list_page};
use crate::list_page::ListPageKind;
use crate::nav::navigate_to;
use crate::worker::WorkerRequest;
use tokio::sync::mpsc;

/// Detail-page reply family shown first in the suggestion strip.
const DETAIL_REPLY_USAGES: &[&str] = &["r 回帖", "r#N 回复楼", "rr#N 引用楼"];

/// One registered `:` command.
#[derive(Debug, Clone, Copy)]
pub struct CommandSpec {
    /// Canonical name (what Tab completes to).
    pub name: &'static str,
    /// Alternate spellings accepted by the executor.
    pub aliases: &'static [&'static str],
    /// Short usage fragment shown in the status-bar suggestion strip.
    pub usage: &'static str,
    /// Human label for help / unknown-command hints.
    pub summary: &'static str,
    /// When true, arguments after the verb are required (e.g. `search`).
    pub requires_args: bool,
    /// Hidden on thread detail (nav jumps, login/logout, …).
    pub hide_on_detail: bool,
    /// Only shown / meaningful on thread detail (e.g. plain `r` 回帖).
    pub detail_only: bool,
}

/// Global command catalog (order = suggestion order).
pub const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "q",
        aliases: &[],
        usage: "q",
        summary: "返回上级",
        requires_args: false,
        hide_on_detail: false,
        detail_only: false,
    },
    CommandSpec {
        name: "r",
        aliases: &[],
        usage: "r",
        summary: "回帖",
        requires_args: false,
        hide_on_detail: false,
        detail_only: true,
    },
    CommandSpec {
        name: "pm",
        aliases: &[],
        usage: "pm",
        summary: "私信",
        requires_args: false,
        hide_on_detail: true,
        detail_only: false,
    },
    CommandSpec {
        name: "notif",
        aliases: &["notifications"],
        usage: "notif",
        summary: "通知",
        requires_args: false,
        hide_on_detail: true,
        detail_only: false,
    },
    CommandSpec {
        name: "search",
        aliases: &[],
        usage: "search <词>",
        summary: "搜索",
        requires_args: true,
        hide_on_detail: true,
        detail_only: false,
    },
    CommandSpec {
        name: "my",
        aliases: &["threads"],
        usage: "my",
        summary: "我的帖子",
        requires_args: false,
        hide_on_detail: true,
        detail_only: false,
    },
    CommandSpec {
        name: "replies",
        aliases: &[],
        usage: "replies",
        summary: "我的回复",
        requires_args: false,
        hide_on_detail: true,
        detail_only: false,
    },
    CommandSpec {
        name: "fav",
        aliases: &["favorites"],
        usage: "fav",
        summary: "我的收藏",
        requires_args: false,
        hide_on_detail: true,
        detail_only: false,
    },
    CommandSpec {
        name: "refresh",
        aliases: &[],
        usage: "refresh",
        summary: "刷新当前页",
        requires_args: false,
        hide_on_detail: false,
        detail_only: false,
    },
    CommandSpec {
        name: "login",
        aliases: &[],
        usage: "login",
        summary: "登录",
        requires_args: false,
        hide_on_detail: true,
        detail_only: false,
    },
    CommandSpec {
        name: "logout",
        aliases: &[],
        usage: "logout",
        summary: "登出",
        requires_args: false,
        hide_on_detail: true,
        detail_only: false,
    },
    CommandSpec {
        name: "exit",
        aliases: &["quit"],
        usage: "exit",
        summary: "退出",
        requires_args: false,
        hide_on_detail: false,
        detail_only: false,
    },
];

fn command_visible_on(spec: &CommandSpec, page: Page) -> bool {
    match page {
        Page::ThreadDetail => !spec.hide_on_detail,
        _ => !spec.detail_only,
    }
}

fn catalog_for(page: Page) -> impl Iterator<Item = &'static CommandSpec> {
    COMMANDS.iter().filter(move |s| command_visible_on(s, page))
}

pub fn execute_command(app: &mut App, raw: &str, worker_tx: &mpsc::UnboundedSender<WorkerRequest>) {
    let input = raw.trim();
    if input.is_empty() {
        close_command_bar(app);
        return;
    }
    let parts: Vec<&str> = input.split_whitespace().collect();
    let verb = parts[0];

    // Detail floor commands: r#3 / rr#3 (must run before catalog resolve).
    if let Some(floor_cmd) = parse_floor_command(verb) {
        execute_floor_command(app, floor_cmd, worker_tx);
        close_command_bar(app);
        return;
    }
    if is_incomplete_floor_prefix(verb) {
        let msg = if app.page == Page::ThreadDetail {
            "用法: r 回帖  ·  r#N 回复楼  ·  rr#N 引用楼"
        } else {
            "r#N / rr#N 仅在详情页可用"
        };
        app.set_toast(msg, true);
        close_command_bar(app);
        return;
    }

    let Some(spec) = resolve_command(verb) else {
        app.set_toast(suggest_unknown(verb, app.page), true);
        close_command_bar(app);
        return;
    };
    match spec.name {
        "exit" => app.quit = true,
        "q" => {
            close_command_bar(app);
            crate::nav::navigate_back(app);
            return;
        }
        "r" => {
            close_command_bar(app);
            open_thread_reply(app);
            return;
        }
        "login" => {
            close_command_bar(app);
            app.page = Page::Login;
            return;
        }
        "logout" => {
            // Clear local creds + worker session. Never show bare "已登出" over an error.
            let local_err = crate::config::clear_credentials(&app.credentials_path)
                .err()
                .map(|e| format!("清除本地凭证失败: {e}"));
            app.bump_session_epoch();
            let send_ok = worker_tx.send(WorkerRequest::Logout).is_ok();
            app.session.logged_in = false;
            app.session.username = None;
            app.session.uid = None;
            app.unread = crate::app::UnreadState::default();
            app.page = Page::Login;
            app.logout_local_error = local_err.clone();
            if send_ok {
                // Final toast comes from LoggedOut (or retains local error if any).
                app.logout_pending = true;
                if let Some(msg) = local_err {
                    app.set_toast(msg, true);
                }
            } else if let Some(msg) = local_err {
                app.logout_pending = false;
                app.set_toast(format!("{msg}；且无法请求后台清理会话"), true);
            } else {
                app.logout_pending = false;
                app.set_toast("已登出，但无法请求后台清理会话", true);
            }
        }
        "pm" => open_page(app, Page::PmList, worker_tx),
        "notif" => open_page(app, Page::Notifications, worker_tx),
        "my" => open_page(app, Page::MyThreads, worker_tx),
        "replies" => open_page(app, Page::MyReplies, worker_tx),
        "fav" => open_page(app, Page::Favorites, worker_tx),
        "refresh" => {
            close_command_bar(app);
            refresh_current_page(app, worker_tx);
            return;
        }
        "search" => {
            if parts.len() < 2 {
                app.set_toast("用法: search <关键词>", true);
                close_command_bar(app);
                return;
            }
            app.list_page.search_query = parts[1..].join(" ");
            open_page(app, Page::Search, worker_tx);
        }
        _ => app.set_toast(format!("未知命令: {input}"), true),
    }
    close_command_bar(app);
}

fn open_thread_reply(app: &mut App) {
    if app.page != Page::ThreadDetail {
        app.set_toast("r 回帖仅在详情页可用", true);
        return;
    }
    if app.detail.tid.is_empty() {
        app.set_toast("当前没有可回复的主题", true);
        return;
    }
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

fn close_command_bar(app: &mut App) {
    app.overlay = Overlay::None;
    app.overlay_state.command_input.clear();
    app.overlay_state.command_cursor = 0;
}

fn resolve_command(verb: &str) -> Option<&'static CommandSpec> {
    let verb = verb.to_ascii_lowercase();
    COMMANDS.iter().find(|spec| {
        spec.name == verb || spec.aliases.iter().any(|a| a.eq_ignore_ascii_case(&verb))
    })
}

fn suggest_unknown(verb: &str, page: Page) -> String {
    let verb = verb.to_ascii_lowercase();
    let mut matches: Vec<&str> = catalog_for(page)
        .filter(|s| s.name.starts_with(&verb) || s.aliases.iter().any(|a| a.starts_with(&verb)))
        .map(|s| s.usage)
        .collect();
    if page == Page::ThreadDetail && (verb.starts_with('r') || verb.is_empty()) {
        for u in DETAIL_REPLY_USAGES {
            if !matches.contains(u) {
                matches.push(u);
            }
        }
    }
    if matches.is_empty() {
        format!("未知命令: {verb}  (Tab 查看可用命令)")
    } else {
        format!("未知命令: {verb}  是否: {}", matches.join(" · "))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FloorCommand {
    /// `r#N` — ReplyPost (reppost) to floor N.
    Reply(u32),
    /// `rr#N` — QuotePost (repquote) floor N.
    Quote(u32),
}

fn parse_floor_command(verb: &str) -> Option<FloorCommand> {
    let v = verb.to_ascii_lowercase();
    if let Some(rest) = v.strip_prefix("rr#") {
        let n: u32 = rest.parse().ok()?;
        if n == 0 {
            return None;
        }
        return Some(FloorCommand::Quote(n));
    }
    if let Some(rest) = v.strip_prefix("r#") {
        let n: u32 = rest.parse().ok()?;
        if n == 0 {
            return None;
        }
        return Some(FloorCommand::Reply(n));
    }
    None
}

/// Incomplete floor targets (`r#` / `rr` / `rr#`) — bare `r` is a real command.
fn is_incomplete_floor_prefix(verb: &str) -> bool {
    let v = verb.to_ascii_lowercase();
    matches!(v.as_str(), "rr" | "r#" | "rr#")
        || (v.starts_with("r#")
            && v.len() > 2
            && v["r#".len()..].chars().all(|c| c.is_ascii_digit())
            && parse_floor_command(&v).is_none())
        || (v.starts_with("rr#")
            && v.len() > 3
            && v["rr#".len()..].chars().all(|c| c.is_ascii_digit())
            && parse_floor_command(&v).is_none())
}

fn execute_floor_command(
    app: &mut App,
    cmd: FloorCommand,
    worker_tx: &mpsc::UnboundedSender<WorkerRequest>,
) {
    if app.page != Page::ThreadDetail {
        app.set_toast("r#N / rr#N 仅在详情页可用", true);
        return;
    }
    let floor = match cmd {
        FloorCommand::Reply(n) | FloorCommand::Quote(n) => n,
    };
    let Some((idx, post)) = find_loaded_floor(app, floor) else {
        app.set_toast(format!("找不到 #{floor} 楼（可能尚未加载到本地）"), true);
        return;
    };

    app.detail.selected = idx;
    app.sync_detail_scroll();

    let tid = app.detail.tid.clone();
    match cmd {
        FloorCommand::Reply(_) => {
            let action = reply_post_action(&tid, &post.pid);
            app.composer = Some(ComposerState::preparing(
                ComposerKind::Reply,
                action.clone(),
                reply_floor_header(&post),
            ));
            let _ = worker_tx.send(WorkerRequest::PreparePost { action });
        }
        FloorCommand::Quote(_) => {
            let action = quote_post_action(&tid, &post.pid);
            app.composer = Some(ComposerState::preparing(
                ComposerKind::Quote,
                action.clone(),
                quote_header(&post),
            ));
            let _ = worker_tx.send(WorkerRequest::PreparePost { action });
        }
    }
}

fn find_loaded_floor(app: &App, floor: u32) -> Option<(usize, hiptty_core::Post)> {
    let detail = app.detail.detail.as_ref()?;
    detail
        .posts
        .iter()
        .enumerate()
        .find(|(_, p)| p.floor == floor)
        .map(|(i, p)| (i, p.clone()))
}

/// Suggestion strip for the status-bar right side while in command mode.
pub fn command_suggestion_strip(input: &str, max_cols: usize, page: Page) -> String {
    if max_cols == 0 {
        return String::new();
    }
    let token = first_token(input);
    let token_l = token.to_ascii_lowercase();

    // Reply family (detail only); nav / login-logout are excluded via catalog_for.
    if page == Page::ThreadDetail {
        if token_l.is_empty() {
            // Prefer reply family, then other detail-visible catalog entries (skip duplicate bare `r`).
            return join_usages(
                DETAIL_REPLY_USAGES
                    .iter()
                    .copied()
                    .chain(catalog_for(page).filter(|s| s.name != "r").map(|s| s.usage)),
                max_cols,
            );
        }
        if token_l.starts_with("rr") || token_l.starts_with("rr#") {
            return truncate_end("rr#N 引用楼", max_cols);
        }
        if token_l == "r" || token_l.starts_with("r#") {
            return join_usages(DETAIL_REPLY_USAGES.iter().copied(), max_cols);
        }
    }

    if input.contains(char::is_whitespace) {
        return resolve_command(token)
            .filter(|s| command_visible_on(s, page))
            .map(|s| {
                let text = format!("{} · {}", s.usage, s.summary);
                truncate_end(&text, max_cols)
            })
            .unwrap_or_else(|| truncate_end("Enter 执行  Esc 取消", max_cols));
    }

    let specs: Vec<&CommandSpec> = if token.is_empty() {
        catalog_for(page).collect()
    } else {
        catalog_for(page)
            .filter(|s| {
                s.name.starts_with(&token_l) || s.aliases.iter().any(|a| a.starts_with(&token_l))
            })
            .collect()
    };

    if specs.is_empty() {
        return truncate_end("无匹配  Esc 取消", max_cols);
    }

    join_usages(specs.iter().map(|s| s.usage), max_cols)
}

fn join_usages<'a>(usages: impl Iterator<Item = &'a str>, max_cols: usize) -> String {
    let mut out = String::new();
    for (i, piece) in usages.enumerate() {
        let add = if i == 0 {
            piece.to_string()
        } else {
            format!(" · {piece}")
        };
        let next_w = str_width(&out) + str_width(&add);
        if next_w > max_cols {
            if out.is_empty() {
                return truncate_end(piece, max_cols);
            }
            break;
        }
        out.push_str(&add);
    }
    out
}

/// Tab-complete the first token. Returns true if input changed.
pub fn tab_complete_command(input: &mut String, cursor: &mut usize, page: Page) -> bool {
    let prefix = first_token(input);
    if prefix.is_empty() || input.chars().any(|c| c.is_whitespace()) {
        return false;
    }

    let matches: Vec<&str> = catalog_for(page)
        .filter(|s| s.name.starts_with(prefix) || s.aliases.iter().any(|a| a.starts_with(prefix)))
        .map(|s| s.name)
        .collect();

    if matches.is_empty() {
        return false;
    }

    let completion = if matches.len() == 1 {
        matches[0].to_string()
    } else {
        longest_common_prefix(&matches)
    };

    if completion.is_empty() || completion == prefix {
        return false;
    }

    if matches.len() == 1 {
        let needs_space = COMMANDS
            .iter()
            .find(|s| s.name == matches[0])
            .is_some_and(|s| s.requires_args);
        *input = if needs_space {
            format!("{completion} ")
        } else {
            completion
        };
    } else {
        *input = completion;
    }
    *cursor = input.len();
    true
}

fn first_token(input: &str) -> &str {
    input.split_whitespace().next().unwrap_or("")
}

fn longest_common_prefix(items: &[&str]) -> String {
    if items.is_empty() {
        return String::new();
    }
    let mut prefix = items[0].to_string();
    for item in &items[1..] {
        while !item.starts_with(&prefix) {
            prefix.pop();
            if prefix.is_empty() {
                return String::new();
            }
        }
    }
    prefix
}

fn truncate_end(s: &str, max_cols: usize) -> String {
    hiptty_render::truncate_str(s, max_cols)
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

fn refresh_current_page(app: &mut App, worker_tx: &mpsc::UnboundedSender<WorkerRequest>) {
    match app.page {
        Page::ThreadFeed => refresh_feed(app, worker_tx),
        Page::ThreadDetail => {
            crate::event::request_thread_detail(
                app,
                worker_tx,
                app.detail
                    .detail
                    .as_ref()
                    .map(|d| d.page.max(1))
                    .unwrap_or(1),
                crate::app::DetailLoadIntent::ReplaceTop,
            );
        }
        Page::PmThread => {
            if !app.pm_thread.peer_uid.is_empty() {
                app.pm_thread.loading = true;
                let uid = app.pm_thread.peer_uid.clone();
                let _ = worker_tx.send(WorkerRequest::LoadPmThread { uid });
            }
        }
        Page::PmList => refresh_list_page(app, worker_tx, ListPageKind::PmList),
        Page::Notifications => refresh_list_page(app, worker_tx, ListPageKind::Notifications),
        Page::Search => refresh_list_page(app, worker_tx, ListPageKind::Search),
        Page::MyThreads => refresh_list_page(app, worker_tx, ListPageKind::MyThreads),
        Page::MyReplies => refresh_list_page(app, worker_tx, ListPageKind::MyReplies),
        Page::Favorites => refresh_list_page(app, worker_tx, ListPageKind::Favorites),
        Page::Startup | Page::Login => {}
    }
}

/// Compact catalog for legacy callers / tests.
pub fn command_hints() -> String {
    COMMANDS
        .iter()
        .map(|s| s.usage)
        .collect::<Vec<_>>()
        .join(" · ")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_completes_unique_prefix() {
        let mut input = "logou".to_string();
        let mut cursor = 5;
        assert!(tab_complete_command(
            &mut input,
            &mut cursor,
            Page::ThreadFeed
        ));
        assert_eq!(input, "logout");
    }

    #[test]
    fn tab_extends_common_prefix() {
        let mut input = "l".to_string();
        let mut cursor = 1;
        assert!(tab_complete_command(
            &mut input,
            &mut cursor,
            Page::ThreadFeed
        ));
        // login + logout share "log"
        assert_eq!(input, "log");
    }

    #[test]
    fn tab_search_appends_space() {
        let mut input = "sear".to_string();
        let mut cursor = 4;
        assert!(tab_complete_command(
            &mut input,
            &mut cursor,
            Page::ThreadFeed
        ));
        assert_eq!(input, "search ");
    }

    #[test]
    fn suggestions_filter_by_prefix() {
        let strip = command_suggestion_strip("no", 80, Page::ThreadFeed);
        assert!(strip.contains("notif"));
        assert!(!strip.contains("login"));
    }

    #[test]
    fn resolve_aliases() {
        assert_eq!(resolve_command("quit").map(|s| s.name), Some("exit"));
        assert_eq!(resolve_command("favorites").map(|s| s.name), Some("fav"));
    }

    #[test]
    fn parse_floor_commands() {
        assert_eq!(parse_floor_command("r#3"), Some(FloorCommand::Reply(3)));
        assert_eq!(parse_floor_command("RR#12"), Some(FloorCommand::Quote(12)));
        assert_eq!(parse_floor_command("r#0"), None);
        assert_eq!(parse_floor_command("r#"), None);
        assert_eq!(parse_floor_command("reply"), None);
    }

    #[test]
    fn detail_suggestions_hide_nav_show_reply_family() {
        let strip = command_suggestion_strip("", 120, Page::ThreadDetail);
        assert!(strip.contains("r 回帖") || strip.starts_with("r "));
        assert!(strip.contains("r#N"));
        assert!(strip.contains("q"));
        assert!(strip.contains("refresh"));
        assert!(!strip.contains("pm"));
        assert!(!strip.contains("notif"));
        assert!(!strip.contains("search"));
        assert!(!strip.contains("login"));
        assert!(!strip.contains("logout"));
        assert!(!strip.contains("my"));
        assert!(!strip.contains("fav"));
        let r = command_suggestion_strip("r", 40, Page::ThreadDetail);
        assert!(r.contains("回帖") || r.contains("r#N"));
    }

    #[test]
    fn feed_hides_detail_only_r() {
        let strip = command_suggestion_strip("", 80, Page::ThreadFeed);
        // Bare `r` 回帖 is detail-only; feed may still show "refresh".
        assert!(!strip
            .split(" · ")
            .any(|p| p == "r" || p.starts_with("r 回帖")));
    }
}
