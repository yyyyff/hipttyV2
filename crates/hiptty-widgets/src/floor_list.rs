use hiptty_core::Post;
use hiptty_image::{
    draw_graphic_in_viewport, graphics_bottom_margin, layout_post_blocks, ContentBlock, ImageCache,
    ImageKind, ImageState, InlinePart, AVATAR_COLS, IMAGE_FAIL_LABEL,
};
use hiptty_render::{
    clear_content_viewport, floor_header_rows_with_edit, format_signature, mask_line_cjk,
    maybe_mask_cjk, render_post_content_lines, str_width, Palette,
};
use ratatui::{
    layout::{Alignment, Rect},
    widgets::Paragraph,
    Frame,
};

use crate::poll_block::{draw_poll_block, poll_block_height};
use crate::scroll::WHEEL_LINES;

const DETAIL_STEP_LINES: u16 = WHEEL_LINES as u16;

const BAR_W: u16 = 1;
const AVATAR_W: u16 = AVATAR_COLS;
const HEADER_H: u16 = 2;
const GAP_H: u16 = 1;
const SEPARATOR_H: u16 = 1;

/// Absolute document row (scroll offset / floor top). Wider than terminal `u16`.
pub type DocY = u32;

pub struct FloorListProps<'a> {
    pub palette: Palette,
    pub posts: &'a [Post],
    pub selected: usize,
    pub scroll_top: DocY,
    pub show_avatar: bool,
    pub images: Option<&'a mut ImageCache>,
    pub mask_cjk: bool,
    /// When set (and `layout.width` matches the draw area), walk uses cached heights
    /// instead of re-wrapping every floor each frame.
    pub layout: Option<&'a FloorLayout>,
}

/// Precomputed per-floor heights and document offsets for the detail list.
///
/// Built in a **single** pass over posts. Scroll / hit-test helpers should reuse this
/// rather than calling [`measure_floor`] repeatedly (which re-wraps content each time).
///
/// [`Self::first_visible`] / [`Self::floor_index_at_line`] are O(log n) via
/// [`slice::partition_point`] on the monotonic `offsets` (and bottoms).
#[derive(Debug, Clone)]
pub struct FloorLayout {
    pub width: u16,
    /// Bumped when posts / image heights change so caches cannot reuse a same-width, same-count layout.
    pub revision: u64,
    pub heights: Vec<DocY>,
    pub offsets: Vec<DocY>,
    pub total: DocY,
}

impl FloorLayout {
    pub fn build(
        posts: &[Post],
        width: u16,
        palette: Palette,
        images: Option<&ImageCache>,
        revision: u64,
    ) -> Self {
        let mut heights = Vec::with_capacity(posts.len());
        let mut offsets = Vec::with_capacity(posts.len());
        let mut top = 0u32;
        for (idx, post) in posts.iter().enumerate() {
            offsets.push(top);
            let h = measure_floor(post, width, palette, images, idx + 1 < posts.len());
            heights.push(h);
            top = top.saturating_add(h);
        }
        Self {
            width,
            revision,
            heights,
            offsets,
            total: top,
        }
    }

    pub fn floor_count(&self) -> usize {
        self.heights.len()
    }

    pub fn matches(&self, width: u16, revision: u64) -> bool {
        self.width == width && self.revision == revision
    }

    pub fn height(&self, idx: usize) -> DocY {
        self.heights.get(idx).copied().unwrap_or(0)
    }

    pub fn offset(&self, idx: usize) -> DocY {
        self.offsets.get(idx).copied().unwrap_or(0)
    }

    /// Max document scroll so the last line can sit at the bottom of the viewport.
    pub fn max_scroll(&self, viewport_h: u16) -> DocY {
        if self.heights.is_empty() || viewport_h == 0 {
            return 0;
        }
        self.total.saturating_sub(u32::from(viewport_h))
    }

