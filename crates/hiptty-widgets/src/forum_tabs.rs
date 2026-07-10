use hiptty_core::forum_name;
use hiptty_render::{maybe_mask_cjk, str_width, truncate_str, Palette};
use ratatui::{
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

const SEP: &str = " │ ";

pub struct ForumTabsProps<'a> {
    pub palette: Palette,
    pub default_forums: &'a [u32; 3],
    pub active_fid: u32,
    pub hover_tab: Option<usize>,
    pub mask_cjk: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ForumTabHits {
    pub tabs: [Option<Rect>; 3],
}

pub fn forum_tab_hits(area: Rect, props: &ForumTabsProps<'_>) -> ForumTabHits {
    layout_forum_tabs(area, props).1
}

pub fn draw_forum_tabs(
    frame: &mut Frame<'_>,
    area: Rect,
    props: ForumTabsProps<'_>,
) -> ForumTabHits {
    if area.width == 0 || area.height == 0 {
        return ForumTabHits::default();
    }

    let (line, hits) = layout_forum_tabs(area, &props);
    frame.render_widget(Paragraph::new(line), area);
    hits
}

fn layout_forum_tabs(area: Rect, props: &ForumTabsProps<'_>) -> (Line<'static>, ForumTabHits) {
    let mut hits = ForumTabHits::default();
    if area.width == 0 {
        return (Line::from(""), hits);
    }

    let sep_w = str_width(SEP) as u16;
    let mut spans = Vec::new();
    let mut x = area.x;
    let mut remaining = area.width;

    for (i, &fid) in props.default_forums.iter().enumerate() {
        if i > 0 {
            if remaining < sep_w {
                break;
            }
            spans.push(Span::styled(SEP, props.palette.muted_style()));
            x = x.saturating_add(sep_w);
            remaining = remaining.saturating_sub(sep_w);
        }

        let active = fid == props.active_fid;
        let prefix_w = 1usize;
        let max_label = remaining.saturating_sub(prefix_w as u16) as usize;
        if max_label == 0 {
            break;
        }

        let name = forum_name(fid).unwrap_or("?");
        let label = maybe_mask_cjk(name, props.mask_cjk);
        let text = truncate_str(label.as_ref(), max_label);
        let display = if active {
            format!("▸{text}")
        } else {
            format!(" {text}")
        };
        let width = str_width(&display).min(remaining as usize) as u16;
        if width == 0 {
            break;
        }

        let hovered = props.hover_tab == Some(i);
        let style = if active {
            props.palette.accent_style().add_modifier(Modifier::BOLD)
        } else if hovered {
            props
                .palette
                .accent_style()
                .add_modifier(Modifier::UNDERLINED)
        } else {
            props.palette.secondary_style()
        };
        spans.push(Span::styled(display, style));
        hits.tabs[i] = Some(Rect {
            x,
            y: area.y,
            width,
            height: area.height.max(1),
        });
        x = x.saturating_add(width);
        remaining = remaining.saturating_sub(width);
    }

    (Line::from(spans), hits)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn props(active_fid: u32) -> ForumTabsProps<'static> {
        ForumTabsProps {
            palette: Palette::default(),
            default_forums: &[2, 6, 7],
            active_fid,
            hover_tab: None,
            mask_cjk: false,
        }
    }

    fn tab_label(fid: u32, active: bool) -> String {
        let name = forum_name(fid).unwrap_or("?");
        if active {
            format!("▸{name}")
        } else {
            format!(" {name}")
        }
    }

    #[test]
    fn hit_widths_match_rendered_labels() {
        let area = Rect::new(5, 2, 120, 1);
        let p = props(2);
        let (line, hits) = layout_forum_tabs(area, &p);

        let first = hits.tabs[0].expect("first tab hit");
        assert_eq!(first.x, area.x);
        assert_eq!(
            first.width as usize,
            str_width(line.spans[0].content.as_ref())
        );
        assert_eq!(first.width as usize, str_width(&tab_label(2, true)));

        let second = hits.tabs[1].expect("second tab hit");
        assert_eq!(
            second.x,
            first.x.saturating_add(first.width) + str_width(SEP) as u16
        );
        assert_eq!(
            second.width as usize,
            str_width(line.spans[2].content.as_ref())
        );
    }

    #[test]
    fn hits_are_contiguous_without_gaps() {
        let area = Rect::new(0, 0, 80, 1);
        let (_, hits) = layout_forum_tabs(area, &props(6));
        let rects: Vec<Rect> = hits.tabs.into_iter().flatten().collect();
        assert_eq!(rects.len(), 3);

        let mut cursor = area.x;
        for (i, rect) in rects.iter().enumerate() {
            if i > 0 {
                cursor = cursor.saturating_add(str_width(SEP) as u16);
            }
            assert_eq!(rect.x, cursor, "tab {i} x mismatch");
            cursor = cursor.saturating_add(rect.width);
        }
    }
}
