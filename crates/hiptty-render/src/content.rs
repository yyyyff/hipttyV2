use hiptty_core::{ContentNode, ContentSpan, Post, Style as CoreStyle};
use ratatui::{
    style::{Modifier, Style},
    text::Line,
};

use crate::text::{format_relative_time, str_width, truncate_str};
use crate::theme::{parse_hex_color, Palette};
use crate::wrap::{wrap_plain, wrap_segments, StyledSegment};

const IMAGE_PLACEHOLDER: &str = "[图片]";
const SMILEY_PLACEHOLDER: &str = "[表情]";

pub fn render_post_content_lines(post: &Post, width: u16, palette: Palette) -> Vec<Line<'static>> {
    let max_cols = width.saturating_sub(2) as usize;
    if max_cols == 0 {
        return Vec::new();
    }

    let mut lines = Vec::new();
    for node in &post.content {
        lines.extend(render_content_node(node, max_cols, palette));
        if !lines.is_empty() {
            lines.push(Line::from(""));
        }
    }
    while lines
        .last()
        .map(|l| l.spans.iter().all(|s| s.content.is_empty()))
        == Some(true)
    {
        lines.pop();
    }
    lines
}

pub fn render_content_node(
    node: &ContentNode,
    max_cols: usize,
    palette: Palette,
) -> Vec<Line<'static>> {
    match node {
        ContentNode::Text { spans } => render_text_spans(spans, max_cols, palette),
        ContentNode::Quote {
            author,
            time: _,
            text,
            ..
        } => render_quote(author.as_deref(), text, max_cols, palette),
        ContentNode::Image { .. } => vec![Line::styled(IMAGE_PLACEHOLDER, palette.muted_style())],
        ContentNode::Attachment { name, size, .. } => {
            let size_text = format_attachment_size(*size);
            let line = if size_text.is_empty() {
                format!("\u{f0c6} {name}")
            } else {
                format!("\u{f0c6} {name} ({size_text})")
            };
            vec![Line::styled(line, palette.foreground_style())]
        }
        ContentNode::FloorRef { floor, author, .. } => {
            let author = author.as_deref().unwrap_or("?");
            let line = format!(">>> #{floor} @{author}");
            vec![Line::styled(line, palette.link_style())]
        }
        ContentNode::AppMark { text, .. } => {
            vec![Line::styled(format!("▸ {text}"), palette.muted_style())]
        }
    }
}

fn render_text_spans(
    spans: &[ContentSpan],
    max_cols: usize,
    palette: Palette,
) -> Vec<Line<'static>> {
    let mut segments = Vec::new();
    for span in spans {
        match span {
            ContentSpan::Text { text, style, .. } => {
                if !text.is_empty() {
                    segments.push(StyledSegment {
                        text: text.clone(),
                        style: core_style_to_ratatui(style, palette),
                    });
                }
            }
            ContentSpan::Smiley { code, .. } => {
                let label = code
                    .as_deref()
                    .map(|c| format!(":{c}:"))
                    .unwrap_or_else(|| SMILEY_PLACEHOLDER.to_string());
                segments.push(StyledSegment {
                    text: label,
                    style: palette.secondary_style(),
                });
            }
        }
    }
    if segments.is_empty() {
        return Vec::new();
    }
    wrap_segments(&segments, max_cols)
}

fn render_quote(
    author: Option<&str>,
    text: &str,
    max_cols: usize,
    palette: Palette,
) -> Vec<Line<'static>> {
    let prefix_w = 3usize;
    let body_w = max_cols.saturating_sub(prefix_w);
    if body_w == 0 {
        return Vec::new();
    }

    let header = match author {
        Some(a) => format!("@{} 说:", a),
        None => "@? 说:".to_string(),
    };
    let mut lines = Vec::new();
    lines.extend(wrap_plain(
        &header,
        body_w,
        palette.link_style().add_modifier(Modifier::BOLD),
    ));
    lines.extend(wrap_plain(text, body_w, palette.secondary_style()));

    lines
        .into_iter()
        .map(|line| prefix_quote_line(line, palette))
        .collect()
}

