use std::sync::LazyLock;

use hiptty_core::{ContentNode, ContentSpan, Style};
use regex::Regex;
use scraper::{ElementRef, Node, Selector};

use crate::http::urls::ForumUrls;
use crate::parser::common::extract_param;

static URL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[(http(s)?):\/\/(www\.)?a-zA-Z0-9@:%._\+~#=]{2,256}\.[a-z]{2,6}\b([-a-zA-Z0-9@:%_\+.~#?&//=]*)")
        .expect("valid url regex")
});

pub fn parse_content(root: ElementRef<'_>, urls: &ForumUrls) -> Vec<ContentNode> {
    let mut builder = ContentBuilder::new(urls);
    builder.walk_element(root, Style::default());
    builder.finish()
}

struct ContentBuilder<'a> {
    urls: &'a ForumUrls,
    nodes: Vec<ContentNode>,
    spans: Vec<ContentSpan>,
}

impl<'a> ContentBuilder<'a> {
    fn new(urls: &'a ForumUrls) -> Self {
        Self {
            urls,
            nodes: Vec::new(),
            spans: Vec::new(),
        }
    }

    fn finish(mut self) -> Vec<ContentNode> {
        self.flush_spans();
        for node in &mut self.nodes {
            normalize_node_text(node);
        }
        trim_trailing_empty_nodes(&mut self.nodes);
        self.nodes
    }

    fn flush_spans(&mut self) {
        if self.spans.is_empty() {
            return;
        }
        let spans = std::mem::take(&mut self.spans);
        if !spans_have_content(&spans) {
            return;
        }
        self.nodes.push(ContentNode::Text { spans });
    }

    fn push_text(&mut self, text: &str, style: &Style, url: Option<String>) {
        if text.is_empty() {
            return;
        }
        let style = style.clone();
        if let Some(ContentNode::Text { spans }) = self.nodes.last_mut() {
            if let Some(ContentSpan::Text {
                text: last_text,
                style: last_style,
                url: last_url,
            }) = spans.last_mut()
            {
                if *last_style == style && *last_url == url {
                    last_text.push_str(text);
                    return;
                }
            }
        }
        if let Some(ContentSpan::Text {
            text: last_text,
            style: last_style,
            url: last_url,
        }) = self.spans.last_mut()
        {
            if *last_style == style && *last_url == url {
                last_text.push_str(text);
                return;
            }
        }
        self.spans.push(ContentSpan::Text {
            text: text.to_string(),
            style,
            url,
        });
    }

    fn push_smiley(&mut self, url: String, code: Option<String>, smilie_id: Option<String>) {
        self.spans.push(ContentSpan::Smiley {
            url,
            code,
            smilie_id,
        });
    }

    fn handle_img(&mut self, element: ElementRef<'_>) {
        if let Some((url, code, smilie_id)) = parse_smiley(element, self.urls) {
            self.push_smiley(url, code, smilie_id);
            return;
        }
        if let Some(image) = parse_block_image(element, self.urls) {
            self.flush_spans();
            self.nodes.push(image);
        }
    }

    fn push_plain_with_urls(&mut self, text: &str, style: &Style) {
        let mut last = 0;
        for cap in URL_RE.find_iter(text) {
            let before = &text[last..cap.start()];
            self.push_text(before, style, None);
            let url = cap.as_str();
            if url.contains('@') && !url.contains('/') {
                self.push_text(url, style, None);
            } else {
                self.push_text(url, style, Some(normalize_url(url)));
            }
            last = cap.end();
        }
        if last < text.len() {
            self.push_text(&text[last..], style, None);
        }
    }

    fn walk_children(&mut self, element: ElementRef<'_>, style: Style) {
        for child in element.children() {
            match child.value() {
                Node::Text(text) => {
                    if let Some(t) = normalize_html_text(text) {
                        self.push_plain_with_urls(&t, &style);
                    }
                }
                Node::Element(_) => {
                    if let Some(el) = ElementRef::wrap(child) {
                        self.walk_element(el, style.clone());
                    }
                }
                _ => {}
            }
        }
    }

