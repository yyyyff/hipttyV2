use hiptty_render::{str_width, wrap_plain, Palette};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
use ratatui_textarea::TextArea;

use crate::ime::{cursor_after_text, set_ime_cursor, textarea_cursor_position};

pub const COMPOSER_MIN_HEIGHT: u16 = 8;
pub const COMPOSER_MAX_HEIGHT: u16 = 14;

pub fn composer_height(viewport_height: u16) -> u16 {
    (viewport_height / 3).clamp(COMPOSER_MIN_HEIGHT, COMPOSER_MAX_HEIGHT)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposerFocus {
    Type,
    Subject,
    Body,
}

pub struct ComposerProps<'a> {
    pub palette: Palette,
    pub header: &'a str,
    pub subject: &'a str,
    pub show_subject: bool,
    pub show_type: bool,
    pub type_label: &'a str,
    pub type_unset: bool,
    pub focus: ComposerFocus,
    pub textarea: &'a TextArea<'static>,
    pub error: Option<&'a str>,
    pub preparing: bool,
    pub submitting: bool,
    /// Read-only quote / reppost block above the body.
    pub quote_preview: Option<&'a str>,
    /// Scroll mirror for the body textarea (row, col); updated while drawing for IME.
    pub textarea_view_top: &'a mut (u16, u16),
}

pub fn draw_composer(frame: &mut Frame<'_>, area: Rect, props: ComposerProps<'_>) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let height = composer_height(area.height).min(area.height);
    let y = area.y + area.height.saturating_sub(height);
    let panel = Rect {
        x: area.x,
        y,
        width: area.width,
        height,
    };

    frame.render_widget(Clear, panel);

    let quote_lines = props
        .quote_preview
        .map(|q| quote_preview_line_count(q, panel.width.saturating_sub(2) as usize))
        .unwrap_or(0);

    let mut constraints = vec![Constraint::Length(1)]; // header
    if props.show_type {
        constraints.push(Constraint::Length(1));
    }
    if props.show_subject {
        constraints.push(Constraint::Length(1));
    }
    if quote_lines > 0 {
        constraints.push(Constraint::Length(quote_lines));
    }
    if props.error.is_some() {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Min(3)); // body
    constraints.push(Constraint::Length(1)); // hint

    let chunks = Layout::vertical(constraints).split(panel);
    let mut idx = 0;

    let busy = props.preparing || props.submitting;
    let header_style = if busy {
        props.palette.muted_style().add_modifier(Modifier::ITALIC)
    } else {
        props.palette.accent_style()
    };
    frame.render_widget(
        Paragraph::new(props.header).style(header_style),
        chunks[idx],
    );
    idx += 1;

    if props.show_type {
        let prefix = "分类  ";
        let focused = props.focus == ComposerFocus::Type;
        let value_style = if props.type_unset {
            if focused {
                props.palette.warn_style().add_modifier(Modifier::BOLD)
            } else {
                props.palette.warn_style()
            }
        } else if focused {
            props.palette.accent_style().add_modifier(Modifier::BOLD)
        } else {
            props.palette.foreground_style()
        };
        let value = if focused {
            format!("◂ {} ▸", props.type_label)
        } else {
            format!("{} ▸", props.type_label)
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(prefix, props.palette.secondary_style()),
                Span::styled(value, value_style),
            ])),
            chunks[idx],
        );
        idx += 1;
    }

    if props.show_subject {
        let prefix = "标题  ";
        let subject_prefix = Span::styled(prefix, props.palette.secondary_style());
        let subject_text = Span::styled(
            props.subject,
            if props.focus == ComposerFocus::Subject {
                props.palette.accent_style()
            } else {
                props.palette.foreground_style()
            },
        );
        frame.render_widget(
            Paragraph::new(Line::from(vec![subject_prefix, subject_text])),
            chunks[idx],
        );
        if props.focus == ComposerFocus::Subject {
            set_ime_cursor(
                frame,
                cursor_after_text(chunks[idx], str_width(prefix) as u16, props.subject),
            );
        }
        idx += 1;
    }

    if let Some(quote) = props.quote_preview {
        if quote_lines > 0 {
            let width = chunks[idx].width.saturating_sub(1).max(1) as usize;
            let lines = wrap_plain(quote, width, props.palette.muted_style());
            let shown: Vec<Line<'static>> = lines.into_iter().take(quote_lines as usize).collect();
            frame.render_widget(Paragraph::new(shown), chunks[idx]);
            idx += 1;
        }
    }

    if let Some(err) = props.error {
        frame.render_widget(
            Paragraph::new(err).style(props.palette.error_style()),
            chunks[idx],
        );
        idx += 1;
    }

    let shortcuts = if props.submitting {
        "发送中…（含图片压缩/上传）"
    } else if props.preparing {
        "准备中…"
    } else if props.show_type && props.show_subject {
        "Ctrl+Enter/S 发送  Esc 取消  Tab 切换  ←→ 分类"
    } else if props.show_subject {
        "Ctrl+Enter/S 发送  Esc 取消  Tab 切换标题"
    } else {
        "Ctrl+Enter/S 发送  Esc 取消"
    };
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(props.palette.muted_style())
        .title(Span::styled(shortcuts, props.palette.secondary_style()));
    let textarea_area = chunks[idx];
    let inner = block.inner(textarea_area);
    frame.render_widget(block, textarea_area);
    frame.render_widget(props.textarea, inner);
    if props.focus == ComposerFocus::Body && !busy {
        let pos = textarea_cursor_position(props.textarea, inner, props.textarea_view_top);
        set_ime_cursor(frame, pos);
    }
    idx += 1;

    if idx < chunks.len() {
        let hint = match props.focus {
            ComposerFocus::Type => "←→ / h l 选择分类  Tab 下一栏",
            ComposerFocus::Subject if props.show_subject => "Tab 切换  输入标题",
            ComposerFocus::Body if props.quote_preview.is_some() => {
                "上方为引用原文（只读）· 在此输入回复"
            }
            ComposerFocus::Body | ComposerFocus::Subject => "正文",
        };
        frame.render_widget(
            Paragraph::new(hint).style(props.palette.secondary_style()),
            chunks[idx],
        );
    }
}

