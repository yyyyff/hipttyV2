use hiptty_core::{AdapterError, UserInfo};
use scraper::{Html, Selector};

use crate::http::urls::ForumUrls;
use crate::parser::common::{ensure_parseable, extract_param, parse_html};

pub fn parse(html: &str, urls: &ForumUrls) -> Result<UserInfo, AdapterError> {
    ensure_parseable(html)?;

    let document = parse_html(html);
    let username = document
        .select(&selector("div#profilecontent div.itemtitle h1")?)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AdapterError::Parse("user profile username not found".into()))?;

    let online = document
        .select(&selector("div#profilecontent div.itemtitle img")?)
        .next()
        .and_then(|img| img.value().attr("src"))
        .map(|src| src.contains("online"))
        .unwrap_or(false);

    let uid = document
        .select(&selector("div#profilecontent div.itemtitle ul li")?)
        .next()
        .map(|el| {
            let text = el.text().collect::<String>();
            extract_param(&text, "(UID:", ")").trim().to_string()
        })
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AdapterError::Parse("user profile uid not found".into()))?;

    let avatar_url = document
        .select(&selector("div.side div.profile_side div.avatar img")?)
        .next()
        .and_then(|img| img.value().attr("src"))
        .map(str::to_string)
        .or_else(|| urls.avatar_by_uid(&uid));

    let detail = build_detail(&document)?;

    Ok(UserInfo {
        uid,
        username,
        avatar_url,
        online,
        detail,
    })
}

pub fn parse_blacklist(html: &str) -> Result<Vec<String>, AdapterError> {
    ensure_parseable(html)?;

    let document = parse_html(html);
    let blacklist_div = document
        .select(&selector("div.blacklist")?)
        .next()
        .ok_or_else(|| AdapterError::Parse("blacklist section not found".into()))?;

    let link_sel = selector("ul.commonlist a")?;
    let mut names = Vec::new();
    for link in blacklist_div.select(&link_sel) {
        let href = link.value().attr("href").unwrap_or_default();
        if href.contains("space.php") {
            let username = extract_param(href, "username=", "&");
            if !username.is_empty() && !names.contains(&username) {
                names.push(username);
            }
        }
    }

    if names.is_empty()
        && !blacklist_div
            .text()
            .collect::<String>()
            .contains("暂无数据")
    {
        return Err(AdapterError::Parse("blacklist data parse error".into()));
    }

    Ok(names)
}

fn build_detail(document: &Html) -> Result<String, AdapterError> {
    let title_sel = selector("h3.blocktitle")?;
    let mut sb = String::new();
    let titles: Vec<_> = document.select(&title_sel).collect();

    for (i, title_el) in titles.iter().take(2).enumerate() {
        sb.push_str(&title_el.text().collect::<String>());
        sb.push_str("\n\n");

        if i == 0 {
            let detail_sel = selector("div.main div.s_clear ul.commonlist li")?;
            for detail in document.select(&detail_sel) {
                sb.push_str(&detail.text().collect::<String>());
                sb.push('\n');
            }
        }

        sb.push('\n');
    }

    Ok(sb)
}

fn selector(sel: &str) -> Result<Selector, AdapterError> {
    Selector::parse(sel).map_err(|e| AdapterError::Parse(format!("invalid selector: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_user_info_fixture() {
        let html = include_str!("../../tests/fixtures/user_info_189027.html");
        let urls = ForumUrls::default_4d4y();
        let info = parse(html, &urls).expect("parse user info");

        assert_eq!(info.uid, "189027");
        assert!(!info.username.is_empty());
        assert!(!info.detail.is_empty());
    }

    #[test]
    fn parse_blacklist_fixture() {
        let html = include_str!("../../tests/fixtures/blacklist.html");
        let names = parse_blacklist(html).expect("parse blacklist");

        assert_eq!(names, vec!["blocked_user".to_string()]);
    }
}
