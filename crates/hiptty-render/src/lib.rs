pub mod text;
pub mod theme;

pub use text::{display_title, format_count, str_width, truncate_str};
pub use theme::{logo_char_color, logo_color, parse_hex_color, Palette};
