use hiptty_widgets::{
    draw_composer, draw_confirm_dialog, draw_dim_rule, draw_floor_list, draw_forum_picker,
    draw_loading_indicator, draw_login, draw_startup, draw_status_bar, draw_thread_list,
    draw_title_bar, main_layout, ComposerProps, ConfirmProps, FloorListProps, ForumPickerProps,
    LoginFormProps, StartupProps, ThreadListProps, TitleBarProps,
};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    widgets::{Clear, Paragraph},
    Frame,
};

use crate::app::{App, Overlay, Page};

pub fn draw(frame: &mut Frame<'_>, app: &mut App) {
    let area = frame.area();
    let _images_changed = app.images_mut().map(|cache| cache.poll()).unwrap_or(false);
    let prev_w = app.viewport_width;
    let prev_h = app.viewport_height;
    app.viewport_width = area.width;
    app.viewport_height = area.height;
    if app.page == Page::ThreadFeed {
        app.sync_feed_scroll();
    }
    if app.page == Page::ThreadDetail && (prev_w != area.width || prev_h != area.height) {
        app.sync_detail_scroll();
    }

    if area.width < 80 || area.height < 24 {
        frame.render_widget(
            Paragraph::new("终端窗口过小，建议至少 80×24").style(app.palette().warn_style()),
            area,
        );
        return;
    }

    match app.page {
        Page::Startup => draw_startup_page(frame, app, area),
        Page::Login => draw_login_page(frame, app, area),
        Page::ThreadFeed => draw_feed_shell(frame, app, area),
        Page::ThreadDetail => draw_detail_shell(frame, app, area),
    }

    if let Some(confirm) = &app.confirm_delete {
        draw_confirm_dialog(
            frame,
            area,
            ConfirmProps {
                palette: app.palette(),
                title: "确认删除",
                message: &confirm.label,
                loading: confirm.submitting,
            },
        );
    }

    if let Some(composer) = &app.composer {
        draw_composer(
            frame,
            area,
            ComposerProps {
                palette: app.palette(),
                header: &composer.header,
                subject: &composer.subject,
                show_subject: composer.show_subject,
                focus: composer.focus,
                textarea: &composer.textarea,
                error: composer.error.as_deref(),
                loading: composer.preparing || composer.submitting,
                image_path: composer.image_path.as_deref(),
            },
        );
    }

    if let Some(toast) = &app.toast {
        draw_toast(frame, area, app, toast);
    }
}

fn draw_startup_page(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let palette = app.palette();
    let chunks = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);

    draw_startup(
        frame,
        chunks[0],
        StartupProps {
            palette,
            message: app.startup_message(),
        },
    );
    draw_dim_rule(frame, chunks[1], palette);
    draw_status_bar(frame, chunks[2], palette, app.status_hints());
}

fn draw_login_page(frame: &mut Frame<'_>, app: &App, area: Rect) {
    draw_login(
        frame,
        area,
        LoginFormProps {
            palette: app.palette(),
            username: &app.login.username,
            password: &app.login.password,
            security_index: app.login.security_index,
            security_answer: &app.login.security_answer,
            focused: app.login.focused,
            error: app.login.error.as_deref(),
            loading: app.login.loading,
        },
    );
}

