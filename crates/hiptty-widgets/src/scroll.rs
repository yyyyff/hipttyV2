use hiptty_render::Palette;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    Frame,
};
use ratatui::widgets::Widget;
use tui_scrollbar::{GlyphSet, ScrollLengths, TrackClickBehavior};

/// Lines moved per mouse wheel tick (smooth scroll, not j/k steps).
pub const WHEEL_LINES: i32 = 3;

pub const SCROLLBAR_COLS: u16 = 1;

pub fn split_content_scrollbar(area: Rect) -> (Rect, Rect) {
    if area.width <= SCROLLBAR_COLS || area.height == 0 {
        return (area, Rect::default());
    }
    let chunks = Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(SCROLLBAR_COLS),
    ])
    .split(area);
    (chunks[0], chunks[1])
}

pub fn list_content_lines(item_count: usize, item_height: u16) -> u16 {
    (item_count as u16).saturating_mul(item_height)
}

pub fn max_scroll_lines(content_lines: u16, viewport_h: u16) -> u16 {
    content_lines.saturating_sub(viewport_h)
}

pub fn apply_scroll_delta(offset: u16, delta: i32, max: u16) -> u16 {
    let next = offset as i32 + delta;
    next.clamp(0, max as i32) as u16
}

/// Keep `selected` visible while preserving offset when possible.
pub fn ensure_thread_scroll_lines(
    selected: usize,
    scroll_lines: u16,
    viewport_h: u16,
    item_height: u16,
) -> u16 {
    if viewport_h == 0 || item_height == 0 {
        return 0;
    }
    let selected_top = (selected as u16).saturating_mul(item_height);
    let selected_bottom = selected_top.saturating_add(item_height);
    if selected_top < scroll_lines {
        return selected_top;
    }
    if selected_bottom > scroll_lines.saturating_add(viewport_h) {
        return selected_bottom.saturating_sub(viewport_h);
    }
    scroll_lines
}

pub fn item_index_at_row(rel_y: u16, scroll_lines: u16, item_height: u16) -> usize {
    let line = scroll_lines.saturating_add(rel_y);
    (line / item_height.max(1)) as usize
}

/// Clamp line offset without re-centering on the selected item (for wheel / scrollbar).
pub fn clamp_thread_scroll_lines(scroll_lines: u16, max: u16) -> u16 {
    scroll_lines.min(max)
}

/// Snap to item top boundary so j/k never leaves a partial row clipped at the viewport top.
pub fn align_scroll_to_item_top(scroll_lines: u16, item_height: u16) -> u16 {
    if item_height == 0 {
        return scroll_lines;
    }
    scroll_lines
        .saturating_sub(scroll_lines % item_height)
}

fn selected_item_visible(
    selected: usize,
    scroll_lines: u16,
    viewport_h: u16,
    item_height: u16,
) -> bool {
    if item_height == 0 || viewport_h == 0 {
        return true;
    }
    let sel_top = (selected as u16).saturating_mul(item_height);
    let sel_bottom = sel_top.saturating_add(item_height);
    sel_top >= scroll_lines && sel_bottom <= scroll_lines.saturating_add(viewport_h)
}

/// Align upward to the next item boundary (never leaves a partial row at the viewport top).
pub fn align_scroll_to_item_top_ceil(scroll_lines: u16, item_height: u16) -> u16 {
    if item_height == 0 {
        return scroll_lines;
    }
    let rem = scroll_lines % item_height;
    if rem == 0 {
        scroll_lines
    } else {
        scroll_lines.saturating_add(item_height.saturating_sub(rem))
    }
}

/// j/k follow: keep selection visible; prefer item-aligned tops without clipping the selection.
pub fn snap_scroll_to_item(
    selected: usize,
    scroll_lines: u16,
    viewport_h: u16,
    item_height: u16,
) -> u16 {
    let ensured = ensure_thread_scroll_lines(selected, scroll_lines, viewport_h, item_height);
    let floor = align_scroll_to_item_top(ensured, item_height);
    if selected_item_visible(selected, floor, viewport_h, item_height) {
        return floor;
    }
    let ceil = align_scroll_to_item_top_ceil(ensured, item_height);
    if selected_item_visible(selected, ceil, viewport_h, item_height) {
        ceil
    } else {
        ensured
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ScrollChrome {
    pub content: Rect,
    pub bar: Rect,
    pub content_len: u16,
    pub viewport_len: u16,
    pub offset: u16,
    pub shown: bool,
}

impl ScrollChrome {
    pub fn max_offset(&self) -> u16 {
        max_scroll_lines(self.content_len, self.viewport_len)
    }

    pub fn apply_command(&self, command: ScrollCommand) -> u16 {
        let ScrollCommand::SetOffset(offset) = command;
        offset.min(self.max_offset() as usize) as u16
    }

    pub fn lengths(&self) -> ScrollLengths {
        ScrollLengths {
            content_len: self.content_len as usize,
            viewport_len: self.viewport_len as usize,
        }
    }
}

pub fn draw_vertical_scrollbar(
    frame: &mut Frame<'_>,
    bar_area: Rect,
    palette: Palette,
    lengths: ScrollLengths,
    offset: u16,
) {
    if bar_area.width == 0 || bar_area.height == 0 {
        return;
    }
    if lengths.content_len <= lengths.viewport_len {
        return;
    }
    let scrollbar = tui_scrollbar::ScrollBar::vertical(lengths)
        .arrows(tui_scrollbar::ScrollBarArrows::None)
        .track_click_behavior(TrackClickBehavior::JumpToClick)
        .offset(offset as usize)
        .scroll_step(1)
        .track_style(palette.dim_style())
        .thumb_style(palette.secondary_style())
        .glyph_set(GlyphSet::unicode());
    scrollbar.render(bar_area, frame.buffer_mut());
}

pub use tui_scrollbar::{ScrollBar, ScrollBarArrows, ScrollBarInteraction, ScrollCommand};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wheel_delta_clamps() {
        assert_eq!(apply_scroll_delta(5, 3, 10), 8);
        assert_eq!(apply_scroll_delta(9, 3, 10), 10);
        assert_eq!(apply_scroll_delta(2, -5, 10), 0);
    }

    #[test]
    fn ensure_follows_selection() {
        let h = 3u16;
        let vh = 9u16;
        assert_eq!(ensure_thread_scroll_lines(5, 0, vh, h), 9);
        assert_eq!(ensure_thread_scroll_lines(2, 12, vh, h), 6);
    }

    #[test]
    fn snap_keeps_last_item_fully_visible_with_aligned_top() {
        let h = 3u16;
        let vh = 28u16;
        // 11 items (33 lines); ceil-align to 6 keeps #11 visible without a clipped top row.
        assert_eq!(snap_scroll_to_item(10, 0, vh, h), 6);
    }

    #[test]
    fn snap_aligns_top_when_selection_fits() {
        let h = 3u16;
        let vh = 28u16;
        assert_eq!(snap_scroll_to_item(5, 12, vh, h), 12);
        assert_eq!(snap_scroll_to_item(5, 14, vh, h), 12);
    }
}