use hiptty_render::{str_width, truncate_str, Palette};
use ratatui::{
    layout::{Alignment, Constraint, Layout, Position, Rect},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::ime::set_ime_cursor;

/// One shortcut shown in the status bar left cluster.
/// Lower [`priority`] is kept longer when width is tight (0 = core).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyHint {
    pub key: &'static str,
    pub label: &'static str,
    pub priority: u8,
}

impl KeyHint {
    pub const fn new(key: &'static str, label: &'static str, priority: u8) -> Self {
        Self {
            key,
            label,
            priority,
        }
    }
}

/// Inline `:` command-line state.
#[derive(Debug, Clone, Copy)]
pub struct CommandLineProps<'a> {
    pub input: &'a str,
    /// Byte offset of the caret inside `input`.
    pub cursor: usize,
}

pub struct StatusBarProps<'a> {
    pub palette: Palette,
    /// Normal mode shortcuts (ignored when [`command`] is set).
    pub hints: &'a [KeyHint],
    /// Right-aligned status: loading / page indicator / command suggestions.
    pub right: Option<&'a str>,
    /// When set, status bar becomes the command line (`:` + input).
    pub command: Option<CommandLineProps<'a>>,
}

pub fn draw_status_bar(frame: &mut Frame<'_>, area: Rect, props: StatusBarProps<'_>) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let right = props.right.unwrap_or("");
    let right_w = if right.is_empty() {
        0
    } else {
        str_width(right).min(area.width as usize) as u16
    };
    // Gap between left cluster and right status when both present.
    let gap = if right_w > 0 && area.width > right_w + 1 {
        1u16
    } else {
        0
    };
    let left_w = area.width.saturating_sub(right_w.saturating_add(gap));

    let chunks = Layout::horizontal([
        Constraint::Length(left_w),
        Constraint::Length(gap),
        Constraint::Length(right_w),
    ])
    .split(area);

    if let Some(cmd) = props.command {
        let (line, caret) = command_line(cmd, chunks[0], props.palette);
        frame.render_widget(Paragraph::new(line), chunks[0]);
        set_ime_cursor(frame, caret);
    } else {
        let fitted = fit_hints(props.hints, chunks[0].width as usize);
        let line = hints_line(&fitted, props.palette);
        frame.render_widget(Paragraph::new(line), chunks[0]);
    }

    if right_w > 0 {
        frame.render_widget(
            Paragraph::new(truncate_str(right, right_w as usize))
                .style(props.palette.muted_style())
                .alignment(Alignment::Right),
            chunks[2],
        );
    }
}

fn command_line(
    cmd: CommandLineProps<'_>,
    area: Rect,
    palette: Palette,
) -> (Line<'static>, Position) {
    let max_cols = area.width as usize;
    if max_cols == 0 {
        return (
            Line::default(),
            Position {
                x: area.x,
                y: area.y,
            },
        );
    }
    let prefix = ":";
    let cursor = cmd.cursor.min(cmd.input.len());
    let (before, after) = cmd.input.split_at(cursor);

    // Fit around the caret: keep caret visible, prefer truncating the head.
    // Reserve 1 col for the terminal caret itself.
    let fixed = str_width(prefix) + 1;
    let budget = max_cols.saturating_sub(fixed);
    let before_w = str_width(before);
    let after_w = str_width(after);

    let (shown_before, shown_after) = if before_w + after_w <= budget {
        (before.to_string(), after.to_string())
    } else if after_w >= budget {
        (String::new(), truncate_str(after, budget))
    } else {
        let before_budget = budget.saturating_sub(after_w);
        let trimmed = trim_left_to_width(before, before_budget);
        (trimmed, after.to_string())
    };

    let caret_x = area
        .x
        .saturating_add(str_width(prefix) as u16)
        .saturating_add(str_width(&shown_before) as u16)
        .min(area.x.saturating_add(area.width.saturating_sub(1)));

    let line = Line::from(vec![
        Span::styled(prefix.to_string(), palette.accent_style()),
        Span::styled(shown_before, palette.foreground_style()),
        Span::styled(shown_after, palette.foreground_style()),
    ]);
    (
        line,
        Position {
            x: caret_x,
            y: area.y,
        },
    )
}

