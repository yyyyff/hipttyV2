use hiptty_core::ListItem;
use hiptty_image::{draw_avatar_entry, ImageCache, AVATAR_COLS};
use hiptty_render::{format_relative_time, str_width, truncate_str, Palette};
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
    pub scroll: usize,
    pub show_avatar: bool,
    pub images: Option<&'a mut ImageCache>,
}

pub fn simple_list_capacity(content_height: u16) -> usize {
    if content_height < SIMPLE_ITEM_HEIGHT {
        return 0;
    }
    (content_height / SIMPLE_ITEM_HEIGHT) as usize
}

pub fn draw_simple_list(frame: &mut Frame<'_>, area: Rect, mut props: SimpleListProps<'_>) {
    if area.height < SIMPLE_ITEM_HEIGHT {
        return;
    }
    let capacity = simple_list_capacity(area.height);
    let mut y = area.y;
    for (idx, item) in props
        .items
        .iter()
        .enumerate()
        .skip(props.scroll)
        .take(capacity)
    {
        let item_h = SIMPLE_ITEM_HEIGHT.min(area.y + area.height - y);
        if item_h < 2 {
            break;
        }
        let item_area = Rect {
            x: area.x,
            y,
            width: area.width,
            height: item_h,
        };
        draw_simple_item(
            frame,
            item_area,
            item,
            idx == props.selected,
            props.palette,
            props.show_avatar,
            &mut props.images,
        );
        y += SIMPLE_ITEM_HEIGHT;
    }
}

fn draw_simple_item(
    frame: &mut Frame<'_>,
    area: Rect,
    item: &ListItem,
    selected: bool,
    palette: Palette,
    show_avatar: bool,
    images: &mut Option<&mut ImageCache>,
) {
    let selector = Rect {
        x: area.x,
        y: area.y,
        width: 1,
        height: 2.min(area.height),
    };
    let bar = if selected { "│" } else { " " };
    frame.render_widget(
        Paragraph::new(bar).style(if selected {
            palette.accent_style()
        } else {
            palette.dim_style()
        }),
        selector,
    );

    let body = Rect {
        x: area.x + 1,
        y: area.y,
        width: area.width.saturating_sub(1),
        height: 2.min(area.height),
    };
    let cols = Layout::horizontal([
        Constraint::Length(if show_avatar { AVATAR_COLS } else { 0 }),
        Constraint::Min(0),
    ])
    .split(body);

    if show_avatar {
        if let Some(cache) = images.as_deref() {
            let (avatar, placeholder) = cache.avatar_entries_for_draw(item.avatar_url.as_deref());
            draw_avatar_entry(frame, cols[0], avatar, placeholder, palette, 0);
        }
    }

    let text_w = cols[1].width.saturating_sub(1) as usize;
    let new_mark = if item.is_new { "● " } else { "  " };
    let author = item.author.as_deref().unwrap_or("系统");
    let preview = item
        .title
        .as_deref()
        .or(item.info.as_deref())
        .unwrap_or("");
    let line1 = format!("{new_mark}{author}  {preview}");
    let line1 = truncate_str(&line1, text_w);
    let time = item
        .time
        .as_deref()
        .map(format_relative_time)
        .unwrap_or_default();
    let line2 = truncate_str(&time, text_w);

    let line1_style = if selected {
        palette.accent_style().add_modifier(Modifier::BOLD)
    } else if item.is_new {
        palette.accent_style()
    } else {
        palette.primary_style()
    };

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(line1, line1_style)),
            Line::from(Span::styled(line2, palette.secondary_style())),
        ]),
        cols[1],
    );
    let _ = str_width;
}