    fn walk_element(&mut self, element: ElementRef<'_>, style: Style) {
        let tag = element.value().name();

        match tag {
            "i" | "em" => {
                let mut s = style.clone();
                s.italic = true;
                self.walk_children(element, s);
            }
            "u" => {
                let mut s = style.clone();
                s.underline = true;
                self.walk_children(element, s);
            }
            "strong" | "b" => {
                if let Some(floor) = parse_floor_ref(&element) {
                    self.flush_spans();
                    self.nodes.push(floor);
                    return;
                }
                let mut s = style.clone();
                s.bold = true;
                self.walk_children(element, s);
            }
            "strike" | "s" | "del" => {
                let mut s = style.clone();
                s.strikethrough = true;
                self.walk_children(element, s);
            }
            "font" => {
                let mut s = style.clone();
                if let Some(color) = element.value().attr("color") {
                    s.fg = Some(color.trim().to_string());
                }
                // font size/face intentionally ignored (terminal controls size).
                self.walk_children(element, s);
            }
            "br" => self.push_text("\n", &style, None),
            "hr" => self.push_text("\n---\n", &style, None),
            "blockquote" => self.walk_children(element, style),
            "p" => {
                if element.value().attr("class") == Some("imgtitle") {
                    return;
                }
                self.push_text("\n", &style, None);
                self.walk_children(element, style.clone());
                self.push_text("\n", &style, None);
            }
            "li" => {
                self.push_text("\n• ", &style, None);
                self.walk_children(element, style);
            }
            "tr" => {
                self.walk_children(element, style.clone());
                self.push_text("\n", &style, None);
            }
            "td" | "th" => {
                self.walk_children(element, style.clone());
                self.push_text(" ", &style, None);
            }
            "ol" | "ul" | "table" | "tbody" | "thead" | "dl" | "dt" | "dd" => {
                if element.value().attr("class") == Some("imgtitle") {
                    return;
                }
                self.walk_children(element, style);
            }
            "img" => self.handle_img(element),
            "a" => {
                let href = element.value().attr("href").unwrap_or_default();
                if href.starts_with("attachment.php?") {
                    self.flush_spans();
                    self.nodes.push(ContentNode::Attachment {
                        name: element.text().collect(),
                        url: absolute_url(self.urls, href),
                        size: None,
                    });
                    return;
                }
                let text = element.text().collect::<String>();
                let url = normalize_link_url(self.urls, href);
                if element.child_elements().any(|c| c.value().name() == "img") {
                    self.push_text(&text, &style, Some(url.clone()));
                    for child in element.child_elements() {
                        if child.value().name() == "img" {
                            self.handle_img(child);
                        }
                    }
                    return;
                }
                self.push_text(&text, &style, Some(url));
            }
            "span" => {
                if element.select(&Selector::parse("a").unwrap()).any(|a| {
                    a.value()
                        .attr("href")
                        .map(|h| h.contains("attachment.php?") && !h.contains("nothumb="))
                        .unwrap_or(false)
                }) {
                    for a in element.select(&Selector::parse("a").unwrap()) {
                        let href = a.value().attr("href").unwrap_or_default();
                        if href.contains("attachment.php?") && !href.contains("nothumb=") {
                            self.flush_spans();
                            self.nodes.push(ContentNode::Attachment {
                                name: a.text().collect(),
                                url: absolute_url(self.urls, href),
                                size: None,
                            });
                        }
                    }
                    return;
                }
                self.walk_children(element, style);
            }
            "div" => {
                let class = element.value().attr("class").unwrap_or_default();
                if class.contains("quote") {
                    self.flush_spans();
                    self.nodes.push(parse_quote(element));
                    return;
                }
                if class.contains("t_attach") || class.contains("attach_popup") {
                    return;
                }
                self.walk_children(element, style);
            }
            "script" => {
                let html = element.html();
                let url = extract_param(&html, "'src', '", "'");
                if !url.is_empty() {
                    self.flush_spans();
                    let link = if url.starts_with("http://player.youku.com/player.php") {
                        let id = extract_param(&url, "sid/", "/v.swf");
                        format!("http://v.youku.com/v_show/id_{id}.html")
                    } else {
                        url
                    };
                    self.nodes.push(ContentNode::AppMark {
                        text: format!("视频: {link}"),
                        url: Some(link),
                    });
                }
            }
            _ => self.walk_children(element, style),
        }
    }
}

