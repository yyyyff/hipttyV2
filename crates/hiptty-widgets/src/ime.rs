//! Terminal caret placement for IME (input method) popup positioning.
//!
//! Drawing a fake `█` does not move the terminal cursor; IMEs then fall back to
//! the origin (top-left). Call [`set_ime_cursor`] after rendering a focused input.

use hiptty_render::str_width;
use ratatui::{
    layout::{Position, Rect},
    Frame,
};
use ratatui_textarea::TextArea;

/// Show the terminal cursor at `pos` after this frame (also anchors the IME window).
pub fn set_ime_cursor(frame: &mut Frame<'_>, pos: Position) {
    frame.set_cursor_position(pos);
}

/// Caret after a fixed-width prefix and `text` within `area` (single-line field).
pub fn cursor_after_text(area: Rect, prefix_cols: u16, text: &str) -> Position {
    if area.width == 0 || area.height == 0 {
        return Position {
            x: area.x,
            y: area.y,
        };
    }
    let text_w = str_width(text) as u16;
    let x = area
        .x
        .saturating_add(prefix_cols)
        .saturating_add(text_w)
        .min(area.x.saturating_add(area.width.saturating_sub(1)));
    Position { x, y: area.y }
}

/// Same algorithm ratatui-textarea uses to keep the caret inside the viewport.
pub fn next_scroll_top(prev_top: u16, cursor: u16, len: u16) -> u16 {
    if len == 0 {
        return 0;
    }
    if cursor < prev_top {
        cursor
    } else if prev_top.saturating_add(len) <= cursor {
        cursor.saturating_add(1).saturating_sub(len)
    } else {
        prev_top
    }
}

/// Map a [`TextArea`] caret to absolute screen coordinates (wrap-none, no line numbers).
///
/// `view_top` is `(row, col)` scroll mirror updated in lockstep with the widget.
pub fn textarea_cursor_position(
    textarea: &TextArea<'_>,
    area: Rect,
    view_top: &mut (u16, u16),
) -> Position {
    if area.width == 0 || area.height == 0 {
        return Position {
            x: area.x,
            y: area.y,
        };
    }
    let sc = textarea.screen_cursor();
    let cursor_row = sc.row.min(u16::MAX as usize) as u16;
    let cursor_col = sc.col.min(u16::MAX as usize) as u16;
    view_top.0 = next_scroll_top(view_top.0, cursor_row, area.height);
    view_top.1 = next_scroll_top(view_top.1, cursor_col, area.width);
    let x = area
        .x
        .saturating_add(cursor_col.saturating_sub(view_top.1))
        .min(area.x.saturating_add(area.width.saturating_sub(1)));
    let y = area
        .y
        .saturating_add(cursor_row.saturating_sub(view_top.0))
        .min(area.y.saturating_add(area.height.saturating_sub(1)));
    Position { x, y }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_top_keeps_cursor_visible() {
        assert_eq!(next_scroll_top(0, 3, 5), 0);
        assert_eq!(next_scroll_top(0, 5, 5), 1);
        assert_eq!(next_scroll_top(3, 2, 5), 2);
    }
}
