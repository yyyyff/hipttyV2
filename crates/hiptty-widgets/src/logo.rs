use hiptty_render::{logo_color, Palette};
use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

const LOGIN_LOGO: [&str; 2] = ["█░█ █  █▀█ █▀▄ █▀█", "█▀█ █  █▀▀ █▄▀ █▀█"];

pub fn draw_login_logo(frame: &mut Frame<'_>, area: Rect, palette: Palette) {
    let hi = Style::default().fg(palette.logo_hi);
    let pda = Style::default().fg(palette.logo_pda);
    let lines: Vec<Line> = LOGIN_LOGO
        .iter()
        .map(|row| {
            let chars: Vec<Span> = row
                .chars()
                .enumerate()
                .map(|(i, c)| {
                    let style = if i < 5 { hi } else { pda };
                    Span::styled(c.to_string(), style)
                })
                .collect();
            Line::from(chars)
        })
        .collect();
    frame.render_widget(Paragraph::new(lines).centered(), area);
}

pub fn draw_title_logo(frame: &mut Frame<'_>, area: Rect, palette: Palette, tick: u64) {
    let color = logo_color(tick, palette);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "HIPDA",
            Style::default().fg(color),
        ))),
        area,
    );
}
