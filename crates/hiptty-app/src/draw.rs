use hiptty_core::{forum_name, list_item_to_thread_summary};
use hiptty_render::{clear_graphics_in_area, fill_area_spaces};
use hiptty_widgets::{
    draw_command_bar, draw_composer, draw_confirm_dialog, draw_dim_rule, draw_floor_list,
    draw_forum_picker, draw_help_overlay, draw_loading_indicator, draw_login, draw_main_menu,
    draw_pm_thread, draw_search_prompt, draw_settings_panel, draw_simple_list, draw_startup,
    draw_status_bar, draw_thread_list, draw_title_bar, draw_vertical_scrollbar, list_content_lines,
    main_layout, title_bar_hits, ComposerProps, ConfirmProps, FloorListProps, ForumPickerProps,
    HelpOverlayProps, ITEM_HEIGHT, LoginFormProps, MainMenuProps, PM_ITEM_HEIGHT, PmThreadProps,
    SettingsProps, SearchPromptProps, SIMPLE_ITEM_HEIGHT, SimpleListProps, StartupProps,
    ThreadListProps, TitleBarProps,
};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    widgets::{Clear, Paragraph},
    Frame,
};

use crate::app::{App, Overlay, Page};
use crate::mouse::install_scroll_chrome;

pub fn draw(frame: &mut Frame<'_>, app: &mut App) {
    let area = frame.area();
    let _images_changed = app.images_mut().map(|cache| cache.poll()).unwrap_or(false);
    app.poll_toast();
    let prev_w = app.viewport_width;
    let prev_h = app.viewport_height;
    app.viewport_width = area.width;
    app.viewport_height = area.height;
    if prev_w != area.width || prev_h != area.height {
        match app.page {
            Page::ThreadFeed => app.sync_feed_scroll(),
            Page::ThreadDetail => app.sync_detail_scroll(),
            Page::PmThread => app.sync_pm_scroll(),
            Page::PmList
            | Page::Notifications
            | Page::Search
            | Page::MyThreads
            | Page::MyReplies
            | Page::Favorites => app.sync_list_scroll(),
            _ => {}
        }
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
        Page::PmList | Page::Notifications => draw_simple_list_shell(frame, app, area, true),
        Page::Search | Page::MyThreads | Page::MyReplies | Page::Favorites => {
            draw_thread_list_shell(frame, app, area, false)
        }
        Page::PmThread => draw_pm_thread_shell(frame, app, area),
    }

    draw_overlays(frame, app, area);

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

fn draw_overlays(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let palette = app.palette();
    match app.overlay {
        Overlay::MainMenu => draw_main_menu(
            frame,
            area,
            MainMenuProps {
                palette,
                selected: app.overlay_state.main_menu_selected,
            },
        ),
        Overlay::Help => draw_help_overlay(frame, area, HelpOverlayProps { palette }),
        Overlay::Settings => draw_settings_panel(
            frame,
            area,
            SettingsProps {
                palette,
                settings: &app.settings,
                selected: app.overlay_state.settings_selected,
                blacklist_count: app.blacklist_count,
            },
        ),
        Overlay::SearchPrompt => draw_search_prompt(
            frame,
            area,
            SearchPromptProps {
                palette,
                input: &app.overlay_state.search_input,
                forum_name: forum_name(app.feed.fid).unwrap_or("Forum"),
            },
        ),
        Overlay::CommandBar => draw_command_bar(
            frame,
            area,
            hiptty_widgets::CommandBarProps {
                palette,
                input: &app.overlay_state.command_input,
            },
        ),
        Overlay::ForumPicker | Overlay::None => {}
    }
}

fn shell_title_bar(frame: &mut Frame<'_>, app: &mut App, title_area: Rect, right: Option<&str>) {
    app.title_bar_area = title_area;
    let props = TitleBarProps {
        palette: app.palette(),
        tick: app.tick,
        username: app.session.username.as_deref(),
        has_notifications: app.unread.has_notifications,
        has_pm: app.unread.has_pm,
        breadcrumb: &app.breadcrumb(),
        breadcrumb_right: right,
    };
    app.title_bar_hits = title_bar_hits(title_area, &props);
    draw_title_bar(frame, title_area, props);
}

/// Kitty graphics in the content pane can extend above `area.y`; repaint title chrome on top.
fn repaint_title_chrome(
    frame: &mut Frame<'_>,
    app: &mut App,
    title_area: Rect,
    title_rule: Rect,
    right: Option<&str>,
) {
    clear_graphics_in_area(frame, title_area);
    clear_graphics_in_area(frame, title_rule);
    fill_area_spaces(frame, title_area);
    fill_area_spaces(frame, title_rule);
    shell_title_bar(frame, app, title_area, right);
    draw_dim_rule(frame, title_rule, app.palette());
}

fn paint_scroll_area(
    frame: &mut Frame<'_>,
    app: &mut App,
    full_content: Rect,
    content_len: u16,
    offset: u16,
) -> Rect {
    let content = install_scroll_chrome(app, full_content, content_len, offset);
    if let Some(chrome) = app.scroll_chrome {
        if chrome.shown {
            draw_vertical_scrollbar(
                frame,
                chrome.bar,
                app.palette(),
                chrome.lengths(),
                offset,
            );
        }
    }
    content
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

    shell_title_bar(frame, app, title_area, None);
    draw_dim_rule(frame, title_rule, palette);

    let content_len = list_content_lines(app.feed.threads.len(), ITEM_HEIGHT);
    let content = paint_scroll_area(frame, app, content_area, content_len, app.feed.scroll_lines);
    {
        let feed = &app.feed;
        let images = app.image_cache.as_mut();
        draw_thread_list(
            frame,
            content,
            ThreadListProps {
                palette,
                threads: &feed.threads,
                selected: feed.selected,
                scroll_lines: feed.scroll_lines,
                show_avatar: true,
                loading: feed.loading,
                images,
            },
        );
    }

    if app.feed.loading {
        draw_loading_indicator(frame, content, palette, app.tick);
    }
    draw_list_error(frame, content, palette, app.feed.error.as_deref());
    repaint_title_chrome(frame, app, title_area, title_rule, None);

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

fn draw_detail_shell(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let palette = app.palette();
    let chunks = main_layout(area);
    let counts = app.detail_title_counts();
    let show_loading = app.detail.loading || app.detail.loading_more;

    shell_title_bar(frame, app, chunks[0], counts.as_deref());
    draw_dim_rule(frame, chunks[1], palette);

    let (pre_content, _) = hiptty_widgets::split_content_scrollbar(chunks[2]);
    let detail_content_len = app
        .detail
        .detail
        .as_ref()
        .map(|detail| {
            hiptty_widgets::floor_list_total_height(
                &detail.posts,
                pre_content.width.max(1),
                palette,
                app.image_cache.as_ref(),
            )
        })
        .unwrap_or(0);
    let content = paint_scroll_area(
        frame,
        app,
        chunks[2],
        detail_content_len,
        app.detail.scroll_top,
    );

    {
        let detail_state = &app.detail;
        let images = app.image_cache.as_mut();
        if let Some(detail) = &detail_state.detail {
            if !show_loading || detail_state.loading_more {
                draw_floor_list(
                    frame,
                    content,
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
            frame.render_widget(Clear, content);
        }
    }

    if show_loading {
        draw_loading_indicator(frame, content, palette, app.tick);
    }
    draw_list_error(frame, content, palette, app.detail.error.as_deref());
    repaint_title_chrome(frame, app, chunks[0], chunks[1], counts.as_deref());
    draw_dim_rule(frame, chunks[3], palette);
    draw_status_bar(frame, chunks[4], palette, app.status_hints());
}

fn draw_simple_list_shell(frame: &mut Frame<'_>, app: &mut App, area: Rect, show_avatar: bool) {
    let palette = app.palette();
    let chunks = main_layout(area);
    shell_title_bar(frame, app, chunks[0], None);
    draw_dim_rule(frame, chunks[1], palette);

    let content_len = list_content_lines(app.list_page.items.len(), SIMPLE_ITEM_HEIGHT);
    let content = paint_scroll_area(
        frame,
        app,
        chunks[2],
        content_len,
        app.list_page.scroll_lines,
    );
    let images = app.image_cache.as_mut();
    draw_simple_list(
        frame,
        content,
        SimpleListProps {
            palette,
            items: &app.list_page.items,
            selected: app.list_page.selected,
            scroll_lines: app.list_page.scroll_lines,
            show_avatar,
            images,
        },
    );
    if app.list_page.loading {
        draw_loading_indicator(frame, content, palette, app.tick);
    }
    draw_list_error(frame, content, palette, app.list_page.error.as_deref());
    repaint_title_chrome(frame, app, chunks[0], chunks[1], None);

    draw_dim_rule(frame, chunks[3], palette);
    draw_status_bar(frame, chunks[4], palette, app.status_hints());
}

fn draw_thread_list_shell(frame: &mut Frame<'_>, app: &mut App, area: Rect, show_avatar: bool) {
    let palette = app.palette();
    let chunks = main_layout(area);
    shell_title_bar(frame, app, chunks[0], None);
    draw_dim_rule(frame, chunks[1], palette);

    let threads: Vec<_> = app
        .list_page
        .items
        .iter()
        .map(list_item_to_thread_summary)
        .collect();
    let content_len = list_content_lines(app.list_page.items.len(), ITEM_HEIGHT);
    let content = paint_scroll_area(
        frame,
        app,
        chunks[2],
        content_len,
        app.list_page.scroll_lines,
    );
    let images = app.image_cache.as_mut();
    draw_thread_list(
        frame,
        content,
        ThreadListProps {
            palette,
            threads: &threads,
            selected: app.list_page.selected,
            scroll_lines: app.list_page.scroll_lines,
            show_avatar,
            loading: app.list_page.loading,
            images,
        },
    );
    if app.list_page.loading {
        draw_loading_indicator(frame, content, palette, app.tick);
    }
    draw_list_error(frame, content, palette, app.list_page.error.as_deref());
    repaint_title_chrome(frame, app, chunks[0], chunks[1], None);

    draw_dim_rule(frame, chunks[3], palette);
    draw_status_bar(frame, chunks[4], palette, app.status_hints());
}

fn draw_pm_thread_shell(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    let palette = app.palette();
    let chunks = main_layout(area);
    shell_title_bar(frame, app, chunks[0], None);
    draw_dim_rule(frame, chunks[1], palette);

    let content_len = list_content_lines(app.pm_thread.messages.len(), PM_ITEM_HEIGHT);
    let content = paint_scroll_area(
        frame,
        app,
        chunks[2],
        content_len,
        app.pm_thread.scroll_lines,
    );
    draw_pm_thread(
        frame,
        content,
        PmThreadProps {
            palette,
            messages: &app.pm_thread.messages,
            my_username: app.session.username.as_deref().unwrap_or(""),
            selected: app.pm_thread.selected,
            scroll_lines: app.pm_thread.scroll_lines,
        },
    );
    if app.pm_thread.loading {
        draw_loading_indicator(frame, content, palette, app.tick);
    }
    draw_list_error(frame, content, palette, app.pm_thread.error.as_deref());
    repaint_title_chrome(frame, app, chunks[0], chunks[1], None);

    draw_dim_rule(frame, chunks[3], palette);
    draw_status_bar(frame, chunks[4], palette, app.status_hints());
}

fn draw_list_error(frame: &mut Frame<'_>, area: Rect, palette: hiptty_render::Palette, err: Option<&str>) {
    if let Some(err) = err {
        frame.render_widget(
            Paragraph::new(err).style(palette.error_style()),
            Rect {
                x: area.x,
                y: area.y,
                width: area.width,
                height: 1,
            },
        );
    }
}

fn draw_toast(frame: &mut Frame<'_>, area: Rect, app: &App, message: &str) {
    let width = (message.chars().count() as u16 + 4).min(area.width.saturating_sub(4));
    let height = 3;
    let x = area.x + area.width.saturating_sub(width + 2);
    let y = area.y + area.height.saturating_sub(height + 2);
    let style = if app.toast_is_error {
        app.palette().error_style()
    } else {
        app.palette().accent_style()
    };
    frame.render_widget(
        Paragraph::new(message)
            .style(style)
            .block(ratatui::widgets::Block::default().borders(ratatui::widgets::Borders::ALL)),
        Rect {
            x,
            y,
            width,
            height,
        },
    );
}