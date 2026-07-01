pub fn str_width(s: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(s)
}

pub fn truncate_str(s: &str, max_cols: usize) -> String {
    if max_cols == 0 {
        return String::new();
    }
    let mut width = 0;
    let mut out = String::new();
    for ch in s.chars() {
        let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if width + ch_width > max_cols {
            if width < max_cols {
                out.push('…');
            }
            break;
        }
        width += ch_width;
        out.push(ch);
    }
    out
}

pub fn format_count(raw: Option<&str>) -> Option<String> {
    let s = raw?.trim();
    if s.is_empty() || s == "0" {
        return None;
    }
    Some(s.to_string())
}

pub fn display_title(title: &str) -> String {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        "(无标题)".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_ascii() {
        assert_eq!(truncate_str("hello world", 5), "hello");
        assert_eq!(truncate_str("hello world", 3), "hel");
    }

    #[test]
    fn zero_count_hidden() {
        assert_eq!(format_count(Some("0")), None);
    }
}