    /// First floor whose content bottom is past `scroll_top` (O(log n)).
    pub fn first_visible(&self, scroll_top: DocY) -> usize {
        if self.heights.is_empty() {
            return 0;
        }
        // Binary search: first index where offset[i] + height[i] > scroll_top.
        let mut lo = 0usize;
        let mut hi = self.heights.len();
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let bottom = self.offsets[mid].saturating_add(self.heights[mid]);
            if bottom <= scroll_top {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo.min(self.heights.len().saturating_sub(1))
    }

    pub fn last_visible(&self, scroll_top: DocY, viewport_h: u16) -> usize {
        if self.heights.is_empty() || viewport_h == 0 {
            return 0;
        }
        let viewport_bottom = scroll_top.saturating_add(u32::from(viewport_h));
        let first = self.first_visible(scroll_top);
        let mut last = first;
        for idx in first..self.heights.len() {
            let top = self.offsets[idx];
            if top >= viewport_bottom {
                break;
            }
            let bottom = top.saturating_add(self.heights[idx]);
            if bottom > scroll_top {
                last = idx;
            }
        }
        last
    }

    /// Floor containing document line `line` (O(log n)).
    pub fn floor_index_at_line(&self, line: DocY) -> usize {
        if self.offsets.is_empty() {
            return 0;
        }
        // Last index where offsets[i] <= line.
        let idx = self.offsets.partition_point(|&top| top <= line);
        idx.saturating_sub(1)
    }

    pub fn floors_per_page(&self, first_floor: usize, viewport_h: u16) -> usize {
        if self.heights.is_empty() || viewport_h == 0 {
            return 0;
        }
        let mut used = 0u32;
        let mut count = 0usize;
        for &h in self.heights.iter().skip(first_floor) {
            if count > 0 && used.saturating_add(h) > u32::from(viewport_h) {
                break;
            }
            used = used.saturating_add(h);
            count += 1;
        }
        count.max(1)
    }

    pub fn ensure_scroll_top(&self, selected: usize, scroll_top: DocY, viewport_h: u16) -> DocY {
        if self.heights.is_empty() || viewport_h == 0 {
            return 0;
        }
        let selected = selected.min(self.heights.len() - 1);
        let sel_top = self.offsets[selected];
        let floor_h = self.heights[selected];
        let sel_bottom = sel_top.saturating_add(floor_h);

        if sel_top < scroll_top {
            return sel_top;
        }
        if sel_bottom <= scroll_top.saturating_add(u32::from(viewport_h)) {
            return scroll_top;
        }
        if floor_h > u32::from(viewport_h) {
            let max_top = sel_bottom.saturating_sub(u32::from(viewport_h));
            return scroll_top.max(sel_top).min(max_top);
        }
        sel_bottom.saturating_sub(u32::from(viewport_h))
    }

    pub fn clamp_scroll_top(&self, scroll_top: DocY, viewport_h: u16) -> DocY {
        scroll_top.min(self.max_scroll(viewport_h))
    }

    pub fn capture_scroll_anchor(&self, scroll_top: DocY) -> DetailScrollAnchor {
        if self.heights.is_empty() {
            return DetailScrollAnchor {
                floor: 0,
                offset_in_floor: 0,
            };
        }
        let floor = self.first_visible(scroll_top);
        let floor_top = self.offset(floor);
        DetailScrollAnchor {
            floor,
            offset_in_floor: scroll_top.saturating_sub(floor_top),
        }
    }

    pub fn restore_scroll_anchor(&self, anchor: DetailScrollAnchor, viewport_h: u16) -> DocY {
        if self.heights.is_empty() {
            return 0;
        }
        let floor = anchor.floor.min(self.heights.len().saturating_sub(1));
        let floor_top = self.offset(floor);
        let floor_h = self.height(floor);
        let max_intra = floor_h.saturating_sub(1);
        let target = floor_top.saturating_add(anchor.offset_in_floor.min(max_intra));
        self.clamp_scroll_top(target, viewport_h)
    }

    pub fn detail_line_scroll(&self, scroll_top: DocY, delta_lines: i32, viewport_h: u16) -> DocY {
        if self.heights.is_empty() || viewport_h == 0 || delta_lines == 0 {
            return scroll_top;
        }
        let max_scroll = self.total.saturating_sub(u32::from(viewport_h));

        if delta_lines > 0 {
            let step = delta_lines.unsigned_abs();
            if scroll_top >= max_scroll {
                return max_scroll;
            }
            let first = self.first_visible(scroll_top);
            let floor_h = self.height(first);
            if floor_h <= u32::from(viewport_h) {
                return scroll_top.saturating_add(step).min(max_scroll);
            }
            let floor_top = self.offset(first);
            let floor_end = floor_read_end_scroll(floor_top, floor_h, viewport_h);
            if scroll_top >= floor_end {
                if first + 1 < self.heights.len() {
                    return self.offset(first + 1);
                }
                return max_scroll;
            }
            let candidate = scroll_top.saturating_add(step);
            if candidate > floor_end {
                if first + 1 < self.heights.len() {
                    return self.offset(first + 1);
                }
                return floor_end.min(max_scroll);
            }
            candidate.min(max_scroll)
        } else {
            let step = delta_lines.unsigned_abs();
            if scroll_top == 0 {
                return 0;
            }
            let first = self.first_visible(scroll_top);
            let floor_h = self.height(first);
            let floor_top = self.offset(first);
            if floor_h <= u32::from(viewport_h) {
                if scroll_top <= floor_top {
                    return self.scroll_to_previous_floor(first, viewport_h);
                }
                let candidate = scroll_top.saturating_sub(step);
                if candidate < floor_top {
                    return self.scroll_to_previous_floor(first, viewport_h);
                }
                return candidate;
            }
            if scroll_top <= floor_top {
                return self.scroll_to_previous_floor(first, viewport_h);
            }
            let candidate = scroll_top.saturating_sub(step);
            if candidate < floor_top {
                return self.scroll_to_previous_floor(first, viewport_h);
            }
            candidate
        }
    }

    pub fn detail_step_down(
        &self,
        selected: usize,
        scroll_top: DocY,
        viewport_h: u16,
    ) -> (usize, DocY) {
        if self.heights.is_empty() || viewport_h == 0 {
            return (0, 0);
        }
        let selected = selected.min(self.heights.len() - 1);
        let max_scroll = self.total.saturating_sub(u32::from(viewport_h));
        if scroll_top >= max_scroll {
            return (selected, max_scroll);
        }
        let first = self.first_visible(scroll_top);
        let floor_h = self.height(first);
        let new_scroll = if floor_h > u32::from(viewport_h) {
            self.detail_line_scroll(scroll_top, i32::from(DETAIL_STEP_LINES), viewport_h)
        } else if first + 1 < self.heights.len() {
            self.offset(first + 1)
        } else {
            max_scroll
        };
        (self.first_visible(new_scroll), new_scroll)
    }

    pub fn detail_step_up(
        &self,
        selected: usize,
        scroll_top: DocY,
        viewport_h: u16,
    ) -> (usize, DocY) {
        if self.heights.is_empty() || viewport_h == 0 {
            return (0, 0);
        }
        let _ = selected.min(self.heights.len() - 1);
        if scroll_top == 0 {
            return (0, 0);
        }
        let first = self.first_visible(scroll_top);
        let floor_h = self.height(first);
        let new_scroll = if floor_h > u32::from(viewport_h) {
            self.detail_line_scroll(scroll_top, -i32::from(DETAIL_STEP_LINES), viewport_h)
        } else {
            let floor_top = self.offset(first);
            if scroll_top > floor_top {
                floor_top
            } else if first > 0 {
                self.offset(first - 1)
            } else {
                0
            }
        };
        (self.first_visible(new_scroll), new_scroll)
    }

    pub fn page_scroll_top(&self, scroll_top: DocY, delta: i32, viewport_h: u16) -> DocY {
        if self.heights.is_empty() || viewport_h == 0 {
            return 0;
        }
        let first = self.first_visible(scroll_top);
        let page_size = self.floors_per_page(first, viewport_h);
        let target_floor = if delta > 0 {
            (first + page_size).min(self.heights.len().saturating_sub(1))
        } else {
            first.saturating_sub(page_size)
        };
        self.offset(target_floor)
    }

    fn scroll_to_previous_floor(&self, first: usize, viewport_h: u16) -> DocY {
        if first == 0 {
            return 0;
        }
        let prev = first - 1;
        let prev_top = self.offset(prev);
        let prev_h = self.height(prev);
        if prev_h > u32::from(viewport_h) {
            floor_read_end_scroll(prev_top, prev_h, viewport_h)
        } else {
            prev_top
        }
    }
}

fn content_height(post: &Post, body_w: u16, palette: Palette, images: Option<&ImageCache>) -> DocY {
    if let Some(cache) = images {
        layout_post_blocks(post, body_w, palette, cache)
            .iter()
            .map(|b| DocY::from(b.height()))
            .fold(0u32, DocY::saturating_add)
            .max(1)
    } else {
        (render_post_content_lines(post, body_w, palette)
            .len()
            .max(1)) as DocY
    }
}

pub fn measure_floor(
    post: &Post,
    width: u16,
    palette: Palette,
    images: Option<&ImageCache>,
    include_separator: bool,
) -> DocY {
    if width < 8 {
        return 0;
    }
    let body_w = width.saturating_sub(BAR_W);
    let poll_h = post
        .poll
        .as_ref()
        .map(|p| poll_block_height(p, body_w))
        .unwrap_or(0);
    let content_h = content_height(post, body_w, palette, images);
    let sep = u32::from(u16::from(include_separator) * SEPARATOR_H);
    u32::from(HEADER_H) + u32::from(GAP_H) + u32::from(poll_h) + content_h + sep
}

pub fn floor_list_total_height(
    posts: &[Post],
    width: u16,
    palette: Palette,
    images: Option<&ImageCache>,
) -> DocY {
    FloorLayout::build(posts, width, palette, images, 0).total
}

pub fn floor_offsets(
    posts: &[Post],
    width: u16,
    palette: Palette,
    images: Option<&ImageCache>,
) -> Vec<DocY> {
    FloorLayout::build(posts, width, palette, images, 0).offsets
}

pub fn first_visible_floor(
    scroll_top: DocY,
    posts: &[Post],
    width: u16,
    palette: Palette,
    images: Option<&ImageCache>,
) -> usize {
    FloorLayout::build(posts, width, palette, images, 0).first_visible(scroll_top)
}

/// Anchor for keeping the viewport stable across layout reflows (image decode, append).
///
/// Stores the first visible floor and the scroll offset *within* that floor — not the floor
/// top alone, which would yank the user back when they had scrolled mid-floor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DetailScrollAnchor {
    pub floor: usize,
    pub offset_in_floor: DocY,
}