fn trim_left_to_width(s: &str, max_cols: usize) -> String {
    if max_cols == 0 {
        return String::new();
    }
    if str_width(s) <= max_cols {
        return s.to_string();
    }
    // Drop leading chars until it fits; optionally prefix with ellipsis if room.
    let mut chars: Vec<char> = s.chars().collect();
    while !chars.is_empty() && str_width(&chars.iter().collect::<String>()) > max_cols {
        chars.remove(0);
    }
    let out: String = chars.into_iter().collect();
    if max_cols >= 1 && str_width(s) > max_cols {
        // If we trimmed, try to mark with …
        if str_width(&out) < max_cols {
            return format!("…{out}");
        }
        // Replace first display col with …
        let mut cs: Vec<char> = out.chars().collect();
        if !cs.is_empty() {
            cs[0] = '…';
        }
        return cs.into_iter().collect();
    }
    out
}

fn hints_line(hints: &[KeyHint], palette: Palette) -> Line<'static> {
    let mut spans = Vec::new();
    for (i, hint) in hints.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  ".to_string(), palette.muted_style()));
        }
        spans.push(Span::styled(
            hint.key.to_string(),
            palette.accent_style(),
        ));
        if !hint.label.is_empty() {
            spans.push(Span::styled(
                format!(" {}", hint.label),
                palette.secondary_style(),
            ));
        }
    }
    Line::from(spans)
}

/// Drop lowest-priority (highest `priority` value) hints until the cluster fits.
pub fn fit_hints(hints: &[KeyHint], max_cols: usize) -> Vec<KeyHint> {
    if max_cols == 0 || hints.is_empty() {
        return Vec::new();
    }
    let mut kept: Vec<KeyHint> = hints.to_vec();
    while !kept.is_empty() && hints_width(&kept) > max_cols {
        let max_p = kept.iter().map(|h| h.priority).max().unwrap_or(0);
        if let Some(idx) = kept.iter().rposition(|h| h.priority == max_p) {
            if kept.len() == 1 {
                break;
            }
            kept.remove(idx);
        } else {
            break;
        }
    }
    kept
}

fn hints_width(hints: &[KeyHint]) -> usize {
    if hints.is_empty() {
        return 0;
    }
    let mut w = 0usize;
    for (i, hint) in hints.iter().enumerate() {
        if i > 0 {
            w += 2;
        }
        w += str_width(hint.key);
        if !hint.label.is_empty() {
            w += 1 + str_width(hint.label);
        }
    }
    w
}

/// Animated "加载中" label driven by app tick (≈50ms).
pub fn loading_status_label(tick: u64) -> String {
    let dots = match (tick / 3) % 4 {
        0 => "",
        1 => ".",
        2 => "..",
        _ => "...",
    };
    format!("加载中{dots}")
}

/// Format `page/max` when meaningful; `None` if not yet loaded.
pub fn page_status_label(page: u32, max_page: u32) -> Option<String> {
    if page == 0 && max_page == 0 {
        return None;
    }
    let page = page.max(1);
    let max = max_page.max(page);
    Some(format!("{page}/{max}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_secondary_hints_first() {
        let hints = [
            KeyHint::new("j/k", "导航", 0),
            KeyHint::new("Enter", "打开", 0),
            KeyHint::new(":", "命令", 2),
            KeyHint::new("f", "更多", 2),
        ];
        let full = fit_hints(&hints, 80);
        assert_eq!(full.len(), 4);

        let tight = fit_hints(&hints, 20);
        assert!(tight.iter().all(|h| h.priority == 0));
        assert!(!tight.is_empty());
    }

    #[test]
    fn page_label_skips_unloaded() {
        assert_eq!(page_status_label(0, 0), None);
        assert_eq!(page_status_label(1, 12).as_deref(), Some("1/12"));
        assert_eq!(page_status_label(3, 0).as_deref(), Some("3/3"));
    }
}
