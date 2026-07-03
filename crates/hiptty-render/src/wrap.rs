use ratatui::text::{Line, Span};
use ratatui::style::Style;

use crate::text::str_width;

#[derive(Debug, Clone)]
pub struct StyledSegment {
    pub text: String,
    pub style: Style,
}

pub fn wrap_segments(segments: &[StyledSegment], max_cols: usize) -> Vec<Line<'static>> {
    if max_cols == 0 {
        return Vec::new();
    }
    if segments.is_empty() {
        return vec![Line::from("")];
    }

    let mut lines: Vec<Vec<Span<'static>>> = vec![Vec::new()];
    let mut current = String::new();
    let mut current_style = segments.first().map(|s| s.style).unwrap_or_default();
    let mut line_width = 0usize;

    let flush = |lines: &mut Vec<Vec<Span<'static>>>, current: &mut String, style: Style| {
        if !current.is_empty() {
            lines
                .last_mut()
                .unwrap()
                .push(Span::styled(std::mem::take(current), style));
        }
    };

    for segment in segments {
        for ch in segment.text.chars() {
            let ch_w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
            if line_width + ch_w > max_cols && line_width > 0 {
                flush(&mut lines, &mut current, current_style);
                lines.push(Vec::new());
                line_width = 0;
                current_style = segment.style;
            } else if current.is_empty() {
                current_style = segment.style;
            } else if current_style != segment.style {
                flush(&mut lines, &mut current, current_style);
                current_style = segment.style;
            }
            current.push(ch);
            line_width += ch_w;
        }
    }
    flush(&mut lines, &mut current, current_style);

    lines
        .into_iter()
        .map(|spans| {
            if spans.is_empty() {
                Line::from("")
            } else {
                Line::from(spans)
            }
        })
        .collect()
}

pub fn wrap_plain(text: &str, max_cols: usize, style: Style) -> Vec<Line<'static>> {
    wrap_segments(&[StyledSegment { text: text.to_string(), style }], max_cols)
}

pub fn pad_line_left(line: Line<'static>, width: usize) -> Line<'static> {
    let used = line
        .spans
        .iter()
        .map(|s| str_width(s.content.as_ref()))
        .sum::<usize>();
    if used >= width {
        return line;
    }
    let mut spans = vec![Span::raw(" ".repeat(width - used))];
    spans.extend(line.spans);
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wraps_ascii_by_width() {
        let lines = wrap_plain("helloworld", 5, Style::default());
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].spans[0].content, "hello");
        assert_eq!(lines[1].spans[0].content, "world");
    }
}