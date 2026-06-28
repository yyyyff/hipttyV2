use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Style {
    pub fg: Option<String>,
    pub bold: bool,
    pub italic: bool,
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
        style.fg.is_none() && !style.bold && !style.italic
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
