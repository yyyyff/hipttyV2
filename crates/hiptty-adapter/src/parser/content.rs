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
                    let t = text.replace(['<', '>'], " ");
                    if !t.trim().is_empty() {
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
            "u" => self.walk_children(element, style),
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
            "strike" | "s" => self.walk_children(element, style),
            "font" => {
                let mut s = style.clone();
                if let Some(color) = element.value().attr("color") {
                    s.fg = Some(color.trim().to_string());
                }
                self.walk_children(element, s);
            }
            "br" => self.push_text("\n", &style, None),
            "hr" => self.push_text("\n---\n", &style, None),
            "blockquote" => self.walk_children(element, style),
            "ol" | "ul" | "li" | "p" | "table" | "tbody" | "tr" | "dl" | "dt" | "dd" => {
                if tag == "tr" {
                    self.push_text("\n", &style, None);
                } else if tag == "td" {
                    self.push_text(" ", &style, None);
                }
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
    let mut text = element.text().collect::<String>();

    if let Ok(sel) = Selector::parse("font[size=\"2\"], font") {
        if let Some(header) = element.select(&sel).next() {
            let header_text = header.text().collect::<String>();
            if header_text.contains("发表于") {
                let parts: Vec<_> = header_text.split("发表于").collect();
                if !parts.is_empty() {
                    author = Some(parts[0].trim().to_string());
                }
                if parts.len() > 1 {
                    time = Some(parts[1].trim().to_string());
                }
            }
            text = element
                .select(&Selector::parse("blockquote").unwrap())
                .next()
                .map(|b| b.text().collect())
                .unwrap_or(text);
        }
    }

    let cleaned = text.trim().to_string();
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

fn parse_block_image(element: ElementRef<'_>, urls: &ForumUrls) -> Option<ContentNode> {
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
        let mut full_url = if onclick.contains("attachment") {
            let part = extract_param(onclick, "attachment", "'");
            absolute_url(urls, &format!("attachment{part}"))
        } else {
            file
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
}
