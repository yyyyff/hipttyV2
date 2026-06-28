use std::path::{Path, PathBuf};

use hiptty_core::{AdapterError, AdapterResult};
use regex::Regex;
use serde::Serialize;
use std::sync::LazyLock;

use crate::http::HttpClient;

static META_CHARSET: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(charset\s*=\s*["']?)([\w-]+)(["']?)"#).expect("valid regex")
});

#[derive(Debug, Clone, Serialize)]
pub struct FixtureDump {
    pub url: String,
    pub output: PathBuf,
    pub bytes: usize,
    pub charset: String,
}

pub async fn dump_fixture(
    http: &HttpClient,
    forum_base: &str,
    url: &str,
    output: Option<&Path>,
) -> AdapterResult<FixtureDump> {
    let full_url = resolve_url(forum_base, url);
    let html = http.get_text(&full_url).await?;
    let charset = detect_charset(&html).unwrap_or_else(|| "utf-8".into());
    let normalized = normalize_fixture_html(&html);
    let bytes = normalized.len();

    let output_path = output
        .map(PathBuf::from)
        .unwrap_or_else(|| default_output_path(&full_url));

    if let Some(parent) = output_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| {
                AdapterError::InvalidInput(format!(
                    "cannot create output directory {}: {e}",
                    parent.display()
                ))
            })?;
        }
    }

    std::fs::write(&output_path, normalized).map_err(|e| {
        AdapterError::InvalidInput(format!(
            "cannot write fixture to {}: {e}",
            output_path.display()
        ))
    })?;

    Ok(FixtureDump {
        url: full_url,
        output: output_path,
        bytes,
        charset,
    })
}

fn resolve_url(forum_base: &str, url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        return url.to_string();
    }

    let path = url.trim_start_matches('/');
    if path.is_empty() {
        forum_base.trim_end_matches('/').to_string()
    } else {
        format!("{}{path}", forum_base)
    }
}

fn default_output_path(url: &str) -> PathBuf {
    let stem = url
        .split('?')
        .next()
        .and_then(|s| s.rsplit('/').next())
        .unwrap_or("fixture")
        .replace(".php", "");

    let mut name = stem;
    if let Some(query) = url.split('?').nth(1) {
        for part in query.split('&') {
            let Some((key, value)) = part.split_once('=') else {
                continue;
            };
            if matches!(key, "fid" | "tid" | "page" | "uid") && !value.is_empty() {
                name.push('_');
                name.push_str(key);
                name.push('_');
                name.push_str(value);
            }
        }
    }

    PathBuf::from(format!("{name}.html"))
}

fn normalize_fixture_html(html: &str) -> String {
    META_CHARSET.replace_all(html, "${1}utf-8${3}").into_owned()
}

fn detect_charset(html: &str) -> Option<String> {
    META_CHARSET
        .captures(&html[..html.len().min(4096)])
        .and_then(|c| c.get(2))
        .map(|m| m.as_str().to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_relative_forum_url() {
        let base = "https://www.4d4y.com/forum/";
        assert_eq!(
            resolve_url(base, "forumdisplay.php?fid=7&page=1"),
            "https://www.4d4y.com/forum/forumdisplay.php?fid=7&page=1"
        );
    }

    #[test]
    fn default_output_path_from_url() {
        let path = default_output_path("https://www.4d4y.com/forum/forumdisplay.php?fid=7&page=1");
        assert_eq!(path, PathBuf::from("forumdisplay_fid_7_page_1.html"));
    }

    #[test]
    fn normalize_charset_to_utf8() {
        let html = r#"<meta http-equiv="Content-Type" content="text/html; charset=gbk" />"#;
        let out = normalize_fixture_html(html);
        assert!(out.contains("charset=utf-8"));
    }
}
