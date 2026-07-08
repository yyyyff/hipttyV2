use hiptty_core::Post;
use hiptty_image::{
    draw_graphic_in_viewport, graphics_bottom_margin, layout_post_blocks, ContentBlock, ImageCache,
    ImageKind, ImageState, AVATAR_COLS, IMAGE_FAIL_LABEL,
};
use hiptty_render::{
    clear_content_viewport, floor_header_rows, format_signature, mask_line_cjk, maybe_mask_cjk,
    render_post_content_lines, str_width, Palette,
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

pub struct FloorListProps<'a> {
    pub palette: Palette,
    pub posts: &'a [Post],
    pub selected: usize,
    pub scroll_top: u16,
    pub show_avatar: bool,
    pub images: Option<&'a mut ImageCache>,
    pub mask_cjk: bool,
}

fn content_height(post: &Post, body_w: u16, palette: Palette, images: Option<&ImageCache>) -> u16 {
    if let Some(cache) = images {
        layout_post_blocks(post, body_w, palette, cache)
            .iter()
            .map(ContentBlock::height)
            .sum::<u16>()
            .max(1)
    } else {
        render_post_content_lines(post, body_w, palette)
            .len()
            .max(1) as u16
    }
}

pub fn measure_floor(
    post: &Post,
    width: u16,
    palette: Palette,
    images: Option<&ImageCache>,
    include_separator: bool,
) -> u16 {
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
    let sep = u16::from(include_separator) * SEPARATOR_H;
    HEADER_H + GAP_H + poll_h + content_h + sep
}

pub fn floor_list_total_height(
    posts: &[Post],
    width: u16,
    palette: Palette,
    images: Option<&ImageCache>,
) -> u16 {
    posts
        .iter()
        .enumerate()
        .map(|(idx, p)| measure_floor(p, width, palette, images, idx + 1 < posts.len()))
        .sum()
}

pub fn floor_offsets(
    posts: &[Post],
    width: u16,
    palette: Palette,
    images: Option<&ImageCache>,
) -> Vec<u16> {
    let mut offsets = Vec::with_capacity(posts.len());
    let mut top = 0u16;
    for (idx, post) in posts.iter().enumerate() {
        offsets.push(top);
        top = top.saturating_add(measure_floor(
            post,
            width,
            palette,
            images,
            idx + 1 < posts.len(),
        ));
    }
    offsets
}

pub fn first_visible_floor(
    scroll_top: u16,
    posts: &[Post],
    width: u16,
    palette: Palette,
    images: Option<&ImageCache>,
) -> usize {
    let offsets = floor_offsets(posts, width, palette, images);
    for (idx, &top) in offsets.iter().enumerate() {
        let bottom = top.saturating_add(measure_floor(
            &posts[idx],
            width,
            palette,
            images,
            idx + 1 < posts.len(),
        ));
        if bottom > scroll_top {
            return idx;
        }
    }
    posts.len().saturating_sub(1)
}

/// Floor index containing document line `line` (0-based from the list top).
pub fn floor_index_at_line(
    line: u16,
    posts: &[Post],
    width: u16,
    palette: Palette,
    images: Option<&ImageCache>,
) -> usize {
    if posts.is_empty() {
        return 0;
    }
    let offsets = floor_offsets(posts, width, palette, images);
    for (idx, &top) in offsets.iter().enumerate().rev() {
        if line >= top {
            return idx;
        }
    }
    0
}

