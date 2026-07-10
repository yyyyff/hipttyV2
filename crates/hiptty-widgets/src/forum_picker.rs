use hiptty_core::{forum_children, forum_name, forum_picker_fids, FORUMS};
use hiptty_render::Palette;
use ratatui::{
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::{List, ListItem},
    Frame,
};

use crate::modal::{begin_modal, draw_menu_item};

const POPUP_WIDTH: u16 = 36;
const POPUP_HEIGHT: u16 = 22;

pub struct ForumPickerProps<'a> {
    pub palette: Palette,
    pub default_forums: &'a [u32; 3],
    pub current_fid: u32,
    pub selected: usize,
    pub scroll_offset: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct ForumPickerHit {
    pub entry_index: usize,
    pub area: Rect,
}

pub struct ForumPickerFrame {
    pub scroll_offset: usize,
    pub hits: Vec<ForumPickerHit>,
}

struct ForumPickerModel {
    rows: Vec<ListItem<'static>>,
    entry_lines: Vec<usize>,
}

type PickerItem = ListItem<'static>;

pub fn forum_picker_entries(current_fid: u32, default_forums: &[u32; 3]) -> Vec<u32> {
    forum_picker_fids(current_fid, default_forums)
}

pub fn draw_forum_picker(
    frame: &mut Frame<'_>,
    area: Rect,
    props: ForumPickerProps<'_>,
) -> ForumPickerFrame {
    let popup_width = area.width.min(POPUP_WIDTH);
    let popup_height = area.height.min(POPUP_HEIGHT);
    let modal = begin_modal(
        frame,
        area,
        props.palette,
        "切换版块",
        popup_width,
        popup_height,
        Some("j/k 移动  Enter 切换  Esc 关闭"),
    );

    let list_area = modal.body;
    let viewport = list_area.height.max(1) as usize;
    let model = build_forum_picker_model(
        props.current_fid,
        props.default_forums,
        props.palette,
        props.current_fid,
        props.selected,
    );
    let scroll = ensure_line_scroll(
        model.entry_lines.get(props.selected).copied().unwrap_or(0),
        props.scroll_offset,
        viewport,
    );

    let hits = picker_hits_for_viewport(&model.entry_lines, list_area, scroll, viewport);

    let visible: Vec<ListItem> = model.rows.into_iter().skip(scroll).take(viewport).collect();
    frame.render_widget(List::new(visible), list_area);

    ForumPickerFrame {
        scroll_offset: scroll,
        hits,
    }
}

fn picker_hits_for_viewport(
    entry_lines: &[usize],
    list_area: Rect,
    scroll: usize,
    viewport: usize,
) -> Vec<ForumPickerHit> {
    let mut hits = Vec::new();
    for vi in 0..viewport {
        let global_line = scroll + vi;
        let Some(entry_index) = entry_lines.iter().position(|&line| line == global_line) else {
            continue;
        };
        hits.push(ForumPickerHit {
            entry_index,
            area: Rect {
                x: list_area.x,
                y: list_area.y.saturating_add(vi as u16),
                width: list_area.width,
                height: 1,
            },
        });
    }
    hits
}

fn build_forum_picker_model(
    current_fid: u32,
    default_forums: &[u32; 3],
    palette: Palette,
    marker_fid: u32,
    selected: usize,
) -> ForumPickerModel {
    let subforums = forum_children(current_fid);
    let mut excluded: std::collections::HashSet<u32> = default_forums.iter().copied().collect();
    excluded.extend(subforums.iter().copied());

    let all_forums: Vec<u32> = FORUMS
        .iter()
        .map(|forum| forum.id)
        .filter(|fid| !excluded.contains(fid))
        .collect();

    let mut rows = Vec::new();
    let mut entry_lines = Vec::new();
    let mut line = 0usize;
    let mut entry_idx = 0usize;

    if !subforums.is_empty() {
        rows.push(section_header(palette, "子版块"));
        line += 1;
        rows.push(section_rule(palette));
        line += 1;
        for &fid in subforums {
            entry_lines.push(line);
            let row_selected = entry_idx == selected;
            rows.push(forum_row(palette, fid, marker_fid, row_selected));
            line += 1;
            entry_idx += 1;
        }
        rows.push(ListItem::new(Line::from("")));
        line += 1;
    }

    if !all_forums.is_empty() {
        rows.push(section_header(palette, "全部版块"));
        line += 1;
        rows.push(section_rule(palette));
        line += 1;
        for fid in all_forums {
            entry_lines.push(line);
            let row_selected = entry_idx == selected;
            rows.push(forum_row(palette, fid, marker_fid, row_selected));
            line += 1;
            entry_idx += 1;
        }
    }

    ForumPickerModel { rows, entry_lines }
}

fn section_header(palette: Palette, title: &'static str) -> PickerItem {
    ListItem::new(Line::from(Span::styled(
        title,
        palette.secondary_style().add_modifier(Modifier::BOLD),
    )))
}

fn section_rule(palette: Palette) -> ListItem<'static> {
    ListItem::new(Line::from(Span::styled(
        "──────────",
        palette.muted_style(),
    )))
}

fn forum_row(palette: Palette, fid: u32, marker_fid: u32, selected: bool) -> ListItem<'static> {
    let name = forum_name(fid).unwrap_or("?");
    let label = if fid == marker_fid {
        format!("● {name}")
    } else {
        name.to_string()
    };
    ListItem::new(draw_menu_item(palette, &label, selected))
}

fn ensure_line_scroll(selected_line: usize, scroll: usize, viewport: usize) -> usize {
    if viewport == 0 {
        return 0;
    }
    if selected_line < scroll {
        selected_line
    } else if selected_line >= scroll.saturating_add(viewport) {
        selected_line.saturating_sub(viewport.saturating_sub(1))
    } else {
        scroll
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_follows_selection_without_growing_modal() {
        let scroll = ensure_line_scroll(12, 0, 8);
        assert_eq!(scroll, 5);
        assert!(12 < scroll + 8);
    }

    #[test]
    fn discovery_picker_lists_subforums_first() {
        let model = build_forum_picker_model(2, &[2, 6, 7], Palette::default(), 2, 0);
        assert_eq!(
            model.entry_lines.len(),
            forum_picker_fids(2, &[2, 6, 7]).len()
        );
        assert_eq!(forum_children(2).len(), 2);
    }

    #[test]
    fn viewport_hits_map_visible_rows_to_entry_indices() {
        let model = build_forum_picker_model(2, &[2, 6, 7], Palette::default(), 2, 0);
        let area = Rect::new(10, 5, 40, 8);
        let hits = picker_hits_for_viewport(&model.entry_lines, area, 0, 8);
        let first = hits.first().expect("entry hit");
        assert_eq!(first.entry_index, 0);
        assert_eq!(
            first.area.y,
            area.y.saturating_add(model.entry_lines[0] as u16)
        );
    }
}
