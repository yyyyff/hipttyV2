use hiptty_render::Palette;
use ratatui::{
    layout::{Alignment, Rect},
    widgets::Paragraph,
    Frame,
};

pub fn draw_status_bar(frame: &mut Frame<'_>, area: Rect, palette: Palette, hints: &str) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    frame.render_widget(
        Paragraph::new(hints)
            .style(palette.secondary_style())
            .alignment(Alignment::Left),
        area,
    );
}
