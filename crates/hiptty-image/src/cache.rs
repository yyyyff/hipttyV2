use std::collections::HashMap;
use std::io::Cursor;
use std::sync::{Arc, mpsc::{self, Receiver, Sender}};
use std::thread;

use hiptty_render::is_windows_terminal;
use ratatui::layout::Size;
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::Protocol;
use ratatui_image::sliced::SlicedProtocol;
use ratatui_image::Resize;

use crate::avatar_disk::{AvatarDiskCache, AvatarDiskEntry};
use crate::avatar_placeholder::noavatar_bytes;
use crate::layout::{avatar_cell_size, content_image_cell_size, smiley_cell_size};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchOutcome {
    Ok(Vec<u8>),
    NotFound,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageKind {
    Avatar,
    Smiley,
    Content { max_cols: u16 },
}

/// Decoded image payload: full widget for small tiles, sliced for scrollable content.
#[derive(Clone)]
pub enum ReadyDraw {
    Full(Protocol),
    Sliced(Arc<SlicedProtocol>),
}

impl ReadyDraw {
    pub fn size(&self) -> Size {
        match self {
            Self::Full(protocol) => protocol.size(),
            Self::Sliced(sliced) => sliced.size(),
        }
    }
}

#[derive(Clone)]
pub enum ImageState {
    Loading,
    Ready {
        draw: ReadyDraw,
        width: u16,
        height: u16,
    },
    Failed,
}

impl std::fmt::Debug for ImageState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Loading => write!(f, "Loading"),
            Self::Ready { width, height, .. } => write!(f, "Ready({width}x{height})"),
            Self::Failed => write!(f, "Failed"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImageEntry {
    pub state: ImageState,
    pub kind: ImageKind,
}

struct DecodeJob {
    url: String,
    kind: ImageKind,
    bytes: Vec<u8>,
}

struct DecodeResult {
    url: String,
    kind: ImageKind,
    result: Result<DecodeOutput, ()>,
}

struct DecodeOutput {
    draw: ReadyDraw,
    size: Size,
}

pub struct ImageCache {
    picker: Picker,
    avatar_disk: Option<AvatarDiskCache>,
    avatar_placeholder: Option<ImageEntry>,
    entries: HashMap<String, ImageEntry>,
    job_tx: Sender<DecodeJob>,
    result_rx: Receiver<DecodeResult>,
}

impl std::fmt::Debug for ImageCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageCache")
            .field("entries", &self.entries.len())
            .finish()
    }
}

impl ImageCache {
    pub fn new(picker: Picker, avatar_disk: Option<AvatarDiskCache>) -> Self {
        let (job_tx, job_rx) = mpsc::channel::<DecodeJob>();
        let (result_tx, result_rx) = mpsc::channel::<DecodeResult>();
        let worker_picker = picker.clone();

        thread::spawn(move || {
            while let Ok(job) = job_rx.recv() {
                let result = decode_image(&worker_picker, job.kind, &job.bytes).map_err(|_| ());
                let _ = result_tx.send(DecodeResult {
                    url: job.url,
                    kind: job.kind,
                    result,
                });
            }
        });

        let avatar_placeholder = noavatar_bytes().and_then(|bytes| {
            decode_image(&picker, ImageKind::Avatar, &bytes)
                .ok()
                .map(|out| ImageEntry {
                    state: ready_state(out),
                    kind: ImageKind::Avatar,
                })
        });

        Self {
            picker,
            avatar_disk,
            avatar_placeholder,
            entries: HashMap::new(),
            job_tx,
            result_rx,
        }
    }

    pub fn avatar_entries_for_draw(
        &self,
        url: Option<&str>,
    ) -> (Option<&ImageEntry>, Option<&ImageEntry>) {
        (
            url.and_then(|u| self.entries.get(u)),
            self.avatar_placeholder.as_ref(),
        )
    }

    pub fn picker(&self) -> &Picker {
        &self.picker
    }

    pub fn avatar_placeholder(&self) -> Option<&ImageEntry> {
        self.avatar_placeholder.as_ref()
    }

    pub fn get(&self, url: &str) -> Option<&ImageEntry> {
        self.entries.get(url)
    }

    pub fn cell_size(&self, url: &str) -> Option<Size> {
        match self.entries.get(url).map(|e| &e.state) {
            Some(ImageState::Ready { width, height, .. }) => Some(Size::new(*width, *height)),
            _ => None,
        }
    }

