use hiptty_widgets::{
    draw_dim_rule, draw_forum_picker, draw_loading_indicator, draw_login, draw_status_bar,
    draw_thread_list, draw_title_bar, main_layout, ForumPickerProps, LoginFormProps,
    ThreadListProps, TitleBarProps,
};
use ratatui::{layout::Rect, widgets::Paragraph, Frame};

use crate::app::{App, Overlay, Page};

pub fn draw(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    if area.width < 80 || area.height < 24 {
        frame.render_widget(
            Paragraph::new("终端窗口过小，建议至少 80×24").style(app.palette().warn_style()),
            area,
        );
        return;
    }

    match app.page {
        Page::Login => draw_login_page(frame, app, area),
        Page::ThreadFeed => draw_main_shell(frame, app, area),
    }

    if let Some(toast) = &app.toast {
        draw_toast(frame, area, app, toast);
    }
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

fn draw_main_shell(frame: &mut Frame<'_>, app: &App, area: Rect) {
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
            breadcrumb: &app.breadcrumb(),
            breadcrumb_right: None,
        },
    );
    draw_dim_rule(frame, title_rule, palette);

    draw_thread_list(
        frame,
        content_area,
        ThreadListProps {
            palette,
            threads: &app.feed.threads,
            selected: app.feed.selected,
            scroll: app.feed.scroll,
            show_avatar: true,
            loading: app.feed.loading,
        },
    );

    if app.feed.loading {
        draw_loading_indicator(frame, content_area, palette);
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
    draw_status_bar(frame, status_area, palette, app.status_hints());

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
