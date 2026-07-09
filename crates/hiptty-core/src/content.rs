use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Style {
    pub fg: Option<String>,
    pub bold: bool,
    pub italic: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub underline: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub strikethrough: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextSpan {
    pub text: String,
    #[serde(default, skip_serializing_if = "Style::is_default")]
    pub style: Style,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Inline fragment inside a `Text` block (plain text or forum smilie).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentSpan {
    Text {
        text: String,
        #[serde(default, skip_serializing_if = "Style::is_default")]
        style: Style,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        url: Option<String>,
    },
    Smiley {
        url: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        code: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        smilie_id: Option<String>,
    },
}

impl From<TextSpan> for ContentSpan {
    fn from(span: TextSpan) -> Self {
        Self::Text {
            text: span.text,
            style: span.style,
            url: span.url,
        }
    }
}

impl Style {
    fn is_default(style: &Style) -> bool {
        style.fg.is_none()
            && !style.bold
            && !style.italic
            && !style.underline
            && !style.strikethrough
    }
}

/// Structured forum post body — maps to hipda `ContentAbs` hierarchy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentNode {
    Text {
        spans: Vec<ContentSpan>,
    },
    Quote {
        author: Option<String>,
        time: Option<String>,
        text: String,
        pid: Option<String>,
        tid: Option<String>,
        reply_to: Option<String>,
    },
    Image {
        url: String,
        thumb_url: Option<String>,
        size: Option<u64>,
    },
    Attachment {
        name: String,
        url: String,
        size: Option<u64>,
    },
    FloorRef {
        floor: u32,
        author: Option<String>,
        pid: Option<String>,
        tid: Option<String>,
    },
    AppMark {
        text: String,
        url: Option<String>,
    },
}

/// Flatten parsed post content into BBCode-ish plain text for editing.
pub fn content_nodes_to_plain(nodes: &[ContentNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            ContentNode::Text { spans } => {
                for span in spans {
                    match span {
                        ContentSpan::Text { text, .. } => out.push_str(text),
                        ContentSpan::Smiley { code, .. } => {
                            if let Some(code) = code {
                                out.push_str(code);
                            }
                        }
                    }
                }
            }
            ContentNode::Quote { text, .. } => {
                if !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str(text);
            }
            ContentNode::Image { url, .. } => {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(&format!("[img]{url}[/img]"));
            }
            ContentNode::Attachment { url, .. } => {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(&format!("[url]{url}[/url]"));
            }
            ContentNode::FloorRef { floor, author, .. } => {
                let author = author.as_deref().unwrap_or("?");
                out.push_str(&format!(">>> #{floor} @{author} "));
            }
            ContentNode::AppMark { text, .. } => out.push_str(text),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_joins_spans() {
        let nodes = vec![ContentNode::Text {
            spans: vec![ContentSpan::Text {
                text: "hello".into(),
                style: Style::default(),
                url: None,
            }],
        }];
        assert_eq!(content_nodes_to_plain(&nodes), "hello");
    }
}
