use hiptty_core::ThreadSummary;
use hiptty_image::{draw_avatar_entry, ImageCache, AVATAR_COLS, AVATAR_ROWS};
use hiptty_render::{
    clear_content_viewport, display_title, format_count, format_relative_time, maybe_mask_cjk,
    str_width, truncate_str, Palette,
};
use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    widgets::Paragraph,
    Frame,
};

pub const ITEM_HEIGHT: u16 = 3;
const CONTENT_ROWS: u16 = 2;
const CONTENT_RIGHT_PAD: u16 = 1;
/// Left focus bar (`│`) — keyboard selection affordance shared with simple/floor lists.
const BAR_W: u16 = 1;
const AVATAR_W: u16 = AVATAR_COLS;
const AVATAR_GAP: u16 = 1;
const COUNT_GAP: &str = "   ";
/// Hide reply/view counts when the text column is tighter than this
/// (≈ terminal width ≲ 45 after focus bar / no avatar).
const MIN_COUNTS_COLS: u16 = 42;

pub struct ThreadListProps<'a> {
    pub palette: Palette,
    pub threads: &'a [ThreadSummary],
    pub selected: usize,
    pub scroll_lines: u16,
    pub show_avatar: bool,
    pub loading: bool,
    pub images: Option<&'a mut ImageCache>,
    pub mask_cjk: bool,
}

pub fn thread_list_capacity(content_height: u16) -> usize {
    if content_height < ITEM_HEIGHT {
        return 0;
    }
    (content_height / ITEM_HEIGHT) as usize
}

pub fn ensure_thread_scroll(selected: usize, scroll: usize, capacity: usize) -> usize {
    if capacity == 0 {
        return 0;
    }
    if selected < scroll {
        selected
    } else if selected >= scroll.saturating_add(capacity) {
        selected.saturating_sub(capacity.saturating_sub(1))
    } else {
        scroll
    }
}

pub fn ensure_thread_scroll_lines(selected: usize, scroll_lines: u16, viewport_h: u16) -> u16 {
    crate::scroll::ensure_thread_scroll_lines(selected, scroll_lines, viewport_h, ITEM_HEIGHT)
}

pub fn draw_thread_list(frame: &mut Frame<'_>, area: Rect, mut props: ThreadListProps<'_>) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    clear_content_viewport(frame, area);

    let scroll = props.scroll_lines;
    let viewport_bottom = scroll.saturating_add(area.height);

    for (idx, thread) in props.threads.iter().enumerate() {
        let item_top = (idx as u16).saturating_mul(ITEM_HEIGHT);
        let item_bottom = item_top.saturating_add(ITEM_HEIGHT);
        if item_bottom <= scroll {
            continue;
        }
        if item_top >= viewport_bottom {
            break;
        }

        let draw_y = area.y.saturating_add(item_top.saturating_sub(scroll));
        let draw_h = item_bottom
            .min(viewport_bottom)
            .saturating_sub(item_top.max(scroll));
        if draw_h == 0 {
            continue;
        }

        let item_area = Rect {
            x: area.x,
            y: draw_y,
            width: area.width,
            height: draw_h,
        };
        let intra_skip = scroll.saturating_sub(item_top);
        let selected = idx == props.selected && !props.mask_cjk;
        draw_thread_item(
            frame,
            item_area,
            thread,
            selected,
            props.palette,
            props.show_avatar,
            intra_skip,
            props.mask_cjk,
            &mut props.images,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_thread_item(
    frame: &mut Frame<'_>,
    area: Rect,
    thread: &ThreadSummary,
    selected: bool,
    palette: Palette,
    show_avatar: bool,
    intra_skip: u16,
    mask_cjk: bool,
    images: &mut Option<&mut ImageCache>,
) {
    let band_h = CONTENT_ROWS.saturating_sub(intra_skip).min(area.height);
    if band_h == 0 {
        return;
    }
    let band = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: band_h,
    };

    let avatar_h = if show_avatar {
        AVATAR_ROWS.saturating_sub(intra_skip).min(band_h)
    } else {
        0
    };

    // Focus bar | avatar | gap | text
    let cols = Layout::horizontal([
        Constraint::Length(BAR_W),
        Constraint::Length(if show_avatar { AVATAR_W } else { 0 }),
        Constraint::Length(if show_avatar { AVATAR_GAP } else { 0 }),
        Constraint::Min(0),
    ])
    .split(band);

    let bar = if selected { "│" } else { " " };
    frame.render_widget(
        Paragraph::new(bar).style(if selected {
            palette.accent_style()
        } else {
            palette.muted_style()
        }),
        cols[0],
    );

    if show_avatar && avatar_h > 0 && intra_skip < AVATAR_ROWS {
        if let Some(cache) = images.as_deref() {
            let (avatar, placeholder) = cache.avatar_entries_for_draw(thread.avatar_url.as_deref());
            let avatar_area = Rect {
                height: avatar_h,
                ..cols[1]
            };
            draw_avatar_entry(frame, avatar_area, avatar, placeholder, palette, intra_skip);
        }
    }

    let right_area = cols[3];
    let mut text_y = right_area.y;
    if intra_skip == 0 && band_h >= 1 {
        let row = Rect {
            x: right_area.x,
            y: text_y,
            width: right_area.width,
            height: 1,
        };
        draw_title_row(frame, row, thread, selected, palette, mask_cjk);
        text_y = text_y.saturating_add(1);
    }
    if intra_skip <= 1 && text_y < right_area.y + band_h {
        let row = Rect {
            x: right_area.x,
            y: text_y,
            width: right_area.width,
            height: 1,
        };
        draw_meta_row(frame, row, thread, selected, palette, mask_cjk);
    }
}

fn draw_title_row(
    frame: &mut Frame<'_>,
    area: Rect,
    thread: &ThreadSummary,
    selected: bool,
    palette: Palette,
    mask_cjk: bool,
) {
    let usable_w = area.width.saturating_sub(CONTENT_RIGHT_PAD);
    let show_counts = usable_w >= MIN_COUNTS_COLS;
    let counts = if show_counts {
        build_counts(thread)
    } else {
        String::new()
    };
    let counts_w = if counts.is_empty() {
        0
    } else {
        str_width(&counts).min(usable_w as usize) as u16 + 1
    };

    let cols = Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(counts_w.min(usable_w)),
    ])
    .split(Rect {
        width: usable_w,
        ..area
    });

    let title_line =
        build_title_with_icons(thread, cols[0].width.saturating_sub(1) as usize, mask_cjk);

    // Focus: left bar + bold accent title (no local accent_bg wash).
    let title_style = if selected {
        palette.selected_style()
    } else {
        palette.title_style(thread.title_color.as_deref())
    };

    frame.render_widget(Paragraph::new(title_line).style(title_style), cols[0]);

    if !counts.is_empty() && cols[1].width > 0 {
        frame.render_widget(
            Paragraph::new(truncate_str(
                &counts,
                cols[1].width.saturating_sub(1) as usize,
            ))
            .style(palette.secondary_style())
            .alignment(Alignment::Right),
            cols[1],
        );
    }
}

