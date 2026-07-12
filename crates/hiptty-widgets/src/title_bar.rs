use hiptty_render::{maybe_mask_cjk, str_width, truncate_str, Palette};
use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::forum_tabs::{draw_forum_tabs, forum_tab_hits, ForumTabHits, ForumTabsProps};
use crate::layout::title_bar_rows;
use crate::logo::draw_title_logo;

/// Nerd Font: bell (notifications).
const ICON_NOTIFICATIONS: char = '\u{f0a2}';
/// Nerd Font: envelope (PM).
const ICON_PM: char = '\u{f0e0}';

/// Which unread icon the pointer is over (hover chrome only; open on click).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitleUnreadHover {
    Notifications,
    Pm,
}

pub struct TitleBarProps<'a> {
    pub palette: Palette,
    pub tick: u64,
    pub username: Option<&'a str>,
    pub has_notifications: bool,
    pub has_pm: bool,
    /// Pointer over 🔔 / ✉️ — accent+underline; does not open the page.
    pub unread_hover: Option<TitleUnreadHover>,
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

/// Unread icons for row 2 (right-aligned). Empty when neither flag is set.
fn unread_icons_label(has_notifications: bool, has_pm: bool) -> String {
    let mut s = String::new();
    if has_notifications {
        s.push(ICON_NOTIFICATIONS);
    }
    if has_pm {
        if !s.is_empty() {
            s.push(' ');
        }
        s.push(ICON_PM);
    }
    s
}

/// Width of the trailing unread-icons cluster (includes leading gap when non-empty).
fn unread_icons_cluster_width(has_notifications: bool, has_pm: bool) -> u16 {
    let label = unread_icons_label(has_notifications, has_pm);
    if label.is_empty() {
        return 0;
    }
    // Leading space separates icons from tabs / breadcrumb.
    (str_width(&label) as u16).saturating_add(1)
}

/// Hit targets for icons packed at the right edge of `icons_area`.
fn unread_icon_hits(
    icons_area: Rect,
    has_notifications: bool,
    has_pm: bool,
) -> (Option<Rect>, Option<Rect>) {
    if icons_area.width == 0 || icons_area.height == 0 {
        return (None, None);
    }
    let label = unread_icons_label(has_notifications, has_pm);
    if label.is_empty() {
        return (None, None);
    }
    let total_w = str_width(&label).min(icons_area.width as usize) as u16;
    let mut x = icons_area
        .x
        .saturating_add(icons_area.width.saturating_sub(total_w));

    let mut notif = None;
    let mut pm = None;

    if has_notifications {
        notif = Some(Rect {
            x,
            y: icons_area.y,
            width: 1,
            height: 1,
        });
        x = x.saturating_add(1);
        if has_pm {
            x = x.saturating_add(1); // space between icons
        }
    }
    if has_pm {
        pm = Some(Rect {
            x,
            y: icons_area.y,
            width: 1,
            height: 1,
        });
    }
    (notif, pm)
}

/// Split row2 into left content and optional right unread-icons strip.
fn row2_split(row2: Rect, has_notifications: bool, has_pm: bool) -> (Rect, Rect) {
    let icons_w = unread_icons_cluster_width(has_notifications, has_pm).min(row2.width);
    if icons_w == 0 {
        return (
            row2,
            Rect {
                x: row2.x.saturating_add(row2.width),
                y: row2.y,
                width: 0,
                height: row2.height,
            },
        );
    }
    let chunks = Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(icons_w),
    ])
    .split(row2);
    (chunks[0], chunks[1])
}

pub fn title_bar_hits(area: Rect, props: &TitleBarProps<'_>) -> TitleBarHits {
    let [row1, row2] = title_bar_rows(area);
    if row1.width == 0 {
        return TitleBarHits::default();
    }

    let mut hits = TitleBarHits::default();
    let (left, icons_area) = row2_split(row2, props.has_notifications, props.has_pm);

    if let Some(tabs) = props.forum_tabs.as_ref() {
        hits.forum_tabs = forum_tab_hits(left, tabs);
    }

    let (notif, pm) = unread_icon_hits(icons_area, props.has_notifications, props.has_pm);
    hits.notifications = notif;
    hits.pm = pm;
    hits
}