/// Last floor with any row inside the viewport `[scroll_top, scroll_top + viewport_h)`.
pub fn last_visible_floor(
    scroll_top: u16,
    posts: &[Post],
    width: u16,
    viewport_h: u16,
    palette: Palette,
    images: Option<&ImageCache>,
) -> usize {
    if posts.is_empty() || viewport_h == 0 {
        return 0;
    }
    let viewport_bottom = scroll_top.saturating_add(viewport_h);
    let mut last = first_visible_floor(scroll_top, posts, width, palette, images);
    let offsets = floor_offsets(posts, width, palette, images);
    for (idx, &top) in offsets.iter().enumerate() {
        if top >= viewport_bottom {
            break;
        }
        let bottom = top.saturating_add(measure_floor(
            &posts[idx],
            width,
            palette,
            images,
            idx + 1 < posts.len(),
        ));
        if bottom > scroll_top {
            last = idx;
        }
    }
    last
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
    if posts.is_empty() || viewport_h == 0 {
        return 0;
    }
    let mut used = 0u16;
    let mut count = 0usize;
    for (idx, post) in posts.iter().enumerate().skip(first_floor) {
        let h = measure_floor(post, width, palette, images, idx + 1 < posts.len());
        if count > 0 && used.saturating_add(h) > viewport_h {
            break;
        }
        used = used.saturating_add(h);
        count += 1;
    }
    count.max(1)
}

pub fn ensure_scroll_top(
    selected: usize,
    scroll_top: u16,
    posts: &[Post],
    width: u16,
    viewport_h: u16,
    palette: Palette,
    images: Option<&ImageCache>,
) -> u16 {
    if posts.is_empty() || viewport_h == 0 {
        return 0;
    }
    let selected = selected.min(posts.len() - 1);
    let offsets = floor_offsets(posts, width, palette, images);
    let sel_top = offsets[selected];
    let floor_h = measure_floor(
        &posts[selected],
        width,
        palette,
        images,
        selected + 1 < posts.len(),
    );
    let sel_bottom = sel_top.saturating_add(floor_h);

    if sel_top < scroll_top {
        return sel_top;
    }
    if sel_bottom <= scroll_top.saturating_add(viewport_h) {
        return scroll_top;
    }

    // Selected floor taller than viewport: scroll within floor, don't snap to top.
    if floor_h > viewport_h {
        let max_top = sel_bottom.saturating_sub(viewport_h);
        return scroll_top.max(sel_top).min(max_top);
    }

    sel_bottom.saturating_sub(viewport_h)
}

pub fn clamp_scroll_top(
    scroll_top: u16,
    posts: &[Post],
    width: u16,
    viewport_h: u16,
    palette: Palette,
    images: Option<&ImageCache>,
) -> u16 {
    if posts.is_empty() || viewport_h == 0 {
        return 0;
    }
    let total = floor_list_total_height(posts, width, palette, images);
    scroll_top.min(total.saturating_sub(viewport_h))
}

fn top_visible_floor_height(
    scroll_top: u16,
    posts: &[Post],
    width: u16,
    palette: Palette,
    images: Option<&ImageCache>,
) -> (usize, u16) {
    let first = first_visible_floor(scroll_top, posts, width, palette, images);
    let floor_h = measure_floor(
        &posts[first],
        width,
        palette,
        images,
        first + 1 < posts.len(),
    );
    (first, floor_h)
}

/// Maximum `scroll_top` while the viewport bottom aligns with the floor bottom.
fn floor_read_end_scroll(floor_top: u16, floor_h: u16, viewport_h: u16) -> u16 {
    let floor_bottom = floor_top.saturating_add(floor_h);
    floor_bottom.saturating_sub(viewport_h).max(floor_top)
}

fn scroll_to_previous_floor(
    first: usize,
    posts: &[Post],
    width: u16,
    viewport_h: u16,
    palette: Palette,
    images: Option<&ImageCache>,
) -> u16 {
    if first == 0 {
        return 0;
    }
    let offsets = floor_offsets(posts, width, palette, images);
    let prev = first - 1;
    let prev_top = offsets[prev];
    let prev_h = measure_floor(
        &posts[prev],
        width,
        palette,
        images,
        prev + 1 < posts.len(),
    );
    if prev_h > viewport_h {
        floor_read_end_scroll(prev_top, prev_h, viewport_h)
    } else {
        prev_top
    }
}

