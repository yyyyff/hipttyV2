use hiptty_core::Poll;
use hiptty_render::{str_width, truncate_str, Palette};
use ratatui::{
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

pub fn poll_block_height(poll: &Poll, width: u16) -> u16 {
    if width < 10 {
        return 0;
    }
    let mut h = 2u16; // top border + spacer row
    h = h.saturating_add(poll.options.len() as u16);
    if poll_footer_text(poll).is_some() {
        h += 1;
    }
    h += 1; // bottom border
    h
}

fn poll_footer_text(poll: &Poll) -> Option<String> {
    if let Some(text) = poll.footer.as_deref().filter(|s| !s.is_empty()) {
        return Some(text.to_string());
    }
    if poll.max_answers > 1 {
        return Some(format!("最多可选 {} 项", poll.max_answers));
    }
    None
}

pub fn draw_poll_block(
    frame: &mut Frame<'_>,
    area: ratatui::layout::Rect,
    poll: &Poll,
    palette: Palette,
    skip_rows: u16,
) {
    if area.height == 0 || area.width < 10 {
        return;
    }

    let inner_w = area.width.saturating_sub(4) as usize;
    let top = format!(
        "┌─ \u{f681} {}",
        truncate_str(poll.title.lines().next().unwrap_or(""), inner_w)
    );
    let top_pad = "─".repeat(inner_w.saturating_sub(str_width(&top)));
    let top_line = format!("{top}{top_pad}┐");

    let max_votes = poll
        .options
        .iter()
        .filter_map(|o| o.votes)
        .max()
        .unwrap_or(1)
        .max(1);
    let bar_width = inner_w.saturating_sub(24).max(8);

    let mut logical_row = 0u16;
    let mut y = area.y;

    let draw_row = |frame: &mut Frame<'_>, y: u16, content: Paragraph<'_>| {
        frame.render_widget(
            content,
            ratatui::layout::Rect {
                x: area.x,
                y,
                width: area.width,
                height: 1,
            },
        );
    };

    // Top border
    if logical_row >= skip_rows && y < area.y + area.height {
        draw_row(
            frame,
            y,
            Paragraph::new(top_line.clone()).style(palette.secondary_style()),
        );
        y += 1;
    }
    logical_row += 1;

    // Spacer row
    if logical_row >= skip_rows && y < area.y + area.height {
        draw_row(frame, y, Paragraph::new("│"));
        y += 1;
    }
    logical_row += 1;

    for (idx, option) in poll.options.iter().enumerate() {
        if logical_row >= skip_rows && y < area.y + area.height {
            let votes = option.votes.unwrap_or(0);
            let percent = option.percent.as_deref().unwrap_or("");
            let filled = ((votes as f64 / max_votes as f64) * bar_width as f64).round() as usize;
            let bar = format!(
                "{}{}",
                "█".repeat(filled.min(bar_width)),
                "░".repeat(bar_width.saturating_sub(filled))
            );
            let marker = if idx == 0 { "●" } else { " " };
            let label = truncate_str(&option.label, 12);
            let line = format!("{marker}  {label:<12} {bar}  {votes}票 {percent}");
            let style = if idx == 0 {
                palette.accent_style()
            } else {
                palette.foreground_style()
            };
            draw_row(
                frame,
                y,
                Paragraph::new(Line::from(vec![Span::raw("│ "), Span::styled(line, style)])),
            );
            y += 1;
        }
        logical_row += 1;
    }

    if let Some(footer_text) = poll_footer_text(poll) {
        if logical_row >= skip_rows && y < area.y + area.height {
            draw_row(
                frame,
                y,
                Paragraph::new(Line::from(vec![
                    Span::raw("│ "),
                    Span::styled(footer_text, palette.secondary_style()),
                ])),
            );
            y += 1;
        }
        logical_row += 1;
    }

    if logical_row >= skip_rows && y < area.y + area.height {
        let bottom = format!("└{}┘", "─".repeat(inner_w.max(1)));
        draw_row(
            frame,
            y,
            Paragraph::new(bottom).style(palette.muted_style()),
        );
    }
}
