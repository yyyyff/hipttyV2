use std::collections::{HashMap, HashSet, VecDeque};
use std::io::Cursor;
use std::sync::{
    mpsc::{self, Receiver},
    Arc, Condvar, Mutex,
};
use std::thread;

use hiptty_render::is_windows_terminal;
use ratatui::layout::Size;
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::Protocol;
use ratatui_image::sliced::SlicedProtocol;
use ratatui_image::{FilterType, Resize};

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
        /// Byte cost charged against the soft Ready budget (pixel-based estimate at decode).
        estimated_bytes: u64,
    },
    Failed,
}

impl std::fmt::Debug for ImageState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Loading => write!(f, "Loading"),
            Self::Ready {
                width,
                height,
                estimated_bytes,
                ..
            } => write!(f, "Ready({width}x{height},~{estimated_bytes}B)"),
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
    estimated_bytes: u64,
}

/// Cap in-memory decoded images (protocol payloads are heavy). FIFO among
/// non-`Loading` entries so long browsing sessions stay bounded.
pub const MAX_MEMORY_ENTRIES: usize = 256;
/// Soft byte budget for Ready protocol payloads (render-pixel estimate). Complements entry count.
/// Not a hard OOM bound: pinned / just-inserted Ready may temporarily exceed it.
/// Decode-queue compressed bytes are capped separately by [`MAX_DECODE_QUEUE_BYTES`].
pub const MAX_MEMORY_BYTES: u64 = 128 * 1024 * 1024;

/// Max decode jobs waiting (compressed bytes still held in RAM).
pub const MAX_DECODE_QUEUE_JOBS: usize = 16;
/// Soft cap on compressed bytes sitting in the decode queue (~2× max download).
pub const MAX_DECODE_QUEUE_BYTES: usize = 16 * 1024 * 1024;

/// Reject decompression bombs / absurd pixel grids after decode.
pub const MAX_DECODE_PIXELS: u64 = 16 * 1024 * 1024; // 16 MP
pub const MAX_DECODE_SIDE: u32 = 8192;

pub struct ImageCache {
    picker: Picker,
    avatar_disk: Option<AvatarDiskCache>,
    avatar_placeholder: Option<ImageEntry>,
    entries: HashMap<String, ImageEntry>,
    /// Insertion/access order for eviction (oldest at front).
    order: VecDeque<String>,
    /// Sum of Ready `estimated_bytes` currently held.
    memory_bytes: u64,
    /// URLs in the current viewport (± pad). Soft budget must not evict these.
    pinned_urls: HashSet<String>,
    job_tx: DecodeJobTx,
    result_rx: Receiver<DecodeResult>,
}

impl std::fmt::Debug for ImageCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageCache")
            .field("entries", &self.entries.len())
            .field("memory_bytes", &self.memory_bytes)
            .field("pinned", &self.pinned_urls.len())
            .finish()
    }
}

/// Parallel decode workers (JPEG/PNG → terminal protocol). Bounded so many large
/// content images do not thrash the machine; HTTP fetch concurrency is separate.
fn decode_worker_count() -> usize {
    thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2)
        .clamp(2, 4)
}

struct DecodeQueue {
    jobs: Mutex<VecDeque<DecodeJob>>,
    /// Total compressed bytes currently queued (for budget).
    queued_bytes: Mutex<usize>,
    cvar: Condvar,
    closed: Mutex<bool>,
}

