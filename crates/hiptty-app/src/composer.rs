use std::collections::BTreeMap;

use hiptty_core::{content_nodes_to_plain, Post, PostAction, PrePostInfo};
use hiptty_widgets::ComposerFocus;
use ratatui_textarea::{CursorMove, TextArea};

/// Buy & Sell forum id (hipda `FID_BS`).
pub const FID_BS: u32 = 6;

/// Forums that hard-require a thread type on new posts (hipda hardcodes B&S only).
pub const REQUIRES_TYPE_FIDS: &[u32] = &[FID_BS];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposerKind {
    Reply,
    Quote,
    NewThread,
    Edit,
    PmReply,
}

#[derive(Debug, Clone)]
pub struct TypeChoice {
    pub id: String,
    pub label: String,
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
    pub pm_uid: Option<String>,
    /// Read-only quote/reppost block (server prepends it on submit; not part of body).
    pub quote_preview: Option<String>,
    /// Thread type options from PrePost `#typeid` (may include placeholder `0`).
    pub type_choices: Vec<TypeChoice>,
    /// Currently selected type id (`None` / `"0"` = unset).
    pub type_id: Option<String>,
    /// Mirror of textarea viewport scroll (row, col) for IME caret placement.
    pub textarea_view_top: (u16, u16),
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
            move_textarea_to_end(&mut textarea);
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
            pm_uid: None,
            quote_preview: None,
            type_choices: Vec::new(),
            type_id: None,
            textarea_view_top: (0, 0),
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

    /// Floor reply / quote: quote block is display-only; body is only user text.
    pub fn uses_quote_preview(&self) -> bool {
        matches!(
            self.action,
            PostAction::ReplyPost { .. } | PostAction::QuotePost { .. }
        )
    }

    pub fn need_type_ui(&self) -> bool {
        self.real_type_choices().next().is_some()
    }

    /// Must pick a non-placeholder type before send.
    /// Only hard-coded forums (hipda: B&S); most forums may have types but not require them.
    pub fn require_type(&self) -> bool {
        if !self.need_type_ui() {
            return false;
        }
        let fid = match self.action {
            PostAction::NewThread { fid, .. } => fid,
            _ => return false,
        };
        REQUIRES_TYPE_FIDS.contains(&fid)
    }

    /// Tab / default-focus order depends on whether type is mandatory.
    pub fn focus_order(&self) -> Vec<ComposerFocus> {
        let mut order = Vec::new();
        if self.require_type() {
            // 分类 → 标题 → 正文
            if self.need_type_ui() {
                order.push(ComposerFocus::Type);
            }
            if self.show_subject {
                order.push(ComposerFocus::Subject);
            }
            order.push(ComposerFocus::Body);
        } else {
            // 标题 → 正文 → 分类（可选）
            if self.show_subject {
                order.push(ComposerFocus::Subject);
            }
            order.push(ComposerFocus::Body);
            if self.need_type_ui() {
                order.push(ComposerFocus::Type);
            }
        }
        order
    }

    pub fn type_selected_ok(&self) -> bool {
        !self.require_type()
            || self
                .type_id
                .as_ref()
                .is_some_and(|id| !is_placeholder_type_id(id))
    }

