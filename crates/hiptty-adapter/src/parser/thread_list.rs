use hiptty_core::{AdapterError, ThreadList, ThreadSummary};
use scraper::{ElementRef, Html, Selector};

use crate::http::urls::ForumUrls;
use crate::parser::common::{extract_param, parse_int};
use crate::parser::simple_list::max_page_from_elements;

pub fn parse(html: &str, page: u32, urls: &ForumUrls) -> Result<ThreadList, AdapterError> {
    let document = Html::parse_document(html);
    let tbody_sel = Selector::parse("tbody[id]").map_err(|e| AdapterError::Parse(e.to_string()))?;

    let mut threads = Vec::new();
    let mut uid_hint = None;

    if let Ok(umenu_sel) = Selector::parse("#umenu cite a") {
        if let Some(link) = document.select(&umenu_sel).next() {
            let href = link.value().attr("href").unwrap_or_default();
            let uid = extract_param(href, "space.php?uid=", "&");
            if !uid.is_empty() {
                uid_hint = Some(uid);
            }
        }
    }

    for tbody in document.select(&tbody_sel) {
        let Some(thread) = parse_thread_row(tbody, urls) else {
            continue;
        };
        threads.push(thread);
    }

    if threads.is_empty() {
        return Err(AdapterError::Parse("no threads found in forum page".into()));
    }

    let max_page = parse_forum_list_max_page(&document).max(page).max(1);

    Ok(ThreadList {
        threads,
        page,
        max_page,
        uid_hint,
    })
}

/// Forum thread-list pagination (`div.pages`), not per-thread `span.threadpages`.
fn parse_forum_list_max_page(document: &Html) -> u32 {
    let Ok(page_sel) =
        Selector::parse("div.pages_btns div.pages a, div.pages_btns div.pages strong, div.pages a, div.pages strong")
    else {
        return 1;
    };
    max_page_from_elements(document, &page_sel)
}

fn parse_thread_row(tbody: ElementRef<'_>, urls: &ForumUrls) -> Option<ThreadSummary> {
    let folder_td = tbody.select(&Selector::parse("td.folder").ok()?).next()?;
    let sticky = folder_td
        .select(&Selector::parse("img").ok()?)
        .next()
        .and_then(|img| img.value().attr("src"))
        .map(|src| src.contains("/pin_"))
        .unwrap_or(false);

    let icon_td = tbody.select(&Selector::parse("td.icon").ok()?).next();
    let is_poll = icon_td
        .and_then(|td| td.select(&Selector::parse("img").ok()?).next())
        .and_then(|img| img.value().attr("src"))
        .map(|src| src.contains("/poll"))
        .unwrap_or(false);

    let subject_th = tbody.select(&Selector::parse("th.subject").ok()?).next()?;
    let title_link = subject_th.select(&Selector::parse("span a").ok()?).next()?;
    let title = title_link.text().collect::<String>();
    let href = title_link.value().attr("href").unwrap_or_default();
    let tid = extract_param(href, "tid=", "&");
    if tid.is_empty() {
        return None;
    }

    let title_color = title_link.value().attr("style").and_then(|style| {
        let color = extract_param(style, "color:", "");
        (!color.is_empty()).then(|| color.trim().to_string())
    });

    let thread_type = subject_th
        .select(&Selector::parse("em a").ok()?)
        .next()
        .map(|el| el.text().collect::<String>())
        .filter(|s| !s.is_empty());

    let is_new = folder_td
        .select(&Selector::parse("img").ok()?)
        .next()
        .and_then(|img| img.value().attr("src"))
        .map(|src| src.contains("new"))
        .unwrap_or(false);

    let author_td = tbody.select(&Selector::parse("td.author").ok()?).next()?;
    let author_link = author_td.select(&Selector::parse("cite a").ok()?).next()?;
    let author = author_link.text().collect::<String>();
    let user_href = author_link.value().attr("href").unwrap_or_default();
    let author_id = extract_param(user_href, "uid=", "&");
    let avatar_url = urls.avatar_by_uid(&author_id);

    let time_create = author_td
        .select(&Selector::parse("em").ok()?)
        .next()
        .map(|el| el.text().collect::<String>());

    let time_update = tbody
        .select(&Selector::parse("td.lastpost em a").ok()?)
        .next()
        .map(|el| el.text().collect::<String>());

    let nums_td = tbody.select(&Selector::parse("td.nums").ok()?).next()?;
    let reply_count = nums_td
        .select(&Selector::parse("strong").ok()?)
        .next()
        .map(|el| el.text().collect::<String>());
    let view_count = nums_td
        .select(&Selector::parse("em").ok()?)
        .next()
        .map(|el| el.text().collect::<String>());

    let last_post = tbody
        .select(&Selector::parse("td.lastpost cite").ok()?)
        .next()
        .map(|el| el.text().collect::<String>());

    let with_pic = tbody
        .select(&Selector::parse("img.attach").ok()?)
        .any(|img| {
            img.value()
                .attr("src")
                .map(|src| src.ends_with("image_s.gif"))
                .unwrap_or(false)
        });

    let max_page = tbody
        .select(&Selector::parse("span.threadpages a").ok()?)
        .last()
        .and_then(|page_link| page_link.value().attr("href"))
        .map(|href| extract_param(href, "page=", "&"))
        .filter(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
        .map(|p| parse_int(&p))
        .filter(|&p| p > 0)
        .unwrap_or(1);

    Some(ThreadSummary {
        tid,
        title,
        title_color,
        author: Some(author),
        author_id: (!author_id.is_empty()).then_some(author_id),
        avatar_url,
        last_post,
        reply_count,
        view_count,
        time_create,
        time_update,
        thread_type,
        sticky,
        with_pic,
        is_new,
        is_poll,
        max_page,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_geek_talks_fixture() {
        let html = include_str!("../../tests/fixtures/thread_list_geek_talks_p1.html");
        let urls = ForumUrls::default_4d4y();
        let list = parse(html, 1, &urls).expect("parse fixture");

        assert!(list.threads.len() > 10);
        assert!(list.threads.iter().all(|t| !t.tid.is_empty()));
        assert!(list.threads.iter().all(|t| !t.title.is_empty()));
        assert!(list
            .threads
            .iter()
            .any(|t| !t.author.as_deref().unwrap_or("").is_empty()));
        // Forum list chrome last page is 267 — not the max of thread reply pages.
        assert_eq!(list.page, 1);
        assert_eq!(
            list.max_page, 267,
            "must parse forum div.pages (not per-thread threadpages)"
        );
        let thread_reply_max = list.threads.iter().map(|t| t.max_page).max().unwrap_or(1);
        assert!(
            thread_reply_max < list.max_page,
            "thread reply max ({thread_reply_max}) must not be used as list max_page"
        );
    }
}
