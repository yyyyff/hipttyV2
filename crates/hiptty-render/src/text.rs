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

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, TimeDelta};

/// Format forum datetime as Chinese relative time (e.g. `3小时前`).
pub fn format_relative_time(raw: &str) -> String {
    format_relative_time_at(raw, Local::now())
}

pub fn format_relative_time_at(raw: &str, now: DateTime<Local>) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if is_already_relative(trimmed) {
        return trimmed.to_string();
    }
    match parse_forum_datetime(trimmed) {
        Some(dt) => format_duration_zh(now.signed_duration_since(dt)),
        None => trimmed.to_string(),
    }
}

fn is_already_relative(raw: &str) -> bool {
    raw == "刚刚" || raw.ends_with('前')
}

fn parse_forum_datetime(raw: &str) -> Option<DateTime<Local>> {
    if let Ok(dt) = NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M") {
        return dt.and_local_timezone(Local).single();
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S") {
        return dt.and_local_timezone(Local).single();
    }
    for fmt in ["%Y-%m-%d", "%Y-%m-%d %H:%M", "%Y-%m-%d %H:%M:%S"] {
        if let Ok(date) = NaiveDate::parse_from_str(raw, fmt) {
            return date
                .and_hms_opt(0, 0, 0)?
                .and_local_timezone(Local)
                .single();
        }
    }
    parse_flexible_forum_datetime(raw)
}

/// Discuz often uses single-digit month/day, e.g. `2026-3-10 15:33`.
fn parse_flexible_forum_datetime(raw: &str) -> Option<DateTime<Local>> {
    let (date_part, time_part) = raw.split_once(' ').map_or((raw, None), |(d, t)| (d, Some(t)));
    let mut date_bits = date_part.split('-');
    let year: i32 = date_bits.next()?.parse().ok()?;
    let month: u32 = date_bits.next()?.parse().ok()?;
    let day: u32 = date_bits.next()?.parse().ok()?;
    let (hour, minute, second) = parse_time_part(time_part.unwrap_or_default());
    NaiveDate::from_ymd_opt(year, month, day)?
        .and_hms_opt(hour, minute, second)?
        .and_local_timezone(Local)
        .single()
}

fn parse_time_part(raw: &str) -> (u32, u32, u32) {
    let mut parts = raw.split(':');
    let hour = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let minute = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let second = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    (hour, minute, second)
}

fn format_duration_zh(delta: TimeDelta) -> String {
    if delta <= TimeDelta::zero() {
        return "刚刚".to_string();
    }
    let secs = delta.num_seconds();
    if secs < 60 {
        return "刚刚".to_string();
    }
    let mins = delta.num_minutes();
    if mins < 60 {
        return format!("{mins}分钟前");
    }
    let hours = delta.num_hours();
    if hours < 24 {
        return format!("{hours}小时前");
    }
    let days = delta.num_days();
    if days < 7 {
        return format!("{days}天前");
    }
    if days < 30 {
        return format!("{}周前", days / 7);
    }
    if days < 365 {
        return format!("{}个月前", days / 30);
    }
    format!("{}年前", days / 365)
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

    #[test]
    fn relative_time_from_forum_format() {
        let now = DateTime::parse_from_rfc3339("2026-07-01T18:00:00+08:00")
            .unwrap()
            .with_timezone(&Local);
        assert_eq!(
            format_relative_time_at("2026-7-1 17:00", now),
            "1小时前"
        );
        assert_eq!(
            format_relative_time_at("2026-6-24 15:53", now),
            "1周前"
        );
    }

    #[test]
    fn relative_time_passthrough() {
        assert_eq!(format_relative_time("刚刚"), "刚刚");
        assert_eq!(format_relative_time("5分钟前"), "5分钟前");
    }
}
