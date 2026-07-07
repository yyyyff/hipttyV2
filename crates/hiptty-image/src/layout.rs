use hiptty_render::is_windows_terminal;
use ratatui::layout::{Rect, Size};
use ratatui_image::picker::{Picker, ProtocolType};

use crate::cache::ImageKind;

pub const AVATAR_COLS: u16 = 4;
pub const AVATAR_ROWS: u16 = 2;
pub const SMILEY_COLS: u16 = 3;
pub const SMILEY_ROWS: u16 = 1;

/// Rows to leave empty at the bottom of a graphics viewport before the status chrome.
///
/// - **Sixel** ([ratatui-image#57](https://github.com/ratatui/ratatui-image/issues/57)):
///   graphics on the terminal's last row can trigger an unwanted scroll.
/// - **Kitty**: layered pixels can extend below placeholder cells (mdfried model).
pub fn graphics_bottom_margin(picker: &Picker, kind: ImageKind) -> u16 {
    match effective_protocol(picker, kind) {
        ProtocolType::Sixel | ProtocolType::Kitty => 1,
        _ => 0,
    }
}

pub fn shrink_viewport_bottom(viewport: Rect, margin: u16) -> Rect {
    let mut area = viewport;
    if margin > 0 && area.height > margin {
        area.height -= margin;
    }
    area
}

fn effective_protocol(picker: &Picker, kind: ImageKind) -> ProtocolType {
    if is_windows_terminal() && matches!(kind, ImageKind::Content { .. }) {
        return ProtocolType::Sixel;
    }
    picker.protocol_type()
}

pub fn avatar_cell_size() -> Size {
    Size::new(AVATAR_COLS, AVATAR_ROWS)
}

pub fn smiley_cell_size() -> Size {
    Size::new(SMILEY_COLS, SMILEY_ROWS)
}

/// Fit a post image into `max_cols` width, preserving aspect ratio.
pub fn content_image_cell_size(picker: &Picker, pixel_w: u32, pixel_h: u32, max_cols: u16) -> Size {
    if max_cols == 0 || pixel_w == 0 || pixel_h == 0 {
        return Size::new(1, 1);
    }
    let font = picker.font_size();
    let max_px_w = u32::from(max_cols) * u32::from(font.width);
    let scale = (max_px_w as f64 / pixel_w as f64).min(1.0);
    let scaled_w = (pixel_w as f64 * scale).round().max(1.0) as u32;
    let scaled_h = (pixel_h as f64 * scale).round().max(1.0) as u32;
    let cols = scaled_w.div_ceil(u32::from(font.width)).max(1) as u16;
    let rows = scaled_h.div_ceil(u32::from(font.height)).max(1) as u16;
    Size::new(cols.min(max_cols).max(1), rows.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui_image::picker::Picker;

    #[test]
    fn sixel_and_kitty_reserve_bottom_row() {
        let mut picker = Picker::halfblocks();
        picker.set_protocol_type(ProtocolType::Sixel);
        assert_eq!(graphics_bottom_margin(&picker, ImageKind::Avatar), 1);

        picker.set_protocol_type(ProtocolType::Kitty);
        assert_eq!(graphics_bottom_margin(&picker, ImageKind::Smiley), 1);
    }

    #[test]
    fn halfblocks_has_no_bottom_margin() {
        let picker = Picker::halfblocks();
        assert_eq!(graphics_bottom_margin(&picker, ImageKind::Avatar), 0);
        assert_eq!(graphics_bottom_margin(&picker, ImageKind::Smiley), 0);
    }

    #[test]
    fn shrink_viewport_bottom_respects_margin() {
        let viewport = Rect::new(0, 0, 80, 10);
        let shrunk = shrink_viewport_bottom(viewport, 1);
        assert_eq!(shrunk.height, 9);
    }
}
