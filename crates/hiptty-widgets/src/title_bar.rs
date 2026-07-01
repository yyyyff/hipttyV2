use hiptty_render::{truncate_str, Palette};
use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    widgets::Paragraph,
    Frame,
};

use crate::layout::title_bar_rows;
use crate::logo::draw_title_logo;

pub struct TitleBarProps<'a> {
    pub palette: Palette,
    pub tick: u64,
    pub username: Option<&'a str>,
    pub has_notifications: bool,
    pub has_pm: bool,
    pub breadcrumb: &'a str,
    pub breadcrumb_right: Option<&'a str>,
}

pub fn draw_title_bar(frame: &mut Frame<'_>, area: Rect, props: TitleBarProps<'_>) {
    let [row1, row2] = title_bar_rows(area);

    let row1_cols = Layout::horizontal([Constraint::Length(8), Constraint::Min(0)]).split(row1);
    draw_title_logo(frame, row1_cols[0], props.palette, props.tick);

    let mut right = String::new();
    if let Some(user) = props.username {
        right.push_str(user);
    }
    if props.has_notifications {
        if !right.is_empty() {
            right.push(' ');
        }
        right.push('\u{f0a2}');
    }
    if props.has_pm {
        if !right.is_empty() {
            right.push(' ');
        }
        right.push('\u{f0e0}');
    }

    if !right.is_empty() {
        frame.render_widget(
            Paragraph::new(right)
                .style(props.palette.secondary_style())
                .alignment(Alignment::Right),
            row1_cols[1],
        );
    }

    let row2_cols = Layout::horizontal([Constraint::Min(0), Constraint::Length(18)]).split(row2);
    let breadcrumb = truncate_str(
        props.breadcrumb,
        row2_cols[0].width.saturating_sub(1) as usize,
    );
    frame.render_widget(
        Paragraph::new(breadcrumb).style(props.palette.primary_style()),
        row2_cols[0],
    );

    if let Some(right_text) = props.breadcrumb_right {
        frame.render_widget(
            Paragraph::new(right_text)
                .style(props.palette.secondary_style())
                .alignment(Alignment::Right),
            row2_cols[1],
        );
    }
}