fn parse_floor_ref(element: &ElementRef<'_>) -> Option<ContentNode> {
    let text = element.text().collect::<String>();
    if !text.starts_with("回复 ") || !text.contains('#') {
        return None;
    }
    let floor_part = text.split('#').next()?;
    let floor: u32 = floor_part.trim_start_matches("回复 ").trim().parse().ok()?;
    if floor == 0 {
        return None;
    }
    let author = text
        .split('#')
        .next_back()
        .map(str::trim)
        .unwrap_or("")
        .to_string();
    let (pid, tid) = element
        .select(&Selector::parse("a[href]").ok()?)
        .next()
        .map(|a| {
            let href = a.value().attr("href").unwrap_or_default();
            (
                extract_param(href, "pid=", "&"),
                extract_param(href, "ptid=", "&"),
            )
        })
        .unwrap_or_default();
    if pid.is_empty() {
        return None;
    }
    Some(ContentNode::FloorRef {
        floor,
        author: (!author.is_empty()).then_some(author),
        pid: Some(pid),
        tid: (!tid.is_empty()).then_some(tid),
    })
}

fn parse_quote(element: ElementRef<'_>) -> ContentNode {
    let mut tid = String::new();
    let mut pid = String::new();
    for a in element.select(&Selector::parse("a").unwrap()) {
        let href = a.value().attr("href").unwrap_or_default();
        if href.contains("redirect.php?goto=findpost") {
            pid = extract_param(href, "pid=", "&");
            tid = extract_param(href, "ptid=", "&");
            break;
        }
    }

    let mut author = None;
    let mut time = None;
    let mut reply_to = None;

    // Prefer blockquote body; fall back to whole quote div text.
    let mut text = element
        .select(&Selector::parse("blockquote").unwrap())
        .next()
        .map(|b| b.text().collect::<String>())
        .unwrap_or_else(|| element.text().collect());

    // Older Discuz markup: header in <font>…发表于…</font> (often at end of quote).
    if let Ok(sel) = Selector::parse("font[size=\"2\"], font") {
        for font in element.select(&sel) {
            let header_text = font.text().collect::<String>();
            if let Some((a, t)) = parse_pure_quote_meta_line(&header_text) {
                if author.is_none() {
                    author = a;
                }
                if time.is_none() {
                    time = t;
                }
                break;
            }
        }
    }

    // Strip meta from body: leading, trailing, or any pure header line.
    // Modern Discuz often puts `AUTHOR 发表于 TIME` after the quoted text.
    let (a2, t2, body) = extract_and_strip_quote_meta(&text);
    if author.is_none() {
        author = a2;
    }
    if time.is_none() {
        time = t2;
    }
    text = body;

    let cleaned = collapse_blank_lines(text.trim());
    if cleaned.starts_with("回复") {
        let rest = cleaned.trim_start_matches("回复").trim();
        if let Some(idx) = rest.find("    ").filter(|&i| i < 10) {
            reply_to = Some(rest[..idx].trim().to_string());
        } else if let Some(idx) = rest.find(' ').filter(|&i| i < 10) {
            reply_to = Some(rest[..idx].trim().to_string());
        }
    }

    ContentNode::Quote {
        author,
        time,
        text: cleaned,
        pid: (!pid.is_empty()).then_some(pid),
        tid: (!tid.is_empty()).then_some(tid),
        reply_to,
    }
}

