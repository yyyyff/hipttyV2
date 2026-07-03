use hiptty_core::{content_nodes_to_plain, Post, PostAction};
use hiptty_widgets::ComposerFocus;
use ratatui_textarea::TextArea;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposerKind {
    Reply,
    Quote,
    NewThread,
    Edit,
    PmReply,
}

#[derive(Debug)]
pub struct ComposerState {
    pub kind: ComposerKind,
    pub action: PostAction,
    pub header: String,
    pub textarea: TextArea<'static>,
    pub subject: String,
    pub show_subject: bool,
    pub focus: ComposerFocus,
    pub preparing: bool,
    pub submitting: bool,
    pub error: Option<String>,
    pub image_path: Option<String>,
    pub pm_uid: Option<String>,
}

impl ComposerState {
    pub fn open(
        kind: ComposerKind,
        action: PostAction,
        header: String,
        body: String,
        subject: Option<String>,
    ) -> Self {
        let show_subject = matches!(kind, ComposerKind::NewThread | ComposerKind::Edit);
        let mut textarea = TextArea::default();
        if !body.is_empty() {
            textarea = TextArea::from(body.lines().map(str::to_string).collect::<Vec<_>>());
        }
        Self {
            kind,
            action,
            header,
            textarea,
            subject: subject.unwrap_or_default(),
            show_subject,
            focus: if show_subject {
                ComposerFocus::Subject
            } else {
                ComposerFocus::Body
            },
            preparing: false,
            submitting: false,
            error: None,
            image_path: None,
            pm_uid: None,
        }
    }

    pub fn preparing(kind: ComposerKind, action: PostAction, header: String) -> Self {
        let mut state = Self::open(kind, action, header, String::new(), None);
        state.preparing = true;
        state
    }

    pub fn body_text(&self) -> String {
        self.textarea.lines().join("\n")
    }

    pub fn apply_prepare(
        &mut self,
        quote_text: Option<String>,
        subject: Option<String>,
        fallback_body: Option<String>,
    ) {
        self.preparing = false;
        if let Some(subject) = subject {
            self.subject = subject;
        }
        let body = quote_text.or(fallback_body).unwrap_or_default();
        if !body.is_empty() {
            self.textarea = TextArea::from(body.lines().map(str::to_string).collect::<Vec<_>>());
        }
        if self.show_subject {
            self.focus = ComposerFocus::Body;
        }
    }
}

#[derive(Debug)]
pub struct ConfirmDeleteState {
    pub action: PostAction,
    pub label: String,
    pub submitting: bool,
}

pub fn reply_thread_action(tid: &str) -> PostAction {
    PostAction::ReplyThread { tid: tid.to_string() }
}

pub fn quote_post_action(tid: &str, pid: &str) -> PostAction {
    PostAction::QuotePost {
        tid: tid.to_string(),
        pid: pid.to_string(),
    }
}

pub fn edit_post_action(tid: &str, pid: &str, fid: u32, page: u32) -> PostAction {
    PostAction::EditPost {
        tid: tid.to_string(),
        pid: pid.to_string(),
        fid,
        page,
    }
}

pub fn new_thread_action(fid: u32) -> PostAction {
    PostAction::NewThread { fid, type_id: None }
}

pub fn delete_post_action(tid: &str, pid: &str, fid: u32) -> PostAction {
    PostAction::QuickDelete {
        tid: tid.to_string(),
        pid: pid.to_string(),
        fid,
    }
}

pub fn quote_header(post: &Post) -> String {
    format!("引用 #{} @{}", post.floor, post.author)
}

pub fn edit_header(post: &Post) -> String {
    format!("编辑 #{}", post.floor)
}

pub fn edit_body(post: &Post) -> String {
    content_nodes_to_plain(&post.content)
}

pub fn delete_label(post: &Post) -> String {
    format!("确定删除 #{} @{} 的帖子？", post.floor, post.author)
}