/// Line-wise detail scroll (wheel or j/k inside a tall floor). Tall floors snap to the next /
/// previous floor instead of leaving the successor half off-screen.
pub fn detail_line_scroll(
    scroll_top: u16,
    delta_lines: i32,
    posts: &[Post],
    width: u16,
    viewport_h: u16,
    palette: Palette,
    images: Option<&ImageCache>,
) -> u16 {
    if posts.is_empty() || viewport_h == 0 || delta_lines == 0 {
        return scroll_top;
    }
    let total = floor_list_total_height(posts, width, palette, images);
    let max_scroll = total.saturating_sub(viewport_h);

    if delta_lines > 0 {
        let step = delta_lines as u16;
        if scroll_top >= max_scroll {
            return max_scroll;
        }
        let (first, floor_h) = top_visible_floor_height(scroll_top, posts, width, palette, images);
        if floor_h <= viewport_h {
            return scroll_top.saturating_add(step).min(max_scroll);
        }

        let offsets = floor_offsets(posts, width, palette, images);
        let floor_top = offsets[first];
        let floor_end = floor_read_end_scroll(floor_top, floor_h, viewport_h);

        if scroll_top >= floor_end {
            if first + 1 < posts.len() {
                return offsets[first + 1];
            }
            return max_scroll;
        }

        let candidate = scroll_top.saturating_add(step);
        if candidate > floor_end {
            if first + 1 < posts.len() {
                return offsets[first + 1];
            }
            return floor_end.min(max_scroll);
        }
        candidate.min(max_scroll)
    } else {
        let step = (-delta_lines) as u16;
        if scroll_top == 0 {
            return 0;
        }
        let (first, floor_h) = top_visible_floor_height(scroll_top, posts, width, palette, images);
        let offsets = floor_offsets(posts, width, palette, images);
        let floor_top = offsets[first];

        if floor_h <= viewport_h {
            if scroll_top <= floor_top {
                return scroll_to_previous_floor(first, posts, width, viewport_h, palette, images);
            }
            let candidate = scroll_top.saturating_sub(step);
            if candidate < floor_top {
                return scroll_to_previous_floor(first, posts, width, viewport_h, palette, images);
            }
            return candidate;
        }

        if scroll_top <= floor_top {
            return scroll_to_previous_floor(first, posts, width, viewport_h, palette, images);
        }

        let candidate = scroll_top.saturating_sub(step);
        if candidate < floor_top {
            return scroll_to_previous_floor(first, posts, width, viewport_h, palette, images);
        }
        candidate
    }
}

/// j/Down: short floors advance one floor; tall floors scroll [`DETAIL_STEP_LINES`] lines.
/// Selection follows the top visible floor.
pub fn detail_step_down(
    selected: usize,
    scroll_top: u16,
    posts: &[Post],
    width: u16,
    viewport_h: u16,
    palette: Palette,
    images: Option<&ImageCache>,
) -> (usize, u16) {
    if posts.is_empty() || viewport_h == 0 {
        return (0, 0);
    }
    let selected = selected.min(posts.len() - 1);
    let total = floor_list_total_height(posts, width, palette, images);
    let max_scroll = total.saturating_sub(viewport_h);
    if scroll_top >= max_scroll {
        return (selected, max_scroll);
    }

    let (first, floor_h) = top_visible_floor_height(scroll_top, posts, width, palette, images);
    let new_scroll = if floor_h > viewport_h {
        detail_line_scroll(
            scroll_top,
            i32::from(DETAIL_STEP_LINES),
            posts,
            width,
            viewport_h,
            palette,
            images,
        )
    } else {
        let offsets = floor_offsets(posts, width, palette, images);
        if first + 1 < posts.len() {
            offsets[first + 1]
        } else {
            max_scroll
        }
    };
    let new_selected = first_visible_floor(new_scroll, posts, width, palette, images);
    (new_selected, new_scroll)
}

