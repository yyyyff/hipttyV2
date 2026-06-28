use hiptty_core::AdapterError;
use scraper::{Html, Selector};

pub fn parse_html(html: &str) -> Html {
    Html::parse_document(html)
}

pub fn select_one<'a>(
    document: &'a Html,
    selector: &str,
) -> Result<scraper::ElementRef<'a>, AdapterError> {
    let sel = Selector::parse(selector)
        .map_err(|e| AdapterError::Parse(format!("invalid selector: {e}")))?;
    document
        .select(&sel)
        .next()
        .ok_or_else(|| AdapterError::Parse(format!("selector not found: {selector}")))
}

pub fn parse_formhash(html: &str) -> Option<String> {
    let document = parse_html(html);
    let sel = Selector::parse("input[name=formhash]").ok()?;
    document
        .select(&sel)
        .next()
        .and_then(|el| el.value().attr("value"))
        .map(str::to_string)
}

pub fn parse_error_message(html: &str) -> Option<String> {
    let document = parse_html(html);
    let sel = Selector::parse("div.alert_error").ok()?;
    document.select(&sel).next().map(|el| el.text().collect())
}

pub fn auth_required(html: &str) -> bool {
    html.contains("alert_error") && html.contains("尚未登录")
}

pub fn ensure_parseable(html: &str) -> Result<(), AdapterError> {
    if auth_required(html) {
        return Err(AdapterError::AuthRequired);
    }
    Ok(())
}

pub fn is_logged_in(html: &str) -> bool {
    parse_session_identity(html).is_some()
}

pub fn parse_session_identity(html: &str) -> Option<(String, String)> {
    let document = parse_html(html);
    let sel = Selector::parse("#umenu cite a").ok()?;
    let link = document.select(&sel).next()?;
    let href = link.value().attr("href").unwrap_or_default();
    if !href.contains("space.php") {
        return None;
    }
    let uid = extract_param(href, "uid=", "&");
    if uid.is_empty() {
        return None;
    }
    let username = link.text().collect::<String>().trim().to_string();
    if username.is_empty() {
        return None;
    }
    Some((uid, username))
}

/// Parse attachment size from Discuz markup like `(170.99 KB)`.
pub fn parse_size_text(text: &str) -> Option<u64> {
    let inner = text.trim();
    let inner = inner.strip_prefix('(').and_then(|s| s.strip_suffix(')'))?;
    let upper = inner.trim().to_ascii_uppercase();
    if let Some(num) = upper.strip_suffix("KB") {
        let kb: f64 = num.trim().parse().ok()?;
        return Some((kb * 1024.0).round() as u64);
    }
    if let Some(num) = upper.strip_suffix("MB") {
        let mb: f64 = num.trim().parse().ok()?;
        return Some((mb * 1024.0 * 1024.0).round() as u64);
    }
    if let Some(num) = upper.strip_suffix("BYTES") {
        return num.trim().parse().ok();
    }
    None
}

pub fn extract_param(source: &str, start: &str, end: &str) -> String {
    let Some(start_idx) = source.find(start) else {
        return String::new();
    };
    let rest = &source[start_idx + start.len()..];
    if end.is_empty() {
        return rest.to_string();
    }
    if let Some(end_idx) = rest.find(end) {
        rest[..end_idx].to_string()
    } else {
        rest.to_string()
    }
}

pub fn parse_int(s: &str) -> u32 {
    s.trim().parse().unwrap_or(0)
}

pub fn extract_cdata(body: &str) -> Option<String> {
    let start = body.find("<![CDATA[")? + "<![CDATA[".len();
    let rest = &body[start..];
    let end = rest.find("]]>")?;
    let text = rest[..end].trim().to_string();
    (!text.is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_tid_from_href() {
        let href = "viewthread.php?tid=12345&page=2";
        assert_eq!(extract_param(href, "tid=", "&"), "12345");
        assert_eq!(extract_param("viewthread.php?tid=99", "tid=", "&"), "99");
    }

    #[test]
    fn login_formhash_from_fixture() {
        let html = include_str!("../../tests/fixtures/login_form.html");
        assert!(parse_formhash(html).is_some());
    }

    #[test]
    fn parse_size_kb() {
        assert_eq!(parse_size_text("(170.99 KB)"), Some(175094));
    }

    #[test]
    fn parse_session_identity_from_fixture() {
        let html = include_str!("../../tests/fixtures/pm_thread_empty.html");
        let (uid, username) = parse_session_identity(html).expect("logged-in umenu");
        assert_eq!(uid, "10001");
        assert_eq!(username, "me");
    }
}