fn draw_meta_row(
    frame: &mut Frame<'_>,
    area: Rect,
    thread: &ThreadSummary,
    selected: bool,
    palette: Palette,
    mask_cjk: bool,
) {
    let author = build_author_line(thread, mask_cjk);
    let time = maybe_mask_cjk(&build_time_line(thread), mask_cjk).into_owned();
    let usable_w = area.width.saturating_sub(CONTENT_RIGHT_PAD);
    let _ = selected; // selection is the left bar + title; meta stays secondary
    let meta_style = palette.secondary_style();

    if time.is_empty() {
        frame.render_widget(
            Paragraph::new(truncate_str(&author, usable_w.saturating_sub(1) as usize))
                .style(meta_style),
            area,
        );
        return;
    }

    let time_w = str_width(&time);
    let total = usable_w as usize;

    if time_w >= total {
        frame.render_widget(
            Paragraph::new(truncate_str(&time, total.saturating_sub(1)))
                .style(meta_style)
                .alignment(Alignment::Right),
            area,
        );
        return;
    }

    let left_w = total.saturating_sub(time_w) as u16;
    let left_area = Rect {
        x: area.x,
        y: area.y,
        width: left_w,
        height: area.height,
    };
    let right_area = Rect {
        x: area.x + left_w,
        y: area.y,
        width: time_w as u16,
        height: area.height,
    };

    frame.render_widget(
        Paragraph::new(truncate_str(&author, left_w.saturating_sub(1) as usize)).style(meta_style),
        left_area,
    );
    frame.render_widget(
        Paragraph::new(time)
            .style(meta_style)
            .alignment(Alignment::Right),
        right_area,
    );
}

fn build_title_with_icons(thread: &ThreadSummary, max_cols: usize, mask_cjk: bool) -> String {
    if max_cols == 0 {
        return String::new();
    }

    let raw_title = display_title(&thread.title);
    let title = maybe_mask_cjk(&raw_title, mask_cjk);
    let icons = build_status_icons(thread);
    if icons.is_empty() {
        return truncate_str(title.as_ref(), max_cols);
    }

    let icon_suffix = format!(" {icons}");
    let icon_w = str_width(&icon_suffix);
    let title_budget = max_cols.saturating_sub(icon_w);
    format!(
        "{}{}",
        truncate_str(title.as_ref(), title_budget),
        icon_suffix
    )
}

