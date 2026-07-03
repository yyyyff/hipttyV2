use hiptty_render::str_width;
use ratatui::{
    layout::Rect,
    widgets::Paragraph,
    Frame,
};
use ratatui_image::sliced::{SignedPosition, SlicedImage};
use ratatui_image::Image;

use ratatui_image::picker::Picker;

use crate::cache::{ImageEntry, ImageKind, ImageState, ReadyDraw};
use crate::layout::{graphics_bottom_margin, shrink_viewport_bottom};

pub const IMAGE_FAIL_LABEL: &str = "[图片加载失败]";

/// Draw a graphic at document coordinates into `viewport`, following the mdfried model.
///
/// `doc_x` / `doc_y` are absolute positions in the scrolled document; `scroll_top` is the
/// viewport's scroll offset. `SlicedImage` receives the full viewport and clips naturally.
pub fn draw_graphic_in_viewport(
    frame: &mut Frame<'_>,
    viewport: Rect,
    entry: Option<&ImageEntry>,
    picker: &Picker,
    kind: ImageKind,
    palette: hiptty_render::Palette,
    fail_label: &str,
    doc_x: u16,
    doc_y: i32,
    scroll_top: u16,
) {
    if viewport.width == 0 || viewport.height == 0 {
        return;
    }
    let margin = graphics_bottom_margin(picker, kind);
    let image_viewport = shrink_viewport_bottom(viewport, margin);
    if image_viewport.height == 0 {
        return;
    }
    let pos_y = doc_y - scroll_top as i32;
    let pos_x = doc_x.saturating_sub(viewport.x) as i16;

    match entry.map(|e| &e.state) {
        Some(ImageState::Ready { draw, .. }) => match draw {
            ReadyDraw::Sliced(sliced) => {
                // SlicedImage handles negative positions and clipping automatically
                frame.render_widget(
                    SlicedImage::new(sliced.as_ref(), SignedPosition::from((pos_x, pos_y as i16))),
                    image_viewport,
                );
            }
            ReadyDraw::Full(protocol) => {
                if pos_y < 0 || pos_y >= image_viewport.height as i32 {
                    return;
                }
                let size = protocol.size();
                let area = Rect {
                    x: image_viewport.x.saturating_add(pos_x.max(0) as u16),
                    y: image_viewport.y.saturating_add(pos_y as u16),
                    width: size.width.min(image_viewport.width),
                    height: size
                        .height
                        .min(image_viewport.height.saturating_sub(pos_y as u16)),
                };
                if area.width > 0 && area.height > 0 {
                    frame.render_widget(Image::new(protocol), area);
                }
            }
        },
        Some(ImageState::Failed) => draw_fail_label(frame, viewport, fail_label, doc_x, doc_y, scroll_top, palette),
        _ => draw_loading_label(frame, viewport, doc_x, doc_y, scroll_top, palette),
    }
}

pub fn draw_image_entry(
    frame: &mut Frame<'_>,
    area: Rect,
    entry: Option<&ImageEntry>,
    palette: hiptty_render::Palette,
    fail_label: &str,
    slice_skip_rows: u16,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    match entry.map(|e| &e.state) {
        Some(ImageState::Ready { draw, .. }) => match draw {
            ReadyDraw::Sliced(sliced) => {
                let position = SignedPosition::from((0, -(slice_skip_rows as i16)));
                frame.render_widget(SlicedImage::new(sliced.as_ref(), position), area);
            }
            ReadyDraw::Full(protocol) => {
                frame.render_widget(Image::new(protocol), area);
            }
        },
        Some(ImageState::Failed) => {
            frame.render_widget(
                Paragraph::new(fail_label).style(palette.dim_style()),
                area,
            );
        }
        _ => {
            frame.render_widget(
                Paragraph::new("…").style(palette.dim_style()),
                area,
            );
        }
    }
}

pub fn draw_avatar_entry(
    frame: &mut Frame<'_>,
    area: Rect,
    entry: Option<&ImageEntry>,
    placeholder: Option<&ImageEntry>,
    palette: hiptty_render::Palette,
    slice_skip_rows: u16,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    if matches!(entry.map(|e| &e.state), Some(ImageState::Ready { .. })) {
        draw_image_entry(frame, area, entry, palette, "", slice_skip_rows);
        return;
    }
    if matches!(
        placeholder.map(|e| &e.state),
        Some(ImageState::Ready { .. })
    ) {
        draw_image_entry(frame, area, placeholder, palette, "", slice_skip_rows);
        return;
    }
    frame.render_widget(
        Paragraph::new("…").style(palette.dim_style()),
        area,
    );
}

fn draw_fail_label(
    frame: &mut Frame<'_>,
    viewport: Rect,
    label: &str,
    doc_x: u16,
    doc_y: i32,
    scroll_top: u16,
    palette: hiptty_render::Palette,
) {
    let row = doc_y - scroll_top as i32;
    if row < 0 || row >= viewport.height as i32 {
        return;
    }
    let w = image_area_width(label).min(viewport.width);
    frame.render_widget(
        Paragraph::new(label).style(palette.dim_style()),
        Rect {
            x: doc_x.max(viewport.x),
            y: viewport.y.saturating_add(row as u16),
            width: w,
            height: 1,
        },
    );
}

fn draw_loading_label(
    frame: &mut Frame<'_>,
    viewport: Rect,
    doc_x: u16,
    doc_y: i32,
    scroll_top: u16,
    palette: hiptty_render::Palette,
) {
    let row = doc_y - scroll_top as i32;
    if row < 0 || row >= viewport.height as i32 {
        return;
    }
    frame.render_widget(
        Paragraph::new("…").style(palette.dim_style()),
        Rect {
            x: doc_x.max(viewport.x),
            y: viewport.y.saturating_add(row as u16),
            width: 1,
            height: 1,
        },
    );
}

pub fn image_area_width(label: &str) -> u16 {
    str_width(label).max(4) as u16
}