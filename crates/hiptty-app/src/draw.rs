use hiptty_core::{forum_name, list_item_to_thread_summary};
use hiptty_render::{clear_graphics_in_area, fill_area_spaces};
use hiptty_widgets::{
    draw_composer, draw_confirm_dialog, draw_dim_rule, draw_floor_list, draw_forum_picker,
    draw_login, draw_main_menu, draw_pm_thread, draw_search_prompt, draw_settings_panel,
    draw_simple_list, draw_startup, draw_status_bar, draw_thread_list, draw_title_bar, draw_toast,
    draw_vertical_scrollbar, list_content_lines, main_layout, title_bar_hits, ComposerProps,
    ConfirmProps, FloorListProps, ForumPickerProps, ForumTabsProps, LoginFormProps, MainMenuProps,
    CommandLineProps, PmThreadProps, SearchPromptProps, SettingsProps, SimpleListProps, StartupProps,
    StatusBarProps, ThreadListProps, TitleBarProps, ToastProps, ITEM_HEIGHT, PM_ITEM_HEIGHT,
    SIMPLE_ITEM_HEIGHT,
};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    widgets::Paragraph,
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

    let palette = app.palette();
    let composer_bottom = if let Some(composer) = app.composer.as_mut() {
        let type_unset = !composer.type_selected_ok() && composer.require_type();
        let type_label = composer.type_label().to_string();
        let show_type = composer.need_type_ui();
        let h = hiptty_widgets::composer_height(area.height).min(area.height);
        draw_composer(
            frame,
            area,
            ComposerProps {
                palette,
                header: &composer.header,
                subject: &composer.subject,
                show_subject: composer.show_subject,
                show_type,
                type_label: &type_label,
                type_unset,
                focus: composer.focus,
                textarea: &composer.textarea,
                error: composer.error.as_deref(),
                preparing: composer.preparing,
                submitting: composer.submitting,
                quote_preview: composer.quote_preview.as_deref(),
                textarea_view_top: &mut composer.textarea_view_top,
            },
        );
        h
    } else {
        0
    };

    // Toast last (above composer / modals in the cell buffer) and lifted when composer is open.
    if let Some(toast) = &app.toast {
        draw_toast(
            frame,
            area,
            ToastProps {
                palette: app.palette(),
                message: &toast.message,
                is_error: toast.is_error,
                tick: app.tick,
                started_at: toast.started_at,
                duration_ticks: toast.duration_ticks,
                bottom_inset: composer_bottom,
            },
        );
    }
}

fn draw_overlays(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    let palette = app.palette();
    app.main_menu_hits.clear();
    match app.overlay {
        Overlay::MainMenu => {
            app.main_menu_hits = draw_main_menu(
                frame,
                area,
                MainMenuProps {
                    palette,
                    selected: app.overlay_state.main_menu_selected,
                },
            );
        }
        Overlay::Settings => {
            app.settings_hits = draw_settings_panel(
                frame,
                area,
                SettingsProps {
                    palette,
                    settings: &app.settings,
                    selected: app.overlay_state.settings_selected,
                    blacklist_count: app.blacklist_count,
                },
            );
        }
        Overlay::SearchPrompt => draw_search_prompt(
            frame,
            area,
            SearchPromptProps {
                palette,
                input: &app.overlay_state.search_input,
                forum_name: forum_name(app.feed.fid).unwrap_or("Forum"),
            },
        ),
        // Command bar is rendered inline in the status bar (vim-style).
        Overlay::CommandBar | Overlay::ForumPicker | Overlay::None => {}
    }
}

fn paint_status_bar(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let hints = app.status_hints();
    let command = app.status_command_input().map(|input| CommandLineProps {
        input,
        cursor: app.status_command_cursor().unwrap_or(input.len()),
    });

    // Command mode: reserve ~45% width for suggestions when possible.
    let right_owned = if command.is_some() {
        let suggest_budget = (area.width as usize * 45 / 100).clamp(16, 48);
        Some(crate::commands::command_suggestion_strip(
            app.overlay_state.command_input.as_str(),
            suggest_budget,
            app.page,
        ))
    } else {
        app.status_right()
    };

    draw_status_bar(
        frame,
        area,
        StatusBarProps {
            palette: app.palette(),
            hints: &hints,
            right: right_owned.as_deref(),
            command,
        },
    );
}

