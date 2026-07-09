use hiptty_core::{ContentNode, ContentSpan, Post};
use hiptty_render::{quote_header_label, str_width, wrap_plain, Palette};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::cache::{ImageCache, ImageKind, ImageState};
use crate::draw::IMAGE_FAIL_LABEL;
use crate::layout::{SMILEY_COLS, SMILEY_ROWS};
use crate::smiley::smiley_cache_key;

#[derive(Debug, Clone)]
pub enum InlinePart {
    Text(Line<'static>),
    Smiley {
        key: String,
        width: u16,
        height: u16,
        failed: bool,
    },
}

#[derive(Debug, Clone)]
pub enum ContentBlock {
    Text(Line<'static>),
    Image {
        url: String,
        width: u16,
        height: u16,
        failed: bool,
    },
    /// One display row mixing text and inline smileys (original post flow).
    Inline {
        parts: Vec<InlinePart>,
    },
    /// Full-row smiley (kept for height math; normally smileys are in `Inline`).
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
            Self::Inline { parts } => parts
                .iter()
                .map(|p| match p {
                    InlinePart::Text(_) => 1,
                    InlinePart::Smiley { height, .. } => *height,
                })
                .max()
                .unwrap_or(1),
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
        let chunk = layout_content_node(node, max_cols, palette, cache);
        if chunk.is_empty() {
            continue;
        }
        blocks.extend(chunk);
        blocks.push(ContentBlock::Text(Line::from("")));
    }
    collapse_empty_blocks(&mut blocks);
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
            time,
            text,
            ..
        } => layout_quote(
            author.as_deref(),
            time.as_deref(),
            text,
            max_cols,
            palette,
        ),
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

enum LayoutAtom {
    Char(char, Style),
    Smiley {
        key: String,
        width: u16,
        height: u16,
        failed: bool,
    },
    Break,
}

fn layout_text_spans(
    spans: &[ContentSpan],
    max_cols: usize,
    palette: Palette,
    cache: &ImageCache,
) -> Vec<ContentBlock> {
    if max_cols == 0 {
        return Vec::new();
    }

    let mut atoms = Vec::new();
    for span in spans {
        match span {
            ContentSpan::Text { text, style, .. } => {
                let st = core_style_to_ratatui(style, palette);
                for ch in text.chars() {
                    if ch == '\n' {
                        atoms.push(LayoutAtom::Break);
                    } else {
                        atoms.push(LayoutAtom::Char(ch, st));
                    }
                }
            }
            ContentSpan::Smiley {
                url,
                code,
                smilie_id,
            } => {
                let key = smiley_cache_key(code.as_deref(), smilie_id.as_deref(), url);
                let block = image_block(key, ImageKind::Smiley, cache);
                if let ContentBlock::Smiley {
                    key,
                    width,
                    height,
                    failed,
                } = block
                {
                    atoms.push(LayoutAtom::Smiley {
                        key,
                        width,
                        height,
                        failed,
                    });
                }
            }
        }
    }

    pack_atoms(atoms, max_cols)
}

fn pack_atoms(atoms: Vec<LayoutAtom>, max_cols: usize) -> Vec<ContentBlock> {
    let mut rows: Vec<Vec<InlinePart>> = vec![Vec::new()];
    let mut row_w = 0usize;
    let mut text_run = String::new();
    let mut text_style = Style::default();
    let mut has_style = false;

    let flush_text = |rows: &mut [Vec<InlinePart>],
                      text_run: &mut String,
                      text_style: Style,
                      has_style: &mut bool| {
        if text_run.is_empty() {
            return;
        }
        let style = if *has_style {
            text_style
        } else {
            Style::default()
        };
        rows.last_mut().unwrap().push(InlinePart::Text(Line::from(
            Span::styled(std::mem::take(text_run), style),
        )));
        *has_style = false;
    };

    let new_row = |rows: &mut Vec<Vec<InlinePart>>, row_w: &mut usize| {
        rows.push(Vec::new());
        *row_w = 0;
    };

    for atom in atoms {
        match atom {
            LayoutAtom::Break => {
                flush_text(&mut rows, &mut text_run, text_style, &mut has_style);
                new_row(&mut rows, &mut row_w);
            }
            LayoutAtom::Char(ch, style) => {
                let ch_w = str_width(&ch.to_string());
                if row_w + ch_w > max_cols && row_w > 0 {
                    flush_text(&mut rows, &mut text_run, text_style, &mut has_style);
                    new_row(&mut rows, &mut row_w);
                }
                if text_run.is_empty() {
                    text_style = style;
                    has_style = true;
                } else if has_style && text_style != style {
                    flush_text(&mut rows, &mut text_run, text_style, &mut has_style);
                    text_style = style;
                    has_style = true;
                }
                text_run.push(ch);
                row_w += ch_w;
            }
            LayoutAtom::Smiley {
                key,
                width,
                height,
                failed,
            } => {
                let w = width as usize;
                if row_w + w > max_cols && row_w > 0 {
                    flush_text(&mut rows, &mut text_run, text_style, &mut has_style);
                    new_row(&mut rows, &mut row_w);
                }
                flush_text(&mut rows, &mut text_run, text_style, &mut has_style);
                rows.last_mut().unwrap().push(InlinePart::Smiley {
                    key,
                    width,
                    height,
                    failed,
                });
                row_w += w;
            }
        }
    }
    flush_text(&mut rows, &mut text_run, text_style, &mut has_style);

    let mut blocks = Vec::new();
    for parts in rows {
        if parts.is_empty() {
            blocks.push(ContentBlock::Text(Line::from("")));
            continue;
        }
        let has_smiley = parts
            .iter()
            .any(|p| matches!(p, InlinePart::Smiley { .. }));
        if !has_smiley {
            // Merge pure-text parts into one Line.
            let mut spans = Vec::new();
            for part in parts {
                if let InlinePart::Text(line) = part {
                    spans.extend(line.spans);
                }
            }
            blocks.push(ContentBlock::Text(if spans.is_empty() {
                Line::from("")
            } else {
                Line::from(spans)
            }));
        } else {
            blocks.push(ContentBlock::Inline { parts });
        }
    }

    collapse_empty_blocks(&mut blocks);
    blocks
}

fn collapse_empty_blocks(blocks: &mut Vec<ContentBlock>) {
    let mut out = Vec::with_capacity(blocks.len());
    let mut last_blank = false;
    for block in blocks.drain(..) {
        let blank = matches!(&block, ContentBlock::Text(line) if line_is_blank(line));
        if blank {
            if !last_blank && !out.is_empty() {
                out.push(block);
            }
            last_blank = true;
        } else {
            out.push(block);
            last_blank = false;
        }
    }
    while matches!(out.last(), Some(ContentBlock::Text(line)) if line_is_blank(line)) {
        out.pop();
    }
    *blocks = out;
}

fn line_is_blank(line: &Line<'_>) -> bool {
    line.spans
        .iter()
        .all(|s| s.content.chars().all(|c| c.is_whitespace()))
}

fn layout_quote(
    author: Option<&str>,
    time: Option<&str>,
    text: &str,
    max_cols: usize,
    palette: Palette,
) -> Vec<ContentBlock> {
    let prefix_w = 3usize;
    let body_w = max_cols.saturating_sub(prefix_w);
    if body_w == 0 {
        return Vec::new();
    }

    let header = quote_header_label(author, time);
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
        // Full content width so horizontal layout does not jump when Ready.
        ImageKind::Content { max_cols } => max_cols.max(4),
        ImageKind::Avatar => crate::layout::AVATAR_COLS,
    }
}

