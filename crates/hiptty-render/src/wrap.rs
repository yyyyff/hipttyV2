use ratatui::style::Style;
use ratatui::text::{Line, Span};

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
            if ch == '\n' {
                flush(&mut lines, &mut current, current_style);
                lines.push(Vec::new());
                line_width = 0;
                current_style = segment.style;
                continue;
            }
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

    let mapped: Vec<Line<'static>> = lines
        .into_iter()
        .map(|spans| {
            if spans.is_empty() {
                Line::from("")
            } else {
                Line::from(spans)
            }
        })
        .collect();
    collapse_empty_lines(mapped)
}

/// Keep at most one consecutive blank line; drop trailing blanks.
pub fn collapse_empty_lines(lines: Vec<Line<'static>>) -> Vec<Line<'static>> {
    let mut out = Vec::with_capacity(lines.len());
    let mut last_blank = false;
    for line in lines {
        let empty = line_is_blank(&line);
        if empty {
            if !last_blank && !out.is_empty() {
                out.push(line);
            }
            last_blank = true;
        } else {
            out.push(line);
            last_blank = false;
        }
    }
    while out.last().is_some_and(line_is_blank) {
        out.pop();
    }
    out
}

fn line_is_blank(line: &Line<'_>) -> bool {
    line.spans
        .iter()
        .all(|s| s.content.chars().all(|c| c.is_whitespace()))
}

pub fn wrap_plain(text: &str, max_cols: usize, style: Style) -> Vec<Line<'static>> {
    wrap_segments(
        &[StyledSegment {
            text: text.to_string(),
            style,
        }],
        max_cols,
    )
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

    #[test]
    fn hard_breaks_on_newline() {
        let lines = wrap_plain("hello\nworld", 80, Style::default());
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].spans[0].content, "hello");
        assert_eq!(lines[1].spans[0].content, "world");
    }

    #[test]
    fn collapses_multi_blank_lines() {
        let lines = wrap_plain("a\n\n\n\nb\n\n", 80, Style::default());
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].spans[0].content, "a");
        assert!(line_is_blank(&lines[1]));
        assert_eq!(lines[2].spans[0].content, "b");
    }
}
