use hiptty_core::{forum_name, Theme};
use hiptty_render::{str_width, truncate_str, Palette};
use ratatui::{
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

pub const MAIN_MENU_ITEMS: &[&str] = &[
    "私信",
    "通知",
    "我的帖子",
    "我的回复",
    "我的收藏",
    "设置",
];

pub struct MainMenuProps {
    pub palette: Palette,
    pub selected: usize,
}

pub fn draw_main_menu(frame: &mut Frame<'_>, area: Rect, props: MainMenuProps) {
    let width = area.width.min(36);
    let height = (MAIN_MENU_ITEMS.len() as u16 + 4).min(area.height.saturating_sub(4));
    let dialog = centered_rect(width, height, area);
    frame.render_widget(Clear, dialog);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(props.palette.accent_style())
        .title("菜单");
    let inner = block.inner(dialog);
    frame.render_widget(block, dialog);
    let mut lines = vec![Line::from(Span::styled(
        "Esc 关闭",
        props.palette.dim_style(),
    ))];
    for (i, item) in MAIN_MENU_ITEMS.iter().enumerate() {
        let prefix = if i == props.selected { "● " } else { "  " };
        let style = if i == props.selected {
            props.palette.accent_style().add_modifier(Modifier::BOLD)
        } else {
            props.palette.primary_style()
        };
        lines.push(Line::from(Span::styled(format!("{prefix}{item}"), style)));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "j/k  Enter  Esc",
        props.palette.dim_style(),
    )));
    frame.render_widget(Paragraph::new(lines), inner);
}

pub struct HelpOverlayProps {
    pub palette: Palette,
}

pub fn draw_help_overlay(frame: &mut Frame<'_>, area: Rect, props: HelpOverlayProps) {
    let width = area.width.min(58);
    let height = area.height.min(18);
    let dialog = centered_rect(width, height, area);
    frame.render_widget(Clear, dialog);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(props.palette.accent_style())
        .title("快捷键");
    let inner = block.inner(dialog);
    frame.render_widget(block, dialog);
    let text = "\
导航\n\
j/k ↑↓      上下移动\n\
Enter       打开/确认\n\
b / Esc     返回/关闭菜单\n\
\n\
帖子\n\
r 回复  n 新帖  f 版块  / 搜索\n\
q 引用  e 编辑  d 删除\n\
\n\
全局\n\
? 帮助  : 命令  Esc 菜单\n\
\n\
编辑器\n\
Ctrl+S 发送  Ctrl+I 插图\n\
\n\
Enter / Esc 关闭";
    frame.render_widget(
        Paragraph::new(text).style(props.palette.primary_style()),
        inner,
    );
}

pub struct SettingsProps<'a> {
    pub palette: Palette,
    pub settings: &'a hiptty_core::AppSettings,
    pub selected: usize,
    pub blacklist_count: usize,
}

pub fn draw_settings_panel(frame: &mut Frame<'_>, area: Rect, props: SettingsProps<'_>) {
    let width = area.width.min(44);
    let height = 12.min(area.height.saturating_sub(4));
    let dialog = centered_rect(width, height, area);
    frame.render_widget(Clear, dialog);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(props.palette.accent_style())
        .title("设置");
    let inner = block.inner(dialog);
    frame.render_widget(block, dialog);

    let theme = match props.settings.theme {
        Theme::Dark => "Dark",
        Theme::Light => "Light",
    };
    let rows = [
        format!("主题         [{theme}]"),
        format!(
            "默认版块 1   [{}]",
            forum_name(props.settings.default_forums[0]).unwrap_or("?")
        ),
        format!(
            "默认版块 2   [{}]",
            forum_name(props.settings.default_forums[1]).unwrap_or("?")
        ),
        format!(
            "默认版块 3   [{}]",
            forum_name(props.settings.default_forums[2]).unwrap_or("?")
        ),
        format!("黑名单       [{} 人]", props.blacklist_count),
    ];
    let mut lines = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        let prefix = if i == props.selected { "▸ " } else { "  " };
        let style = if i == props.selected {
            props.palette.accent_style()
        } else {
            props.palette.primary_style()
        };
        lines.push(Line::from(Span::styled(format!("{prefix}{row}"), style)));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Enter 修改  Esc 关闭",
        props.palette.dim_style(),
    )));
    frame.render_widget(Paragraph::new(lines), inner);
}

pub struct CommandBarProps<'a> {
    pub palette: Palette,
    pub input: &'a str,
}

pub fn draw_command_bar(frame: &mut Frame<'_>, area: Rect, props: CommandBarProps<'_>) {
    let bar_h = 2.min(area.height);
    let bar = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(bar_h),
        width: area.width,
        height: bar_h,
    };
    frame.render_widget(Clear, bar);
    let line = format!(":{}█", props.input);
    frame.render_widget(
        Paragraph::new(truncate_str(&line, bar.width.saturating_sub(1) as usize))
            .style(props.palette.accent_style()),
        bar,
    );
    let _ = str_width;
}

pub struct SearchPromptProps<'a> {
    pub palette: Palette,
    pub input: &'a str,
    pub forum_name: &'a str,
}

pub fn draw_search_prompt(frame: &mut Frame<'_>, area: Rect, props: SearchPromptProps<'_>) {
    let width = area.width.min(50);
    let height = 5;
    let dialog = centered_rect(width, height, area);
    frame.render_widget(Clear, dialog);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(props.palette.accent_style())
        .title("搜索");
    let inner = block.inner(dialog);
    frame.render_widget(block, dialog);
    let prompt = format!("在 {} 搜索: {}█", props.forum_name, props.input);
    frame.render_widget(
        Paragraph::new(prompt).style(props.palette.primary_style()),
        inner,
    );
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}