pub fn capture_detail_scroll_anchor(
    scroll_top: DocY,
    posts: &[Post],
    width: u16,
    palette: Palette,
    images: Option<&ImageCache>,
) -> DetailScrollAnchor {
    FloorLayout::build(posts, width, palette, images, 0).capture_scroll_anchor(scroll_top)
}

/// Restore [`DetailScrollAnchor`] after heights changed (e.g. images became Ready).
pub fn restore_detail_scroll_anchor(
    anchor: DetailScrollAnchor,
    posts: &[Post],
    width: u16,
    viewport_h: u16,
    palette: Palette,
    images: Option<&ImageCache>,
) -> DocY {
    FloorLayout::build(posts, width, palette, images, 0).restore_scroll_anchor(anchor, viewport_h)
}

/// Floor index containing document line `line` (0-based from the list top).
pub fn floor_index_at_line(
    line: DocY,
    posts: &[Post],
    width: u16,
    palette: Palette,
    images: Option<&ImageCache>,
) -> usize {
    FloorLayout::build(posts, width, palette, images, 0).floor_index_at_line(line)
}

/// Last floor with any row inside the viewport `[scroll_top, scroll_top + viewport_h)`.
pub fn last_visible_floor(
    scroll_top: DocY,
    posts: &[Post],
    width: u16,
    viewport_h: u16,
    palette: Palette,
    images: Option<&ImageCache>,
) -> usize {
    FloorLayout::build(posts, width, palette, images, 0).last_visible(scroll_top, viewport_h)
}

/// How many floors fit in one viewport page starting at `first_floor`.
pub fn floors_per_page(
    first_floor: usize,
    posts: &[Post],
    width: u16,
    viewport_h: u16,
    palette: Palette,
    images: Option<&ImageCache>,
) -> usize {
    FloorLayout::build(posts, width, palette, images, 0).floors_per_page(first_floor, viewport_h)
}

pub fn ensure_scroll_top(
    selected: usize,
    scroll_top: DocY,
    posts: &[Post],
    width: u16,
    viewport_h: u16,
    palette: Palette,
    images: Option<&ImageCache>,
) -> DocY {
    FloorLayout::build(posts, width, palette, images, 0)
        .ensure_scroll_top(selected, scroll_top, viewport_h)
}

pub fn clamp_scroll_top(
    scroll_top: DocY,
    posts: &[Post],
    width: u16,
    viewport_h: u16,
    palette: Palette,
    images: Option<&ImageCache>,
) -> DocY {
    FloorLayout::build(posts, width, palette, images, 0).clamp_scroll_top(scroll_top, viewport_h)
}

/// Maximum `scroll_top` while the viewport bottom aligns with the floor bottom.
fn floor_read_end_scroll(floor_top: DocY, floor_h: DocY, viewport_h: u16) -> DocY {
    let floor_bottom = floor_top.saturating_add(floor_h);
    floor_bottom
        .saturating_sub(u32::from(viewport_h))
        .max(floor_top)
}

/// Line-wise detail scroll (wheel or j/k inside a tall floor). Tall floors snap to the next /
/// previous floor instead of leaving the successor half off-screen.
pub fn detail_line_scroll(
    scroll_top: DocY,
    delta_lines: i32,
    posts: &[Post],
    width: u16,
    viewport_h: u16,
    palette: Palette,
    images: Option<&ImageCache>,
) -> DocY {
    FloorLayout::build(posts, width, palette, images, 0).detail_line_scroll(
        scroll_top,
        delta_lines,
        viewport_h,
    )
}

/// j/Down: short floors advance one floor; tall floors scroll [`DETAIL_STEP_LINES`] lines.
/// Selection follows the top visible floor.
pub fn detail_step_down(
    selected: usize,
    scroll_top: DocY,
    posts: &[Post],
    width: u16,
    viewport_h: u16,
    palette: Palette,
    images: Option<&ImageCache>,
) -> (usize, DocY) {
    FloorLayout::build(posts, width, palette, images, 0)
        .detail_step_down(selected, scroll_top, viewport_h)
}