    pub fn type_label(&self) -> &str {
        let id = self.type_id.as_deref().unwrap_or("0");
        if is_placeholder_type_id(id) {
            return "请选择分类";
        }
        self.type_choices
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.label.as_str())
            .unwrap_or("请选择分类")
    }

    fn real_type_choices(&self) -> impl Iterator<Item = &TypeChoice> {
        self.type_choices
            .iter()
            .filter(|c| !is_placeholder_type_id(&c.id))
    }

    /// Cycle among real types (skips placeholder). `delta` is ±1.
    pub fn cycle_type(&mut self, delta: i32) {
        let real: Vec<&TypeChoice> = self.real_type_choices().collect();
        if real.is_empty() {
            return;
        }
        let cur = self.type_id.as_deref().unwrap_or("");
        let idx = real
            .iter()
            .position(|c| c.id == cur)
            .map(|i| {
                let n = real.len() as i32;
                ((i as i32 + delta).rem_euclid(n)) as usize
            })
            .unwrap_or(if delta >= 0 { 0 } else { real.len() - 1 });
        self.type_id = Some(real[idx].id.clone());
    }

    pub fn apply_prepare(&mut self, info: PrePostInfo, fallback_body: Option<String>) {
        self.preparing = false;
        if let Some(subject) = info.subject {
            if self.subject.is_empty() {
                self.subject = subject;
            }
        }

        if self.kind == ComposerKind::NewThread {
            self.apply_type_choices(info.type_id, info.type_values);
        }

        if self.uses_quote_preview() {
            let preview = info.quote_text.filter(|s| !s.trim().is_empty());
            self.quote_preview = preview;
            self.textarea = TextArea::default();
            self.textarea_view_top = (0, 0);
            self.focus = ComposerFocus::Body;
            return;
        }

        let body = info.quote_text.or(fallback_body).unwrap_or_default();
        if !body.is_empty() {
            self.textarea = TextArea::from(body.lines().map(str::to_string).collect::<Vec<_>>());
            self.textarea_view_top = (0, 0);
            move_textarea_to_end(&mut self.textarea);
        }

        // Required forums: start on type. Optional types: start on title/body.
        self.focus = self
            .focus_order()
            .into_iter()
            .next()
            .unwrap_or(ComposerFocus::Body);
    }

    fn apply_type_choices(
        &mut self,
        type_id: Option<String>,
        type_values: BTreeMap<String, String>,
    ) {
        let mut choices: Vec<TypeChoice> = type_values
            .into_iter()
            .map(|(id, label)| TypeChoice { id, label })
            .collect();
        // Stable order: placeholder first, then by id.
        choices.sort_by(|a, b| {
            let ap = is_placeholder_type_id(&a.id);
            let bp = is_placeholder_type_id(&b.id);
            match (ap, bp) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.id.cmp(&b.id),
            }
        });
        self.type_choices = choices;
        // Required forums: leave unset so user must pick. Optional: prefer a real type.
        if self.require_type() {
            self.type_id = self
                .type_choices
                .iter()
                .find(|c| is_placeholder_type_id(&c.id))
                .map(|c| c.id.clone())
                .or(Some("0".into()));
        } else {
            self.type_id = type_id
                .filter(|id| {
                    !is_placeholder_type_id(id)
                        && self.type_choices.iter().any(|c| c.id == *id)
                })
                .or_else(|| {
                    self.type_choices
                        .iter()
                        .find(|c| !is_placeholder_type_id(&c.id))
                        .map(|c| c.id.clone())
                })
                .or_else(|| self.type_choices.first().map(|c| c.id.clone()));
        }
    }

    /// Write selected type into `NewThread` action before submit.
    pub fn sync_type_into_action(&mut self) {
        if let PostAction::NewThread { fid, .. } = self.action {
            self.action = PostAction::NewThread {
                fid,
                type_id: self.type_id.clone(),
            };
        }
    }
}

pub fn is_placeholder_type_id(id: &str) -> bool {
    id.is_empty() || id == "0"
}

fn move_textarea_to_end(textarea: &mut TextArea<'_>) {
    textarea.move_cursor(CursorMove::Bottom);
    textarea.move_cursor(CursorMove::End);
}

#[derive(Debug)]
pub struct ConfirmDeleteState {
    pub action: PostAction,
    pub label: String,
    pub submitting: bool,
}

pub fn reply_thread_action(tid: &str) -> PostAction {
    PostAction::ReplyThread {
        tid: tid.to_string(),
    }
}

/// Discuz `reppost` — reply targeting a specific floor (not a full quote block).
pub fn reply_post_action(tid: &str, pid: &str) -> PostAction {
    PostAction::ReplyPost {
        tid: tid.to_string(),
        pid: pid.to_string(),
    }
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
    PostAction::NewThread {
        fid,
        type_id: None,
    }
}

pub fn delete_post_action(tid: &str, pid: &str, fid: u32) -> PostAction {
    PostAction::QuickDelete {
        tid: tid.to_string(),
        pid: pid.to_string(),
        fid,
    }
}

pub fn reply_floor_header(post: &Post) -> String {
    format!("回复 #{} @{}", post.floor, post.author)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn bs_choices() -> BTreeMap<String, String> {
        BTreeMap::from([
            ("0".into(), "请选择".into()),
            ("1".into(), "手机".into()),
            ("2".into(), "掌上电脑".into()),
        ])
    }

    #[test]
    fn bs_requires_type_and_rejects_placeholder() {
        let mut c = ComposerState::preparing(
            ComposerKind::NewThread,
            new_thread_action(FID_BS),
            "新帖".into(),
        );
        c.apply_type_choices(Some("0".into()), bs_choices());
        assert!(c.need_type_ui());
        assert!(c.require_type());
        assert!(!c.type_selected_ok());
        assert_eq!(c.focus_order().first(), Some(&ComposerFocus::Type));
        c.cycle_type(1);
        assert!(c.type_selected_ok());
        assert_eq!(c.type_id.as_deref(), Some("1"));
    }

    #[test]
    fn optional_type_forum_starts_on_subject_and_allows_unset() {
        let mut c = ComposerState::preparing(
            ComposerKind::NewThread,
            new_thread_action(2), // Discovery — not hard-required
            "新帖".into(),
        );
        c.apply_type_choices(Some("0".into()), bs_choices());
        assert!(c.need_type_ui());
        assert!(!c.require_type());
        assert!(c.type_selected_ok()); // not forced
        assert_eq!(
            c.focus_order(),
            vec![
                ComposerFocus::Subject,
                ComposerFocus::Body,
                ComposerFocus::Type
            ]
        );
    }

    #[test]
    fn discovery_without_types_ok() {
        let mut c = ComposerState::preparing(
            ComposerKind::NewThread,
            new_thread_action(2),
            "新帖".into(),
        );
        c.apply_type_choices(None, BTreeMap::new());
        assert!(!c.need_type_ui());
        assert!(!c.require_type());
        assert!(c.type_selected_ok());
    }
}
