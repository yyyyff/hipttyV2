use hiptty_render::{maybe_mask_cjk, str_width, truncate_str, Palette};
use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    widgets::Paragraph,
    Frame,
};

use crate::forum_tabs::{draw_forum_tabs, forum_tab_hits, ForumTabHits, ForumTabsProps};
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
    pub forum_tabs: Option<ForumTabsProps<'a>>,
    pub mask_cjk: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct TitleBarHits {
    pub notifications: Option<Rect>,
    pub pm: Option<Rect>,
    pub forum_tabs: ForumTabHits,
}

pub fn title_bar_hits(area: Rect, props: &TitleBarProps<'_>) -> TitleBarHits {
    let [row1, row2] = title_bar_rows(area);
    if row1.width == 0 {
        return TitleBarHits::default();
    }

    let mut hits = TitleBarHits::default();
    if let Some(tabs) = props.forum_tabs.as_ref() {
        hits.forum_tabs = forum_tab_hits(row2, tabs);
    }

    let has_user = props.username.is_some();
    if !has_user && !props.has_notifications && !props.has_pm {
        return hits;
    }

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

    let total_w = str_width(&right).min(row1.width as usize) as u16;
    let mut x = row1.x.saturating_add(row1.width.saturating_sub(total_w));

    if let Some(user) = props.username {
        x = x.saturating_add(str_width(user).min(row1.width as usize) as u16);
        if props.has_notifications || props.has_pm {
            x = x.saturating_add(1);
        }
    }

    if props.has_notifications {
        hits.notifications = Some(Rect {
            x,
            y: row1.y,
            width: 1,
            height: 1,
        });
        x = x.saturating_add(1);
        if props.has_pm {
            x = x.saturating_add(1);
        }
    }

    if props.has_pm {
        hits.pm = Some(Rect {
            x,
            y: row1.y,
            width: 1,
            height: 1,
        });
    }

    hits
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

    if let Some(tabs) = props.forum_tabs {
        draw_forum_tabs(frame, row2, tabs);
        return;
    }

    let right_w = props
        .breadcrumb_right
        .map(|t| str_width(t).min(row2.width as usize) as u16 + 1)
        .unwrap_or(0);
    let row2_cols = Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(right_w.min(row2.width)),
    ])
    .split(row2);
    let breadcrumb = truncate_str(
        maybe_mask_cjk(props.breadcrumb, props.mask_cjk).as_ref(),
        row2_cols[0].width.saturating_sub(1) as usize,
    );
    frame.render_widget(
        Paragraph::new(breadcrumb).style(props.palette.foreground_style()),
        row2_cols[0],
    );

    if let Some(right_text) = props.breadcrumb_right {
        frame.render_widget(
            Paragraph::new(truncate_str(
                maybe_mask_cjk(right_text, props.mask_cjk).as_ref(),
                row2_cols[1].width.saturating_sub(1) as usize,
            ))
            .style(props.palette.secondary_style())
            .alignment(Alignment::Right),
            row2_cols[1],
        );
    }
}
