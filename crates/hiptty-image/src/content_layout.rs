use hiptty_core::{ContentNode, ContentSpan, Post};
use hiptty_render::{wrap_plain, wrap_segments, Palette, StyledSegment};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::cache::{ImageCache, ImageKind, ImageState};
use crate::draw::IMAGE_FAIL_LABEL;
use crate::layout::{SMILEY_COLS, SMILEY_ROWS};
use crate::smiley::smiley_cache_key;

#[derive(Debug, Clone)]
pub enum ContentBlock {
    Text(Line<'static>),
    Image {
        url: String,
        width: u16,
        height: u16,
        failed: bool,
    },
    Smiley {
        key: String,
        width: u16,
        height: u16,
        failed: bool,
    },
}

impl ContentBlock {
    pub fn height(&self) -> u16 {
        match self {
            Self::Text(_) => 1,
            Self::Image { height, .. } | Self::Smiley { height, .. } => *height,
        }
    }
}

pub fn layout_post_blocks(
    post: &Post,
    width: u16,
    palette: Palette,
    cache: &ImageCache,
) -> Vec<ContentBlock> {
    let max_cols = width.saturating_sub(2) as usize;
    if max_cols == 0 {
        return Vec::new();
    }

    let mut blocks = Vec::new();
    for node in &post.content {
        blocks.extend(layout_content_node(node, max_cols, palette, cache));
        if !blocks.is_empty() {
            blocks.push(ContentBlock::Text(Line::from("")));
        }
    }
    while matches!(blocks.last(), Some(ContentBlock::Text(line)) if line.spans.iter().all(|s| s.content.is_empty()))
    {
        blocks.pop();
    }
    blocks
}

fn layout_content_node(
    node: &ContentNode,
    max_cols: usize,
    palette: Palette,
    cache: &ImageCache,
) -> Vec<ContentBlock> {
    match node {
        ContentNode::Text { spans } => layout_text_spans(spans, max_cols, palette, cache),
        ContentNode::Quote {
            author,
            time: _,
            text,
            ..
        } => layout_quote(author.as_deref(), text, max_cols, palette),
        ContentNode::Image { url, thumb_url, .. } => {
            let image_url = thumb_url.as_deref().unwrap_or(url.as_str()).to_string();
            vec![image_block(
                image_url,
                ImageKind::Content {
                    max_cols: max_cols as u16,
                },
                cache,
            )]
        }
        ContentNode::Attachment { name, size, .. } => {
            let size_text = format_attachment_size(*size);
            let line = if size_text.is_empty() {
                format!("\u{f0c6} {name}")
            } else {
                format!("\u{f0c6} {name} ({size_text})")
            };
            vec![ContentBlock::Text(Line::styled(
                line,
                palette.foreground_style(),
            ))]
        }
        ContentNode::FloorRef { floor, author, .. } => {
            let author = author.as_deref().unwrap_or("?");
            let line = format!(">>> #{floor} @{author}");
            vec![ContentBlock::Text(Line::styled(line, palette.link_style()))]
        }
        ContentNode::AppMark { text, .. } => {
            vec![ContentBlock::Text(Line::styled(
                format!("▸ {text}"),
                palette.muted_style(),
            ))]
        }
    }
}

fn layout_text_spans(
    spans: &[ContentSpan],
    max_cols: usize,
    palette: Palette,
    cache: &ImageCache,
) -> Vec<ContentBlock> {
    let mut blocks = Vec::new();
    let mut segments = Vec::new();

    let flush_text = |segments: &mut Vec<StyledSegment>, blocks: &mut Vec<ContentBlock>| {
        if segments.is_empty() {
            return;
        }
        for line in wrap_segments(segments, max_cols) {
            blocks.push(ContentBlock::Text(line));
        }
        segments.clear();
    };

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
            ContentSpan::Smiley {
                url,
                code,
                smilie_id,
            } => {
                flush_text(&mut segments, &mut blocks);
                let key = smiley_cache_key(code.as_deref(), smilie_id.as_deref(), url);
                blocks.push(image_block(key, ImageKind::Smiley, cache));
            }
        }
    }
    flush_text(&mut segments, &mut blocks);
    blocks
}

fn layout_quote(
    author: Option<&str>,
    text: &str,
    max_cols: usize,
    palette: Palette,
) -> Vec<ContentBlock> {
    let prefix_w = 3usize;
    let body_w = max_cols.saturating_sub(prefix_w);
    if body_w == 0 {
        return Vec::new();
    }

    let header = match author {
        Some(a) => format!("@{} 说:", a),
        None => "@? 说:".to_string(),
    };
    let mut blocks = Vec::new();
    for line in wrap_plain(
        &header,
        body_w,
        palette.link_style().add_modifier(Modifier::BOLD),
    ) {
        blocks.push(ContentBlock::Text(prefix_quote_line(line, palette)));
    }
    for line in wrap_plain(text, body_w, palette.secondary_style()) {
        blocks.push(ContentBlock::Text(prefix_quote_line(line, palette)));
    }
    blocks
}