fn image_loading_height(kind: ImageKind) -> u16 {
    match kind {
        ImageKind::Smiley => SMILEY_ROWS,
        // Estimate square-ish image at full width. With typical ~½-width font cells,
        // square pixel image → ~max_cols/2 rows. Clamp so short posts stay compact
        // but tall phone screenshots do not explode from a 4-row stub into 30+.
        ImageKind::Content { max_cols } => content_loading_height_estimate(max_cols),
        ImageKind::Avatar => crate::layout::AVATAR_ROWS,
    }
}

/// Placeholder rows before pixels are known (no HTML pixel size in our model).
pub fn content_loading_height_estimate(max_cols: u16) -> u16 {
    (max_cols / 2).clamp(8, 20)
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
        if let Some(color) = hiptty_render::parse_hex_color(fg)
            .or_else(|| parse_named_color(fg, palette))
        {
            out = out.fg(color);
        }
    }
    if style.bold {
        out = out.add_modifier(Modifier::BOLD);
    }
    if style.italic {
        out = out.add_modifier(Modifier::ITALIC);
    }
    if style.underline {
        out = out.add_modifier(Modifier::UNDERLINED);
    }
    if style.strikethrough {
        out = out.add_modifier(Modifier::CROSSED_OUT);
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
    fn smileys_pack_inline_with_surrounding_text() {
        let picker = Picker::halfblocks();
        let cache = ImageCache::new(picker, None);
        let palette = Palette::default();
        let post = Post {
            pid: "1".into(),
            floor: 1,
            author: "u".into(),
            uid: None,
            avatar_url: None,
            time: "t".into(),
            content: vec![ContentNode::Text {
                spans: vec![
                    ContentSpan::Text {
                        text: "hi".into(),
                        style: Style::default(),
                        url: None,
                    },
                    ContentSpan::Smiley {
                        url: "https://img02.4d4y.com/forum/images/smilies/default/lol.gif".into(),
                        code: Some("default_lol".into()),
                        smilie_id: Some("9".into()),
                    },
                    ContentSpan::Text {
                        text: "there".into(),
                        style: Style::default(),
                        url: None,
                    },
                ],
            }],
            poll: None,
            page: 1,
            warned: false,
            signature: None,
            edited_by: None,
            edited_at: None,
        };
        let blocks = layout_post_blocks(&post, 40, palette, &cache);
        let inline = blocks.iter().find(|b| matches!(b, ContentBlock::Inline { .. }));
        assert!(
            inline.is_some(),
            "expected a single Inline row, got {blocks:?}"
        );
        if let Some(ContentBlock::Inline { parts }) = inline {
            assert!(parts.len() >= 3, "text + smiley + text, got {parts:?}");
            assert!(matches!(parts[0], InlinePart::Text(_)));
            assert!(matches!(parts[1], InlinePart::Smiley { .. }));
            assert!(matches!(parts[2], InlinePart::Text(_)));
            assert_eq!(
                blocks
                    .iter()
                    .filter(|b| matches!(b, ContentBlock::Inline { .. } | ContentBlock::Smiley { .. }))
                    .count(),
                1,
                "smiley should not force its own full-width row"
            );
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
                edited_by: None,
                edited_at: None,
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
            let loading_h = content_loading_height_estimate(79u16.saturating_sub(2));
            assert!(
                image.0 >= loading_h || image.0 > 4,
                "{name} ready image height {} should be non-trivial (loading est {loading_h})",
                image.0
            );
        }
    }
}