impl DecodeQueue {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            jobs: Mutex::new(VecDeque::new()),
            queued_bytes: Mutex::new(0),
            cvar: Condvar::new(),
            closed: Mutex::new(false),
        })
    }

    /// Push a job. Returns the job if the queue is over budget (caller marks Failed).
    fn push(&self, job: DecodeJob) -> Option<DecodeJob> {
        let mut q = self.jobs.lock().expect("decode queue lock");
        let mut bytes = self.queued_bytes.lock().expect("decode bytes lock");
        let job_len = job.bytes.len();
        if q.len() >= MAX_DECODE_QUEUE_JOBS || *bytes + job_len > MAX_DECODE_QUEUE_BYTES {
            return Some(job);
        }
        *bytes = bytes.saturating_add(job_len);
        q.push_back(job);
        self.cvar.notify_one();
        None
    }

    fn pop(&self) -> Option<DecodeJob> {
        let mut q = self.jobs.lock().expect("decode queue lock");
        loop {
            if let Some(job) = q.pop_front() {
                let mut bytes = self.queued_bytes.lock().expect("decode bytes lock");
                *bytes = bytes.saturating_sub(job.bytes.len());
                return Some(job);
            }
            if *self.closed.lock().expect("decode closed lock") {
                return None;
            }
            q = self.cvar.wait(q).expect("decode queue wait");
        }
    }
}

/// Sender side: enqueue decode work for the worker pool.
struct DecodeJobTx {
    queue: Arc<DecodeQueue>,
}

impl DecodeJobTx {
    /// Returns `Err(job)` if the queue is closed or the job could not be accepted.
    fn send(&self, job: DecodeJob) -> Result<(), DecodeJob> {
        if *self.queue.closed.lock().expect("decode closed lock") {
            return Err(job);
        }
        match self.queue.push(job) {
            None => Ok(()),
            Some(rejected) => Err(rejected),
        }
    }
}

