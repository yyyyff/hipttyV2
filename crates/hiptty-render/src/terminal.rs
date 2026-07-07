use std::io::{self, Write};

use ratatui::layout::Rect;

/// True when running inside Windows Terminal (including WSL sessions launched from WT).
pub fn is_windows_terminal() -> bool {
    std::env::var_os("WT_SESSION").is_some()
        || std::env::var_os("WT_PROFILE_ID").is_some()
        || std::env::var("TERM_PROGRAM")
            .ok()
            .is_some_and(|v| v.eq_ignore_ascii_case("WindowsTerminal"))
}

/// Drop Kitty graphics layers that survive cell scroll/redraw in Windows Terminal.
pub fn clear_terminal_graphics() -> io::Result<()> {
    if !is_windows_terminal() {
        return Ok(());
    }
    let mut out = io::stdout();
    out.write_all(b"\x1b_Ga=d,d=all\x1b\\")?;
    out.flush()
}

/// Delete visible Kitty placements but keep image data in terminal memory.
///
/// Used on scroll: the same encoded image data is reused, only the placeholder cells are
/// redrawn at the new position. This avoids re-encoding the image on every scroll.
///
/// `d=a` alone is not enough on Windows Terminal: placements whose top-left has scrolled
/// off-screen but whose bottom row is still visible are not deleted. We therefore also send
/// `d=y` for every terminal row to force deletion of any placement intersecting the screen.
pub fn clear_terminal_placements(height: u16) -> io::Result<()> {
    if !is_windows_terminal() || height == 0 {
        return Ok(());
    }
    clear_terminal_placements_in_area(Rect::new(0, 0, u16::MAX, height))
}

/// Delete Kitty placements intersecting `area` (1-indexed terminal rows).
///
/// Image data stays in terminal memory; the next draw pass re-places visible rows only.
pub fn clear_terminal_placements_in_area(area: Rect) -> io::Result<()> {
    if !is_windows_terminal() || area.width == 0 || area.height == 0 {
        return Ok(());
    }
    let mut out = io::stdout();
    for row in 0..area.height {
        let y = u32::from(area.y.saturating_add(row).saturating_add(1));
        write!(out, "\x1b_Ga=d,d=y,y={y}\x1b\\")?;
    }
    out.flush()
}
