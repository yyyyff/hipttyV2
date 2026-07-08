use hiptty_core::ListItem;
use hiptty_image::{draw_avatar_entry, ImageCache, AVATAR_COLS, AVATAR_ROWS};
use hiptty_render::{
    clear_content_viewport, format_relative_time, maybe_mask_cjk, str_width, truncate_str, Palette,
};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

pub const SIMPLE_ITEM_HEIGHT: u16 = 3;

pub struct SimpleListProps<'a> {
    pub palette: Palette,
    pub items: &'a [ListItem],
    pub selected: usize,
    pub scroll_lines: u16,
    pub show_avatar: bool,
    pub images: Option<&'a mut ImageCache>,
    pub mask_cjk: bool,
}

pub fn simple_list_capacity(content_height: u16) -> usize {
    if content_height < SIMPLE_ITEM_HEIGHT {
        return 0;
    }
    (content_height / SIMPLE_ITEM_HEIGHT) as usize
}

pub fn draw_simple_list(frame: &mut Frame<'_>, area: Rect, mut props: SimpleListProps<'_>) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    clear_content_viewport(frame, area);

    let scroll = props.scroll_lines;
    let viewport_bottom = scroll.saturating_add(area.height);

    for (idx, item) in props.items.iter().enumerate() {
        let item_top = (idx as u16).saturating_mul(SIMPLE_ITEM_HEIGHT);
        let item_bottom = item_top.saturating_add(SIMPLE_ITEM_HEIGHT);
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
        draw_simple_item(
            frame,
            item_area,
            item,
            selected,
            props.palette,
            props.show_avatar,
            intra_skip,
            props.mask_cjk,
            &mut props.images,
        );
    }
}

fn draw_simple_item(
    frame: &mut Frame<'_>,
    area: Rect,
    item: &ListItem,
    selected: bool,
    palette: Palette,
    show_avatar: bool,
    intra_skip: u16,
    mask_cjk: bool,
    images: &mut Option<&mut ImageCache>,
) {
    const CONTENT_ROWS: u16 = 2;
    let visible_rows = CONTENT_ROWS.saturating_sub(intra_skip).min(area.height);
    if visible_rows == 0 {
        return;
    }

    let selector = Rect {
        x: area.x,
        y: area.y,
        width: 1,
        height: visible_rows,
    };
    let bar = if selected { "│" } else { " " };
    frame.render_widget(
        Paragraph::new(bar).style(if selected {
            palette.accent_style()
        } else {
            palette.muted_style()
        }),
        selector,
    );

    let body = Rect {
        x: area.x + 1,
        y: area.y,
        width: area.width.saturating_sub(1),
        height: visible_rows,
    };
    let cols = Layout::horizontal([
        Constraint::Length(if show_avatar { AVATAR_COLS } else { 0 }),
        Constraint::Min(0),
    ])
    .split(body);

    if show_avatar && intra_skip < AVATAR_ROWS {
        if let Some(cache) = images.as_deref() {
            let (avatar, placeholder) = cache.avatar_entries_for_draw(item.avatar_url.as_deref());
            let avatar_area = Rect {
                height: AVATAR_ROWS.saturating_sub(intra_skip).min(area.height),
                ..cols[0]
            };
            draw_avatar_entry(frame, avatar_area, avatar, placeholder, palette, intra_skip);
        }
    }

    let text_w = cols[1].width.saturating_sub(1) as usize;
    let new_mark = if item.is_new { "● " } else { "  " };
    let author = maybe_mask_cjk(item.author.as_deref().unwrap_or("系统"), mask_cjk);
    let preview = maybe_mask_cjk(
        item.title.as_deref().or(item.info.as_deref()).unwrap_or(""),
        mask_cjk,
    );
    let line1 = format!("{new_mark}{author}  {preview}");
    let line1 = truncate_str(&line1, text_w);
    let time = item
        .time
        .as_deref()
        .map(format_relative_time)
        .unwrap_or_default();
    let line2 = truncate_str(maybe_mask_cjk(&time, mask_cjk).as_ref(), text_w);

    let line1_style = if selected {
        palette.accent_style().add_modifier(Modifier::BOLD)
    } else if item.is_new {
        palette.accent_style()
    } else {
        palette.foreground_style()
    };

    let mut lines = Vec::new();
    if intra_skip == 0 && visible_rows >= 1 {
        lines.push(Line::from(Span::styled(line1, line1_style)));
    }
    if intra_skip <= 1 && lines.len() < visible_rows as usize {
        lines.push(Line::from(Span::styled(line2, palette.secondary_style())));
    }
    if !lines.is_empty() {
        frame.render_widget(Paragraph::new(lines), cols[1]);
    }
    let _ = str_width;
}