/// Parse a pure meta line: `原帖由 A 于 T 发表` or `A 发表于 T` (datetime only after).
fn parse_pure_quote_meta_line(header: &str) -> Option<(Option<String>, Option<String>)> {
    let header = header.trim();
    if header.is_empty() {
        return None;
    }
    if let Some(rest) = header.strip_prefix("原帖由") {
        let rest = rest.trim();
        if let Some((author, after)) = rest.split_once(" 于 ") {
            let author = author.trim();
            if let Some((time, tail)) = after.split_once("发表") {
                let t = time.trim();
                // Whole-line meta only: nothing meaningful after 发表.
                if !tail.trim().is_empty() {
                    return None;
                }
                if author.is_empty() && t.is_empty() {
                    return None;
                }
                return Some((
                    (!author.is_empty()).then(|| author.to_string()),
                    (!t.is_empty()).then(|| t.to_string()),
                ));
            }
        }
        return None;
    }
    if let Some((author, after)) = header.split_once("发表于") {
        let author = author.trim();
        // Usernames are short; long left-hand side is almost certainly body text.
        if author.is_empty() || author.chars().count() > 30 {
            return None;
        }
        let after = after.trim();
        let (time, rest) = split_datetime_prefix(after);
        if time.is_some() && rest.is_empty() {
            return Some((Some(author.to_string()), time));
        }
    }
    None
}

/// Extract author/time from leading meta, then drop every pure meta line (incl. trailing).
fn extract_and_strip_quote_meta(text: &str) -> (Option<String>, Option<String>, String) {
    let mut author = None;
    let mut time = None;
    let mut rest = text.to_string();

    // Leading header (own line, or `原帖由…发表` / `A 发表于 T` glued to body).
    if let Some((a, t, body)) = split_leading_quote_header(&rest) {
        author = a;
        time = t;
        rest = body;
    }

    let mut kept = Vec::new();
    for line in rest.split('\n') {
        if let Some((a, t)) = parse_pure_quote_meta_line(line) {
            if author.is_none() {
                author = a;
            }
            if time.is_none() {
                time = t;
            }
            continue;
        }
        kept.push(line);
    }

    (author, time, kept.join("\n"))
}

/// Split body when a header is present at the start (possibly glued to body on one line).
fn split_leading_quote_header(text: &str) -> Option<(Option<String>, Option<String>, String)> {
    let trimmed = text.trim_start();
    let first_line_end = trimmed.find('\n').unwrap_or(trimmed.len());
    let first = trimmed[..first_line_end].trim();
    let rest_after_line = trimmed[first_line_end..].trim_start();

    if let Some((author, time)) = parse_pure_quote_meta_line(first) {
        return Some((author, time, rest_after_line.to_string()));
    }

    // Header + body on one line: "原帖由 A 于 T 发表BODY"
    if let Some(rest) = first.strip_prefix("原帖由") {
        let rest = rest.trim_start();
        if let Some((author, after)) = rest.split_once(" 于 ") {
            if let Some(idx) = after.find("发表") {
                let time = after[..idx].trim();
                let mut body = after[idx + "发表".len()..].trim_start().to_string();
                if !rest_after_line.is_empty() {
                    if !body.is_empty() {
                        body.push('\n');
                    }
                    body.push_str(rest_after_line);
                }
                let author = author.trim();
                return Some((
                    (!author.is_empty()).then(|| author.to_string()),
                    (!time.is_empty()).then(|| time.to_string()),
                    body,
                ));
            }
        }
    }
    // "A 发表于 T BODY" on one line
    if let Some((author, after)) = first.split_once("发表于") {
        let author = author.trim();
        if !author.is_empty() && author.chars().count() <= 30 {
            let after = after.trim_start();
            let (time, body_inline) = split_datetime_prefix(after);
            if time.is_some() {
                let mut body = body_inline.trim_start().to_string();
                if !rest_after_line.is_empty() {
                    if !body.is_empty() {
                        body.push('\n');
                    }
                    body.push_str(rest_after_line);
                }
                return Some((Some(author.to_string()), time, body));
            }
        }
    }
    None
}

