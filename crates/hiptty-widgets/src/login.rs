use hiptty_core::{security_question_label, SECURITY_QUESTIONS};
use hiptty_render::Palette;
use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::logo::draw_login_logo;

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

pub fn draw_login(frame: &mut Frame<'_>, area: Rect, props: LoginFormProps<'_>) {
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .split(area);

    draw_login_logo(frame, chunks[0], props.palette);

    let label_style = props.palette.secondary_style();
    let field_style = |focused: bool| {
        if focused {
            Style::default()
                .fg(props.palette.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            props.palette.primary_style()
        }
    };

    draw_labeled_field(
        frame,
        chunks[2],
        "用户名",
        props.username,
        props.focused == LoginField::Username,
        label_style,
        field_style(props.focused == LoginField::Username),
    );
    draw_labeled_field(
        frame,
        chunks[3],
        "密码",
        &"*".repeat(props.password.chars().count()),
        props.focused == LoginField::Password,
        label_style,
        field_style(props.focused == LoginField::Password),
    );

    let qid = SECURITY_QUESTIONS[props.security_index].0;
    let q_label = security_question_label(qid);
    let q_display = format!("[ {q_label}  ◂ ▸ ]");
    draw_labeled_field(
        frame,
        chunks[4],
        "安全提问",
        &q_display,
        props.focused == LoginField::SecurityQuestion,
        label_style,
        field_style(props.focused == LoginField::SecurityQuestion),
    );

    let answer_active = props.security_index > 0;
    let answer_style = if answer_active {
        field_style(props.focused == LoginField::SecurityAnswer)
    } else {
        props.palette.dim_style()
    };
    draw_labeled_field(
        frame,
        chunks[5],
        "回答",
        if answer_active {
            props.security_answer
        } else {
            ""
        },
        props.focused == LoginField::SecurityAnswer && answer_active,
        label_style,
        answer_style,
    );

    let submit_label = if props.loading {
        "[  登录中...  ]"
    } else {
        "[  登  录  ]  (Enter 确认)"
    };
    let submit_style = field_style(props.focused == LoginField::Submit);
    frame.render_widget(
        Paragraph::new(submit_label)
            .style(submit_style)
            .alignment(Alignment::Center),
        chunks[7],
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
        Paragraph::new("Tab 切换 · Esc 退出")
            .style(props.palette.dim_style())
            .alignment(Alignment::Center),
        chunks[9],
    );
}

fn draw_labeled_field(
    frame: &mut Frame<'_>,
    area: Rect,
    label: &str,
    value: &str,
    _focused: bool,
    label_style: Style,
    value_style: Style,
) {
    let cols = Layout::horizontal([Constraint::Length(14), Constraint::Min(0)]).split(area);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(label, label_style))),
        cols[0],
    );
    let display = format!("[{value}]");
    frame.render_widget(Paragraph::new(display).style(value_style), cols[1]);
}