fn shell_title_bar(frame: &mut Frame<'_>, app: &mut App, title_area: Rect, right: Option<&str>) {
    app.title_bar_area = title_area;
    let palette = app.content_palette();
    let mask_cjk = app.dims_background();
    let default_forums = app.settings.default_forums;
    let forum_tabs = if app.page == Page::ThreadFeed {
        Some(ForumTabsProps {
            palette,
            default_forums: &default_forums,
            active_fid: app.feed.fid,
            hover_tab: app.forum_tab_hover,
            mask_cjk,
        })
    } else {
        None
    };
    let props = TitleBarProps {
        palette,
        tick: app.tick,
        username: app.session.username.as_deref(),
        has_notifications: app.unread.has_notifications,
        has_pm: app.unread.has_pm,
        breadcrumb: &app.breadcrumb(),
        breadcrumb_right: right,
        forum_tabs,
        mask_cjk,
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
    draw_dim_rule(frame, title_rule, app.content_palette());
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
            draw_vertical_scrollbar(frame, chrome.bar, app.content_palette(), chrome.lengths(), offset);
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
    paint_status_bar(frame, app, chunks[2]);
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
    let palette = app.content_palette();
    let mask_cjk = app.dims_background();
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
                mask_cjk,
            },
        );
    }

    draw_list_error(frame, content, palette, app.feed.error.as_deref());
    repaint_title_chrome(frame, app, title_area, title_rule, None);

    draw_dim_rule(frame, status_rule, palette);
    paint_status_bar(frame, app, status_area);

    if app.overlay == Overlay::ForumPicker {
        let frame_state = draw_forum_picker(
            frame,
            area,
            ForumPickerProps {
                palette: app.palette(),
                default_forums: &app.settings.default_forums,
                selected: app.forum_picker_selected,
                current_fid: app.feed.fid,
                scroll_offset: app.forum_picker_scroll,
            },
        );
        app.forum_picker_scroll = frame_state.scroll_offset;
        app.forum_picker_hits = frame_state.hits;
    } else {
        app.forum_picker_hits.clear();
    }
}

fn draw_detail_shell(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let palette = app.content_palette();
    let mask_cjk = app.dims_background();
    let chunks = main_layout(area);
    let counts = app.detail_title_counts();

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
            // Keep existing floors visible while reloading; loading is shown in status bar.
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
                    mask_cjk,
                },
            );
        }
    }

    draw_list_error(frame, content, palette, app.detail.error.as_deref());
    repaint_title_chrome(frame, app, chunks[0], chunks[1], counts.as_deref());
    draw_dim_rule(frame, chunks[3], palette);
    paint_status_bar(frame, app, chunks[4]);
}

fn draw_simple_list_shell(frame: &mut Frame<'_>, app: &mut App, area: Rect, show_avatar: bool) {
    let palette = app.content_palette();
    let mask_cjk = app.dims_background();
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
            mask_cjk,
        },
    );
    draw_list_error(frame, content, palette, app.list_page.error.as_deref());
    repaint_title_chrome(frame, app, chunks[0], chunks[1], None);

    draw_dim_rule(frame, chunks[3], palette);
    paint_status_bar(frame, app, chunks[4]);
}

fn draw_thread_list_shell(frame: &mut Frame<'_>, app: &mut App, area: Rect, show_avatar: bool) {
    let palette = app.content_palette();
    let mask_cjk = app.dims_background();
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
            mask_cjk,
        },
    );
    draw_list_error(frame, content, palette, app.list_page.error.as_deref());
    repaint_title_chrome(frame, app, chunks[0], chunks[1], None);

    draw_dim_rule(frame, chunks[3], palette);
    paint_status_bar(frame, app, chunks[4]);
}

fn draw_pm_thread_shell(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    let palette = app.content_palette();
    let mask_cjk = app.dims_background();
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
            mask_cjk,
        },
    );
    draw_list_error(frame, content, palette, app.pm_thread.error.as_deref());
    repaint_title_chrome(frame, app, chunks[0], chunks[1], None);

    draw_dim_rule(frame, chunks[3], palette);
    paint_status_bar(frame, app, chunks[4]);
}

fn draw_list_error(
    frame: &mut Frame<'_>,
    area: Rect,
    palette: hiptty_render::Palette,
    err: Option<&str>,
) {
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


