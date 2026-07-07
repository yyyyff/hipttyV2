use hiptty_core::{forum_name, FORUMS};
use hiptty_render::Palette;
use ratatui::{
    layout::{Alignment, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};

pub struct ForumPickerProps<'a> {
    pub palette: Palette,
    pub default_forums: &'a [u32],
    pub selected: usize,
    pub current_fid: u32,
}

pub fn draw_forum_picker(frame: &mut Frame<'_>, area: Rect, props: ForumPickerProps<'_>) {
    let popup_width = area.width.min(50);
    let popup_height = area.height.min(22);
    let x = area.x + (area.width.saturating_sub(popup_width)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_height)) / 2;
    let popup = Rect {
        x,
        y,
        width: popup_width,
        height: popup_height,
    };

    frame.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(props.palette.muted_style())
        .title(" 切换版块 ");
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let mut items: Vec<ListItem> = Vec::new();
    items.push(ListItem::new(Line::from(Span::styled(
        "默认版块",
        props.palette.secondary_style().add_modifier(Modifier::BOLD),
    ))));
    items.push(ListItem::new(Line::from(Span::styled(
        "──────────",
        props.palette.muted_style(),
    ))));

    let mut idx = 0;
    for &fid in props.default_forums {
        let name = forum_name(fid).unwrap_or("?");
        let marker = if fid == props.current_fid {
            "● "
        } else {
            "  "
        };
        let selected = idx == props.selected;
        let style = if selected {
            props.palette.selected_style()
        } else {
            props.palette.foreground_style()
        };
        items.push(ListItem::new(Line::from(Span::styled(
            format!("{marker}{name}"),
            style,
        ))));
        idx += 1;
    }

    items.push(ListItem::new(Line::from("")));
    items.push(ListItem::new(Line::from(Span::styled(
        "全部版块",
        props.palette.secondary_style().add_modifier(Modifier::BOLD),
    ))));
    items.push(ListItem::new(Line::from(Span::styled(
        "──────────",
        props.palette.muted_style(),
    ))));

    for forum in FORUMS {
        if props.default_forums.contains(&forum.id) {
            continue;
        }
        let selected = idx == props.selected;
        let style = if selected {
            props.palette.selected_style()
        } else {
            props.palette.foreground_style()
        };
        items.push(ListItem::new(Line::from(Span::styled(
            format!("  {}", forum.name),
            style,
        ))));
        idx += 1;
    }

    let list_height = inner.height.saturating_sub(2);
    frame.render_widget(
        List::new(items),
        Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: list_height,
        },
    );

    let footer = Rect {
        x: inner.x,
        y: inner.y + list_height,
        width: inner.width,
        height: 1,
    };
    frame.render_widget(
        Paragraph::new("j/k 移动  Enter 切换  Esc 关闭")
            .style(props.palette.muted_style())
            .alignment(Alignment::Center),
        footer,
    );
}

pub fn forum_picker_entries(default_forums: &[u32]) -> Vec<u32> {
    let mut entries: Vec<u32> = default_forums.to_vec();
    for forum in FORUMS {
        if !entries.contains(&forum.id) {
            entries.push(forum.id);
        }
    }
    entries
}
