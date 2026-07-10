use std::collections::HashMap;
use std::path::{Path, PathBuf};

use hiptty_core::{AdapterError, AdapterResult, PrePostInfo};
use image::imageops::FilterType;
use image::DynamicImage;
use regex::Regex;
use std::sync::LazyLock;

use crate::http::{urls::ForumUrls, HttpClient};
use crate::write;

/// Default max upload size (2 MiB), aligned with hipda when forum limit is unknown.
pub const DEFAULT_MAX_UPLOAD_BYTES: usize = 2 * 1024 * 1024;
const MAX_DIMENSION: u32 = 2560;
const JPEG_QUALITY: u8 = 80;

static IMAGE_PATH_CANDIDATE: LazyLock<Regex> = LazyLock::new(|| {
    // HEIC/HEIF intentionally omitted: current `image` features cannot decode them.
    let ext = r"(?:jpg|jpeg|png|gif|webp|bmp)";
    Regex::new(&format!(
        r#"(?i)(/[^\s\[\]<>{{}}"'，。！？；：、]+\.{ext}|(?:[\w\-\.%+@]+/)+[\w\-\.]+\.{ext}|"[^"]+\.{ext}"|'[^']+\.{ext}')"#
    ))
    .expect("valid image path candidate regex")
});

static IMAGE_URL_CANDIDATE: LazyLock<Regex> = LazyLock::new(|| {
    let ext = r"(?:jpg|jpeg|png|gif|webp|bmp)";
    Regex::new(&format!(
        r#"(?i)https?://[^\s\[\]<>{{}}"'，。！？；：、]+\.{ext}(?:\?[^\s\[\]<>{{}}"'，。！？；：、]*)?"#
    ))
    .expect("valid image url candidate regex")
});

const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "bmp"];

/// Content after replacing local image paths, plus attach IDs for `attachnew[][description]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedInlineImages {
    pub content: String,
    pub new_attaches: Vec<String>,
}

/// Scan content outside BBCode tags for local image paths and remote image URLs.
///
/// Local files are uploaded and replaced with `[attachimg]id[/attachimg]`.
/// Remote image URLs are wrapped as `[img]url[/img]` without downloading.
pub async fn resolve_inline_images(
    http: &HttpClient,
    urls: &ForumUrls,
    credentials: &PrePostInfo,
    content: &str,
) -> AdapterResult<ResolvedInlineImages> {
    let content = transform_outside_tags(content, wrap_image_urls);
    let placeholders = collect_image_paths(&content);
    if placeholders.is_empty() {
        return Ok(ResolvedInlineImages {
            content: content.to_string(),
            new_attaches: Vec::new(),
        });
    }

    let uid = credentials
        .uid
        .as_deref()
        .ok_or_else(|| AdapterError::Parse("upload uid not found in post form".into()))?;
    let hash = credentials
        .hash
        .as_deref()
        .ok_or_else(|| AdapterError::Parse("upload hash not found in post form".into()))?;

    let mut resolved = content;
    let mut new_attaches = Vec::new();
    let mut uploaded: HashMap<PathBuf, String> = HashMap::new();

    for (matched_text, path) in placeholders {
        let image_id = if let Some(id) = uploaded.get(&path) {
            id.clone()
        } else {
            let (bytes, filename) = prepare_image_bytes(&path)?;
            let id = write::upload_image_bytes(http, urls, uid, hash, &bytes, &filename).await?;
            if !new_attaches.contains(&id) {
                new_attaches.push(id.clone());
            }
            uploaded.insert(path, id.clone());
            id
        };
        let attach = format!("[attachimg]{image_id}[/attachimg]");
        resolved = resolved.replacen(&matched_text, &attach, 1);
    }

    Ok(ResolvedInlineImages {
        content: resolved,
        new_attaches,
    })
}

