use hiptty_core::ThreadSummary;
use hiptty_render::{display_title, format_count, truncate_str, Palette};
use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::Modifier,
    widgets::Paragraph,
    Frame,
};

const AVATAR_W: u16 = 5;
const ICONS_W: u16 = 12;
const ITEM_HEIGHT: u16 = 3;

pub struct ThreadListProps<'a> {
    pub palette: Palette,
    pub threads: &'a [ThreadSummary],
    pub selected: usize,
    pub scroll: usize,
    pub show_avatar: bool,
    pub loading: bool,
}

pub fn draw_thread_list(frame: &mut Frame<'_>, area: Rect, props: ThreadListProps<'_>) {
    if area.height < ITEM_HEIGHT {
        return;
    }

    let capacity = area.height / ITEM_HEIGHT;
    let start = props.scroll;
    let mut y = area.y;

    for (idx, thread) in props
        .threads
        .iter()
        .enumerate()
        .skip(start)
        .take(capacity as usize)
    {
        let item_h = ITEM_HEIGHT.min(area.y + area.height - y);
        if item_h < 2 {
            break;
        }
        let item_area = Rect {
            x: area.x,
            y,
            width: area.width,
            height: item_h,
        };
        draw_thread_item(
            frame,
            item_area,
            thread,
            idx == props.selected,
            props.palette,
            props.show_avatar,
        );
        y += ITEM_HEIGHT;
    }
}

fn draw_thread_item(
    frame: &mut Frame<'_>,
    area: Rect,
    thread: &ThreadSummary,
    selected: bool,
    palette: Palette,
    show_avatar: bool,
) {
    let content_h = 2u16.min(area.height);
    let body = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: content_h,
    };

    let (avatar_area, right_area) = if show_avatar {
        let cols =
            Layout::horizontal([Constraint::Length(AVATAR_W), Constraint::Min(0)]).split(body);
        (Some(cols[0]), cols[1])
    } else {
        (None, body)
    };

    if let Some(av) = avatar_area {
        draw_avatar_placeholder(frame, av, thread.author.as_deref(), palette);
    }

    let rows = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(right_area);

    let icons = build_icons(thread);
    let icons_w = ICONS_W.min(rows[0].width);
    let title_cols =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(icons_w)]).split(rows[0]);

    let title = display_title(&thread.title);
    let title_budget = title_cols[0].width.saturating_sub(1) as usize;
    let title_trunc = truncate_str(&title, title_budget);

    let title_style = if selected {
        palette.selected_style()
    } else {
        palette.title_style(thread.title_color.as_deref())
    };

    frame.render_widget(
        Paragraph::new(title_trunc).style(title_style),
        title_cols[0],
    );
    frame.render_widget(
        Paragraph::new(icons)
            .style(palette.secondary_style())
            .alignment(Alignment::Right),
        title_cols[1],
    );

    let meta_style = if selected {
        palette.accent_style().add_modifier(Modifier::BOLD)
    } else {
        palette.secondary_style()
    };
    frame.render_widget(
        Paragraph::new(build_meta_line(thread, rows[1].width as usize)).style(meta_style),
        rows[1],
    );
}

fn draw_avatar_placeholder(
    frame: &mut Frame<'_>,
    area: Rect,
    author: Option<&str>,
    palette: Palette,
) {
    if area.height < 2 || area.width < 3 {
        return;
    }

    let initial = author
        .and_then(|a| a.chars().next())
        .map(|c| c.to_string())
        .unwrap_or_else(|| "?".to_string());

    let top = Rect {
        x: area.x,
        y: area.y,
        width: area.width.min(4),
        height: 1,
    };
    let mid = Rect {
        x: area.x,
        y: area.y + 1,
        width: area.width.min(4),
        height: 1,
    };

    frame.render_widget(Paragraph::new("┌──┐").style(palette.dim_style()), top);
    let inner = truncate_str(&initial, 2);
    let padded = format!("│{inner:>2}│");
    frame.render_widget(Paragraph::new(padded).style(palette.dim_style()), mid);
}

fn build_icons(thread: &ThreadSummary) -> String {
    let mut parts = Vec::new();
    if thread.sticky {
        parts.push("\u{f08d}");
    }
    if thread.with_pic {
        parts.push("\u{f03e}");
    }
    if thread.is_poll {
        parts.push("\u{f681}");
    }
    if thread.is_new {
        parts.push("NEW");
    }
    parts.join(" ")
}

fn build_meta_line(thread: &ThreadSummary, max_cols: usize) -> String {
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

    let raw = if meta.is_empty() {
        format!("{author}{thread_type}  {time}{last}")
    } else {
        format!("{author}{thread_type}{meta}  {time}{last}")
    };

    truncate_str(&raw, max_cols.saturating_sub(1))
}

pub fn draw_loading_indicator(frame: &mut Frame<'_>, area: Rect, palette: Palette) {
    if area.height == 0 {
        return;
    }
    let indicator_area = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(1),
        width: area.width,
        height: 1,
    };
    frame.render_widget(
        Paragraph::new("── 正在加载... ──")
            .style(palette.dim_style())
            .alignment(Alignment::Center),
        indicator_area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use hiptty_render::str_width;

    #[test]
    fn meta_truncates_to_width() {
        let thread = ThreadSummary {
            tid: "1".into(),
            title: "t".into(),
            title_color: None,
            author: Some("admin".into()),
            author_id: None,
            avatar_url: None,
            last_post: None,
            reply_count: Some("99".into()),
            view_count: Some("1000".into()),
            time_create: Some("today".into()),
            time_update: None,
            thread_type: None,
            sticky: false,
            with_pic: false,
            is_new: false,
            is_poll: false,
            max_page: 1,
        };
        let line = build_meta_line(&thread, 20);
        assert!(str_width(&line) <= 20);
    }
}
