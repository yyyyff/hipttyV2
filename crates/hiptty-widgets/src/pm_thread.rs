use hiptty_core::ListItem;
use hiptty_render::{
    clear_content_viewport, format_relative_time, maybe_mask_cjk, str_width, truncate_str, Palette,
};
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
    pub scroll_lines: u16,
    pub mask_cjk: bool,
}

pub fn pm_thread_capacity(content_height: u16) -> usize {
    if content_height < PM_ITEM_HEIGHT {
        return 0;
    }
    (content_height / PM_ITEM_HEIGHT) as usize
}

pub fn draw_pm_thread(frame: &mut Frame<'_>, area: Rect, props: PmThreadProps<'_>) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    clear_content_viewport(frame, area);

    let scroll = props.scroll_lines;
    let viewport_bottom = scroll.saturating_add(area.height);

    for (idx, msg) in props.messages.iter().enumerate() {
        let item_top = (idx as u16).saturating_mul(PM_ITEM_HEIGHT);
        let item_bottom = item_top.saturating_add(PM_ITEM_HEIGHT);
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
        draw_pm_message(
            frame,
            item_area,
            msg,
            props.my_username,
            selected,
            props.palette,
            intra_skip,
            props.mask_cjk,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_pm_message(
    frame: &mut Frame<'_>,
    area: Rect,
    msg: &ListItem,
    my_username: &str,
    selected: bool,
    palette: Palette,
    intra_skip: u16,
    mask_cjk: bool,
) {
    let is_mine = msg.author.as_deref() == Some(my_username);
    let bar_style = if is_mine {
        palette.accent_style()
    } else {
        palette.muted_style()
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
    let author = maybe_mask_cjk(msg.author.as_deref().unwrap_or("?"), mask_cjk);
    let time = msg
        .time
        .as_deref()
        .map(format_relative_time)
        .unwrap_or_default();
    let time = maybe_mask_cjk(&time, mask_cjk);
    let header = format!("{author}  {time}");
    let text = msg.title.as_deref().or(msg.info.as_deref()).unwrap_or("");
    let plain = strip_html_tags(text);
    let plain = maybe_mask_cjk(&plain, mask_cjk);
    let width = body.width.saturating_sub(1) as usize;
    let header = truncate_str(header.as_ref(), width);
    let body_text = truncate_str(plain.as_ref(), width);

    let header_style = if selected {
        palette.accent_style().add_modifier(Modifier::BOLD)
    } else {
        palette.secondary_style()
    };
    let mut lines = Vec::new();
    if intra_skip == 0 {
        lines.push(Line::from(Span::styled(header, header_style)));
    }
    if intra_skip <= 1 {
        lines.push(Line::from(Span::styled(
            body_text,
            palette.foreground_style(),
        )));
    }
    if !lines.is_empty() {
        frame.render_widget(Paragraph::new(lines), body);
    }
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