const QUOTE_PREVIEW_MAX_LINES: u16 = 4;

fn quote_preview_line_count(quote: &str, width: usize) -> u16 {
    if quote.trim().is_empty() || width == 0 {
        return 0;
    }
    let lines = wrap_plain(quote, width, Default::default());
    (lines.len() as u16).clamp(1, QUOTE_PREVIEW_MAX_LINES)
}

pub struct ConfirmProps<'a> {
    pub palette: Palette,
    pub title: &'a str,
    pub message: &'a str,
    pub loading: bool,
}

pub fn draw_confirm_dialog(frame: &mut Frame<'_>, area: Rect, props: ConfirmProps<'_>) {
    let width = area.width.min(48);
    let height = 8;
    let modal =
        crate::modal::begin_modal(frame, area, props.palette, props.title, width, height, None);
    let chunks = Layout::vertical([Constraint::Min(2), Constraint::Length(1)]).split(modal.body);
    frame.render_widget(
        Paragraph::new(props.message).style(props.palette.foreground_style()),
        chunks[0],
    );
    if props.loading {
        frame.render_widget(
            Paragraph::new("删除中...").style(props.palette.muted_style()),
            chunks[1],
        );
    } else {
        let actions = Line::from(vec![
            Span::styled(" [ 确认 (y) ] ", props.palette.accent_style()),
            Span::styled(" [ 取消 (n) ] ", props.palette.secondary_style()),
        ]);
        frame.render_widget(Paragraph::new(actions), chunks[1]);
    }
}
