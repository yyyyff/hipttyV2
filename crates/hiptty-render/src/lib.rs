pub mod text;
pub mod theme;

pub use text::{display_title, format_count, truncate_str};
pub use theme::{logo_color, parse_hex_color, Palette};