impl ImageCache {
    pub fn new(picker: Picker, avatar_disk: Option<AvatarDiskCache>) -> Self {
        let (result_tx, result_rx) = mpsc::channel::<DecodeResult>();
        let queue = DecodeQueue::new();
        let job_tx = DecodeJobTx {
            queue: Arc::clone(&queue),
        };
        let workers = decode_worker_count();
        for _ in 0..workers {
            let queue = Arc::clone(&queue);
            let result_tx = result_tx.clone();
            let worker_picker = picker.clone();
            thread::spawn(move || {
                while let Some(job) = queue.pop() {
                    let result = decode_image(&worker_picker, job.kind, &job.bytes).map_err(|_| ());
                    let _ = result_tx.send(DecodeResult {
                        url: job.url,
                        kind: job.kind,
                        result,
                    });
                }
            });
        }
        // Drop the original result_tx so workers' clones keep the channel open.
        drop(result_tx);

        let avatar_placeholder = noavatar_bytes().and_then(|bytes| {
            decode_image(&picker, ImageKind::Avatar, &bytes)
                .ok()
                .map(|out| ImageEntry {
                    state: ready_state(out),
                    kind: ImageKind::Avatar,
                })
        });

        if let Some(disk) = avatar_disk.as_ref() {
            let _ = disk.purge();
        }

        Self {
            picker,
            avatar_disk,
            avatar_placeholder,
            entries: HashMap::new(),
            order: VecDeque::new(),
            memory_bytes: 0,
            pinned_urls: HashSet::new(),
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

    /// Replace the pinned (viewport) URL set. Soft-budget eviction re-runs after unpin.
    pub fn set_pinned_urls(&mut self, urls: impl IntoIterator<Item = String>) {
        self.pinned_urls.clear();
        self.pinned_urls.extend(urls);
        self.evict_overflow(None);
    }

    pub fn clear_pinned(&mut self) {
        if self.pinned_urls.is_empty() {
            return;
        }
        self.pinned_urls.clear();
        self.evict_overflow(None);
    }

    pub fn is_pinned(&self, url: &str) -> bool {
        self.pinned_urls.contains(url)
    }

    #[cfg(test)]
    pub fn memory_bytes(&self) -> u64 {
        self.memory_bytes
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
                    self.touch(&url);
                    return false;
                }
                (ImageState::Ready { .. }, ImageKind::Avatar) => {
                    self.remove_entry(&url);
                }
                (ImageState::Ready { .. }, _) => {
                    self.touch(&url);
                    return false;
                }
                (ImageState::Failed, ImageKind::Avatar | ImageKind::Content { .. }) => {
                    self.remove_entry(&url);
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
                            self.insert_entry(
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
        self.insert_entry(
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
        self.insert_entry(
            url.clone(),
            ImageEntry {
                state: ImageState::Loading,
                kind,
            },
        );
        if let Err(rejected) = self.job_tx.send(DecodeJob {
            url: url.clone(),
            kind,
            bytes,
        }) {
            // Queue full / closed: free compressed bytes and mark failed so the slot can retry later.
            let _ = rejected;
            self.mark_failed(&url);
        }
    }

    pub fn mark_failed(&mut self, url: &str) {
        if let Some(entry) = self.entries.get_mut(url) {
            let old = entry_bytes(&entry.state);
            entry.state = ImageState::Failed;
            self.memory_bytes = self.memory_bytes.saturating_sub(old);
            self.touch(url);
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
            self.insert_entry(
                result.url,
                ImageEntry {
                    state,
                    kind: result.kind,
                },
            );
        }
        changed
    }

    fn touch(&mut self, url: &str) {
        if !self.entries.contains_key(url) {
            return;
        }
        self.order.retain(|u| u != url);
        self.order.push_back(url.to_string());
    }

    fn insert_entry(&mut self, url: String, entry: ImageEntry) {
        let new_cost = entry_bytes(&entry.state);
        if let Some(old) = self.entries.insert(url.clone(), entry) {
            self.memory_bytes = self
                .memory_bytes
                .saturating_sub(entry_bytes(&old.state))
                .saturating_add(new_cost);
            self.touch(&url);
        } else {
            self.memory_bytes = self.memory_bytes.saturating_add(new_cost);
            self.order.push_back(url.clone());
        }
        // Protect the URL just inserted so a single tall Ready cannot evict itself,
        // and two viewport images cannot thrash each other mid-insert.
        self.evict_overflow(Some(url.as_str()));
    }

    fn remove_entry(&mut self, url: &str) {
        if let Some(old) = self.entries.remove(url) {
            self.memory_bytes = self.memory_bytes.saturating_sub(entry_bytes(&old.state));
        }
        self.order.retain(|u| u != url);
    }

    /// Evict overflow.
    ///
    /// - **Hard** entry cap (`MAX_MEMORY_ENTRIES`): prefer unpinned non-Loading; may drop
    ///   pinned only if needed to stay under the cap. Never drops `protect`.
    /// - **Soft** Ready byte budget (`MAX_MEMORY_BYTES`): only unpinned non-Loading non-protect.
    ///   Pinned / protected Ready may leave the cache temporarily over budget.
    fn evict_overflow(&mut self, protect: Option<&str>) {
        // --- Hard entry count ---
        while self.entries.len() > MAX_MEMORY_ENTRIES {
            let Some(victim) = self.pick_hard_eviction_victim(protect) else {
                break;
            };
            self.remove_entry(&victim);
        }

        // --- Soft Ready byte budget ---
        while self.memory_bytes > MAX_MEMORY_BYTES {
            let Some(victim) = self
                .order
                .iter()
                .find(|u| {
                    let Some(url) = protect else {
                        return self.is_soft_evictable(u);
                    };
                    u.as_str() != url && self.is_soft_evictable(u)
                })
                .cloned()
            else {
                // Only pinned/protected Ready left — allow soft overshoot.
                break;
            };
            self.remove_entry(&victim);
        }
    }

    fn is_soft_evictable(&self, url: &str) -> bool {
        if self.pinned_urls.contains(url) {
            return false;
        }
        self.entries
            .get(url)
            .is_some_and(|e| matches!(e.state, ImageState::Ready { .. } | ImageState::Failed))
    }

    fn pick_hard_eviction_victim(&self, protect: Option<&str>) -> Option<String> {
        let is_protect = |u: &str| protect == Some(u);
        // 1) Oldest non-Loading, unpinned, unprotected
        if let Some(v) = self.order.iter().find(|u| {
            !is_protect(u)
                && !self.pinned_urls.contains(u.as_str())
                && self
                    .entries
                    .get(*u)
                    .is_some_and(|e| !matches!(e.state, ImageState::Loading))
        }) {
            return Some(v.clone());
        }
        // 2) Oldest non-Loading unprotected (may be pinned — hard cap wins)
        if let Some(v) = self.order.iter().find(|u| {
            !is_protect(u)
                && self
                    .entries
                    .get(*u)
                    .is_some_and(|e| !matches!(e.state, ImageState::Loading))
        }) {
            return Some(v.clone());
        }
        // 3) Oldest Loading unprotected
        self.order.iter().find(|u| !is_protect(u)).cloned()
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
            self.remove_entry(&url);
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

fn entry_bytes(state: &ImageState) -> u64 {
    match state {
        ImageState::Loading | ImageState::Failed => 0,
        ImageState::Ready {
            estimated_bytes, ..
        } => *estimated_bytes,
    }
}

/// Cost of a Ready entry from terminal cell size × font pixels (RGBA × 2 for protocol overhead).
pub fn estimate_ready_bytes(font_w: u16, font_h: u16, cell_w: u16, cell_h: u16) -> u64 {
    let pixel_w = u64::from(cell_w).saturating_mul(u64::from(font_w));
    let pixel_h = u64::from(cell_h).saturating_mul(u64::from(font_h));
    let rgba_bytes = pixel_w.saturating_mul(pixel_h).saturating_mul(4);
    rgba_bytes.saturating_mul(2).max(64 * 1024)
}

fn decode_limits() -> image::Limits {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DECODE_SIDE);
    limits.max_image_height = Some(MAX_DECODE_SIDE);
    // ~64 MiB decoded RGBA budget (plus image crate overhead).
    limits.max_alloc = Some(64 * 1024 * 1024);
    limits
}

fn decode_dynamic_image(bytes: &[u8]) -> Result<image::DynamicImage, image::ImageError> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    reader.limits(decode_limits());
    match reader.decode() {
        Ok(img) => {
            check_decoded_bounds(&img)?;
            Ok(img)
        }
        // Odd JPEG SOI/headers: force Jpeg format, but never drop Limits (decompression-bomb path).
        Err(err) if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) => {
            if matches!(err, image::ImageError::Limits(_)) {
                return Err(err);
            }
            let mut reader = image::ImageReader::new(Cursor::new(bytes));
            reader.set_format(image::ImageFormat::Jpeg);
            reader.limits(decode_limits());
            let img = reader.decode()?;
            check_decoded_bounds(&img)?;
            Ok(img)
        }
        Err(err) => Err(err),
    }
}

fn check_decoded_bounds(img: &image::DynamicImage) -> Result<(), image::ImageError> {
    let w = img.width();
    let h = img.height();
    if w > MAX_DECODE_SIDE || h > MAX_DECODE_SIDE {
        return Err(image::ImageError::Limits(
            image::error::LimitError::from_kind(image::error::LimitErrorKind::DimensionError),
        ));
    }
    let pixels = u64::from(w) * u64::from(h);
    if pixels > MAX_DECODE_PIXELS {
        return Err(image::ImageError::Limits(
            image::error::LimitError::from_kind(image::error::LimitErrorKind::DimensionError),
        ));
    }
    Ok(())
}

fn ready_state(out: DecodeOutput) -> ImageState {
    ImageState::Ready {
        draw: out.draw,
        width: out.size.width,
        height: out.size.height,
        estimated_bytes: out.estimated_bytes,
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
        ImageKind::Smiley => Ok(ReadyDraw::Full(decode_picker.new_protocol(
            (*pixels).clone(),
            size,
            resize,
        )?)),
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
        ImageKind::Content { max_cols } => {
            content_image_cell_size(&decode_picker, dyn_img.width(), dyn_img.height(), max_cols)
        }
    };
    // Avatars must fill the layout slot exactly. `Resize::Fit`/`Scale` preserve aspect ratio
    // (contain), which leaves a transparent band on the shorter axis. Stretch to the exact slot
    // pixel size instead so the glyph aligns with the selection highlight.
    let dyn_img = if kind == ImageKind::Avatar {
        let font = picker.font_size();
        let target_w = u32::from(size.width) * u32::from(font.width);
        let target_h = u32::from(size.height) * u32::from(font.height);
        dyn_img.resize_exact(target_w, target_h, FilterType::Lanczos3)
    } else {
        dyn_img
    };
    let pixels = Arc::new(dyn_img);
    let draw = build_draw(picker, pixels, size, kind)?;
    let font = decode_picker.font_size();
    let estimated_bytes = estimate_ready_bytes(font.width, font.height, size.width, size.height);
    Ok(DecodeOutput {
        draw,
        size,
        estimated_bytes,
    })
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
            *width = 3;
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
        cache.request(url.clone(), ImageKind::Content { max_cols: 40 });
        cache.mark_failed(&url);
        assert!(cache.request(url.clone(), ImageKind::Content { max_cols: 40 }));
        assert!(matches!(
            cache.get(&url).map(|e| &e.state),
            Some(ImageState::Loading)
        ));
    }

    #[test]
    fn memory_cache_evicts_when_over_capacity() {
        let mut cache = ImageCache::new(Picker::halfblocks(), None);
        // Force a tiny budget by filling with Failed entries (not Loading).
        for i in 0..(MAX_MEMORY_ENTRIES + 8) {
            let url = format!("http://example.com/{i}.png");
            cache.insert_entry(
                url,
                ImageEntry {
                    state: ImageState::Failed,
                    kind: ImageKind::Smiley,
                },
            );
        }
        assert!(cache.entries.len() <= MAX_MEMORY_ENTRIES);
        assert_eq!(cache.entries.len(), cache.order.len());
    }

    /// Synthetic Ready; `estimated_bytes` is the soft-budget charge (protocol is a tiny stub).
    fn fake_ready(cell_w: u16, cell_h: u16, estimated_bytes: u64) -> ImageState {
        ImageState::Ready {
            draw: ReadyDraw::Full({
                let picker = Picker::halfblocks();
                let img: image::DynamicImage =
                    image::ImageBuffer::<image::Rgba<u8>, _>::from_pixel(
                        8,
                        8,
                        image::Rgba([1, 2, 3, 255]),
                    )
                    .into();
                picker
                    .new_protocol(img, Size::new(1, 1), Resize::Fit(None))
                    .expect("tiny protocol")
            }),
            width: cell_w,
            height: cell_h,
            estimated_bytes,
        }
    }

    #[test]
    fn tall_ready_does_not_self_evict() {
        let mut cache = ImageCache::new(Picker::halfblocks(), None);
        // Charge more than the soft budget alone — insert protect must keep it.
        let url = "http://example.com/tall.jpg".to_string();
        let fat = MAX_MEMORY_BYTES + 10 * 1024 * 1024;
        cache.insert_entry(
            url.clone(),
            ImageEntry {
                state: fake_ready(78, 200, fat),
                kind: ImageKind::Content { max_cols: 78 },
            },
        );
        assert!(
            cache.get(&url).is_some(),
            "just-inserted tall Ready must survive soft budget"
        );
        assert!(
            !cache.request(url.clone(), ImageKind::Content { max_cols: 78 }),
            "Ready must not re-enter Loading"
        );
    }

    #[test]
    fn two_pinned_large_images_do_not_evict_each_other() {
        let mut cache = ImageCache::new(Picker::halfblocks(), None);
        let a = "http://example.com/a.jpg".to_string();
        let b = "http://example.com/b.jpg".to_string();
        // Each ~80 MiB so together exceed 128 MiB soft budget.
        let each = 80 * 1024 * 1024u64;
        assert!(each.saturating_mul(2) > MAX_MEMORY_BYTES);
        cache.set_pinned_urls([a.clone(), b.clone()]);
        cache.insert_entry(
            a.clone(),
            ImageEntry {
                state: fake_ready(80, 120, each),
                kind: ImageKind::Content { max_cols: 80 },
            },
        );
        cache.insert_entry(
            b.clone(),
            ImageEntry {
                state: fake_ready(80, 120, each),
                kind: ImageKind::Content { max_cols: 80 },
            },
        );
        assert!(cache.get(&a).is_some(), "pinned a kept");
        assert!(cache.get(&b).is_some(), "pinned b kept");
        assert!(
            cache.memory_bytes() > MAX_MEMORY_BYTES,
            "soft overshoot allowed while pinned"
        );
    }

    #[test]
    fn unpin_allows_soft_budget_eviction() {
        let mut cache = ImageCache::new(Picker::halfblocks(), None);
        let a = "http://example.com/old.jpg".to_string();
        let b = "http://example.com/new.jpg".to_string();
        let each = 80 * 1024 * 1024u64;
        cache.set_pinned_urls([a.clone(), b.clone()]);
        cache.insert_entry(
            a.clone(),
            ImageEntry {
                state: fake_ready(80, 120, each),
                kind: ImageKind::Content { max_cols: 80 },
            },
        );
        cache.insert_entry(
            b.clone(),
            ImageEntry {
                state: fake_ready(80, 120, each),
                kind: ImageKind::Content { max_cols: 80 },
            },
        );
        // Unpin both; soft eviction should reclaim until under budget (LRU drops oldest).
        cache.clear_pinned();
        assert!(
            cache.memory_bytes() <= MAX_MEMORY_BYTES,
            "after unpin, soft budget enforced, got {}",
            cache.memory_bytes()
        );
        // At least one of the oversized pair must be gone.
        let kept = cache.get(&a).is_some() as u8 + cache.get(&b).is_some() as u8;
        assert!(
            kept <= 1,
            "LRU should drop at least the older of two fat images"
        );
    }

    #[test]
    fn ready_not_re_requested_on_prefetch() {
        let mut cache = ImageCache::new(Picker::halfblocks(), None);
        let url = "http://example.com/stable.jpg".to_string();
        cache.insert_entry(
            url.clone(),
            ImageEntry {
                state: fake_ready(40, 20, 64 * 1024),
                kind: ImageKind::Content { max_cols: 40 },
            },
        );
        for _ in 0..20 {
            assert!(!cache.request(url.clone(), ImageKind::Content { max_cols: 40 }));
        }
        assert!(matches!(
            cache.get(&url).map(|e| &e.state),
            Some(ImageState::Ready { .. })
        ));
    }

    #[test]
    fn estimate_ready_bytes_is_pixel_based_not_16kib_per_cell() {
        // Old formula: 78*200*16KiB = 249.6 MiB. New: ~ font 10x20 → 780*4000*4*2 ≈ 25 MiB.
        let est = estimate_ready_bytes(10, 20, 78, 200);
        assert!(est < 64 * 1024 * 1024, "got {est}");
        assert!(est >= 64 * 1024);
        let old_style = 78u64 * 200 * 16 * 1024;
        assert!(est < old_style / 4);
    }

    #[test]
    fn decode_rejects_absurd_dimensions() {
        // 1x1 is fine; bounds helper rejects synthetic huge metadata via check path.
        let picker = Picker::halfblocks();
        let img: image::DynamicImage =
            image::ImageBuffer::<image::Rgba<u8>, _>::from_pixel(8, 8, image::Rgba([1, 2, 3, 255]))
                .into();
        let mut bytes = Vec::new();
        img.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
            .expect("encode");
        assert!(decode_image(&picker, ImageKind::Smiley, &bytes).is_ok());
        assert!(
            check_decoded_bounds(&image::DynamicImage::new_rgba8(MAX_DECODE_SIDE + 1, 1)).is_err()
        );
    }
}
