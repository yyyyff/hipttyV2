use ratatui::{layout::Rect, style::Color, text::Line, widgets::Paragraph, Frame};

use crate::terminal::clear_terminal_placements_in_area;

/// Delete Kitty placements and reset cells in `area` (text buffer unchanged).
pub fn clear_graphics_in_area(frame: &mut Frame<'_>, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let _ = clear_terminal_placements_in_area(area);
    clear_rect(frame, area);
}

/// Clear text buffer and Kitty placements in `area` before drawing scrollable content.
pub fn clear_content_viewport(frame: &mut Frame<'_>, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let _ = clear_terminal_placements_in_area(area);
    clear_rect(frame, area);
    fill_area_spaces(frame, area);
}

/// Paint every row in `area` with spaces so scrolled-out graphics rows are overwritten.
///
/// Ratatui starts each frame with an empty buffer, but terminal graphics (Kitty) can outlive
/// placeholder cells on Windows Terminal. Explicit space rows force a diff that clears the strip.
pub fn fill_area_spaces(frame: &mut Frame<'_>, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let blank = " ".repeat(area.width as usize);
    let lines = std::iter::repeat_n(Line::from(blank), area.height as usize).collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines), area);
}

/// Wipe the bottom `rows` of a content pane: delete Kitty placements, reset cells, fill spaces.
///
/// **Deprecated — do not use in the draw loop.** This was an early Windows Terminal Kitty
/// workaround that fought with placement-based scroll clearing and caused image ghosting.
/// Sixel last-row scroll is handled separately via `hiptty_image::graphics_bottom_margin`.
pub fn erase_graphics_guard_band(frame: &mut Frame<'_>, content_area: Rect, rows: u16) {
    let rows = rows.min(content_area.height);
    if rows == 0 {
        return;
    }
    let band = Rect {
        x: content_area.x,
        y: content_area
            .y
            .saturating_add(content_area.height.saturating_sub(rows)),
        width: content_area.width,
        height: rows,
    };
    let _ = clear_terminal_placements_in_area(band);
    clear_rect(frame, band);
    fill_area_spaces(frame, band);
}

/// Reset every cell in `area` so terminal graphics (Kitty/Sixel placeholders) are removed.
pub fn clear_rect(frame: &mut Frame<'_>, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let bottom = area.y.saturating_add(area.height);
    let right = area.x.saturating_add(area.width);
    for y in area.y..bottom {
        for x in area.x..right {
            if let Some(cell) = frame.buffer_mut().cell_mut((x, y)) {
                cell.reset();
                cell.set_char(' ');
                cell.set_fg(Color::Reset);
                cell.set_bg(Color::Reset);
            }
        }
    }
}
