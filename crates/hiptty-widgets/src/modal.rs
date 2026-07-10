use hiptty_render::{fill_area_bg, Palette};
use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

pub const MODAL_FOOTER_LINES: u16 = 1;

pub struct ModalRects {
    pub dialog: Rect,
    pub body: Rect,
    pub footer: Option<Rect>,
}

pub fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

pub fn draw_modal_backdrop(_frame: &mut Frame<'_>, _area: Rect, _palette: Palette) {
    // Depth comes from `Palette::dimmed` on the page drawn underneath.
}

pub fn begin_modal<'a>(
    frame: &mut Frame<'_>,
    area: Rect,
    palette: Palette,
    title: &'a str,
    width: u16,
    height: u16,
    footer: Option<&'a str>,
) -> ModalRects {
    draw_modal_backdrop(frame, area, palette);

    let dialog = centered_rect(width, height, area);
    frame.render_widget(Clear, dialog);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(palette.accent_style())
        .title(Span::styled(
            format!(" {title} "),
            palette.foreground_style(),
        ))
        .title_alignment(Alignment::Center)
        .style(palette.modal_surface_style());
    let inner = block.inner(dialog);
    frame.render_widget(block, dialog);
    fill_area_bg(frame, inner, palette.modal_surface_style());

    let footer_lines = if footer.is_some() {
        MODAL_FOOTER_LINES
    } else {
        0
    };
    let chunks = if footer_lines > 0 {
        Layout::vertical([Constraint::Min(1), Constraint::Length(footer_lines)]).split(inner)
    } else {
        [inner, Rect::default()].into()
    };

    fill_area_bg(frame, chunks[0], palette.modal_surface_style());
    if footer.is_some() {
        fill_area_bg(frame, chunks[1], palette.modal_surface_style());
    }

    if let Some(hint) = footer {
        frame.render_widget(
            Paragraph::new(hint)
                .style(palette.muted_style())
                .alignment(Alignment::Center),
            chunks[1],
        );
    }

    ModalRects {
        dialog,
        body: chunks[0],
        footer: if footer.is_some() {
            Some(chunks[1])
        } else {
            None
        },
    }
}

pub fn draw_menu_item(palette: Palette, label: &str, selected: bool) -> Line<'static> {
    let prefix = if selected { "▸ " } else { "  " };
    let style = if selected {
        palette.accent_style().add_modifier(Modifier::BOLD)
    } else {
        palette.foreground_style()
    };
    Line::from(Span::styled(format!("{prefix}{label}"), style))
}

pub fn draw_label_value_row(
    palette: Palette,
    label: &str,
    value: &str,
    selected: bool,
    label_width: usize,
) -> Line<'static> {
    let prefix = if selected { "▸ " } else { "  " };
    let style = if selected {
        palette.accent_style()
    } else {
        palette.foreground_style()
    };
    Line::from(Span::styled(
        format!("{prefix}{label:<label_width$} [{value}]"),
        style,
    ))
}
