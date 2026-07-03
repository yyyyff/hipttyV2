use hiptty_core::ListItem;
use hiptty_render::{format_relative_time, str_width, truncate_str, Palette};
use ratatui::{
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

pub const PM_ITEM_HEIGHT: u16 = 4;

pub struct PmThreadProps<'a> {
    pub palette: Palette,
    pub messages: &'a [ListItem],
    pub my_username: &'a str,
    pub selected: usize,
    pub scroll: usize,
}

pub fn pm_thread_capacity(content_height: u16) -> usize {
    if content_height < PM_ITEM_HEIGHT {
        return 0;
    }
    (content_height / PM_ITEM_HEIGHT) as usize
}

pub fn draw_pm_thread(frame: &mut Frame<'_>, area: Rect, props: PmThreadProps<'_>) {
    if area.height < PM_ITEM_HEIGHT {
        return;
    }
    let capacity = pm_thread_capacity(area.height);
    let mut y = area.y;
    for (idx, msg) in props
        .messages
        .iter()
        .enumerate()
        .skip(props.scroll)
        .take(capacity)
    {
        let item_h = PM_ITEM_HEIGHT.min(area.y + area.height - y);
        if item_h < 3 {
            break;
        }
        let item_area = Rect {
            x: area.x,
            y,
            width: area.width,
            height: item_h,
        };
        draw_pm_message(
            frame,
            item_area,
            msg,
            props.my_username,
            idx == props.selected,
            props.palette,
        );
        y += PM_ITEM_HEIGHT;
    }
}

fn draw_pm_message(
    frame: &mut Frame<'_>,
    area: Rect,
    msg: &ListItem,
    my_username: &str,
    selected: bool,
    palette: Palette,
) {
    let is_mine = msg.author.as_deref() == Some(my_username);
    let bar_style = if is_mine {
        palette.accent_style()
    } else {
        palette.dim_style()
    };
    let bar = Rect {
        x: area.x,
        y: area.y,
        width: 1,
        height: area.height.saturating_sub(1),
    };
    frame.render_widget(Paragraph::new("│").style(bar_style), bar);

    let body = Rect {
        x: area.x + 1,
        y: area.y,
        width: area.width.saturating_sub(1),
        height: area.height.saturating_sub(1),
    };
    let author = msg.author.as_deref().unwrap_or("?");
    let time = msg
        .time
        .as_deref()
        .map(format_relative_time)
        .unwrap_or_default();
    let header = format!("{author}  {time}");
    let text = msg
        .title
        .as_deref()
        .or(msg.info.as_deref())
        .unwrap_or("");
    let plain = strip_html_tags(text);
    let width = body.width.saturating_sub(1) as usize;
    let header = truncate_str(&header, width);
    let body_text = truncate_str(&plain, width);

    let header_style = if selected {
        palette.accent_style().add_modifier(Modifier::BOLD)
    } else {
        palette.secondary_style()
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(header, header_style)),
            Line::from(Span::styled(body_text, palette.primary_style())),
        ]),
        body,
    );
    let _ = str_width;
}

fn strip_html_tags(input: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.trim().to_string()
}