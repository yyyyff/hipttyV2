use hiptty_core::ThreadSummary;
use hiptty_render::{display_title, format_count, truncate_str, Palette};
use ratatui::{
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::{List, ListItem},
    Frame,
};

pub struct ThreadListProps<'a> {
    pub palette: Palette,
    pub threads: &'a [ThreadSummary],
    pub selected: usize,
    pub scroll: usize,
    pub show_avatar: bool,
    pub loading: bool,
}

pub fn draw_thread_list(frame: &mut Frame<'_>, area: Rect, props: ThreadListProps<'_>) {
    let visible = area.height as usize / 3;
    let start = props.scroll;
    let end = (start + visible).min(props.threads.len());

    let items: Vec<ListItem> = props
        .threads
        .iter()
        .enumerate()
        .skip(start)
        .take(end.saturating_sub(start))
        .map(|(idx, thread)| {
            let selected = idx == props.selected;
            let title_style = if selected {
                props.palette.selected_style()
            } else {
                props.palette.title_style(thread.title_color.as_deref())
            };

            let mut icons = String::new();
            if thread.sticky {
                icons.push('\u{f08d}');
            }
            if thread.with_pic {
                icons.push('\u{f03e}');
            }
            if thread.is_poll {
                icons.push('\u{f681}');
            }
            if thread.is_new {
                icons.push_str(" NEW");
            }

            let title = display_title(&thread.title);
            let line1 = format!(
                "{}{}",
                if props.show_avatar {
                    " ┌──┐ "
                } else {
                    " "
                },
                truncate_str(&title, 40)
            );

            let author = thread.author.as_deref().unwrap_or("?");
            let thread_type = thread
                .thread_type
                .as_deref()
                .filter(|t| !t.is_empty())
                .map(|t| format!(" · {t}"))
                .unwrap_or_default();

            let mut meta = String::new();
            if let Some(replies) = format_count(thread.reply_count.as_deref()) {
                meta.push_str(&format!(" \u{f27a}{replies}"));
            }
            if let Some(views) = format_count(thread.view_count.as_deref()) {
                meta.push_str(&format!(" \u{f06e}{views}"));
            }

            let time = thread
                .time_update
                .as_deref()
                .or(thread.time_create.as_deref())
                .unwrap_or("");
            let last = thread
                .last_post
                .as_deref()
                .filter(|s| !s.is_empty())
                .map(|s| format!(" · {s}"))
                .unwrap_or_default();

            let prefix = if props.show_avatar { " │av│ " } else { " " };
            let author_line = format!("{author}{thread_type}");
            let line2 = if meta.is_empty() {
                format!("{prefix}{author_line}  {time}{last}")
            } else {
                format!("{prefix}{author_line}  {meta}  {time}{last}")
            };

            let style = if selected {
                props.palette.accent_style().add_modifier(Modifier::BOLD)
            } else {
                props.palette.secondary_style()
            };

            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(line1, title_style),
                    Span::styled(format!(" {icons}"), props.palette.secondary_style()),
                ]),
                Line::from(Span::styled(line2, style)),
                Line::from(""),
            ])
        })
        .collect();

    if props.loading {
        // loading indicator appended via separate draw in app
    }

    let list = List::new(items);
    frame.render_widget(list, area);
}

pub fn draw_loading_indicator(frame: &mut Frame<'_>, area: Rect, palette: Palette) {
    if area.height == 0 {
        return;
    }
    let y = area.y + area.height.saturating_sub(1);
    let indicator_area = Rect {
        x: area.x,
        y,
        width: area.width,
        height: 1,
    };
    frame.render_widget(
        ratatui::widgets::Paragraph::new("── 正在加载... ──").style(palette.dim_style()),
        indicator_area,
    );
}