/// k/Up: short floors snap to the current floor top, then the previous floor; tall floors
/// scroll [`DETAIL_STEP_LINES`] lines. Selection follows the top visible floor.
pub fn detail_step_up(
    selected: usize,
    scroll_top: u16,
    posts: &[Post],
    width: u16,
    viewport_h: u16,
    palette: Palette,
    images: Option<&ImageCache>,
) -> (usize, u16) {
    if posts.is_empty() || viewport_h == 0 {
        return (0, 0);
    }
    let _ = selected.min(posts.len() - 1);
    if scroll_top == 0 {
        return (0, 0);
    }

    let (first, floor_h) = top_visible_floor_height(scroll_top, posts, width, palette, images);
    let new_scroll = if floor_h > viewport_h {
        detail_line_scroll(
            scroll_top,
            -i32::from(DETAIL_STEP_LINES),
            posts,
            width,
            viewport_h,
            palette,
            images,
        )
    } else {
        let offsets = floor_offsets(posts, width, palette, images);
        let floor_top = offsets[first];
        if scroll_top > floor_top {
            floor_top
        } else if first > 0 {
            offsets[first - 1]
        } else {
            0
        }
    };
    let new_selected = first_visible_floor(new_scroll, posts, width, palette, images);
    (new_selected, new_scroll)
}

pub fn page_scroll_top(
    scroll_top: u16,
    delta: i32,
    posts: &[Post],
    width: u16,
    viewport_h: u16,
    palette: Palette,
    images: Option<&ImageCache>,
) -> u16 {
    if posts.is_empty() || viewport_h == 0 {
        return 0;
    }
    let offsets = floor_offsets(posts, width, palette, images);
    let first = first_visible_floor(scroll_top, posts, width, palette, images);
    let page_size = floors_per_page(first, posts, width, viewport_h, palette, images);

    let target_floor = if delta > 0 {
        (first + page_size).min(posts.len().saturating_sub(1))
    } else {
        first.saturating_sub(page_size)
    };

    offsets[target_floor]
}