fn transform_outside_tags(content: &str, mut transform: impl FnMut(&str) -> String) -> String {
    let mut text = content;
    let mut out = String::new();
    loop {
        let Some(tag_start) = text.find('[') else {
            out.push_str(&transform(text));
            break;
        };
        let Some(tag_end) = text[tag_start..].find(']') else {
            out.push_str(&transform(text));
            break;
        };
        let tag_end = tag_start + tag_end;
        let tag_name = &text[tag_start + 1..tag_end];
        let tag_key = tag_name.split('=').next().unwrap_or(tag_name);
        let close = format!("[/{tag_key}]");
        let Some(close_rel) = text[tag_end..].find(&close) else {
            out.push_str(&transform(text));
            break;
        };
        let close_end = tag_end + close_rel + close.len();
        out.push_str(&transform(&text[..tag_start]));
        out.push_str(&text[tag_start..close_end]);
        text = &text[close_end..];
    }
    out
}

fn wrap_image_urls(text: &str) -> String {
    if text.is_empty() {
        return text.to_string();
    }
    let mut out = String::new();
    let mut last = 0usize;
    for mat in IMAGE_URL_CANDIDATE.find_iter(text) {
        out.push_str(&text[last..mat.start()]);
        let url = mat.as_str();
        out.push_str(&format!("[img]{url}[/img]"));
        last = mat.end();
    }
    out.push_str(&text[last..]);
    out
}

fn collect_image_paths(content: &str) -> Vec<(String, PathBuf)> {
    let mut candidates = Vec::new();
    for mat in IMAGE_PATH_CANDIDATE.find_iter(content) {
        let matched = mat.as_str();
        let Some(path) = resolve_path_candidate(matched) else {
            continue;
        };
        candidates.push((mat.start(), mat.end(), matched.to_string(), path));
    }

    candidates.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| (b.1 - b.0).cmp(&(a.1 - a.0))));

    let mut selected = Vec::new();
    let mut occupied: Vec<(usize, usize)> = Vec::new();
    for (start, end, text, path) in candidates {
        if occupied
            .iter()
            .any(|(s, e)| ranges_overlap(start, end, *s, *e))
        {
            continue;
        }
        selected.push((text, path));
        occupied.push((start, end));
    }

    selected
}

fn ranges_overlap(a_start: usize, a_end: usize, b_start: usize, b_end: usize) -> bool {
    a_start < b_end && b_start < a_end
}

fn resolve_path_candidate(text: &str) -> Option<PathBuf> {
    let path_text = strip_quotes(text.trim());
    if path_text.is_empty() || path_text.contains("://") {
        return None;
    }
    let path = Path::new(path_text);
    if !has_image_extension(path) {
        return None;
    }
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(path)
    };
    resolved.is_file().then_some(resolved)
}

fn strip_quotes(text: &str) -> &str {
    text.strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .or_else(|| text.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
        .unwrap_or(text)
}

fn has_image_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| IMAGE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn prepare_image_bytes(path: &Path) -> AdapterResult<(Vec<u8>, String)> {
    let raw = std::fs::read(path).map_err(|e| {
        AdapterError::InvalidInput(format!("cannot read image {}: {e}", path.display()))
    })?;

    if raw.len() <= DEFAULT_MAX_UPLOAD_BYTES && !needs_resize(&raw)? {
        let filename = upload_filename(path, false);
        return Ok((raw, filename));
    }

    let compressed = compress_image(&raw, path)?;
    let filename = upload_filename(path, true);
    Ok((compressed, filename))
}

fn needs_resize(raw: &[u8]) -> AdapterResult<bool> {
    let img = image::load_from_memory(raw)
        .map_err(|e| AdapterError::InvalidInput(format!("invalid image: {e}")))?;
    Ok(img.width().max(img.height()) > MAX_DIMENSION)
}

fn compress_image(raw: &[u8], path: &Path) -> AdapterResult<Vec<u8>> {
    let img = image::load_from_memory(raw)
        .map_err(|e| AdapterError::InvalidInput(format!("invalid image: {e}")))?;

    if is_gif(raw) && raw.len() > DEFAULT_MAX_UPLOAD_BYTES {
        return Err(AdapterError::InvalidInput(format!(
            "GIF exceeds max upload size ({} bytes)",
            DEFAULT_MAX_UPLOAD_BYTES
        )));
    }

    let max_side = img.width().max(img.height()).min(MAX_DIMENSION);
    for step in 0..5 {
        let side = ((max_side as f32) * (5 - step) as f32 * 0.1).max(1.0) as u32;
        let resized = resize_to_max_side(&img, side);
        let bytes = encode_jpeg(&resized)?;
        if bytes.len() <= DEFAULT_MAX_UPLOAD_BYTES {
            return Ok(bytes);
        }
    }

    Err(AdapterError::InvalidInput(format!(
        "cannot compress image to <= {} bytes: {}",
        DEFAULT_MAX_UPLOAD_BYTES,
        path.display()
    )))
}

fn resize_to_max_side(img: &DynamicImage, max_side: u32) -> DynamicImage {
    let (w, h) = (img.width(), img.height());
    if w.max(h) <= max_side {
        return img.clone();
    }
    let (nw, nh) = if w >= h {
        let nh = (h as f64 * max_side as f64 / w as f64).round() as u32;
        (max_side, nh.max(1))
    } else {
        let nw = (w as f64 * max_side as f64 / h as f64).round() as u32;
        (nw.max(1), max_side)
    };
    img.resize_exact(nw, nh, FilterType::Triangle)
}

fn encode_jpeg(img: &DynamicImage) -> AdapterResult<Vec<u8>> {
    let rgb = img.to_rgb8();
    let mut out = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY);
    encoder
        .encode(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|e| AdapterError::InvalidInput(format!("jpeg encode failed: {e}")))?;
    Ok(out)
}