/// k/Up: short floors snap to the current floor top, then the previous floor; tall floors
/// scroll [`DETAIL_STEP_LINES`] lines. Selection follows the top visible floor.
pub fn detail_step_up(
    selected: usize,
    scroll_top: DocY,
    posts: &[Post],
    width: u16,
    viewport_h: u16,
    palette: Palette,
    images: Option<&ImageCache>,
) -> (usize, DocY) {
    FloorLayout::build(posts, width, palette, images, 0)
        .detail_step_up(selected, scroll_top, viewport_h)
}

pub fn page_scroll_top(
    scroll_top: DocY,
    delta: i32,
    posts: &[Post],
    width: u16,
    viewport_h: u16,
    palette: Palette,
    images: Option<&ImageCache>,
) -> DocY {
    FloorLayout::build(posts, width, palette, images, 0)
        .page_scroll_top(scroll_top, delta, viewport_h)
}

pub fn draw_floor_list(frame: &mut Frame<'_>, area: Rect, mut props: FloorListProps<'_>) {
    if area.height == 0 || area.width < 8 {
        return;
    }
    if props.posts.is_empty() {
        return;
    }
    clear_content_viewport(frame, area);
    let viewport_bottom = props.scroll_top.saturating_add(u32::from(area.height));

    // Prefer precomputed heights: O(1) jump to first visible floor, no re-wrap for off-screen.
    // Caller owns revision matching via ensure_detail_layout; here only re-check width.
    let cached = props
        .layout
        .filter(|l| l.width == area.width && l.floor_count() == props.posts.len());
    let start = cached
        .map(|l| l.first_visible(props.scroll_top))
        .unwrap_or(0);
    let mut logical_top = cached.map(|l| l.offset(start)).unwrap_or_else(|| {
        // Fallback: measure floors above the viewport once (should be rare without cache).
        let mut top = 0u32;
        for (idx, post) in props.posts.iter().enumerate().take(start) {
            top = top.saturating_add(measure_floor(
                post,
                area.width,
                props.palette,
                props.images.as_deref(),
                idx + 1 < props.posts.len(),
            ));
        }
        top
    });

    for idx in start..props.posts.len() {
        let post = &props.posts[idx];
        let floor_h = if let Some(layout) = cached {
            layout.height(idx)
        } else {
            measure_floor(
                post,
                area.width,
                props.palette,
                props.images.as_deref(),
                idx + 1 < props.posts.len(),
            )
        };
        let floor_bottom = logical_top.saturating_add(floor_h);

        if floor_bottom <= props.scroll_top {
            logical_top = floor_bottom;
            continue;
        }
        if logical_top >= viewport_bottom {
            break;
        }

        let skip_lines = props.scroll_top.saturating_sub(logical_top);
        let visible_top = logical_top.max(props.scroll_top);
        let visible_bottom = floor_bottom.min(viewport_bottom);
        if visible_bottom <= visible_top {
            logical_top = floor_bottom;
            continue;
        }

        // Relative offsets are within the viewport → always fit in u16.
        let rel_y = visible_top.saturating_sub(props.scroll_top);
        let rel_h = visible_bottom.saturating_sub(visible_top);
        let draw_y = area.y.saturating_add(rel_y as u16);
        let draw_h = rel_h as u16;
        draw_floor(
            frame,
            area,
            Rect {
                x: area.x,
                y: draw_y,
                width: area.width,
                height: draw_h,
            },
            post,
            idx == props.selected && !props.mask_cjk,
            props.palette,
            props.show_avatar,
            &mut props.images,
            props.scroll_top,
            logical_top,
            skip_lines,
            idx + 1 < props.posts.len(),
            props.mask_cjk,
        );

        logical_top = floor_bottom;
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_floor(
    frame: &mut Frame<'_>,
    viewport: Rect,
    area: Rect,
    post: &Post,
    selected: bool,
    palette: Palette,
    show_avatar: bool,
    images: &mut Option<&mut ImageCache>,
    scroll_top: DocY,
    floor_top: DocY,
    skip_lines: DocY,
    draw_separator: bool,
    mask_cjk: bool,
) {
    let body = Rect {
        x: area.x + BAR_W,
        y: area.y,
        width: area.width.saturating_sub(BAR_W),
        height: area.height,
    };
    let text_x = body.x + if show_avatar { AVATAR_W } else { 0 };
    let text_w = body
        .width
        .saturating_sub(if show_avatar { AVATAR_W } else { 0 });

    let author = maybe_mask_cjk(&post.author, mask_cjk);
    let (mut row1, row2) = floor_header_rows_with_edit(
        author.as_ref(),
        post.floor,
        &post.time,
        post.edited_by.as_deref(),
        post.edited_at.as_deref(),
        text_w as usize,
        palette,
    );
    row1 = mask_line_cjk(row1, mask_cjk);
    let row2 = mask_line_cjk(row2, mask_cjk);
    if post.warned {
        row1.spans.insert(
            0,
            ratatui::text::Span::styled("\u{f071} ", palette.warn_style()),
        );
    }

    let poll_h = post
        .poll
        .as_ref()
        .map(|p| poll_block_height(p, body.width))
        .unwrap_or(0);
    let content_blocks: Vec<ContentBlock> = if let Some(cache) = images.as_deref() {
        layout_post_blocks(post, body.width, palette, cache)
    } else {
        render_post_content_lines(post, body.width, palette)
            .into_iter()
            .map(ContentBlock::Text)
            .collect()
    };

    let mut y = area.y;
    let mut line_idx: DocY = 0;
    let mut header_top = None;

    let next_visible_row =
        |frame: &mut Frame<'_>, y: &mut u16, line_idx: &mut DocY, draw_bar: bool| -> bool {
            if *line_idx < skip_lines {
                *line_idx += 1;
                return true;
            }
            if *y >= area.y + area.height {
                return false;
            }
            if draw_bar {
                draw_nav_bar(frame, area.x, *y, selected, palette);
            }
            *y += 1;
            *line_idx += 1;
            true
        };

    // Header row 1
    if !next_visible_row(frame, &mut y, &mut line_idx, true) {
        return;
    }
    if y > area.y {
        header_top = Some(y - 1);
        frame.render_widget(
            Paragraph::new(row1),
            Rect {
                x: text_x,
                y: y - 1,
                width: text_w,
                height: 1,
            },
        );
    }

    // Header row 2
    if !next_visible_row(frame, &mut y, &mut line_idx, true) {
        return;
    }
    if y > area.y {
        header_top.get_or_insert(y - 1);
        draw_meta_row2(
            frame,
            Rect {
                x: text_x,
                y: y - 1,
                width: text_w,
                height: 1,
            },
            &row2,
            post.signature.as_deref(),
            palette,
            mask_cjk,
        );
    }
    if show_avatar {
        if let Some(cache) = images.as_deref() {
            if header_top.is_some() || floor_top + u32::from(HEADER_H) > scroll_top {
                let (avatar, placeholder) =
                    cache.avatar_entries_for_draw(post.avatar_url.as_deref());
                let entry = avatar
                    .filter(|e| matches!(e.state, ImageState::Ready { .. }))
                    .or(placeholder.filter(|e| matches!(e.state, ImageState::Ready { .. })));
                draw_graphic_in_viewport(
                    frame,
                    viewport,
                    entry,
                    cache.picker(),
                    ImageKind::Avatar,
                    palette,
                    "",
                    body.x,
                    floor_top as i32,
                    scroll_top,
                );
            }
        }
    }

    // Gap row (bar continues through the floor)
    if !next_visible_row(frame, &mut y, &mut line_idx, true) {
        return;
    }

    // Poll block
    if poll_h > 0 {
        if let Some(poll) = &post.poll {
            let poll_start = line_idx;
            let poll_h_doc = u32::from(poll_h);
            while line_idx < poll_start.saturating_add(poll_h_doc) && line_idx < skip_lines {
                line_idx += 1;
            }
            if line_idx < poll_start.saturating_add(poll_h_doc) && y < area.y + area.height {
                let poll_skip = line_idx.saturating_sub(poll_start) as u16;
                let remaining = u32::from(area.y + area.height - y);
                let poll_visible = poll_start
                    .saturating_add(poll_h_doc)
                    .saturating_sub(line_idx)
                    .min(remaining) as u16;
                if poll_visible > 0 && line_idx >= skip_lines {
                    for row in 0..poll_visible {
                        draw_nav_bar(frame, area.x, y + row, selected, palette);
                    }
                    draw_poll_block(
                        frame,
                        Rect {
                            x: body.x,
                            y,
                            width: body.width,
                            height: poll_visible,
                        },
                        poll,
                        palette,
                        poll_skip,
                    );
                    y = y.saturating_add(poll_visible);
                }
            }
            line_idx = poll_start.saturating_add(poll_h_doc);
        }
    }

    // Content
    'content: for block in &content_blocks {
        match block {
            ContentBlock::Text(line) => {
                if !next_visible_row(frame, &mut y, &mut line_idx, true) {
                    break 'content;
                }
                let mut prefixed = mask_line_cjk(line.clone(), mask_cjk);
                prefixed.spans.insert(0, ratatui::text::Span::raw(" "));
                frame.render_widget(
                    Paragraph::new(prefixed),
                    Rect {
                        x: body.x,
                        y: y - 1,
                        width: body.width,
                        height: 1,
                    },
                );
            }
            ContentBlock::Image {
                url,
                width,
                height,
                failed,
            } => {
                if render_image_block(
                    frame,
                    viewport,
                    area,
                    body,
                    images,
                    palette,
                    selected,
                    &mut y,
                    &mut line_idx,
                    scroll_top,
                    floor_top,
                    skip_lines,
                    ImageKind::Content {
                        max_cols: body.width.saturating_sub(2),
                    },
                    url,
                    *width,
                    *height,
                    *failed,
                    body.x.saturating_add(1),
                ) {
                    break 'content;
                }
            }
            ContentBlock::Smiley {
                key,
                width,
                height,
                failed,
            } => {
                if render_image_block(
                    frame,
                    viewport,
                    area,
                    body,
                    images,
                    palette,
                    selected,
                    &mut y,
                    &mut line_idx,
                    scroll_top,
                    floor_top,
                    skip_lines,
                    ImageKind::Smiley,
                    key,
                    *width,
                    *height,
                    *failed,
                    body.x.saturating_add(1),
                ) {
                    break 'content;
                }
            }
            ContentBlock::Inline { parts } => {
                if render_inline_row(
                    frame,
                    viewport,
                    area,
                    body,
                    images,
                    palette,
                    selected,
                    mask_cjk,
                    &mut y,
                    &mut line_idx,
                    scroll_top,
                    floor_top,
                    skip_lines,
                    parts,
                ) {
                    break 'content;
                }
            }
        }
    }

    // Separator between floors (skip on last floor — status bar rule follows).
    if draw_separator && next_visible_row(frame, &mut y, &mut line_idx, false) {
        let rule = "─".repeat(body.width as usize);
        frame.render_widget(
            Paragraph::new(rule).style(palette.muted_style()),
            Rect {
                x: body.x,
                y: y - 1,
                width: body.width,
                height: 1,
            },
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn render_image_block(
    frame: &mut Frame<'_>,
    viewport: Rect,
    area: Rect,
    _body: Rect,
    images: &mut Option<&mut ImageCache>,
    palette: Palette,
    selected: bool,
    y: &mut u16,
    line_idx: &mut DocY,
    scroll_top: DocY,
    floor_top: DocY,
    skip_lines: DocY,
    kind: ImageKind,
    cache_key: &str,
    _width: u16,
    height: u16,
    failed: bool,
    doc_x: u16,
) -> bool {
    let block_start = *line_idx;
    let doc_y = (floor_top.saturating_add(block_start)) as i32;
    let pos_y = doc_y - scroll_top as i32;
    let bottom_margin = images
        .as_deref()
        .map(|cache| graphics_bottom_margin(cache.picker(), kind))
        .unwrap_or(0);

    let height_doc = u32::from(height);
    while *line_idx < block_start.saturating_add(height_doc) && *line_idx < skip_lines {
        *line_idx += 1;
    }
    if *line_idx >= block_start.saturating_add(height_doc) {
        return false;
    }
    let image_viewport_h = viewport.height.saturating_sub(bottom_margin).max(1);
    if pos_y >= image_viewport_h as i32 {
        *line_idx = block_start.saturating_add(height_doc);
        return true;
    }
    if pos_y + i32::from(height) <= 0 {
        *line_idx = block_start.saturating_add(height_doc);
        return false;
    }
    if *y >= area.y + area.height {
        return true;
    }

    let remaining_h = area.y.saturating_add(area.height).saturating_sub(*y);
    if remaining_h > 0 && *line_idx >= skip_lines {
        let slice_skip = (*line_idx).saturating_sub(block_start) as u16;
        let rows_in_view = height.saturating_sub(slice_skip).min(remaining_h);
        for row in 0..rows_in_view {
            draw_nav_bar(frame, area.x, *y + row, selected, palette);
        }
        let fail_label = if failed { IMAGE_FAIL_LABEL } else { "…" };
        if let Some(cache) = images.as_deref() {
            let entry = cache.get(cache_key);
            draw_graphic_in_viewport(
                frame,
                viewport,
                entry,
                cache.picker(),
                kind,
                palette,
                fail_label,
                doc_x,
                doc_y,
                scroll_top,
            );
        }
        *y = y.saturating_add(rows_in_view);
    }
    *line_idx = block_start.saturating_add(height_doc);
    *y >= area.y.saturating_add(area.height)
}

/// Draw one mixed text+smiley row. Returns true if the content viewport is full.
#[allow(clippy::too_many_arguments)]
fn render_inline_row(
    frame: &mut Frame<'_>,
    viewport: Rect,
    area: Rect,
    body: Rect,
    images: &mut Option<&mut ImageCache>,
    palette: Palette,
    selected: bool,
    mask_cjk: bool,
    y: &mut u16,
    line_idx: &mut DocY,
    scroll_top: DocY,
    floor_top: DocY,
    skip_lines: DocY,
    parts: &[InlinePart],
) -> bool {
    let height = parts
        .iter()
        .map(|p| match p {
            InlinePart::Text(_) => 1u16,
            InlinePart::Smiley { height, .. } => *height,
        })
        .max()
        .unwrap_or(1);
    let height_doc = u32::from(height);
    let block_start = *line_idx;
    let doc_y = (floor_top.saturating_add(block_start)) as i32;

    while *line_idx < block_start.saturating_add(height_doc) && *line_idx < skip_lines {
        *line_idx += 1;
    }
    if *line_idx >= block_start.saturating_add(height_doc) {
        return false;
    }
    if *y >= area.y + area.height {
        return true;
    }

    let remaining_h = area.y.saturating_add(area.height).saturating_sub(*y);
    if remaining_h > 0 && *line_idx >= skip_lines {
        let slice_skip = (*line_idx).saturating_sub(block_start) as u16;
        let rows_in_view = height.saturating_sub(slice_skip).min(remaining_h);
        for row in 0..rows_in_view {
            draw_nav_bar(frame, area.x, *y + row, selected, palette);
        }

        // Only the first visible slice of multi-row smileys draws content; height is 1 today.
        if slice_skip == 0 {
            let row_y = *y;
            let mut x = body.x.saturating_add(1);
            for part in parts {
                match part {
                    InlinePart::Text(line) => {
                        let painted = mask_line_cjk(line.clone(), mask_cjk);
                        let w = painted
                            .spans
                            .iter()
                            .map(|s| str_width(s.content.as_ref()) as u16)
                            .sum::<u16>()
                            .max(1)
                            .min(body.x.saturating_add(body.width).saturating_sub(x));
                        if w == 0 {
                            continue;
                        }
                        frame.render_widget(
                            Paragraph::new(painted),
                            Rect {
                                x,
                                y: row_y,
                                width: w,
                                height: 1,
                            },
                        );
                        x = x.saturating_add(w);
                    }
                    InlinePart::Smiley {
                        key, width, failed, ..
                    } => {
                        let fail_label = if *failed { IMAGE_FAIL_LABEL } else { "…" };
                        if let Some(cache) = images.as_deref() {
                            let entry = cache.get(key);
                            draw_graphic_in_viewport(
                                frame,
                                viewport,
                                entry,
                                cache.picker(),
                                ImageKind::Smiley,
                                palette,
                                fail_label,
                                x,
                                doc_y,
                                scroll_top,
                            );
                        }
                        x = x.saturating_add(*width);
                    }
                }
            }
        }
        *y = y.saturating_add(rows_in_view);
    }
    *line_idx = block_start.saturating_add(height_doc);
    *y >= area.y.saturating_add(area.height)
}

fn draw_meta_row2(
    frame: &mut Frame<'_>,
    area: Rect,
    time_line: &ratatui::text::Line<'_>,
    signature: Option<&str>,
    palette: Palette,
    mask_cjk: bool,
) {
    let time_text: String = time_line.spans.iter().map(|s| s.content.as_ref()).collect();
    let time_w = str_width(&time_text);

    let sig = signature
        .filter(|s| !s.is_empty())
        .map(|s| {
            let budget = area.width.saturating_sub(time_w as u16 + 2) as usize;
            let sig = format_signature(s, budget.max(8));
            maybe_mask_cjk(&sig, mask_cjk).into_owned()
        })
        .unwrap_or_default();
    let sig_w = str_width(&sig);

    if sig.is_empty() {
        frame.render_widget(Paragraph::new(time_line.clone()), area);
        return;
    }

    if time_w + sig_w + 1 >= area.width as usize {
        frame.render_widget(Paragraph::new(time_line.clone()), area);
        return;
    }

    let time_area = Rect {
        x: area.x,
        y: area.y,
        width: time_w as u16,
        height: 1,
    };
    let sig_area = Rect {
        x: area.x + area.width.saturating_sub(sig_w as u16),
        y: area.y,
        width: sig_w as u16,
        height: 1,
    };
    frame.render_widget(Paragraph::new(time_line.clone()), time_area);
    frame.render_widget(
        Paragraph::new(sig)
            .style(palette.muted_style())
            .alignment(Alignment::Right),
        sig_area,
    );
}

fn draw_nav_bar(frame: &mut Frame<'_>, x: u16, y: u16, selected: bool, palette: Palette) {
    if !selected {
        return;
    }
    frame.render_widget(
        Paragraph::new("│").style(palette.accent_style()),
        Rect {
            x,
            y,
            width: BAR_W,
            height: 1,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use hiptty_core::{ContentNode, ContentSpan};

    fn sample_post() -> Post {
        Post {
            pid: "1".into(),
            floor: 1,
            author: "user".into(),
            uid: Some("1".into()),
            avatar_url: None,
            time: "2026-7-1 10:00".into(),
            content: vec![ContentNode::Text {
                spans: vec![ContentSpan::Text {
                    text: "hello".into(),
                    style: Default::default(),
                    url: None,
                }],
            }],
            poll: None,
            page: 1,
            warned: false,
            signature: Some("sign".into()),
            edited_by: None,
            edited_at: None,
        }
    }

    #[test]
    fn floor_index_at_line_maps_document_rows() {
        let posts = vec![sample_post(); 4];
        let palette = Palette::default();
        let offsets = floor_offsets(&posts, 80, palette, None);
        let floor_h = measure_floor(&posts[0], 80, palette, None, true);

        assert_eq!(floor_index_at_line(0, &posts, 80, palette, None), 0);
        assert_eq!(
            floor_index_at_line(offsets[1], &posts, 80, palette, None),
            1
        );
        assert_eq!(
            floor_index_at_line(offsets[1] + floor_h - 1, &posts, 80, palette, None),
            1
        );
    }

    #[test]
    fn detail_scroll_anchor_keeps_mid_floor_offset() {
        let posts = vec![sample_post(); 6];
        let palette = Palette::default();
        let h = measure_floor(&posts[0], 80, palette, None, true);
        let viewport = u16::try_from(h).expect("short floor"); // one floor tall → mid-list scroll is valid
        let scroll = h + 3; // mid floor 1
        let anchor = capture_detail_scroll_anchor(scroll, &posts, 80, palette, None);
        assert_eq!(anchor.floor, 1);
        assert_eq!(anchor.offset_in_floor, 3);
        let restored = restore_detail_scroll_anchor(anchor, &posts, 80, viewport, palette, None);
        assert_eq!(restored, scroll);
    }

    #[test]
    fn floor_includes_gap_row() {
        let post = sample_post();
        let palette = Palette::default();
        let h = measure_floor(&post, 80, palette, None, false);
        let without_gap = u32::from(HEADER_H + 1);
        assert!(h > without_gap);
    }

    #[test]
    fn scroll_down_minimal() {
        let posts = vec![sample_post(); 6];
        let palette = Palette::default();
        let h = measure_floor(&posts[0], 80, palette, None, true);
        let viewport = u16::try_from(h).expect("short floor") * 4;
        let scroll = ensure_scroll_top(4, 0, &posts, 80, viewport, palette, None);
        assert_eq!(scroll, h);
    }

    #[test]
    fn tall_floor_does_not_oscillate() {
        let mut post = sample_post();
        post.content = vec![ContentNode::Text {
            spans: vec![ContentSpan::Text {
                text: "x".repeat(500),
                style: Default::default(),
                url: None,
            }],
        }];
        let posts = vec![post];
        let palette = Palette::default();
        let viewport = 10u16;
        let mid = ensure_scroll_top(0, 5, &posts, 80, viewport, palette, None);
        let again = ensure_scroll_top(0, mid, &posts, 80, viewport, palette, None);
        assert_eq!(mid, again);
    }

    #[test]
    fn page_down_advances_by_visible_floors() {
        let posts = vec![sample_post(); 8];
        let palette = Palette::default();
        let h = measure_floor(&posts[0], 80, palette, None, true);
        let viewport = u16::try_from(h).expect("short floor") * 4;
        let next = page_scroll_top(0, 1, &posts, 80, viewport, palette, None);
        assert_eq!(next, h * 4);
    }

    fn tall_post() -> Post {
        let mut post = sample_post();
        post.content = vec![ContentNode::Text {
            spans: vec![ContentSpan::Text {
                text: "x".repeat(2000),
                style: Default::default(),
                url: None,
            }],
        }];
        post
    }

    #[test]
    fn detail_step_down_advances_by_floor_when_short() {
        let posts = vec![sample_post(); 8];
        let palette = Palette::default();
        let floor_h = measure_floor(&posts[0], 80, palette, None, true);
        let viewport = u16::try_from(floor_h)
            .expect("short floor")
            .saturating_mul(3);
        assert!(floor_h <= u32::from(viewport));

        let (sel, scroll) = detail_step_down(0, 0, &posts, 80, viewport, palette, None);
        assert_eq!(scroll, floor_h);
        assert_eq!(sel, 1);

        let max_scroll =
            floor_list_total_height(&posts, 80, palette, None).saturating_sub(u32::from(viewport));
        let (sel, scroll) = detail_step_down(2, max_scroll, &posts, 80, viewport, palette, None);
        assert_eq!(sel, 2);
        assert_eq!(scroll, max_scroll);
    }

    #[test]
    fn detail_step_down_scrolls_three_lines_when_tall() {
        let posts = vec![tall_post()];
        let palette = Palette::default();
        let floor_h = measure_floor(&posts[0], 80, palette, None, false);
        let viewport = 12u16;
        assert!(
            floor_h > u32::from(viewport),
            "floor_h={floor_h} viewport={viewport}"
        );

        let (sel, scroll) = detail_step_down(0, 0, &posts, 80, viewport, palette, None);
        assert_eq!(scroll, u32::from(DETAIL_STEP_LINES));
        assert_eq!(sel, 0);
    }

    #[test]
    fn detail_step_up_advances_by_floor_when_short() {
        let posts = vec![sample_post(); 4];
        let palette = Palette::default();
        let floor_h = measure_floor(&posts[0], 80, palette, None, true);
        let viewport = u16::try_from(floor_h)
            .expect("short floor")
            .saturating_mul(3);
        let offsets = floor_offsets(&posts, 80, palette, None);

        let (sel, scroll) = detail_step_up(2, offsets[2] + 1, &posts, 80, viewport, palette, None);
        assert_eq!(scroll, offsets[2]);
        assert_eq!(sel, 2);

        let (sel, scroll) = detail_step_up(2, offsets[2], &posts, 80, viewport, palette, None);
        assert_eq!(scroll, offsets[1]);
        assert_eq!(sel, 1);

        let (sel, scroll) = detail_step_up(0, 0, &posts, 80, viewport, palette, None);
        assert_eq!(sel, 0);
        assert_eq!(scroll, 0);
    }

    #[test]
    fn detail_step_up_scrolls_three_lines_when_tall() {
        let posts = vec![tall_post()];
        let palette = Palette::default();
        let floor_h = measure_floor(&posts[0], 80, palette, None, false);
        let viewport = 12u16;
        assert!(
            floor_h > u32::from(viewport),
            "floor_h={floor_h} viewport={viewport}"
        );

        let (sel, scroll) = detail_step_up(0, 15, &posts, 80, viewport, palette, None);
        assert_eq!(scroll, 15 - u32::from(DETAIL_STEP_LINES));
        assert_eq!(sel, 0);
    }

    #[test]
    fn detail_line_scroll_snaps_to_next_floor_top() {
        let posts = vec![tall_post(), sample_post()];
        let palette = Palette::default();
        let viewport = 12u16;
        let offsets = floor_offsets(&posts, 80, palette, None);
        let floor_h = measure_floor(&posts[0], 80, palette, None, true);
        assert!(floor_h > u32::from(viewport));
        let floor_end = floor_read_end_scroll(offsets[0], floor_h, viewport);

        let scroll = detail_line_scroll(
            floor_end,
            i32::from(DETAIL_STEP_LINES),
            &posts,
            80,
            viewport,
            palette,
            None,
        );
        assert_eq!(scroll, offsets[1]);

        let near_end = floor_end.saturating_sub(u32::from(DETAIL_STEP_LINES.saturating_sub(1)));
        let scroll = detail_line_scroll(
            near_end,
            i32::from(DETAIL_STEP_LINES),
            &posts,
            80,
            viewport,
            palette,
            None,
        );
        assert_eq!(scroll, offsets[1]);
    }

    #[test]
    fn detail_line_scroll_returns_to_previous_floor_bottom() {
        let posts = vec![tall_post(), sample_post()];
        let palette = Palette::default();
        let viewport = 12u16;
        let offsets = floor_offsets(&posts, 80, palette, None);
        let floor_h = measure_floor(&posts[0], 80, palette, None, true);
        let floor_end = floor_read_end_scroll(offsets[0], floor_h, viewport);

        let scroll = detail_line_scroll(
            offsets[1],
            -i32::from(DETAIL_STEP_LINES),
            &posts,
            80,
            viewport,
            palette,
            None,
        );
        assert_eq!(scroll, floor_end);
    }

    #[test]
    fn last_visible_floor_tracks_viewport_bottom() {
        let posts = vec![sample_post(); 6];
        let palette = Palette::default();
        let floor_h = measure_floor(&posts[0], 80, palette, None, true);
        let viewport = u16::try_from(floor_h)
            .expect("short floor")
            .saturating_mul(2);
        let scroll = floor_h.saturating_mul(3);
        let last = last_visible_floor(scroll, &posts, 80, viewport, palette, None);
        assert!(
            last >= 3,
            "last visible should be near viewport bottom, got {last}"
        );
    }

    #[test]
    fn floor_layout_matches_legacy_helpers() {
        let posts = vec![sample_post(); 40];
        let palette = Palette::default();
        let layout = FloorLayout::build(&posts, 80, palette, None, 0);
        assert_eq!(
            layout.total,
            floor_list_total_height(&posts, 80, palette, None)
        );
        assert_eq!(layout.offsets, floor_offsets(&posts, 80, palette, None));
        let scroll = layout.total / 3;
        assert_eq!(
            layout.first_visible(scroll),
            first_visible_floor(scroll, &posts, 80, palette, None)
        );
    }

    /// Binary search helpers match linear scan semantics on a tall layout.
    #[test]
    fn floor_layout_binary_search_matches_linear_semantics() {
        let posts = vec![sample_post(); 200];
        let palette = Palette::default();
        let layout = FloorLayout::build(&posts, 80, palette, None, 1);
        assert!(layout.matches(80, 1));
        assert!(!layout.matches(80, 2));
        assert!(!layout.matches(100, 1));

        for scroll in [
            0u32,
            1,
            17,
            100,
            999,
            layout.total.saturating_sub(1),
            layout.total + 10,
        ] {
            // Recompute expected with linear scan over offsets.
            let mut expected_first = layout.heights.len().saturating_sub(1);
            for (idx, &top) in layout.offsets.iter().enumerate() {
                let bottom = top.saturating_add(layout.heights[idx]);
                if bottom > scroll {
                    expected_first = idx;
                    break;
                }
            }
            assert_eq!(
                layout.first_visible(scroll),
                expected_first,
                "scroll={scroll}"
            );

            let mut expected_at = 0usize;
            for (idx, &top) in layout.offsets.iter().enumerate().rev() {
                if scroll >= top {
                    expected_at = idx;
                    break;
                }
            }
            assert_eq!(
                layout.floor_index_at_line(scroll),
                expected_at,
                "line={scroll}"
            );
        }
    }

    #[test]
    fn floor_layout_handles_document_beyond_u16() {
        // Synthetic tall single floor: heights sum past u16::MAX must not wrap.
        let layout = FloorLayout {
            width: 80,
            revision: 0,
            heights: vec![40_000, 40_000],
            offsets: vec![0, 40_000],
            total: 80_000,
        };
        assert_eq!(layout.total, 80_000);
        assert_eq!(layout.first_visible(70_000), 1);
        assert_eq!(layout.clamp_scroll_top(100_000, 40), 79_960);
        let anchor = layout.capture_scroll_anchor(70_000);
        assert_eq!(anchor.floor, 1);
        assert_eq!(anchor.offset_in_floor, 30_000);
        let restored = layout.restore_scroll_anchor(anchor, 40);
        assert_eq!(restored, 70_000);
    }
}
