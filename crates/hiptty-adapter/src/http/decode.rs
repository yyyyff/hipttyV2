use encoding_rs::GBK;
use hiptty_core::AdapterError;
use regex::Regex;
use std::sync::LazyLock;

static META_CHARSET: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)charset\s*=\s*["']?([\w-]+)"#).expect("valid regex"));

pub fn decode_response(body: &[u8], content_type: Option<&str>) -> Result<String, AdapterError> {
    let charset = content_type
        .and_then(extract_charset)
        .or_else(|| extract_meta_charset(body))
        .unwrap_or_else(|| "utf-8".to_string());

    decode_bytes(body, &charset)
}

fn extract_charset(content_type: &str) -> Option<String> {
    content_type.split(';').skip(1).find_map(|part| {
        let part = part.trim();
        META_CHARSET
            .captures(part)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_ascii_lowercase())
    })
}

fn extract_meta_charset(body: &[u8]) -> Option<String> {
    let prefix = String::from_utf8_lossy(&body[..body.len().min(4096)]);
    META_CHARSET
        .captures(&prefix)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_ascii_lowercase())
}

pub fn decode_bytes(body: &[u8], charset: &str) -> Result<String, AdapterError> {
    match charset {
        "gbk" | "gb2312" | "gb18030" => {
            let (decoded, _, had_errors) = GBK.decode(body);
            if had_errors {
                return Err(AdapterError::Parse(
                    "GBK decode encountered invalid sequences".into(),
                ));
            }
            Ok(decoded.into_owned())
        }
        "utf-8" | "utf8" => String::from_utf8(body.to_vec())
            .map_err(|e| AdapterError::Parse(format!("UTF-8 decode failed: {e}"))),
        other => Err(AdapterError::Parse(format!("unsupported charset: {other}"))),
    }
}

/// Build `application/x-www-form-urlencoded` body with GBK percent-encoding (hipda default).
pub fn encode_gbk_form(params: &[(&str, &str)]) -> String {
    params
        .iter()
        .map(|(key, value)| format!("{}={}", gbk_urlencode(key), gbk_urlencode(value)))
        .collect::<Vec<_>>()
        .join("&")
}

/// GBK percent-encoding for search parameters (hipda uses `URLEncoder.encode(..., "GBK")`).
pub fn gbk_urlencode(input: &str) -> String {
    let (encoded, _, _) = GBK.encode(input);
    encoded
        .as_ref()
        .iter()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
                char::from(*b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gbk_urlencode_non_ascii() {
        let encoded = gbk_urlencode("测试");
        assert!(encoded.contains('%'));
        assert!(!encoded.contains('测'));
    }

    #[test]
    fn encode_gbk_form_chinese_username() {
        let body = encode_gbk_form(&[("username", "中文用户"), ("password", "abc")]);
        assert!(body.starts_with("username="));
        assert!(body.contains('&'));
        assert!(!body.contains("中文"));
    }
}