pub fn draw_floor_list(frame: &mut Frame<'_>, area: Rect, mut props: FloorListProps<'_>) {
    if area.height == 0 || area.width < 8 {
        return;
    }
    if props.posts.is_empty() {
        return;
    }
    clear_content_viewport(frame, area);
    let viewport_bottom = props.scroll_top.saturating_add(area.height);
    let mut logical_top = 0u16;

    for (idx, post) in props.posts.iter().enumerate() {
        let floor_h = measure_floor(
            post,
            area.width,
            props.palette,
            props.images.as_deref(),
            idx + 1 < props.posts.len(),
        );
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

        let draw_y = area
            .y
            .saturating_add(visible_top.saturating_sub(props.scroll_top));
        let draw_h = visible_bottom.saturating_sub(visible_top);
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
    scroll_top: u16,
    floor_top: u16,
    skip_lines: u16,
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
    let (mut row1, row2) = floor_header_rows(
        author.as_ref(),
        post.floor,
        &post.time,
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
    let mut line_idx = 0u16;
    let mut header_top = None;

    let next_visible_row =
        |frame: &mut Frame<'_>, y: &mut u16, line_idx: &mut u16, draw_bar: bool| -> bool {
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
            if header_top.is_some() || floor_top + HEADER_H > scroll_top {
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
                    i32::from(floor_top),
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
            while line_idx < poll_start.saturating_add(poll_h) && line_idx < skip_lines {
                line_idx += 1;
            }
            if line_idx < poll_start.saturating_add(poll_h) && y < area.y + area.height {
                let poll_skip = line_idx.saturating_sub(poll_start);
                let poll_visible = poll_start
                    .saturating_add(poll_h)
                    .saturating_sub(line_idx)
                    .min(area.y + area.height - y);
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
            line_idx = poll_start.saturating_add(poll_h);
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
    body: Rect,
    images: &mut Option<&mut ImageCache>,
    palette: Palette,
    selected: bool,
    y: &mut u16,
    line_idx: &mut u16,
    scroll_top: u16,
    floor_top: u16,
    skip_lines: u16,
    kind: ImageKind,
    cache_key: &str,
    _width: u16,
    height: u16,
    failed: bool,
) -> bool {
    let block_start = *line_idx;
    let doc_y = i32::from(floor_top.saturating_add(block_start));
    let pos_y = doc_y - scroll_top as i32;
    let bottom_margin = images
        .as_deref()
        .map(|cache| graphics_bottom_margin(cache.picker(), kind))
        .unwrap_or(0);

    while *line_idx < block_start.saturating_add(height) && *line_idx < skip_lines {
        *line_idx += 1;
    }
    if *line_idx >= block_start.saturating_add(height) {
        return false;
    }
    let image_viewport_h = viewport.height.saturating_sub(bottom_margin).max(1);
    if pos_y >= image_viewport_h as i32 {
        *line_idx = block_start.saturating_add(height);
        return true;
    }
    if pos_y + height as i32 <= 0 {
        *line_idx = block_start.saturating_add(height);
        return false;
    }
    if *y >= area.y + area.height {
        return true;
    }

    let remaining_h = area.y.saturating_add(area.height).saturating_sub(*y);
    if remaining_h > 0 && *line_idx >= skip_lines {
        let slice_skip = line_idx.saturating_sub(block_start);
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
                body.x.saturating_add(1),
                doc_y,
                scroll_top,
            );
        }
        *y = y.saturating_add(rows_in_view);
    }
    *line_idx = block_start.saturating_add(height);
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
    fn floor_includes_gap_row() {
        let post = sample_post();
        let palette = Palette::default();
        let h = measure_floor(&post, 80, palette, None, false);
        let without_gap = HEADER_H + 1;
        assert!(h > without_gap);
    }

    #[test]
    fn scroll_down_minimal() {
        let posts = vec![sample_post(); 6];
        let palette = Palette::default();
        let h = measure_floor(&posts[0], 80, palette, None, true);
        let viewport = h * 4;
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
        let viewport = h * 4;
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
        let viewport = floor_h.saturating_mul(3);
        assert!(floor_h <= viewport);

        let (sel, scroll) = detail_step_down(0, 0, &posts, 80, viewport, palette, None);
        assert_eq!(scroll, floor_h);
        assert_eq!(sel, 1);

        let max_scroll =
            floor_list_total_height(&posts, 80, palette, None).saturating_sub(viewport);
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
        assert!(floor_h > viewport, "floor_h={floor_h} viewport={viewport}");

        let (sel, scroll) = detail_step_down(0, 0, &posts, 80, viewport, palette, None);
        assert_eq!(scroll, DETAIL_STEP_LINES);
        assert_eq!(sel, 0);
    }

    #[test]
    fn detail_step_up_advances_by_floor_when_short() {
        let posts = vec![sample_post(); 4];
        let palette = Palette::default();
        let floor_h = measure_floor(&posts[0], 80, palette, None, true);
        let viewport = floor_h.saturating_mul(3);
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
        assert!(floor_h > viewport, "floor_h={floor_h} viewport={viewport}");

        let (sel, scroll) = detail_step_up(0, 15, &posts, 80, viewport, palette, None);
        assert_eq!(scroll, 15 - DETAIL_STEP_LINES);
        assert_eq!(sel, 0);
    }

    #[test]
    fn detail_line_scroll_snaps_to_next_floor_top() {
        let posts = vec![tall_post(), sample_post()];
        let palette = Palette::default();
        let viewport = 12u16;
        let offsets = floor_offsets(&posts, 80, palette, None);
        let floor_h = measure_floor(&posts[0], 80, palette, None, true);
        assert!(floor_h > viewport);
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

        let near_end = floor_end.saturating_sub(DETAIL_STEP_LINES.saturating_sub(1));
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
        let viewport = floor_h.saturating_mul(2);
        let scroll = floor_h.saturating_mul(3);
        let last = last_visible_floor(scroll, &posts, 80, viewport, palette, None);
        assert!(
            last >= 3,
            "last visible should be near viewport bottom, got {last}"
        );
    }
}
