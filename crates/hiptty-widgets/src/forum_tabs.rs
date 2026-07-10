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

/// Layout tabs left-to-right but **always reserve the active forum first**, so a
/// narrow title row never drops the current section while showing inactive ones.
fn layout_forum_tabs(area: Rect, props: &ForumTabsProps<'_>) -> (Line<'static>, ForumTabHits) {
    let mut hits = ForumTabHits::default();
    if area.width == 0 {
        return (Line::from(""), hits);
    }

    let sep_w = str_width(SEP) as u16;
    let active_idx = props
        .default_forums
        .iter()
        .position(|&fid| fid == props.active_fid);

    // Precompute display labels (untruncated) for all three slots.
    let labels: Vec<(usize, String, bool)> = props
        .default_forums
        .iter()
        .enumerate()
        .map(|(i, &fid)| {
            let active = Some(i) == active_idx || (active_idx.is_none() && fid == props.active_fid);
            let name = forum_name(fid).unwrap_or("?");
            let label = maybe_mask_cjk(name, props.mask_cjk);
            let text = if active {
                format!("▸{label}")
            } else {
                format!(" {label}")
            };
            (i, text, active)
        })
        .collect();

    // Order: active tab first for width budget, then remaining in original order.
    let mut order: Vec<usize> = Vec::with_capacity(3);
    if let Some(ai) = active_idx {
        order.push(ai);
    }
    for i in 0..labels.len() {
        if Some(i) != active_idx {
            order.push(i);
        }
    }

    // Measure how many tabs fit when active is guaranteed first.
    let mut fit: Vec<usize> = Vec::new();
    let mut used = 0u16;
    for (k, &idx) in order.iter().enumerate() {
        let label_w = str_width(&labels[idx].1) as u16;
        let need = if k == 0 {
            label_w
        } else {
            sep_w.saturating_add(label_w)
        };
        // Active always takes at least a truncated slot if area is tiny.
        if k == 0 {
            let take = need.min(area.width).max(1.min(area.width));
            if take == 0 {
                break;
            }
            fit.push(idx);
            used = take;
            continue;
        }
        if used.saturating_add(need) > area.width {
            break;
        }
        fit.push(idx);
        used = used.saturating_add(need);
    }

    // Paint in original left-to-right index order among the ones that fit.
    fit.sort_unstable();

    let mut spans = Vec::new();
    let mut x = area.x;
    let mut remaining = area.width;

    for (paint_i, &idx) in fit.iter().enumerate() {
        if paint_i > 0 {
            if remaining < sep_w {
                break;
            }
            spans.push(Span::styled(SEP, props.palette.muted_style()));
            x = x.saturating_add(sep_w);
            remaining = remaining.saturating_sub(sep_w);
        }

        let (i, full_label, active) = &labels[idx];
        let max_label = remaining as usize;
        if max_label == 0 {
            break;
        }
        let display = truncate_str(full_label, max_label);
        let width = str_width(&display).min(remaining as usize) as u16;
        if width == 0 {
            break;
        }

        let hovered = props.hover_tab == Some(*i);
        let style = if *active {
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
        hits.tabs[*i] = Some(Rect {
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

    #[test]
    fn active_tab_visible_when_narrow() {
        // Active is the third default forum; sequential layout would drop it.
        let area = Rect::new(0, 0, 12, 1);
        let (_, hits) = layout_forum_tabs(area, &props(7));
        assert!(
            hits.tabs[2].is_some(),
            "active third tab must remain visible"
        );
        // Prefer active over filling inactive when budget is tight.
        let visible = hits.tabs.iter().filter(|t| t.is_some()).count();
        assert!(visible >= 1);
        assert!(visible <= 2);
    }

    #[test]
    fn active_first_tab_still_leftmost_when_all_fit() {
        let area = Rect::new(0, 0, 80, 1);
        let (_, hits) = layout_forum_tabs(area, &props(2));
        let first = hits.tabs[0].expect("tab0");
        assert_eq!(first.x, area.x);
    }
}