fn prefix_quote_line(line: Line<'static>, palette: Palette) -> Line<'static> {
    let mut spans = vec![Span::styled("┃  ", palette.muted_style())];
    spans.extend(line.spans);
    Line::from(spans)
}

fn image_block(cache_key: String, kind: ImageKind, cache: &ImageCache) -> ContentBlock {
    let (width, height, failed) = match cache.get(&cache_key).map(|e| &e.state) {
        Some(ImageState::Ready { width, height, .. }) => (*width, *height, false),
        Some(ImageState::Failed) => (image_fail_width(kind), image_fail_height(kind), true),
        _ => (image_loading_width(kind), image_loading_height(kind), false),
    };
    match kind {
        ImageKind::Smiley => ContentBlock::Smiley {
            key: cache_key,
            width,
            height,
            failed,
        },
        _ => ContentBlock::Image {
            url: cache_key,
            width,
            height,
            failed,
        },
    }
}

fn image_loading_width(kind: ImageKind) -> u16 {
    match kind {
        ImageKind::Smiley => SMILEY_COLS,
        ImageKind::Content { max_cols } => max_cols.clamp(4, 12),
        ImageKind::Avatar => crate::layout::AVATAR_COLS,
    }
}

fn image_loading_height(kind: ImageKind) -> u16 {
    match kind {
        ImageKind::Smiley => SMILEY_ROWS,
        ImageKind::Content { .. } => 4,
        ImageKind::Avatar => crate::layout::AVATAR_ROWS,
    }
}

fn image_fail_width(kind: ImageKind) -> u16 {
    match kind {
        ImageKind::Smiley => SMILEY_COLS,
        ImageKind::Content { max_cols } => {
            (hiptty_render::str_width(IMAGE_FAIL_LABEL) as u16).min(max_cols)
        }
        ImageKind::Avatar => crate::layout::AVATAR_COLS,
    }
}

fn image_fail_height(kind: ImageKind) -> u16 {
    match kind {
        ImageKind::Smiley => SMILEY_ROWS,
        ImageKind::Content { .. } => 1,
        ImageKind::Avatar => crate::layout::AVATAR_ROWS,
    }
}

fn core_style_to_ratatui(style: &hiptty_core::Style, palette: Palette) -> Style {
    let mut out = palette.foreground_style();
    if let Some(fg) = style.fg.as_deref() {
        if let Some(color) = hiptty_render::parse_hex_color(fg) {
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

#[cfg(test)]
mod tests {
    use std::thread;
    use std::time::Duration;

    use hiptty_core::{ContentNode, ContentSpan, Post, Style};
    use ratatui_image::picker::Picker;

    use super::*;
    use crate::cache::{ImageCache, ImageKind, ImageState};

    fn wait_ready(cache: &mut ImageCache, url: &str) {
        for _ in 0..500 {
            cache.poll();
            match cache.get(url).map(|e| &e.state) {
                Some(ImageState::Ready { .. }) => return,
                Some(ImageState::Failed) => return,
                _ => thread::sleep(Duration::from_millis(2)),
            }
        }
    }

    #[test]
    fn lazy_load_thumb_decodes_for_portrait_and_landscape() {
        let picker = Picker::halfblocks();
        let palette = Palette::default();
        let cases = [
            (
                "7f_landscape",
                "/tmp/f7_thumb.jpg",
                "https://img02.4d4y.com/forum/attachments/day_260630/2606302322368df5cc713ab026.jpeg.thumb.jpg",
                "https://img02.4d4y.com/forum/attachments/day_260630/2606302322368df5cc713ab026.jpeg",
            ),
            (
                "10f_portrait",
                "/tmp/f10_thumb.jpg",
                "https://img02.4d4y.com/forum/attachments/day_260630/2606302332236bbab3e809e92b.jpg.thumb.jpg",
                "https://img02.4d4y.com/forum/attachments/day_260630/2606302332236bbab3e809e92b.jpg",
            ),
        ];
        for (name, path, thumb_url, full_url) in cases {
            if !std::path::Path::new(path).exists() {
                continue;
            }
            let bytes = std::fs::read(path).expect("thumb bytes");
            let mut cache = ImageCache::new(picker.clone(), None);
            cache.ingest_bytes(
                thumb_url.to_string(),
                ImageKind::Content { max_cols: 78 },
                bytes,
            );
            wait_ready(&mut cache, thumb_url);
            let entry = cache.get(thumb_url).expect(name);
            assert!(
                matches!(entry.state, ImageState::Ready { .. }),
                "{name} should decode, got {:?}",
                entry.state
            );
            let post = Post {
                pid: "1".into(),
                floor: 1,
                author: "u".into(),
                uid: None,
                avatar_url: None,
                time: "t".into(),
                content: vec![
                    ContentNode::Text {
                        spans: vec![ContentSpan::Text {
                            text: "hello".into(),
                            style: Style::default(),
                            url: None,
                        }],
                    },
                    ContentNode::Image {
                        url: full_url.to_string(),
                        thumb_url: Some(thumb_url.to_string()),
                        size: None,
                    },
                ],
                poll: None,
                page: 1,
                warned: false,
                signature: None,
            };
            let blocks = layout_post_blocks(&post, 79, palette, &cache);
            let image = blocks
                .iter()
                .find_map(|b| match b {
                    ContentBlock::Image { height, failed, .. } => Some((*height, *failed)),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{name} missing image block"));
            assert!(!image.1, "{name} should not be failed");
            assert!(
                image.0 > 4,
                "{name} ready image should exceed loading placeholder"
            );
        }
    }
}
