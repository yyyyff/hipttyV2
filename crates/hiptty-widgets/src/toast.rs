use hiptty_render::{fill_area_bg, str_width, wrap_plain, Palette};
use ratatui::{
    layout::Rect,
    style::Style,
    text::Line,
    widgets::{Clear, Paragraph},
    Frame,
};

pub const TOAST_TICK_MS: u64 = 50;
pub const TOAST_SUCCESS_TICKS: u64 = 40;
pub const TOAST_ERROR_TICKS: u64 = 100;

/// Inner text column budget before wrapping to a second line.
const TOAST_WRAP_INNER: usize = 30;

pub struct ToastProps<'a> {
    pub palette: Palette,
    pub message: &'a str,
    pub is_error: bool,
    pub tick: u64,
    pub started_at: u64,
    pub duration_ticks: u64,
}

pub fn draw_toast(frame: &mut Frame<'_>, area: Rect, props: ToastProps<'_>) {
    if area.width < 10 || area.height < 4 {
        return;
    }

    let text_style = if props.is_error {
        props.palette.error_style()
    } else {
        props.palette.success_style()
    };
    let icon = if props.is_error { "✗ " } else { "✓ " };
    let text = format!("{icon}{}", props.message);

    let screen_max_inner = area.width.saturating_sub(6) as usize;
    let max_inner = TOAST_WRAP_INNER.min(screen_max_inner).max(8);
    let (wrapped, inner_w) = fit_toast_text(&text, max_inner, text_style);
    if inner_w == 0 {
        return;
    }

    let content_lines = wrapped.len().max(1) as u16;
    let toast_width = inner_w.saturating_add(2).min(area.width as usize) as u16;
    let height = content_lines + 2;

    let x = area
        .x
        .saturating_add(area.width.saturating_sub(toast_width + 2));
    let y = area
        .y
        .saturating_add(area.height.saturating_sub(height + 1));
    let toast_area = Rect {
        x,
        y,
        width: toast_width,
        height,
    };

    frame.render_widget(Clear, toast_area);

    let border_style = if props.is_error {
        props.palette.error_style()
    } else {
        props.palette.success_style()
    };
    let surface = Style::default().bg(props.palette.accent_bg);
    fill_area_bg(frame, toast_area, surface);

    let remaining_frac = toast_remaining_fraction(props);
    draw_toast_border(frame, toast_area, border_style, remaining_frac);

    if toast_area.width < 3 || toast_area.height < 3 {
        return;
    }
    let inner = Rect {
        x: toast_area.x + 1,
        y: toast_area.y + 1,
        width: toast_area.width - 2,
        height: toast_area.height - 2,
    };
    frame.render_widget(Paragraph::new(wrapped), inner);
}

fn fit_toast_text(
    text: &str,
    max_inner: usize,
    style: Style,
) -> (Vec<Line<'static>>, usize) {
    let natural = str_width(text);
    let wrap_cols = if natural <= max_inner {
        natural.max(1)
    } else {
        max_inner
    };
    let lines = wrap_plain(text, wrap_cols, style);
    let inner_w = lines
        .iter()
        .map(line_display_width)
        .max()
        .unwrap_or(1);
    (lines, inner_w)
}

fn line_display_width(line: &Line<'_>) -> usize {
    line.spans
        .iter()
        .map(|span| str_width(span.content.as_ref()))
        .sum()
}

fn toast_remaining_fraction(props: ToastProps<'_>) -> f32 {
    if props.duration_ticks == 0 {
        return 0.0;
    }
    let elapsed = props.tick.saturating_sub(props.started_at);
    let remaining = props
        .duration_ticks
        .saturating_sub(elapsed)
        .min(props.duration_ticks);
    remaining as f32 / props.duration_ticks as f32
}

/// Border cells in clockwise order starting at the top-left corner.
fn toast_border_cells(area: Rect) -> Vec<(u16, u16, char)> {
    let mut cells = Vec::new();
    if area.width < 2 || area.height < 2 {
        return cells;
    }

    let x0 = area.x;
    let y0 = area.y;
    let w = area.width;
    let h = area.height;

    for i in 0..w {
        let ch = if i == 0 {
            '┌'
        } else if i + 1 == w {
            '┐'
        } else {
            '─'
        };
        cells.push((x0 + i, y0, ch));
    }

    for i in 1..h.saturating_sub(1) {
        cells.push((x0 + w - 1, y0 + i, '│'));
    }

    for i in 0..w {
        let x = x0 + w - 1 - i;
        let ch = if i == 0 {
            '┘'
        } else if i + 1 == w {
            '└'
        } else {
            '─'
        };
        cells.push((x, y0 + h - 1, ch));
    }

    for i in 1..h.saturating_sub(1) {
        let y = y0 + h - 1 - i;
        cells.push((x0, y, '│'));
    }

    cells
}

fn draw_toast_border(
    frame: &mut Frame<'_>,
    area: Rect,
    style: Style,
    remaining_frac: f32,
) {
    let cells = toast_border_cells(area);
    if cells.is_empty() {
        return;
    }

    let visible = (cells.len() as f32 * remaining_frac.clamp(0.0, 1.0)).round() as usize;
    for (x, y, ch) in cells.into_iter().take(visible) {
        frame.render_widget(
            Paragraph::new(ch.to_string()).style(style),
            Rect {
                x,
                y,
                width: 1,
                height: 1,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Style;

    #[test]
    fn border_cell_count_matches_perimeter() {
        let area = Rect {
            x: 0,
            y: 0,
            width: 10,
            height: 4,
        };
        assert_eq!(toast_border_cells(area).len(), 2 * 10 + 2 * 4 - 4);
    }

    #[test]
    fn short_message_uses_natural_width() {
        let (lines, w) = fit_toast_text("✓ 发送成功", 30, Style::default());
        assert_eq!(lines.len(), 1);
        assert!(w < 20);
    }

    #[test]
    fn long_message_wraps_without_full_width() {
        let msg = "✓ this is a much longer toast message that should wrap onto another line";
        let (lines, w) = fit_toast_text(msg, 30, Style::default());
        assert!(lines.len() >= 2);
        assert!(w <= 30);
    }
}