fn draw_feed_shell(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let palette = app.palette();
    let [title_area, title_rule, content_area, status_rule, status_area] = main_layout(area);

    draw_title_bar(
        frame,
        title_area,
        TitleBarProps {
            palette,
            tick: app.tick,
            username: app.session.username.as_deref(),
            has_notifications: false,
            has_pm: false,
            breadcrumb: &forum_breadcrumb(app),
            breadcrumb_right: None,
        },
    );
    draw_dim_rule(frame, title_rule, palette);

    {
        let feed = &app.feed;
        let images = app.image_cache.as_mut();
        draw_thread_list(
            frame,
            content_area,
            ThreadListProps {
                palette,
                threads: &feed.threads,
                selected: feed.selected,
                scroll: feed.scroll,
                show_avatar: true,
                loading: feed.loading,
                images,
            },
        );
    }

    if app.feed.loading {
        draw_loading_indicator(frame, content_area, palette, app.tick);
    }

    if let Some(err) = &app.feed.error {
        frame.render_widget(
            Paragraph::new(err.as_str()).style(palette.error_style()),
            Rect {
                x: content_area.x,
                y: content_area.y,
                width: content_area.width,
                height: 1,
            },
        );
    }

    draw_dim_rule(frame, status_rule, palette);
    draw_status_bar(frame, status_area, palette, feed_status_hints());

    if app.overlay == Overlay::ForumPicker {
        draw_forum_picker(
            frame,
            area,
            ForumPickerProps {
                palette,
                default_forums: &app.settings.default_forums,
                selected: app.forum_picker_selected,
                current_fid: app.feed.fid,
            },
        );
    }
}

fn draw_detail_shell(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let palette = app.palette();
    let chunks = main_layout(area);
    let title_area = chunks[0];
    let title_rule = chunks[1];
    let content_area = chunks[2];
    let status_rule = chunks[3];
    let status_area = chunks[4];

    let counts = app.detail_title_counts();
    let show_loading = app.detail.loading || app.detail.loading_more;

    {
        let detail_state = &app.detail;
        let images = app.image_cache.as_mut();
        if let Some(detail) = &detail_state.detail {
            if !show_loading || detail_state.loading_more {
                draw_floor_list(
                    frame,
                    content_area,
                    FloorListProps {
                        palette,
                        posts: &detail.posts,
                        selected: detail_state.selected,
                        scroll_top: detail_state.scroll_top,
                        show_avatar: true,
                        images,
                    },
                );
            }
        } else if detail_state.loading {
            frame.render_widget(Clear, content_area);
        }
    }

    if show_loading {
        draw_loading_indicator(frame, content_area, palette, app.tick);
    }

    if let Some(err) = &app.detail.error {
        frame.render_widget(
            Paragraph::new(err.as_str()).style(palette.error_style()),
            Rect {
                x: content_area.x,
                y: content_area.y,
                width: content_area.width,
                height: 1,
            },
        );
    }

    draw_title_bar(
        frame,
        title_area,
        TitleBarProps {
            palette,
            tick: app.tick,
            username: app.session.username.as_deref(),
            has_notifications: false,
            has_pm: false,
            breadcrumb: &detail_breadcrumb(app),
            breadcrumb_right: counts.as_deref(),
        },
    );
    draw_dim_rule(frame, title_rule, palette);

    draw_dim_rule(frame, status_rule, palette);
    draw_status_bar(frame, status_area, palette, detail_status_hints());
}

fn forum_breadcrumb(app: &App) -> String {
    hiptty_core::forum_name(app.feed.fid)
        .unwrap_or("Forum")
        .to_string()
}

fn detail_breadcrumb(app: &App) -> String {
    app.detail_breadcrumb()
}

fn feed_status_hints() -> &'static str {
    "j/k ↑↓  Enter 打开  r 回复  n 新帖  f 切换版块  / 搜索  b 返回"
}

fn detail_status_hints() -> &'static str {
    "j/k ↑↓  PgUp/Dn  r 回复  q 引用  e 编辑  d 删除  g/G 首页/末页  b 返回"
}

fn draw_toast(frame: &mut Frame<'_>, area: Rect, app: &App, message: &str) {
    let width = (message.len() as u16 + 4).min(area.width.saturating_sub(4));
    let height = 3;
    let x = area.x + area.width.saturating_sub(width + 2);
    let y = area.y + area.height.saturating_sub(height + 2);
    frame.render_widget(
        Paragraph::new(message)
            .style(app.palette().accent_style())
            .block(ratatui::widgets::Block::default().borders(ratatui::widgets::Borders::ALL)),
        Rect {
            x,
            y,
            width,
            height,
        },
    );
}