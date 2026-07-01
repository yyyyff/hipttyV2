use hiptty_core::{security_question_label, SECURITY_QUESTIONS};
use hiptty_render::{str_width, truncate_str, Palette};
use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::logo::draw_login_logo;

/// Total width of the centered login form (logo + fields block).
const FORM_WIDTH: u16 = 52;
const LABEL_WIDTH: u16 = 10;
const MIN_UNDERLINE: usize = 15;
const MAX_UNDERLINE: usize = 30;
const DEFAULT_UNDERLINE: usize = 15;
/// Fixed width of label + gap + input column so field rows center as a block.
const ROW_BLOCK_WIDTH: u16 = LABEL_WIDTH + 1 + MAX_UNDERLINE as u16;
const INPUT_COL_WIDTH: u16 = MAX_UNDERLINE as u16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginField {
    Username,
    Password,
    SecurityQuestion,
    SecurityAnswer,
    Submit,
}

pub struct LoginFormProps<'a> {
    pub palette: Palette,
    pub username: &'a str,
    pub password: &'a str,
    pub security_index: usize,
    pub security_answer: &'a str,
    pub focused: LoginField,
    pub error: Option<&'a str>,
    pub loading: bool,
}

struct InputRowProps<'a> {
    label: &'a str,
    value: &'a str,
    placeholder: &'a str,
    focused: bool,
    disabled: bool,
}

pub fn draw_login(frame: &mut Frame<'_>, area: Rect, props: LoginFormProps<'_>) {
    let form_height = 18u16;
    let form_x = area.x + area.width.saturating_sub(FORM_WIDTH) / 2;
    let form_y = area.y + area.height.saturating_sub(form_height) / 2;
    let form = Rect {
        x: form_x,
        y: form_y,
        width: FORM_WIDTH.min(area.width),
        height: form_height.min(area.height),
    };

    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .split(form);

    draw_login_logo(frame, chunks[0], props.palette);

    let label_style = props.palette.secondary_style();

    draw_text_input(
        frame,
        chunks[2],
        InputRowProps {
            label: "用户名",
            value: props.username,
            placeholder: "",
            focused: props.focused == LoginField::Username,
            disabled: false,
        },
        label_style,
        props.palette,
    );
    draw_text_input(
        frame,
        chunks[3],
        InputRowProps {
            label: "密码",
            value: &"*".repeat(props.password.chars().count()),
            placeholder: "",
            focused: props.focused == LoginField::Password,
            disabled: false,
        },
        label_style,
        props.palette,
    );

    let qid = SECURITY_QUESTIONS[props.security_index].0;
    let q_label = security_question_label(qid);
    draw_security_picker(
        frame,
        chunks[4],
        "安全提问",
        q_label,
        props.focused == LoginField::SecurityQuestion,
        label_style,
        props.palette,
    );

    let answer_active = props.security_index > 0;
    draw_text_input(
        frame,
        chunks[5],
        InputRowProps {
            label: "回答",
            value: if answer_active {
                props.security_answer
            } else {
                ""
            },
            placeholder: if answer_active { "" } else { "—" },
            focused: props.focused == LoginField::SecurityAnswer && answer_active,
            disabled: !answer_active,
        },
        label_style,
        props.palette,
    );

    draw_submit_button(
        frame,
        chunks[7],
        props.focused == LoginField::Submit,
        props.loading,
        props.palette,
    );

    if let Some(err) = props.error {
        frame.render_widget(
            Paragraph::new(err)
                .style(props.palette.error_style())
                .alignment(Alignment::Center),
            chunks[8],
        );
    }

    frame.render_widget(
        Paragraph::new("Tab / ↑↓ 切换字段 · Enter 确认 · Esc 退出")
            .style(props.palette.dim_style())
            .alignment(Alignment::Center),
        chunks[9],
    );
}

/// Horizontally center the label+input row block within `area`.
fn centered_row_block(area: Rect) -> Rect {
    let width = ROW_BLOCK_WIDTH.min(area.width);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y,
        width,
        height: area.height,
    }
}