fn is_gif(raw: &[u8]) -> bool {
    raw.starts_with(b"GIF87a") || raw.starts_with(b"GIF89a")
}

fn upload_filename(path: &Path, compressed: bool) -> String {
    let ext = if compressed {
        "jpg"
    } else {
        path.extension().and_then(|e| e.to_str()).unwrap_or("jpg")
    };
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let marker = if compressed { "_" } else { "-" };
    format!("Hi{marker}{stamp}.{ext}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_path() -> Option<PathBuf> {
        let path = std::env::current_dir()
            .unwrap()
            .join("refs/upload_images.jpg");
        path.is_file().then_some(path)
    }

    #[test]
    fn detects_standalone_local_path() {
        let Some(path) = fixture_path() else {
            return;
        };
        let text = format!("第一行\n{}\n第三行", path.display());
        let placeholders = collect_image_paths(&text);
        assert_eq!(placeholders.len(), 1);
        assert_eq!(placeholders[0].1, path);
    }

    #[test]
    fn detects_inline_local_path_with_surrounding_text() {
        let Some(path) = fixture_path() else {
            return;
        };
        let text = format!("看看这张图 {path} 怎么样", path = path.display());
        let placeholders = collect_image_paths(&text);
        assert_eq!(placeholders.len(), 1);
        assert_eq!(placeholders[0].0, path.display().to_string());
        assert_eq!(placeholders[0].1, path);
    }

    #[test]
    fn detects_relative_local_path() {
        let Some(path) = fixture_path() else {
            return;
        };
        let text = "附图 refs/upload_images.jpg 请审阅";
        let placeholders = collect_image_paths(text);
        assert_eq!(placeholders.len(), 1);
        assert_eq!(placeholders[0].1, path);
    }

    #[test]
    fn attachimg_format() {
        assert_eq!(
            format!("[attachimg]{}[/attachimg]", "12345"),
            "[attachimg]12345[/attachimg]"
        );
    }

    #[test]
    fn rejects_url_and_missing_files() {
        assert!(collect_image_paths("https://example.com/a.jpg").is_empty());
        assert!(collect_image_paths("/no/such/file.jpg").is_empty());
        assert!(resolve_path_candidate("https://example.com/a.jpg").is_none());
    }

    #[test]
    fn wraps_remote_image_url_outside_tags() {
        let text = "看看 https://example.com/pic.png 怎么样";
        let wrapped = transform_outside_tags(text, wrap_image_urls);
        assert_eq!(
            wrapped,
            "看看 [img]https://example.com/pic.png[/img] 怎么样"
        );
    }

    #[test]
    fn skips_url_inside_existing_tag() {
        let text = "[url]https://example.com/a.png[/url]";
        let wrapped = transform_outside_tags(text, wrap_image_urls);
        assert_eq!(wrapped, text);
    }

    #[test]
    fn does_not_match_grok_image_ref() {
        assert!(collect_image_paths("[Image #4]").is_empty());
    }
}