fn build_status_icons(thread: &ThreadSummary) -> String {
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
    parts.join(" ")
}

fn build_author_line(thread: &ThreadSummary, mask_cjk: bool) -> String {
    let author = maybe_mask_cjk(thread.author.as_deref().unwrap_or("?"), mask_cjk);
    thread
        .thread_type
        .as_deref()
        .filter(|t| !t.is_empty())
        .map(|t| {
            let kind = maybe_mask_cjk(t, mask_cjk);
            format!("{} · {}", author, kind)
        })
        .unwrap_or_else(|| author.into_owned())
}

fn build_counts(thread: &ThreadSummary) -> String {
    let mut parts = Vec::new();
    if let Some(replies) = format_count(thread.reply_count.as_deref()) {
        parts.push(format!("\u{f27a} {replies}"));
    }
    if let Some(views) = format_count(thread.view_count.as_deref()) {
        parts.push(format!("\u{f06e} {views}"));
    }
    parts.join(COUNT_GAP)
}

fn build_time_line(thread: &ThreadSummary) -> String {
    let raw_time = thread
        .time_update
        .as_deref()
        .or(thread.time_create.as_deref())
        .unwrap_or("");
    let time = format_relative_time(raw_time);
    let last = thread
        .last_post
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|s| format!(" · {s}"))
        .unwrap_or_default();
    if time.is_empty() && last.is_empty() {
        String::new()
    } else {
        format!("{time}{last}")
    }
}

const LOADING_FRAMES: [&str; 4] = [
    "── 正在加载 ──",
    "── 正在加载. ──",
    "── 正在加载.. ──",
    "── 正在加载... ──",
];

pub fn draw_loading_indicator(frame: &mut Frame<'_>, area: Rect, palette: Palette, tick: u64) {
    if area.height == 0 {
        return;
    }
    let indicator_area = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(1),
        width: area.width,
        height: 1,
    };
    let frame_idx = ((tick / 3) as usize) % LOADING_FRAMES.len();
    frame.render_widget(
        Paragraph::new(LOADING_FRAMES[frame_idx])
            .style(palette.muted_style())
            .alignment(Alignment::Center),
        indicator_area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use hiptty_render::str_width;

    fn sample_thread() -> ThreadSummary {
        ThreadSummary {
            tid: "1".into(),
            title: "t".into(),
            title_color: None,
            author: Some("admin".into()),
            author_id: None,
            avatar_url: None,
            last_post: Some("techfan".into()),
            reply_count: Some("99".into()),
            view_count: Some("1000".into()),
            time_create: Some("2008-11-28".into()),
            time_update: Some("2026-7-1 12:00".into()),
            thread_type: Some("公告".into()),
            sticky: true,
            with_pic: true,
            is_new: true,
            is_poll: false,
            max_page: 1,
        }
    }

    #[test]
    fn new_marker_omitted() {
        let thread = sample_thread();
        assert!(!build_status_icons(&thread).contains("NEW"));
        let title = build_title_with_icons(&thread, 80, false);
        assert!(!title.contains("NEW"));
    }

    #[test]
    fn status_icons_follow_title() {
        let thread = sample_thread();
        let title = build_title_with_icons(&thread, 80, false);
        assert!(title.contains('\u{f08d}'));
        assert!(title.contains('\u{f03e}'));
        let title_end = title.find('\u{f08d}').unwrap();
        assert!(title_end > 0);
    }

    #[test]
    fn counts_have_spacing() {
        let thread = sample_thread();
        let counts = build_counts(&thread);
        assert!(counts.contains("\u{f27a} 99"));
        assert!(counts.contains(COUNT_GAP));
        assert!(counts.contains("\u{f06e} 1000"));
    }

    #[test]
    fn meta_time_uses_relative_format() {
        let thread = sample_thread();
        let time = build_time_line(&thread);
        assert!(time.contains(" · techfan"));
        assert!(!time.contains("2026-7-1"));
        let relative = time.split(" · ").next().unwrap_or("");
        assert!(relative.contains('前') || relative == "刚刚");
    }

    #[test]
    fn meta_parts_respect_width() {
        let thread = sample_thread();
        let author = build_author_line(&thread, false);
        let counts = build_counts(&thread);
        assert!(str_width(&author) <= 20);
        assert!(str_width(&counts) <= 30);
    }

    #[test]
    fn scroll_follows_selection() {
        assert_eq!(ensure_thread_scroll(0, 0, 5), 0);
        assert_eq!(ensure_thread_scroll(4, 0, 5), 0);
        assert_eq!(ensure_thread_scroll(5, 0, 5), 1);
        assert_eq!(ensure_thread_scroll(9, 1, 5), 5);
        assert_eq!(ensure_thread_scroll(3, 5, 5), 3);
    }
}
