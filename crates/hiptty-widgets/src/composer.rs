use hiptty_render::Palette;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
use ratatui_textarea::TextArea;

pub const COMPOSER_MIN_HEIGHT: u16 = 8;
pub const COMPOSER_MAX_HEIGHT: u16 = 12;

pub fn composer_height(viewport_height: u16) -> u16 {
    (viewport_height / 3).clamp(COMPOSER_MIN_HEIGHT, COMPOSER_MAX_HEIGHT)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposerFocus {
    Subject,
    Body,
    ImagePath,
}

pub struct ComposerProps<'a> {
    pub palette: Palette,
    pub header: &'a str,
    pub subject: &'a str,
    pub show_subject: bool,
    pub focus: ComposerFocus,
    pub textarea: &'a TextArea<'static>,
    pub error: Option<&'a str>,
    pub loading: bool,
    pub image_path: Option<&'a str>,
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

    let mut constraints = vec![Constraint::Length(1)];
    if props.show_subject {
        constraints.push(Constraint::Length(1));
    }
    if props.image_path.is_some() {
        constraints.push(Constraint::Length(1));
    }
    if props.error.is_some() {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Min(3));
    constraints.push(Constraint::Length(1));

    let chunks = Layout::vertical(constraints).split(panel);
    let mut idx = 0;

    let header_style = if props.loading {
        props.palette.muted_style().add_modifier(Modifier::ITALIC)
    } else {
        props.palette.accent_style()
    };
    frame.render_widget(
        Paragraph::new(props.header).style(header_style),
        chunks[idx],
    );
    idx += 1;

    if props.show_subject {
        let subject_prefix = Span::styled("标题  ", props.palette.secondary_style());
        let subject_text = Span::styled(
            props.subject,
            if props.focus == ComposerFocus::Subject {
                props.palette.accent_style()
            } else {
                props.palette.foreground_style()
            },
        );
        let cursor = if props.focus == ComposerFocus::Subject {
            Span::styled("█", props.palette.accent_style())
        } else {
            Span::raw("")
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![subject_prefix, subject_text, cursor])),
            chunks[idx],
        );
        idx += 1;
    }

    if let Some(path) = props.image_path {
        let prefix = Span::styled("图片路径  ", props.palette.secondary_style());
        let path_span = Span::styled(path, props.palette.foreground_style());
        let cursor = Span::styled("█", props.palette.accent_style());
        frame.render_widget(
            Paragraph::new(Line::from(vec![prefix, path_span, cursor])),
            chunks[idx],
        );
        idx += 1;
    }

    if let Some(err) = props.error {
        frame.render_widget(
            Paragraph::new(err).style(props.palette.error_style()),
            chunks[idx],
        );
        idx += 1;
    }

    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(props.palette.muted_style())
        .title(Span::styled(
            if props.loading {
                "准备中..."
            } else {
                "Ctrl+S 发送  Esc 取消  Ctrl+I 插图"
            },
            props.palette.muted_style(),
        ));
    let textarea_area = chunks[idx];
    let inner = block.inner(textarea_area);
    frame.render_widget(block, textarea_area);
    frame.render_widget(props.textarea, inner);
    idx += 1;

    if idx < chunks.len() {
        let hint = if props.image_path.is_some() {
            "Enter 确认路径  Esc 返回编辑"
        } else if props.show_subject && props.focus == ComposerFocus::Subject {
            "Tab 进入正文"
        } else {
            "Tab 切换标题"
        };
        frame.render_widget(
            Paragraph::new(hint).style(props.palette.muted_style()),
            chunks[idx],
        );
    }
}

pub struct ConfirmProps<'a> {
    pub palette: Palette,
    pub title: &'a str,
    pub message: &'a str,
    pub loading: bool,
}

pub fn draw_confirm_dialog(frame: &mut Frame<'_>, area: Rect, props: ConfirmProps<'_>) {
    let width = area.width.min(48);
    let height = 7;
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    let dialog = Rect {
        x,
        y,
        width,
        height,
    };
    frame.render_widget(Clear, dialog);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(props.palette.accent_style())
        .title(props.title);
    let inner = block.inner(dialog);
    frame.render_widget(block, dialog);
    let chunks = Layout::vertical([Constraint::Min(2), Constraint::Length(1)]).split(inner);
    frame.render_widget(
        Paragraph::new(props.message).style(props.palette.foreground_style()),
        chunks[0],
    );
    let actions = if props.loading {
        "删除中..."
    } else {
        "y 确认  n/Esc 取消"
    };
    frame.render_widget(
        Paragraph::new(actions).style(props.palette.secondary_style()),
        chunks[1],
    );
}
