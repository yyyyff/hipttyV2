use hiptty_core::forum_name;
use hiptty_render::Palette;
use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
    Frame,
};

use crate::modal::{begin_modal, draw_label_value_row, draw_menu_item};

pub const MAIN_MENU_ITEMS: &[&str] = &[
    "私信",
    "通知",
    "我的帖子",
    "我的回复",
    "我的收藏",
    "设置",
    "退出",
];
pub const MAIN_MENU_HINTS: &str = "j/k 移动 | Enter 确认 | Esc 关闭";

pub struct MainMenuProps {
    pub palette: Palette,
    pub selected: usize,
}

pub fn draw_main_menu(frame: &mut Frame<'_>, area: Rect, props: MainMenuProps) -> Vec<Rect> {
    let item_rows = MAIN_MENU_ITEMS.len() as u16;
    let width = area.width.min(36);
    // Inner: items + spacer + hints; dialog adds top/bottom border (2 rows).
    let height = (item_rows + 4).min(area.height.saturating_sub(4));
    let modal = begin_modal(
        frame,
        area,
        props.palette,
        "菜单",
        width,
        height,
        None,
    );

    let chunks = Layout::vertical([
        Constraint::Length(item_rows),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(modal.body);

    let mut hits = Vec::new();
    for (i, item) in MAIN_MENU_ITEMS.iter().enumerate() {
        let row = Rect {
            x: chunks[0].x,
            y: chunks[0].y.saturating_add(i as u16),
            width: chunks[0].width,
            height: 1,
        };
        if row.y >= chunks[0].y.saturating_add(chunks[0].height) {
            break;
        }
        frame.render_widget(
            Paragraph::new(draw_menu_item(props.palette, item, i == props.selected)),
            row,
        );
        hits.push(row);
    }

    frame.render_widget(
        Paragraph::new(MAIN_MENU_HINTS)
            .style(props.palette.muted_style())
            .alignment(Alignment::Center),
        chunks[2],
    );

    hits
}

pub struct SettingsProps<'a> {
    pub palette: Palette,
    pub settings: &'a hiptty_core::AppSettings,
    pub selected: usize,
    pub blacklist_count: usize,
}

pub fn draw_settings_panel(frame: &mut Frame<'_>, area: Rect, props: SettingsProps<'_>) {
    let width = area.width.min(46);
    let height = 11.min(area.height.saturating_sub(4));
    let modal = begin_modal(
        frame,
        area,
        props.palette,
        "设置",
        width,
        height,
        Some("j/k 移动  Enter 修改  Esc 关闭"),
    );

    let blacklist = format!("{} 人", props.blacklist_count);
    let rows: [(&str, &str); 4] = [
        (
            "默认版块 1",
            forum_name(props.settings.default_forums[0]).unwrap_or("?"),
        ),
        (
            "默认版块 2",
            forum_name(props.settings.default_forums[1]).unwrap_or("?"),
        ),
        (
            "默认版块 3",
            forum_name(props.settings.default_forums[2]).unwrap_or("?"),
        ),
        ("黑名单", &blacklist),
    ];
    let lines: Vec<Line> = rows
        .iter()
        .enumerate()
        .map(|(i, (label, value))| {
            draw_label_value_row(props.palette, label, value, i == props.selected, 12)
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), modal.body);
}

pub struct CommandBarProps<'a> {
    pub palette: Palette,
    pub input: &'a str,
}

pub fn draw_command_bar(frame: &mut Frame<'_>, area: Rect, props: CommandBarProps<'_>) {
    let bar_h = 3.min(area.height);
    crate::modal::draw_modal_backdrop(frame, area, props.palette);
    let bar = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(bar_h),
        width: area.width,
        height: bar_h,
    };
    frame.render_widget(Clear, bar);

    let block = ratatui::widgets::Block::default()
        .borders(ratatui::widgets::Borders::ALL)
        .border_style(props.palette.accent_style())
        .title(" 命令 ")
        .style(props.palette.modal_surface_style());
    let inner = block.inner(bar);
    frame.render_widget(block, bar);

    let chunks = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(inner);
    let input = format!(":{}█", props.input);
    frame.render_widget(
        Paragraph::new(input).style(props.palette.foreground_style()),
        chunks[0],
    );
    frame.render_widget(
        Paragraph::new(":exit :q :login :logout :pm :notif :search <词>")
            .style(props.palette.muted_style()),
        chunks[1],
    );
}

pub struct SearchPromptProps<'a> {
    pub palette: Palette,
    pub input: &'a str,
    pub forum_name: &'a str,
}

pub fn draw_search_prompt(frame: &mut Frame<'_>, area: Rect, props: SearchPromptProps<'_>) {
    let width = area.width.min(52);
    let height = 7.min(area.height.saturating_sub(4));
    let modal = begin_modal(
        frame,
        area,
        props.palette,
        "搜索",
        width,
        height,
        Some("Enter 搜索  Esc 取消"),
    );

    let chunks = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(modal.body);
    frame.render_widget(
        Paragraph::new(format!("在 {} 中搜索", props.forum_name))
            .style(props.palette.secondary_style()),
        chunks[0],
    );
    let input_line = Line::from(vec![
        Span::styled("输入  ", props.palette.muted_style()),
        Span::styled(props.input, props.palette.foreground_style()),
        Span::styled("█", props.palette.accent_style()),
    ]);
    frame.render_widget(Paragraph::new(input_line), chunks[1]);
}