fn draw_text_input(
    frame: &mut Frame<'_>,
    area: Rect,
    row: InputRowProps<'_>,
    label_style: Style,
    palette: Palette,
) {
    let block = centered_row_block(area);
    if block.height < 2 {
        return;
    }

    let [text_row, underline_row] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(block);

    let display = if row.value.is_empty() && !row.placeholder.is_empty() {
        row.placeholder.to_string()
    } else {
        truncate_str(row.value, INPUT_COL_WIDTH as usize)
    };

    let value_style = if row.disabled {
        palette.dim_style()
    } else if row.focused {
        palette.accent_style().add_modifier(Modifier::BOLD)
    } else if row.value.is_empty() && !row.placeholder.is_empty() {
        palette.dim_style()
    } else {
        palette.primary_style()
    };

    let underline_style = if row.disabled {
        palette.dim_style()
    } else if row.focused {
        palette.accent_style()
    } else {
        palette.dim_style()
    };

    let label_area = Rect {
        x: text_row.x,
        y: text_row.y,
        width: LABEL_WIDTH,
        height: 1,
    };
    let input_area = Rect {
        x: text_row.x + LABEL_WIDTH + 1,
        y: text_row.y,
        width: INPUT_COL_WIDTH.min(text_row.width.saturating_sub(LABEL_WIDTH + 1)),
        height: 1,
    };

    let empty_input = row.value.is_empty() && row.placeholder.is_empty();
    let ul_w = underline_width(&display, empty_input);

    frame.render_widget(Paragraph::new(row.label).style(label_style), label_area);
    frame.render_widget(Paragraph::new(display).style(value_style), input_area);
    frame.render_widget(
        Paragraph::new("─".repeat(ul_w as usize)).style(underline_style),
        Rect {
            x: input_area.x,
            y: underline_row.y,
            width: ul_w,
            height: 1,
        },
    );
}

fn underline_width(display: &str, empty_input: bool) -> u16 {
    let w = if empty_input {
        DEFAULT_UNDERLINE
    } else {
        let content = str_width(display);
        if content == 0 {
            DEFAULT_UNDERLINE
        } else {
            content
        }
    };
    w.clamp(MIN_UNDERLINE, MAX_UNDERLINE) as u16
}

fn draw_security_picker(
    frame: &mut Frame<'_>,
    area: Rect,
    label: &str,
    question: &str,
    focused: bool,
    label_style: Style,
    palette: Palette,
) {
    let block = centered_row_block(area);
    if block.height < 2 {
        return;
    }

    let [text_row, underline_row] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(block);

    let arrow_style = if focused {
        palette.accent_style()
    } else {
        palette.dim_style()
    };
    let text_style = if focused {
        palette.accent_style().add_modifier(Modifier::BOLD)
    } else {
        palette.primary_style()
    };
    let underline_style = if focused {
        palette.accent_style()
    } else {
        palette.dim_style()
    };

    let label_area = Rect {
        x: text_row.x,
        y: text_row.y,
        width: LABEL_WIDTH,
        height: 1,
    };
    let input_area = Rect {
        x: text_row.x + LABEL_WIDTH + 1,
        y: text_row.y,
        width: INPUT_COL_WIDTH.min(text_row.width.saturating_sub(LABEL_WIDTH + 1)),
        height: 1,
    };

    frame.render_widget(Paragraph::new(label).style(label_style), label_area);

    let q_trunc = truncate_str(question, INPUT_COL_WIDTH as usize);
    let picker_text = format!("◂ {q_trunc} ▸");
    let line = Line::from(vec![
        Span::styled("◂ ", arrow_style),
        Span::styled(q_trunc, text_style),
        Span::styled(" ▸", arrow_style),
    ]);
    frame.render_widget(Paragraph::new(line), input_area);

    let ul_w = underline_width(&picker_text, false);
    frame.render_widget(
        Paragraph::new("─".repeat(ul_w as usize)).style(underline_style),
        Rect {
            x: input_area.x,
            y: underline_row.y,
            width: ul_w,
            height: 1,
        },
    );
}

fn draw_submit_button(
    frame: &mut Frame<'_>,
    area: Rect,
    focused: bool,
    loading: bool,
    palette: Palette,
) {
    if area.height < 2 {
        return;
    }

    let label = if loading { "登录中…" } else { "登  录" };
    let style = if focused {
        palette.accent_style().add_modifier(Modifier::BOLD)
    } else {
        palette.primary_style()
    };
    let underline_style = if focused {
        palette.accent_style()
    } else {
        palette.dim_style()
    };

    let text_width = str_width(label) as u16;
    let btn_x = area.x + area.width.saturating_sub(text_width) / 2;

    frame.render_widget(
        Paragraph::new(label).style(style),
        Rect {
            x: btn_x,
            y: area.y,
            width: text_width.max(1),
            height: 1,
        },
    );

    let underline_len = (text_width as usize + 2).clamp(MIN_UNDERLINE, MAX_UNDERLINE);
    let ul_x = area.x + area.width.saturating_sub(underline_len as u16) / 2;
    frame.render_widget(
        Paragraph::new("─".repeat(underline_len)).style(underline_style),
        Rect {
            x: ul_x,
            y: area.y + 1,
            width: underline_len as u16,
            height: 1,
        },
    );
}
