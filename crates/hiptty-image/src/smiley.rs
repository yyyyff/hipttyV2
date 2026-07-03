use rust_embed::RustEmbed;

use crate::cache::{ImageCache, ImageKind};

#[derive(RustEmbed)]
#[folder = "assets/emoji/"]
struct EmojiAssets;

/// Stable cache key for a forum smiley. Prefer parsed `code` (e.g. `default_lol`).
pub fn smiley_cache_key(
    code: Option<&str>,
    smilie_id: Option<&str>,
    url: &str,
) -> String {
    if let Some(code) = code.filter(|c| !c.is_empty()) {
        return format!("smiley:{code}");
    }
    if let Some(id) = smilie_id.filter(|id| !id.is_empty()) {
        return format!("smiley:id:{id}");
    }
    smiley_cache_key_from_url(url)
}

pub fn smiley_cache_key_from_url(url: &str) -> String {
    const MARKER: &str = "/forum/images/smilies/";
    if let Some(idx) = url.find(MARKER) {
        let rest = &url[idx + MARKER.len()..];
        if let Some(end) = rest.rfind('.') {
            let code = rest[..end].replace('/', "_");
            if !code.is_empty() {
                return format!("smiley:{code}");
            }
        }
    }
    format!("smiley:url:{url}")
}

fn asset_file_for_key(key: &str) -> Option<String> {
    let code = key.strip_prefix("smiley:")?;
    if code.starts_with("id:") || code.starts_with("url:") {
        return None;
    }
    Some(format!("{code}.png"))
}

/// Load embedded PNG bytes for a smiley cache key, if available.
pub fn smiley_asset_bytes(key: &str) -> Option<Vec<u8>> {
    let file = asset_file_for_key(key)?;
    EmojiAssets::get(&file).map(|f| f.data.into_owned())
}

/// Queue local smiley decode when bytes exist; returns false if asset is missing.
pub fn prefetch_smiley(cache: &mut ImageCache, key: String) -> bool {
    if !cache.request(key.clone(), ImageKind::Smiley) {
        return true;
    }
    let Some(bytes) = smiley_asset_bytes(&key) else {
        cache.mark_failed(&key);
        return false;
    };
    cache.ingest_bytes(key, ImageKind::Smiley, bytes);
    true
}

pub fn prefetch_post_smileys(cache: &mut ImageCache, post: &hiptty_core::Post) {
    for node in &post.content {
        collect_smiley_keys(node, cache);
    }
}

fn collect_smiley_keys(node: &hiptty_core::ContentNode, cache: &mut ImageCache) {
    let hiptty_core::ContentNode::Text { spans } = node else {
        return;
    };
    for span in spans {
        if let hiptty_core::ContentSpan::Smiley {
            url,
            code,
            smilie_id,
        } = span
        {
            let key = smiley_cache_key(
                code.as_deref(),
                smilie_id.as_deref(),
                url,
            );
            let _ = prefetch_smiley(cache, key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_from_code() {
        assert_eq!(
            smiley_cache_key(Some("default_lol"), None, "http://x/lol.gif"),
            "smiley:default_lol"
        );
    }

    #[test]
    fn cache_key_from_url() {
        assert_eq!(
            smiley_cache_key_from_url(
                "https://img02.4d4y.com/forum/images/smilies/default/biggrin.gif"
            ),
            "smiley:default_biggrin"
        );
    }

    #[test]
    fn embedded_default_png_exists() {
        assert!(smiley_asset_bytes("smiley:default_lol").is_some());
    }

    #[test]
    fn embedded_coolmonkey_png_exists() {
        assert_eq!(
            smiley_cache_key_from_url(
                "https://img02.4d4y.com/forum/images/smilies/coolmonkey/01.gif"
            ),
            "smiley:coolmonkey_01"
        );
        assert!(smiley_asset_bytes("smiley:coolmonkey_01").is_some());
    }

    #[test]
    fn embedded_grapeman_png_exists() {
        assert_eq!(
            smiley_cache_key_from_url(
                "https://img02.4d4y.com/forum/images/smilies/grapeman/12.gif"
            ),
            "smiley:grapeman_12"
        );
        assert!(smiley_asset_bytes("smiley:grapeman_12").is_some());
    }
}