/// Pull a Discuz-like datetime prefix (`2026-7-9 09:42`) off the start of `s`.
fn split_datetime_prefix(s: &str) -> (Option<String>, &str) {
    let bytes = s.as_bytes();
    let mut i = 0usize;
    // date: digits-digits-digits
    let mut seps = 0u8;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_digit() {
            i += 1;
        } else if b == b'-' && seps < 2 {
            seps += 1;
            i += 1;
        } else {
            break;
        }
    }
    if seps != 2 || i == 0 {
        return (None, s);
    }
    let mut end = i;
    if bytes.get(i) == Some(&b' ') {
        let mut j = i + 1;
        let mut colons = 0u8;
        let mut digits = 0u8;
        while j < bytes.len() {
            let b = bytes[j];
            if b.is_ascii_digit() {
                digits += 1;
                j += 1;
            } else if b == b':' && colons < 2 {
                colons += 1;
                j += 1;
            } else {
                break;
            }
        }
        if digits >= 3 && colons >= 1 {
            end = j;
        }
    }
    let time = s[..end].trim();
    if time.is_empty() {
        (None, s)
    } else {
        (Some(time.to_string()), s[end..].trim_start())
    }
}

/// Normalize HTML text nodes: keep intentional inter-tag spaces, drop pretty-print newlines.
fn normalize_html_text(raw: &str) -> Option<String> {
    let t = raw.replace(['<', '>'], " ").replace('\u{a0}', " ");
    if t.is_empty() {
        return None;
    }
    if t.chars().all(|c| c.is_whitespace()) {
        // Horizontal spacing only (e.g. &nbsp; between <font> runs) → one space.
        // Vertical / indent whitespace from pretty-printed HTML → drop.
        if t.chars().any(|c| c == ' ') && !t.contains('\n') && !t.contains('\r') {
            return Some(" ".into());
        }
        return None;
    }
    Some(t)
}

