use hiptty_render::{clear_content_viewport, str_width, truncate_str, wrap_plain, Palette};
use ratatui::{
    layout::{Alignment, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

/// Centered content-area placeholder when a list/detail has no rows to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentPlaceholderKind<'a> {
    /// First load with no stale data.
    Loading,
    /// Loaded successfully but the list is empty.
    Empty { title: &'a str, hints: &'a str },
    /// Load failed and there is nothing to fall back on.
    Error {
        message: &'a str,
        retry_hint: &'a str,
    },
}

const LOADING_FRAMES: [&str; 4] = [
    "── 正在加载 ──",
    "── 正在加载. ──",
    "── 正在加载.. ──",
    "── 正在加载... ──",
];

pub fn draw_content_placeholder(
    frame: &mut Frame<'_>,
    area: Rect,
    palette: Palette,
    kind: ContentPlaceholderKind<'_>,
    tick: u64,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    clear_content_viewport(frame, area);

    let lines: Vec<Line<'static>> = match kind {
        ContentPlaceholderKind::Loading => {
            let frame_idx = ((tick / 3) as usize) % LOADING_FRAMES.len();
            vec![Line::from(Span::styled(
                LOADING_FRAMES[frame_idx].to_string(),
                palette.secondary_style(),
            ))]
        }
        ContentPlaceholderKind::Empty { title, hints } => {
            let mut out = vec![Line::from(Span::styled(
                title.to_string(),
                palette.secondary_style().add_modifier(Modifier::BOLD),
            ))];
            if !hints.is_empty() {
                out.push(Line::from(""));
                out.push(Line::from(Span::styled(
                    hints.to_string(),
                    palette.secondary_style(),
                )));
            }
            out
        }
        ContentPlaceholderKind::Error {
            message,
            retry_hint,
        } => {
            let max_w = area.width.saturating_sub(2).max(8) as usize;
            let mut out = wrap_plain(message, max_w, palette.error_style());
            if !retry_hint.is_empty() {
                out.push(Line::from(""));
                out.push(Line::from(Span::styled(
                    retry_hint.to_string(),
                    palette.secondary_style(),
                )));
            }
            out
        }
    };

    let block_h = lines.len().min(area.height as usize) as u16;
    if block_h == 0 {
        return;
    }
    let y = area.y + area.height.saturating_sub(block_h) / 2;
    let inner = Rect {
        x: area.x,
        y,
        width: area.width,
        height: block_h,
    };

    // Center each line horizontally by padding (Paragraph Alignment centers the block).
    let centered: Vec<Line<'static>> = lines
        .into_iter()
        .map(|line| {
            let w = line_display_width(&line);
            if w >= area.width as usize {
                // Truncate oversized single-span lines.
                let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
                let style = line
                    .spans
                    .first()
                    .map(|s| s.style)
                    .unwrap_or_else(|| palette.secondary_style());
                Line::from(Span::styled(
                    truncate_str(&text, area.width as usize),
                    style,
                ))
            } else {
                line
            }
        })
        .collect();

    frame.render_widget(Paragraph::new(centered).alignment(Alignment::Center), inner);
}

fn line_display_width(line: &Line<'_>) -> usize {
    line.spans
        .iter()
        .map(|span| str_width(span.content.as_ref()))
        .sum()
}

/// Resolve placeholder for list-like pages with optional stale rows.
///
/// When `has_items` is true, returns `None` — caller should render the list.
/// Refresh loading and toast-only errors never produce a placeholder.
pub fn list_placeholder<'a>(
    has_items: bool,
    loading: bool,
    error: Option<&'a str>,
    empty_title: &'a str,
    empty_hints: &'a str,
) -> Option<ContentPlaceholderKind<'a>> {
    if has_items {
        return None;
    }
    if loading {
        return Some(ContentPlaceholderKind::Loading);
    }
    if let Some(message) = error {
        return Some(ContentPlaceholderKind::Error {
            message,
            retry_hint: "r 重试",
        });
    }
    Some(ContentPlaceholderKind::Empty {
        title: empty_title,
        hints: empty_hints,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_items_skips_placeholder() {
        assert!(list_placeholder(true, true, Some("err"), "空", "r").is_none());
        assert!(list_placeholder(true, false, Some("err"), "空", "r").is_none());
    }

    #[test]
    fn loading_first_takes_priority_over_error() {
        let kind = list_placeholder(false, true, Some("err"), "空", "r").unwrap();
        assert!(matches!(kind, ContentPlaceholderKind::Loading));
    }

    #[test]
    fn empty_and_error_when_no_items() {
        assert!(matches!(
            list_placeholder(false, false, None, "暂无内容", "r 刷新"),
            Some(ContentPlaceholderKind::Empty { .. })
        ));
        assert!(matches!(
            list_placeholder(false, false, Some("网络错误"), "暂无内容", "r"),
            Some(ContentPlaceholderKind::Error { .. })
        ));
    }
}
