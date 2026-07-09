pub mod content;
pub mod fill;
pub mod terminal;
pub mod text;
pub mod theme;
pub mod wrap;

pub use content::{
    floor_header_rows, floor_header_rows_with_edit, format_signature, quote_header_label,
    render_content_node, render_post_content_lines, signature_line,
};
pub use fill::{
    begin_frame_graphics, clear_content_viewport, clear_graphics_in_area, clear_rect,
    erase_graphics_guard_band, fill_area_bg, fill_area_spaces,
};
pub use terminal::{
    clear_terminal_graphics, clear_terminal_placements, clear_terminal_placements_in_area,
    is_windows_terminal,
};
pub use text::{
    display_title, format_count, format_relative_time, format_relative_time_at, is_cjk_char,
    mask_cjk, mask_line_cjk, maybe_mask_cjk, str_width, strip_published_prefix, truncate_str,
};
pub use theme::{logo_char_color, logo_color, parse_hex_color, Palette};
pub use wrap::{collapse_empty_lines, pad_line_left, wrap_plain, wrap_segments, StyledSegment};