/// Collapse consecutive blank lines to a single blank line; strip trailing blanks.
fn collapse_blank_lines(s: &str) -> String {
    let s = s.replace('\u{a0}', " ");
    let mut lines = Vec::new();
    let mut last_blank = false;
    for line in s.split('\n') {
        let is_blank = line.trim().is_empty();
        if is_blank {
            if !last_blank && !lines.is_empty() {
                lines.push(String::new());
            }
            last_blank = true;
        } else {
            lines.push(line.to_string());
            last_blank = false;
        }
    }
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

fn normalize_node_text(node: &mut ContentNode) {
    match node {
        ContentNode::Text { spans } => {
            for span in spans.iter_mut() {
                if let ContentSpan::Text { text, .. } = span {
                    *text = collapse_blank_lines(text);
                }
            }
        }
        ContentNode::Quote { text, .. } => {
            *text = collapse_blank_lines(text);
        }
        _ => {}
    }
}

fn trim_trailing_empty_nodes(nodes: &mut Vec<ContentNode>) {
    while let Some(last) = nodes.last() {
        let empty = match last {
            ContentNode::Text { spans } => spans.iter().all(|span| match span {
                ContentSpan::Text { text, .. } => text.trim().is_empty(),
                ContentSpan::Smiley { .. } => false,
            }),
            _ => false,
        };
        if empty {
            nodes.pop();
        } else {
            break;
        }
    }
    // Also trim trailing newlines on the final text node.
    if let Some(ContentNode::Text { spans }) = nodes.last_mut() {
        if let Some(ContentSpan::Text { text, .. }) = spans.last_mut() {
            *text = text.trim_end_matches(['\n', '\r', ' ']).to_string();
        }
    }
}

fn spans_have_content(spans: &[ContentSpan]) -> bool {
    spans.iter().any(|span| match span {
        ContentSpan::Text { text, .. } => !text.is_empty(),
        ContentSpan::Smiley { .. } => true,
    })
}

fn parse_smiley(
    element: ElementRef<'_>,
    urls: &ForumUrls,
) -> Option<(String, Option<String>, Option<String>)> {
    let src = absolute_url(urls, element.value().attr("src").unwrap_or_default());
    if !is_smilie_src(&src) {
        return None;
    }
    let code = parse_smilie_code(&src);
    let smilie_id = element.value().attr("smilieid").map(str::to_string);
    Some((src, code, smilie_id))
}

fn extract_quoted_arg(s: &str) -> Option<String> {
    let open = s.find(['\'', '"'])?;
    let quote = s.as_bytes()[open] as char;
    let rest = &s[open + 1..];
    let end = rest.find(quote)?;
    let inner = rest[..end].trim();
    if inner.is_empty() {
        None
    } else {
        Some(inner.to_string())
    }
}

pub(crate) fn parse_block_image(element: ElementRef<'_>, urls: &ForumUrls) -> Option<ContentNode> {
    let src = element.value().attr("src").unwrap_or_default();
    let id = element.value().attr("id").unwrap_or_default();
    let src = absolute_url(urls, src);

    if id.starts_with("aimg") || src.contains("images/common/none.gif") {
        let onclick = element.value().attr("onclick").unwrap_or_default();
        let file = element
            .value()
            .attr("file")
            .map(|f| absolute_url(urls, f))
            .unwrap_or_default();
        let mut full_url = if !file.is_empty() {
            file
        } else if onclick.starts_with("zoom") {
            extract_quoted_arg(onclick)
                .map(|target| absolute_url(urls, &target))
                .unwrap_or_default()
        } else if onclick.contains("attachment.php") {
            let part = extract_param(onclick, "attachment.php", "'");
            let part = if part.is_empty() {
                extract_param(onclick, "attachment.php", "\"")
            } else {
                part
            };
            absolute_url(urls, &format!("attachment.php{part}"))
        } else {
            String::new()
        };
        let thumb_url = if src.contains("thumb.") {
            Some(src.clone())
        } else {
            None
        };
        if full_url.is_empty() {
            full_url = thumb_url.clone().unwrap_or(src);
        }
        if full_url.contains("none.gif") {
            return None;
        }
        return Some(ContentNode::Image {
            url: full_url,
            thumb_url,
            size: None,
        });
    }

    if is_forum_ui_image(&src) || src.contains("data:image/") {
        return None;
    }

    if src.contains("://") {
        return Some(ContentNode::Image {
            url: src,
            thumb_url: None,
            size: None,
        });
    }

    None
}

fn is_smilie_src(src: &str) -> bool {
    src.contains("4d4y.com/forum/images/smilies/")
        || src.contains("hi-pda.com/forum/images/smilies/")
}

fn is_forum_ui_image(src: &str) -> bool {
    (src.contains("4d4y.com/forum/images/") || src.contains("hi-pda.com/forum/images/"))
        && !is_smilie_src(src)
}

fn parse_smilie_code(src: &str) -> Option<String> {
    const MARKER: &str = "/forum/images/smilies/";
    let idx = src.find(MARKER)?;
    let rest = &src[idx + MARKER.len()..];
    let end = rest.rfind('.')?;
    Some(rest[..end].replace('/', "_"))
}

fn absolute_url(urls: &ForumUrls, path: &str) -> String {
    if path.is_empty() || path.contains("://") {
        path.to_string()
    } else {
        format!("{}{}", urls.base, path.trim_start_matches('/'))
    }
}

fn normalize_url(url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else {
        format!("https://{url}")
    }
}

fn normalize_link_url(urls: &ForumUrls, href: &str) -> String {
    let href = href.replace(".hi-pda.com", ".4d4y.com");
    if href.starts_with("http://") || href.starts_with("https://") {
        href
    } else {
        absolute_url(urls, &href)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scraper::{Html, Selector};

    #[test]
    fn parses_text_and_link() {
        let html = Html::parse_document(
            r#"<div class="t_msgfont">hello <a href="https://example.com">link</a> world</div>"#,
        );
        let root = html
            .select(&Selector::parse("div.t_msgfont").unwrap())
            .next()
            .expect("content root");
        let nodes = parse_content(root, &ForumUrls::default_4d4y());
        assert!(!nodes.is_empty());
    }

    #[test]
    fn parses_inline_smilies() {
        let html = Html::parse_document(
            r#"<div class="t_msgfont">那只狗很帅<img src="https://img02.4d4y.com/forum/images/smilies/default/lol.gif" smilieid="9" border="0" alt="" /> </div>"#,
        );
        let root = html
            .select(&Selector::parse("div.t_msgfont").unwrap())
            .next()
            .expect("content root");
        let nodes = parse_content(root, &ForumUrls::default_4d4y());
        assert!(nodes.iter().any(|n| {
            matches!(
                n,
                ContentNode::Text { spans }
                    if spans.iter().any(|span| matches!(
                        span,
                        ContentSpan::Smiley {
                            code: Some(code),
                            smilie_id: Some(id),
                            ..
                        } if code == "default_lol" && id == "9"
                    ))
            )
        }));
    }

    #[test]
    fn smilie_code_from_path() {
        assert_eq!(
            parse_smilie_code("https://img02.4d4y.com/forum/images/smilies/default/biggrin.gif"),
            Some("default_biggrin".into())
        );
    }

    #[test]
    fn quote_yuanti_header_stripped_from_body() {
        let html = Html::parse_document(
            r#"<div class="t_msgfont"><div class="quote"><blockquote>原帖由 <i>金牙</i> 于 2008-11-29 19:27 发表<br />
中文手写还不错 </blockquote></div>回复正文</div>"#,
        );
        let root = html
            .select(&Selector::parse("div.t_msgfont").unwrap())
            .next()
            .expect("root");
        let nodes = parse_content(root, &ForumUrls::default_4d4y());
        let quote = nodes
            .iter()
            .find_map(|n| match n {
                ContentNode::Quote {
                    author, time, text, ..
                } => Some((author.clone(), time.clone(), text.clone())),
                _ => None,
            })
            .expect("quote node");
        assert_eq!(quote.0.as_deref(), Some("金牙"));
        assert_eq!(quote.1.as_deref(), Some("2008-11-29 19:27"));
        assert!(!quote.2.contains("原帖由"));
        assert!(!quote.2.contains("发表"));
        assert!(quote.2.contains("中文手写还不错"));
    }

    #[test]
    fn quote_pub_at_header_stripped() {
        let html = Html::parse_document(
            r#"<div class="t_msgfont"><div class="quote"><blockquote>puhongyi 发表于 2026-7-9 09:42
hello world</blockquote></div></div>"#,
        );
        let root = html
            .select(&Selector::parse("div.t_msgfont").unwrap())
            .next()
            .expect("root");
        let nodes = parse_content(root, &ForumUrls::default_4d4y());
        let quote = nodes
            .iter()
            .find_map(|n| match n {
                ContentNode::Quote {
                    author, time, text, ..
                } => Some((author.clone(), time.clone(), text.clone())),
                _ => None,
            })
            .expect("quote");
        assert_eq!(quote.0.as_deref(), Some("puhongyi"));
        assert_eq!(quote.1.as_deref(), Some("2026-7-9 09:42"));
        assert_eq!(quote.2, "hello world");
    }

    #[test]
    fn quote_trailing_pub_at_line_stripped() {
        // Modern Discuz: body first, then `AUTHOR 发表于 TIME` (often in <font>).
        let html = Html::parse_document(
            r#"<div class="t_msgfont"><div class="quote"><blockquote>有一年工会发纪念品 新秀丽的双肩包<br />
<br />
<font size="2">taicaile 发表于 2026-7-3 10:58</font></blockquote></div>让我看看京东</div>"#,
        );
        let root = html
            .select(&Selector::parse("div.t_msgfont").unwrap())
            .next()
            .expect("root");
        let nodes = parse_content(root, &ForumUrls::default_4d4y());
        let quote = nodes
            .iter()
            .find_map(|n| match n {
                ContentNode::Quote {
                    author, time, text, ..
                } => Some((author.clone(), time.clone(), text.clone())),
                _ => None,
            })
            .expect("quote");
        assert_eq!(quote.0.as_deref(), Some("taicaile"));
        assert_eq!(quote.1.as_deref(), Some("2026-7-3 10:58"));
        assert!(
            quote.2.contains("有一年工会发纪念品"),
            "body missing content: {}",
            quote.2
        );
        assert!(
            !quote.2.contains("发表于") && !quote.2.contains("taicaile"),
            "trailing meta still in body: {}",
            quote.2
        );
    }

    #[test]
    fn nbsp_between_fonts_keeps_space() {
        let html = Html::parse_document(
            r#"<div class="t_msgfont"><font>感受</font><font>&nbsp;&nbsp;</font><font>文中</font></div>"#,
        );
        let root = html
            .select(&Selector::parse("div.t_msgfont").unwrap())
            .next()
            .expect("root");
        let nodes = parse_content(root, &ForumUrls::default_4d4y());
        let text = nodes
            .iter()
            .find_map(|n| match n {
                ContentNode::Text { spans } => {
                    let mut s = String::new();
                    for span in spans {
                        if let ContentSpan::Text { text, .. } = span {
                            s.push_str(text);
                        }
                    }
                    Some(s)
                }
                _ => None,
            })
            .expect("text");
        assert!(
            text.contains("感受 文中") || text.contains("感受  文中"),
            "got {text:?}"
        );
    }

    #[test]
    fn underline_and_strike_styles() {
        let html = Html::parse_document(
            r#"<div class="t_msgfont"><u>under</u><strike>out</strike></div>"#,
        );
        let root = html
            .select(&Selector::parse("div.t_msgfont").unwrap())
            .next()
            .expect("root");
        let nodes = parse_content(root, &ForumUrls::default_4d4y());
        let spans = match &nodes[0] {
            ContentNode::Text { spans } => spans,
            _ => panic!("expected text"),
        };
        assert!(spans.iter().any(|s| matches!(
            s,
            ContentSpan::Text {
                text,
                style,
                ..
            } if text.contains("under") && style.underline
        )));
        assert!(spans.iter().any(|s| matches!(
            s,
            ContentSpan::Text {
                text,
                style,
                ..
            } if text.contains("out") && style.strikethrough
        )));
    }

    #[test]
    fn zoom_lazy_load_urls() {
        let urls = ForumUrls::default_4d4y();
        let cases = [
            (
                r#"<img onclick="zoom(this, 'https://img02.4d4y.com/forum/attachments/day_260630/2606302322368df5cc713ab026.jpeg')" src="https://img02.4d4y.com/forum/attachments/day_260630/2606302322368df5cc713ab026.jpeg.thumb.jpg" id="aimg_6121113" />"#,
                "https://img02.4d4y.com/forum/attachments/day_260630/2606302322368df5cc713ab026.jpeg",
                "https://img02.4d4y.com/forum/attachments/day_260630/2606302322368df5cc713ab026.jpeg.thumb.jpg",
            ),
            (
                r#"<img onclick="zoom(this, 'https://img02.4d4y.com/forum/attachments/day_260630/2606302332236bbab3e809e92b.jpg')" src="https://img02.4d4y.com/forum/attachments/day_260630/2606302332236bbab3e809e92b.jpg.thumb.jpg" id="aimg_6121115" />"#,
                "https://img02.4d4y.com/forum/attachments/day_260630/2606302332236bbab3e809e92b.jpg",
                "https://img02.4d4y.com/forum/attachments/day_260630/2606302332236bbab3e809e92b.jpg.thumb.jpg",
            ),
        ];
        for (html, want_url, want_thumb) in cases {
            let doc = Html::parse_document(html);
            let img = doc.select(&Selector::parse("img").unwrap()).next().unwrap();
            let node = parse_block_image(img, &urls).expect("image node");
            if let ContentNode::Image { url, thumb_url, .. } = node {
                assert_eq!(url, want_url);
                assert_eq!(thumb_url.as_deref(), Some(want_thumb));
            } else {
                panic!("expected Image node");
            }
        }
    }
}
