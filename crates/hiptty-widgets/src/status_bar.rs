use hiptty_render::Palette;
use ratatui::{layout::Rect, widgets::Paragraph, Frame};

pub fn draw_status_bar(frame: &mut Frame<'_>, area: Rect, palette: Palette, hints: &str) {
    frame.render_widget(Paragraph::new(hints).style(palette.dim_style()), area);
}