    /// Returns `true` if a network fetch should be started for this URL.
    pub fn request(&mut self, url: String, kind: ImageKind) -> bool {
        if url.is_empty() {
            return false;
        }
        if let Some(entry) = self.entries.get(&url) {
            match (&entry.state, entry.kind) {
                (ImageState::Loading, _) => return false,
                (ImageState::Ready { width, height, .. }, ImageKind::Avatar)
                    if avatar_dimensions_match(*width, *height) =>
                {
                    return false
                }
                (ImageState::Ready { .. }, ImageKind::Avatar) => {
                    self.entries.remove(&url);
                }
                (ImageState::Ready { .. }, _) => return false,
                (ImageState::Failed, ImageKind::Avatar | ImageKind::Content { .. }) => {
                    self.entries.remove(&url);
                }
                _ => {}
            }
        }
        if kind == ImageKind::Avatar {
            if let Some(disk) = self.avatar_disk.as_ref() {
                if let Some(entry) = disk.load(&url) {
                    return match entry {
                        AvatarDiskEntry::Bytes(bytes) => {
                            self.ingest_bytes(url, kind, bytes);
                            false
                        }
                        AvatarDiskEntry::NotFound => {
                            self.entries.insert(
                                url,
                                ImageEntry {
                                    state: ImageState::Failed,
                                    kind,
                                },
                            );
                            false
                        }
                    };
                }
            }
        }
        self.entries.insert(
            url.clone(),
            ImageEntry {
                state: ImageState::Loading,
                kind,
            },
        );
        true
    }

    pub fn apply_fetch(&mut self, url: String, kind: ImageKind, outcome: FetchOutcome) {
        match outcome {
            FetchOutcome::Ok(bytes) => {
                if kind == ImageKind::Avatar {
                    if let Some(disk) = self.avatar_disk.as_ref() {
                        let _ = disk.save_bytes(&url, &bytes);
                    }
                }
                self.ingest_bytes(url, kind, bytes);
            }
            FetchOutcome::NotFound => {
                if kind == ImageKind::Avatar {
                    if let Some(disk) = self.avatar_disk.as_ref() {
                        let _ = disk.save_not_found(&url);
                    }
                }
                self.mark_failed(&url);
            }
            FetchOutcome::Failed => self.mark_failed(&url),
        }
    }

    pub fn ingest_bytes(&mut self, url: String, kind: ImageKind, bytes: Vec<u8>) {
        if url.is_empty() {
            return;
        }
        self.entries.insert(
            url.clone(),
            ImageEntry {
                state: ImageState::Loading,
                kind,
            },
        );
        let _ = self.job_tx.send(DecodeJob { url, kind, bytes });
    }

    pub fn mark_failed(&mut self, url: &str) {
        if let Some(entry) = self.entries.get_mut(url) {
            entry.state = ImageState::Failed;
        }
    }

    /// Drain decode results from the worker thread. Returns true if any entry changed.
    pub fn poll(&mut self) -> bool {
        let mut changed = self.refresh_stale_avatars();
        while let Ok(result) = self.result_rx.try_recv() {
            changed = true;
            let state = match result.result {
                Ok(out) => ready_state(out),
                Err(()) => ImageState::Failed,
            };
            self.entries.insert(
                result.url,
                ImageEntry {
                    state,
                    kind: result.kind,
                },
            );
        }
        changed
    }

    fn refresh_stale_avatars(&mut self) -> bool {
        let expected = avatar_cell_size();
        let stale: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, e)| e.kind == ImageKind::Avatar)
            .filter_map(|(url, e)| match &e.state {
                ImageState::Ready { width, height, .. }
                    if *width != expected.width || *height != expected.height =>
                {
                    Some(url.clone())
                }
                _ => None,
            })
            .collect();
        if stale.is_empty() {
            return false;
        }
        for url in stale {
            self.entries.remove(&url);
            if let Some(disk) = self.avatar_disk.as_ref() {
                if let Some(AvatarDiskEntry::Bytes(bytes)) = disk.load(&url) {
                    self.ingest_bytes(url, ImageKind::Avatar, bytes);
                    continue;
                }
            }
            let _ = self.request(url, ImageKind::Avatar);
        }
        true
    }
}

fn avatar_dimensions_match(width: u16, height: u16) -> bool {
    let expected = avatar_cell_size();
    width == expected.width && height == expected.height
}

fn decode_dynamic_image(bytes: &[u8]) -> Result<image::DynamicImage, image::ImageError> {
    let reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    match reader.decode() {
        Ok(img) => Ok(img),
        Err(_) if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) => image::load_from_memory(bytes),
        Err(err) => Err(err),
    }
}

fn ready_state(out: DecodeOutput) -> ImageState {
    ImageState::Ready {
        draw: out.draw,
        width: out.size.width,
        height: out.size.height,
    }
}

fn picker_for_kind(picker: &Picker, kind: ImageKind) -> Picker {
    let mut p = picker.clone();
    // Kitty uses a separate graphics layer on Windows Terminal; clipped rows leave pixels behind.
    // Sixel is drawn into cells and scrolls/clears with the grid (mdfried-style scroll behavior).
    if is_windows_terminal() && matches!(kind, ImageKind::Content { .. }) {
        p.set_protocol_type(ProtocolType::Sixel);
    }
    p
}