pub fn draw_title_bar(frame: &mut Frame<'_>, area: Rect, props: TitleBarProps<'_>) {
    let [row1, row2] = title_bar_rows(area);

    // Row 1: logo + username only (right).
    let row1_cols = Layout::horizontal([Constraint::Length(8), Constraint::Min(0)]).split(row1);
    draw_title_logo(frame, row1_cols[0], props.palette, props.tick);

    if let Some(user) = props.username {
        frame.render_widget(
            Paragraph::new(user)
                .style(props.palette.secondary_style())
                .alignment(Alignment::Right),
            row1_cols[1],
        );
    }

    // Row 2: tabs / breadcrumb on the left; unread icons (if any) right-aligned.
    let (left, icons_area) = row2_split(row2, props.has_notifications, props.has_pm);

    if icons_area.width > 0 && (props.has_notifications || props.has_pm) {
        draw_unread_icons(frame, icons_area, &props);
    }

    if let Some(tabs) = props.forum_tabs {
        draw_forum_tabs(frame, left, tabs);
    } else {
        draw_breadcrumb_row(frame, left, &props);
    }
}

fn draw_unread_icons(frame: &mut Frame<'_>, area: Rect, props: &TitleBarProps<'_>) {
    let idle = props.palette.secondary_style();
    // Accent only — icons look worse with underline.
    let hover = props.palette.accent_style().add_modifier(Modifier::BOLD);

    let mut spans = Vec::new();
    // Leading space is part of the cluster width reserved by row2_split.
    spans.push(Span::styled(" ", idle));
    if props.has_notifications {
        let style = if props.unread_hover == Some(TitleUnreadHover::Notifications) {
            hover
        } else {
            idle
        };
        spans.push(Span::styled(ICON_NOTIFICATIONS.to_string(), style));
    }
    if props.has_pm {
        if props.has_notifications {
            spans.push(Span::styled(" ", idle));
        }
        let style = if props.unread_hover == Some(TitleUnreadHover::Pm) {
            hover
        } else {
            idle
        };
        spans.push(Span::styled(ICON_PM.to_string(), style));
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).alignment(Alignment::Right),
        area,
    );
}

fn draw_breadcrumb_row(frame: &mut Frame<'_>, area: Rect, props: &TitleBarProps<'_>) {
    if area.width == 0 {
        return;
    }
    let right_w = props
        .breadcrumb_right
        .map(|t| str_width(t).min(area.width as usize) as u16 + 1)
        .unwrap_or(0);
    let row2_cols = Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(right_w.min(area.width)),
    ])
    .split(area);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unread_icons_cluster_empty_without_flags() {
        assert_eq!(unread_icons_cluster_width(false, false), 0);
        assert!(unread_icons_label(false, false).is_empty());
    }

    #[test]
    fn unread_icons_cluster_includes_gap() {
        // "🔔" or "✉" width 1 + leading gap 1.
        assert_eq!(unread_icons_cluster_width(true, false), 2);
        assert_eq!(unread_icons_cluster_width(false, true), 2);
        // "🔔 ✉" = 1 + 1 + 1 + gap 1 = 4
        assert_eq!(unread_icons_cluster_width(true, true), 4);
    }

    #[test]
    fn icon_hits_are_on_row2_not_row1() {
        let area = Rect {
            x: 0,
            y: 0,
            width: 40,
            height: 2,
        };
        let props = TitleBarProps {
            palette: Palette::default(),
            tick: 0,
            username: Some("alice"),
            has_notifications: true,
            has_pm: true,
            unread_hover: None,
            breadcrumb: "Feed",
            breadcrumb_right: None,
            forum_tabs: None,
            mask_cjk: false,
        };
        let hits = title_bar_hits(area, &props);
        let n = hits.notifications.expect("notif hit");
        let p = hits.pm.expect("pm hit");
        assert_eq!(n.y, 1, "notifications on row 2");
        assert_eq!(p.y, 1, "pm on row 2");
        assert!(n.x < p.x, "notif left of pm");
        assert!(p.x + p.width <= area.width);
    }
}
