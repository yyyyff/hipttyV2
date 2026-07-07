use hiptty_render::Palette;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    widgets::Paragraph,
    Frame,
};

pub fn draw_dim_rule(frame: &mut Frame<'_>, area: Rect, palette: Palette) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let line = "─".repeat(area.width as usize);
    frame.render_widget(Paragraph::new(line).style(palette.muted_style()), area);
}

/// Title (2) · rule · content · rule · status (1)
pub fn main_layout(area: Rect) -> [Rect; 5] {
    let chunks = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);
    [chunks[0], chunks[1], chunks[2], chunks[3], chunks[4]]
}

pub fn title_bar_rows(area: Rect) -> [Rect; 2] {
    let chunks = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(area);
    [chunks[0], chunks[1]]
}