fn prefix_quote_line(line: Line<'static>, palette: Palette) -> Line<'static> {
    let mut spans = vec![Span::styled("┃  ", palette.muted_style())];
    spans.extend(line.spans);
    Line::from(spans)
}

use ratatui::text::Span;

fn core_style_to_ratatui(style: &CoreStyle, palette: Palette) -> Style {
    let mut out = palette.foreground_style();
    if let Some(fg) = style.fg.as_deref() {
        if let Some(color) = parse_hex_color(fg).or(parse_named_color(fg, palette)) {
            out = out.fg(color);
        }
    }
    if style.bold {
        out = out.add_modifier(Modifier::BOLD);
    }
    if style.italic {
        out = out.add_modifier(Modifier::ITALIC);
    }
    out
}

fn parse_named_color(name: &str, palette: Palette) -> Option<ratatui::style::Color> {
    match name.to_ascii_lowercase().as_str() {
        "gray" | "grey" => Some(palette.muted),
        "red" => Some(palette.error),
        "blue" => Some(palette.link),
        "green" => Some(palette.success),
        _ => None,
    }
}

fn format_attachment_size(size: Option<u64>) -> String {
    let Some(bytes) = size else {
        return String::new();
    };
    if bytes >= 1024 * 1024 {
        format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{}KB", bytes / 1024)
    } else {
        format!("{bytes}B")
    }
}

pub fn format_signature(signature: &str, max_cols: usize) -> String {
    let trimmed = signature.trim();
    if trimmed.is_empty() || max_cols < 3 {
        return String::new();
    }
    let inner_budget = max_cols.saturating_sub(2);
    format!("\"{}\"", truncate_str(trimmed, inner_budget))
}

pub fn floor_header_rows(
    author: &str,
    floor: u32,
    time_raw: &str,
    width: usize,
    palette: Palette,
) -> (Line<'static>, Line<'static>) {
    let floor_tag = format!("#{floor}");
    let floor_w = str_width(&floor_tag);
    let author_budget = width.saturating_sub(floor_w + 1);
    let author_text = truncate_str(author, author_budget);

    let mut row1_spans = vec![Span::styled(author_text, palette.foreground_style())];
    let author_used: usize = row1_spans
        .iter()
        .map(|s| str_width(s.content.as_ref()))
        .sum();
    if author_used + floor_w < width {
        row1_spans.push(Span::raw(
            " ".repeat(width.saturating_sub(author_used + floor_w)),
        ));
    }
    row1_spans.push(Span::styled(
        floor_tag,
        palette.secondary_style().add_modifier(Modifier::BOLD),
    ));

    let time = format_relative_time(time_raw);
    let time_label = if time.is_empty() {
        String::new()
    } else {
        format!("发表于 {time}")
    };
    let row2 = Line::styled(time_label, palette.secondary_style());

    (Line::from(row1_spans), row2)
}

pub fn signature_line(signature: &str, width: usize, palette: Palette) -> Line<'static> {
    Line::styled(format_signature(signature, width), palette.muted_style())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hiptty_core::ContentNode;

    #[test]
    fn quote_has_bar_prefix() {
        let node = ContentNode::Quote {
            author: Some("bob".into()),
            time: None,
            text: "hello".into(),
            pid: None,
            tid: None,
            reply_to: None,
        };
        let lines = render_content_node(&node, 40, Palette::default());
        assert!(!lines.is_empty());
        assert!(lines[0].spans[0].content.contains('┃'));
    }

    #[test]
    fn signature_truncated() {
        let sig = format_signature("这是一段很长很长很长很长很长很长的签名文字", 20);
        assert!(str_width(&sig) <= 20);
    }
}