fn build_draw(
    picker: &Picker,
    pixels: Arc<image::DynamicImage>,
    size: Size,
    kind: ImageKind,
) -> Result<ReadyDraw, ratatui_image::errors::Errors> {
    let decode_picker = picker_for_kind(picker, kind);
    let resize = Resize::Fit(None);
    match kind {
        // Scrollable images are sliced so they clip naturally at the viewport edge (mdfried model).
        ImageKind::Content { .. } | ImageKind::Avatar => Ok(ReadyDraw::Sliced(Arc::new(
            SlicedProtocol::new_with_resize(&decode_picker, (*pixels).clone(), size, resize)?,
        ))),
        // Smileys are always one row tall and never partially scrolled.
        ImageKind::Smiley => Ok(ReadyDraw::Full(
            decode_picker.new_protocol((*pixels).clone(), size, resize)?,
        )),
    }
}

fn decode_image(
    picker: &Picker,
    kind: ImageKind,
    bytes: &[u8],
) -> Result<DecodeOutput, ratatui_image::errors::Errors> {
    let decode_picker = picker_for_kind(picker, kind);
    let dyn_img = decode_dynamic_image(bytes)?;
    let size = match kind {
        ImageKind::Avatar => avatar_cell_size(),
        ImageKind::Smiley => smiley_cell_size(),
        ImageKind::Content { max_cols } => content_image_cell_size(
            &decode_picker,
            dyn_img.width(),
            dyn_img.height(),
            max_cols,
        ),
    };
    // Avatars always occupy the full layout slot. `Resize::Fit` skips upscaling when the
    // source already fits inside the target at a smaller natural size — pre-resize to the
    // exact cell dimensions first.
    let dyn_img = if kind == ImageKind::Avatar {
        Resize::Fit(None).resize(&dyn_img, picker.font_size(), size, None)
    } else {
        dyn_img
    };
    let pixels = Arc::new(dyn_img);
    let draw = build_draw(picker, pixels, size, kind)?;
    Ok(DecodeOutput { draw, size })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui_image::picker::Picker;

    #[test]
    fn stale_avatar_ready_is_rerequested() {
        let mut cache = ImageCache::new(Picker::halfblocks(), None);
        let url = "http://example.com/avatar.jpg".to_string();
        let mut entry = cache.avatar_placeholder().expect("placeholder").clone();
        if let ImageState::Ready { width, height, .. } = &mut entry.state {
            *width = 4;
            *height = 2;
        }
        entry.kind = ImageKind::Avatar;
        cache.entries.insert(url.clone(), entry);
        assert!(cache.request(url.clone(), ImageKind::Avatar));
        assert!(matches!(
            cache.get(&url).map(|e| &e.state),
            Some(ImageState::Loading)
        ));
    }

    #[test]
    fn avatar_decode_fills_layout_slot() {
        let picker = Picker::halfblocks();
        let font = picker.font_size();
        let size = avatar_cell_size();
        let img: image::DynamicImage = image::ImageBuffer::<image::Rgba<u8>, _>::from_pixel(
            u32::from((size.width - 1) * font.width),
            u32::from(size.height * font.height),
            image::Rgba([40, 80, 120, 255]),
        )
        .into();
        let mut bytes = Vec::new();
        img.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
            .expect("encode png");
        let out = decode_image(&picker, ImageKind::Avatar, &bytes).expect("decode avatar");
        assert_eq!(out.size, size);
        assert_eq!(out.draw.size(), size);
    }

    #[test]
    fn decode_jpeg_bytes() {
        let picker = Picker::halfblocks();
        let bytes = crate::avatar_placeholder::noavatar_bytes().expect("jpeg fixture");
        let out = decode_image(&picker, ImageKind::Avatar, &bytes).expect("jpeg must decode");
        assert!(out.size.width > 0 && out.size.height > 0);
        assert!(out.draw.size().width > 0);
    }

    #[test]
    fn cache_marks_failed_url() {
        let picker = Picker::halfblocks();
        let mut cache = ImageCache::new(picker, None);
        cache.request("http://example.com/x.png".into(), ImageKind::Avatar);
        cache.mark_failed("http://example.com/x.png");
        assert!(matches!(
            cache.get("http://example.com/x.png").map(|e| &e.state),
            Some(ImageState::Failed)
        ));
    }

    #[test]
    fn failed_content_image_is_rerequested() {
        let mut cache = ImageCache::new(Picker::halfblocks(), None);
        let url = "http://example.com/post.jpg".to_string();
        cache.request(
            url.clone(),
            ImageKind::Content { max_cols: 40 },
        );
        cache.mark_failed(&url);
        assert!(cache.request(url.clone(), ImageKind::Content { max_cols: 40 }));
        assert!(matches!(
            cache.get(&url).map(|e| &e.state),
            Some(ImageState::Loading)
        ));
    }
}