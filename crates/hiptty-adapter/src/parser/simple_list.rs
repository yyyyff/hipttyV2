use hiptty_core::{AdapterError, ListItem, SimpleList};
use scraper::{ElementRef, Html, Selector};

use crate::http::urls::ForumUrls;
use crate::parser::common::{ensure_parseable, extract_param, parse_html, parse_int};

const NEW_PM_IMAGE: &str = "notice_newpm.gif";

pub fn parse_search_title(
    html: &str,
    page: u32,
    urls: &ForumUrls,
) -> Result<SimpleList, AdapterError> {
    ensure_parseable(html)?;
    let document = parse_html(html);
    let (max_page, search_id) = parse_search_pagination(&document)?;

    let tbody_sel = selector("tbody")?;
    let mut items = Vec::new();

    for tbody in document.select(&tbody_sel) {
        let Some(item) = parse_search_tbody(tbody, urls) else {
            continue;
        };
        items.push(item);
    }

    if items.is_empty() {
        return Err(AdapterError::Parse("no search results found".into()));
    }

    Ok(SimpleList {
        items,
        page,
        max_page,
        search_id,
    })
}

pub fn parse_search_fulltext(
    html: &str,
    page: u32,
    urls: &ForumUrls,
) -> Result<SimpleList, AdapterError> {
    ensure_parseable(html)?;
    let document = parse_html(html);
    let (max_page, search_id) = parse_search_pagination(&document)?;

    let row_sel = selector("table.datatable tr")?;
    let mut items = Vec::new();

    for row in document.select(&row_sel) {
        let Some(item) = parse_fulltext_row(row, urls) else {
            continue;
        };
        items.push(item);
    }

    if items.is_empty() {
        return Err(AdapterError::Parse(
            "no fulltext search results found".into(),
        ));
    }

    Ok(SimpleList {
        items,
        page,
        max_page,
        search_id,
    })
}

pub fn parse_my_threads(html: &str, page: u32) -> Result<SimpleList, AdapterError> {
    ensure_parseable(html)?;
    let document = parse_html(html);
    let max_page = parse_pages_btns_max(&document)?;

    let table = document
        .select(&selector("table.datatable")?)
        .next()
        .ok_or_else(|| AdapterError::Parse("my threads table not found".into()))?;

    let mut items = Vec::new();
    for (i, row) in table.select(&selector("tr")?).enumerate() {
        if i == 0 {
            continue;
        }
        let Some(item) = parse_my_thread_row(row) else {
            continue;
        };
        items.push(item);
    }

    if items.is_empty() {
        return Err(AdapterError::Parse("no my threads found".into()));
    }

    Ok(SimpleList {
        items,
        page,
        max_page,
        search_id: None,
    })
}

pub fn parse_my_replies(html: &str, page: u32) -> Result<SimpleList, AdapterError> {
    ensure_parseable(html)?;
    let document = parse_html(html);
    let max_page = parse_pages_btns_max(&document)?;

    let table = document
        .select(&selector("table.datatable")?)
        .next()
        .ok_or_else(|| AdapterError::Parse("my replies table not found".into()))?;

    let rows: Vec<_> = table.select(&selector("tr")?).collect();
    let mut items = Vec::new();
    let mut pending: Option<ListItem> = None;

    for (i, row) in rows.into_iter().enumerate() {
        if i == 0 {
            continue;
        }

        if i % 2 == 1 {
            pending = parse_my_reply_title_row(row);
        } else if let Some(mut item) = pending.take() {
            if let Some(th) = row.select(&selector("th")?).next() {
                item.info = Some(th.text().collect::<String>());
            }
            items.push(item);
        }
    }

    if items.is_empty() {
        return Err(AdapterError::Parse("no my replies found".into()));
    }

    Ok(SimpleList {
        items,
        page,
        max_page,
        search_id: None,
    })
}

pub fn parse_favorites(html: &str, page: u32) -> Result<SimpleList, AdapterError> {
    ensure_parseable(html)?;
    let document = parse_html(html);
    let max_page = parse_pages_max(&document)?;

    let row_sel = selector("table.datatable tbody tr")?;
    let mut items = Vec::new();

    for row in document.select(&row_sel) {
        let Some(item) = parse_favorite_row(row) else {
            continue;
        };
        items.push(item);
    }

    if items.is_empty() {
        return Err(AdapterError::Parse("no favorites found".into()));
    }

    Ok(SimpleList {
        items,
        page,
        max_page,
        search_id: None,
    })
}

pub fn parse_pm_list(html: &str, urls: &ForumUrls) -> Result<SimpleList, AdapterError> {
    ensure_parseable(html)?;
    if html.contains(r#"class="nodata""#) {
        return Ok(empty_simple_list());
    }

    let document = parse_html(html);
    let Some(pm_list) = document.select(&selector("ul.pm_list")?).next() else {
        return Ok(empty_simple_list());
    };

    let mut items = Vec::new();
    for li in pm_list.select(&selector("li")?) {
        let Some(item) = parse_pm_list_item(li, urls) else {
            continue;
        };
        items.push(item);
    }

    Ok(SimpleList {
        items,
        page: 1,
        max_page: 1,
        search_id: None,
    })
}

pub fn parse_pm_thread(html: &str, urls: &ForumUrls) -> Result<SimpleList, AdapterError> {
    ensure_parseable(html)?;
    let document = parse_html(html);

    let (my_uid, my_username) = parse_my_identity(&document)?;

    let sms_sel = selector("li.s_clear")?;
    let mut items = Vec::new();

    for li in document.select(&sms_sel) {
        let Some(item) = parse_pm_thread_item(li, &my_uid, &my_username, urls) else {
            continue;
        };
        items.push(item);
    }

    Ok(SimpleList {
        items,
        page: 1,
        max_page: 1,
        search_id: None,
    })
}

pub fn parse_notifications(html: &str, urls: &ForumUrls) -> Result<SimpleList, AdapterError> {
    ensure_parseable(html)?;
    let document = parse_html(html);

    let feed = document
        .select(&selector("ul.feed")?)
        .next()
        .ok_or_else(|| AdapterError::Parse("notification feed not found".into()))?;

    let mut items = Vec::new();
    for li in feed.select(&selector("li")?) {
        let Some(div) = li.select(&selector("div")?).next() else {
            continue;
        };
        let item = if div.value().classes().any(|c| c == "f_thread") {
            parse_notify_thread(div)
        } else if div
            .value()
            .classes()
            .any(|c| c == "f_quote" || c == "f_reply")
        {
            parse_notify_quote_and_reply(div, urls)
        } else if div.value().classes().any(|c| c == "f_manage") {
            parse_system_info(div)
        } else if div.value().classes().any(|c| c == "f_buddy") {
            parse_friend_info(div, urls)
        } else {
            None
        };
        if let Some(item) = item {
            items.push(item);
        }
    }

    if items.is_empty() {
        return Err(AdapterError::Parse("no notifications found".into()));
    }

    Ok(SimpleList {
        items,
        page: 1,
        max_page: 1,
        search_id: None,
    })
}

pub fn has_new_pm(html: &str) -> Result<bool, AdapterError> {
    parse_check_new_pm(html)
}

pub fn parse_check_new_pm(html: &str) -> Result<bool, AdapterError> {
    if html.contains("<root>") {
        return Ok(crate::parser::common::extract_cdata(html).is_some());
    }

    ensure_parseable(html)?;
    match parse_pm_new_list(html, &ForumUrls::default_4d4y()) {
        Ok(list) => Ok(!list.items.is_empty()),
        Err(AdapterError::Parse(_)) => Ok(false),
        Err(err) => Err(err),
    }
}

pub fn parse_pm_new_list(html: &str, urls: &ForumUrls) -> Result<SimpleList, AdapterError> {
    ensure_parseable(html)?;
    if html.contains(r#"class="nodata""#) {
        return Ok(empty_simple_list());
    }
    parse_pm_list(html, urls)
}

fn empty_simple_list() -> SimpleList {
    SimpleList {
        items: Vec::new(),
        page: 1,
        max_page: 1,
        search_id: None,
    }
}

fn parse_search_pagination(document: &Html) -> Result<(u32, Option<String>), AdapterError> {
    let page_sel = selector("div.pages_btns div.pages a, div.pages_btns div.pages strong")?;
    let mut max_page = 1u32;
    let mut search_id = None;

    for el in document.select(&page_sel) {
        if search_id.is_none() {
            let href = el.value().attr("href").unwrap_or_default();
            let id = extract_param(href, "searchid=", "&");
            if !id.is_empty() {
                search_id = Some(id);
            }
        }
        let n = parse_int(&el.text().collect::<String>());
        if n > max_page {
            max_page = n;
        }
    }

    Ok((max_page, search_id))
}

fn parse_pages_btns_max(document: &Html) -> Result<u32, AdapterError> {
    let page_sel = selector("div.pages_btns div.pages a, div.pages_btns div.pages strong")?;
    Ok(max_page_from_elements(document, &page_sel))
}

fn parse_pages_max(document: &Html) -> Result<u32, AdapterError> {
    let page_sel = selector("div.pages a, div.pages strong")?;
    Ok(max_page_from_elements(document, &page_sel))
}

fn max_page_from_elements(document: &Html, page_sel: &Selector) -> u32 {
    let mut max_page = 1u32;
    for el in document.select(page_sel) {
        let n = parse_int(&el.text().collect::<String>());
        if n > max_page {
            max_page = n;
        }
    }
    max_page
}

fn parse_search_tbody(tbody: ElementRef<'_>, urls: &ForumUrls) -> Option<ListItem> {
    let title_link = tbody.select(&selector("tr th.subject a").ok()?).next()?;
    let href = title_link.value().attr("href").unwrap_or_default();
    let tid = extract_param(href, "tid=", "&");
    if tid.is_empty() {
        return None;
    }

    let author_link = tbody
        .select(&selector("tr td.author cite a").ok()?)
        .next()?;
    let author = author_link.text().collect::<String>();
    let space_url = author_link.value().attr("href").unwrap_or_default();
    let uid = extract_param(space_url, "uid=", "&");
    let avatar_url = urls.avatar_by_uid(&uid);

    let time = tbody
        .select(&selector("tr td.author em").ok()?)
        .next()
        .map(|el| el.text().collect::<String>());

    let forum = tbody
        .select(&selector("tr td.forum").ok()?)
        .next()
        .map(|el| el.text().collect::<String>());

    Some(ListItem {
        tid: Some(tid),
        pid: None,
        uid: (!uid.is_empty()).then_some(uid),
        title: Some(title_link.text().collect::<String>()),
        author: Some(author),
        avatar_url,
        forum,
        time,
        info: None,
        is_new: false,
    })
}

fn parse_fulltext_row(row: ElementRef<'_>, urls: &ForumUrls) -> Option<ListItem> {
    let title_link = row.select(&selector("div.sp_title a").ok()?).next()?;
    let post_url = title_link.value().attr("href").unwrap_or_default();
    let pid = extract_param(post_url, "pid=", "&");
    if pid.is_empty() {
        return None;
    }

    let info = row
        .select(&selector("div.sp_content").ok()?)
        .next()
        .map(|el| el.text().collect::<String>());

    let spans: Vec<_> = row.select(&selector("div.sp_theard span").ok()?).collect();
    if spans.len() != 5 {
        return None;
    }

    let author_link = spans[1].select(&selector("a").ok()?).next()?;
    let author = author_link.text().collect::<String>();
    let space_url = author_link.value().attr("href").unwrap_or_default();
    let uid = extract_param(space_url, "uid=", "&");
    let avatar_url = urls.avatar_by_uid(&uid);

    let time_text = spans[4].text().collect::<String>();
    let time = extract_param(&time_text, ":", "&");
    let time = if time.is_empty() {
        time_text.trim().to_string()
    } else {
        time.trim().to_string()
    };

    let forum = spans[0]
        .select(&selector("a").ok()?)
        .next()
        .map(|el| el.text().collect::<String>());

    Some(ListItem {
        tid: None,
        pid: Some(pid),
        uid: (!uid.is_empty()).then_some(uid),
        title: Some(title_link.text().collect::<String>()),
        author: Some(author),
        avatar_url,
        forum,
        time: Some(time),
        info,
        is_new: false,
    })
}

fn parse_my_thread_row(row: ElementRef<'_>) -> Option<ListItem> {
    let th = row.select(&selector("th").ok()?).next()?;
    let links: Vec<_> = th.select(&selector("a").ok()?).collect();
    if links.len() != 1 {
        return None;
    }
    let link = links[0];
    let href = link.value().attr("href").unwrap_or_default();
    if !href.contains("viewthread.php?tid=") {
        return None;
    }
    let tid = extract_param(href, "viewthread.php?tid=", "&");

    let time = row
        .select(&selector("td.lastpost").ok()?)
        .next()
        .map(|el| el.text().collect::<String>())?;

    let forum = row
        .select(&selector("td.forum").ok()?)
        .next()
        .map(|el| el.text().collect::<String>());

    Some(ListItem {
        tid: Some(tid),
        pid: None,
        uid: None,
        title: Some(link.text().collect::<String>()),
        author: None,
        avatar_url: None,
        forum,
        time: Some(time),
        info: None,
        is_new: false,
    })
}

fn parse_my_reply_title_row(row: ElementRef<'_>) -> Option<ListItem> {
    let th = row.select(&selector("th").ok()?).next()?;
    let links: Vec<_> = th.select(&selector("a").ok()?).collect();
    if links.len() != 1 {
        return None;
    }
    let link = links[0];
    let href = link.value().attr("href").unwrap_or_default();
    if !href.contains("redirect.php?goto=") {
        return None;
    }

    let time = row
        .select(&selector("td.lastpost").ok()?)
        .next()
        .map(|el| el.text().collect::<String>())?;

    let forum = row
        .select(&selector("td.forum").ok()?)
        .next()
        .map(|el| el.text().collect::<String>());

    Some(ListItem {
        tid: Some(extract_param(href, "ptid=", "&")),
        pid: Some(extract_param(href, "pid=", "&")),
        uid: None,
        title: Some(link.text().collect::<String>()),
        author: None,
        avatar_url: None,
        forum,
        time: Some(time),
        info: None,
        is_new: false,
    })
}

fn parse_favorite_row(row: ElementRef<'_>) -> Option<ListItem> {
    let th = row.select(&selector("th").ok()?).next()?;
    let title = th.text().collect::<String>();
    let link = th.select(&selector("a").ok()?).next()?;
    let href = link.value().attr("href").unwrap_or_default();
    let tid = extract_param(href, "tid=", "&");
    if tid.is_empty() {
        return None;
    }

    let time = row
        .select(&selector("td.lastpost").ok()?)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string());

    let forum = row
        .select(&selector("td.forum").ok()?)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string());

    Some(ListItem {
        tid: Some(tid),
        pid: None,
        uid: None,
        title: Some(title),
        author: None,
        avatar_url: None,
        forum,
        time,
        info: None,
        is_new: false,
    })
}

fn parse_pm_list_item(li: ElementRef<'_>, urls: &ForumUrls) -> Option<ListItem> {
    let avatar_url = li
        .select(&selector("a.avatar img").ok()?)
        .next()
        .and_then(|img| img.value().attr("src"))
        .map(str::to_string);

    let pcite = li.select(&selector("p.cite").ok()?).next()?;
    let cite = pcite.select(&selector("cite").ok()?).next()?;
    let author = cite.text().collect::<String>();
    let uid_link = cite.select(&selector("a").ok()?).next()?;
    let uid = extract_param(
        uid_link.value().attr("href").unwrap_or_default(),
        "uid=",
        "&",
    );
    let time = pcite
        .text()
        .collect::<String>()
        .replace(&author, "")
        .trim()
        .to_string();

    let title = li
        .select(&selector("div.summary").ok()?)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .filter(|t| !t.is_empty());

    let is_new = pcite
        .select(&selector("img").ok()?)
        .next()
        .and_then(|img| img.value().attr("src"))
        .map(|src| src.contains(NEW_PM_IMAGE))
        .unwrap_or(false);

    Some(ListItem {
        tid: None,
        pid: None,
        uid: Some(uid),
        title,
        author: Some(author),
        avatar_url: avatar_url.or_else(|| {
            urls.avatar_by_uid(&extract_param(
                uid_link.value().attr("href").unwrap_or_default(),
                "uid=",
                "&",
            ))
        }),
        forum: None,
        time: Some(time),
        info: None,
        is_new,
    })
}

fn parse_my_identity(document: &Html) -> Result<(String, String), AdapterError> {
    let link = document
        .select(&selector("#umenu cite a.noborder")?)
        .next()
        .ok_or_else(|| AdapterError::Parse("pm thread identity not found".into()))?;
    let href = link.value().attr("href").unwrap_or_default();
    let uid = extract_param(href, "uid=", "&");
    let username = link.text().collect::<String>();
    Ok((uid, username))
}

fn parse_pm_thread_item(
    li: ElementRef<'_>,
    my_uid: &str,
    my_username: &str,
    urls: &ForumUrls,
) -> Option<ListItem> {
    let pcite = li.select(&selector("p.cite").ok()?).next()?;
    let cite = pcite.select(&selector("cite").ok()?).next()?;
    let author = cite.text().collect::<String>();
    let time = pcite
        .text()
        .collect::<String>()
        .replace(&author, "")
        .trim()
        .to_string();

    let uid = if author == my_username {
        my_uid.to_string()
    } else {
        li.select(&selector("a.avatar").ok()?)
            .next()
            .and_then(|a| a.value().attr("href"))
            .map(|href| extract_param(href, "uid=", "&"))
            .unwrap_or_default()
    };

    let summary = li.select(&selector("div.summary").ok()?).next()?;
    let info = summary.html();
    let title = {
        let text = summary.text().collect::<String>().trim().to_string();
        (!text.is_empty()).then_some(text)
    };

    let is_new = pcite
        .select(&selector("img").ok()?)
        .next()
        .and_then(|img| img.value().attr("src"))
        .map(|src| src.contains(NEW_PM_IMAGE))
        .unwrap_or(false);

    let avatar_url = urls.avatar_by_uid(&uid);
    Some(ListItem {
        tid: None,
        pid: None,
        uid: (!uid.is_empty()).then_some(uid),
        title,
        author: Some(author),
        avatar_url,
        forum: None,
        time: Some(time),
        info: Some(info),
        is_new,
    })
}

fn parse_notify_thread(div: ElementRef<'_>) -> Option<ListItem> {
    let mut info = String::new();
    let mut title = None;
    let mut tid = None;
    let mut pid = None;

    for link in div.select(&selector("a").ok()?).collect::<Vec<_>>() {
        let href = link.value().attr("href").unwrap_or_default();
        if href.contains("space.php") {
            info.push_str(&link.text().collect::<String>());
            info.push(' ');
        } else if href.contains("redirect.php?") {
            title = Some(link.text().collect::<String>());
            tid = Some(extract_param(href, "ptid=", "&"));
            pid = Some(extract_param(href, "pid=", "&"));
            break;
        }
    }

    let time = div
        .select(&selector("em").ok()?)
        .next()?
        .text()
        .collect::<String>();
    title.as_ref()?;

    if div
        .text()
        .collect::<String>()
        .contains("回复了您关注的主题")
    {
        info.push_str("回复了您关注的主题");
    } else {
        info.push_str("回复了您的帖子 ");
    }

    Some(ListItem {
        tid,
        pid,
        uid: None,
        title,
        author: None,
        avatar_url: None,
        forum: None,
        time: Some(time),
        info: Some(info),
        is_new: true,
    })
}

fn parse_notify_quote_and_reply(div: ElementRef<'_>, urls: &ForumUrls) -> Option<ListItem> {
    let mut item = ListItem {
        tid: None,
        pid: None,
        uid: None,
        title: None,
        author: None,
        avatar_url: None,
        forum: None,
        time: None,
        info: None,
        is_new: false,
    };

    for link in div.select(&selector("a").ok()?).collect::<Vec<_>>() {
        let href = link.value().attr("href").unwrap_or_default();
        if href.contains("space.php") {
            let uid = extract_param(href, "uid=", "&");
            item.author = Some(link.text().collect::<String>());
            item.avatar_url = urls.avatar_by_uid(&uid);
            item.uid = (!uid.is_empty()).then_some(uid);
        } else if href.contains("viewthread.php") {
            item.title = Some(link.text().collect::<String>());
            item.tid = Some(extract_param(href, "tid=", "&"));
        } else if href.contains("redirect.php") {
            item.tid = Some(extract_param(href, "ptid=", "&"));
            item.pid = Some(extract_param(href, "pid=", "&"));
        }
    }

    item.time = div
        .select(&selector("em").ok()?)
        .next()
        .map(|el| el.text().collect::<String>());
    item.time.as_ref()?;

    let info = if let Some(summary) = div.select(&selector(".summary").ok()?).next() {
        let dds: Vec<_> = summary.select(&selector("dd").ok()?).collect();
        if dds.len() == 2 {
            let author = item.author.as_deref().unwrap_or("?");
            format!(
                "<u>您的帖子:</u>{}\n<br><u>{} 说:</u>{}",
                dds[0].text().collect::<String>(),
                author,
                dds[1].text().collect::<String>()
            )
        } else {
            summary.text().collect::<String>()
        }
    } else {
        String::new()
    };
    item.info = Some(info);

    item.is_new = div
        .select(&selector("img").ok()?)
        .next()
        .and_then(|img| img.value().attr("src"))
        .map(|src| src.contains(NEW_PM_IMAGE))
        .unwrap_or(false);

    Some(item)
}

fn parse_system_info(div: ElementRef<'_>) -> Option<ListItem> {
    let tid = div.select(&selector("a").ok()?).next().and_then(|a| {
        let href = a.value().attr("href").unwrap_or_default();
        let tid = extract_param(href, "tid=", "&");
        (!tid.is_empty()).then_some(tid)
    });

    let is_new = div
        .select(&selector("img").ok()?)
        .next()
        .and_then(|img| img.value().attr("src"))
        .map(|src| src.contains(NEW_PM_IMAGE))
        .unwrap_or(false);

    Some(ListItem {
        tid,
        pid: None,
        uid: None,
        title: Some("系统信息".into()),
        author: None,
        avatar_url: None,
        forum: None,
        time: None,
        info: Some(div.text().collect::<String>()),
        is_new,
    })
}

fn parse_friend_info(div: ElementRef<'_>, urls: &ForumUrls) -> Option<ListItem> {
    let link = div.select(&selector("a").ok()?).next()?;
    let href = link.value().attr("href").unwrap_or_default();
    let uid = extract_param(href, "uid=", "&");

    let is_new = div
        .select(&selector("img").ok()?)
        .next()
        .and_then(|img| img.value().attr("src"))
        .map(|src| src.contains(NEW_PM_IMAGE))
        .unwrap_or(false);

    let avatar_url = urls.avatar_by_uid(&uid);
    Some(ListItem {
        tid: None,
        pid: None,
        uid: (!uid.is_empty()).then_some(uid),
        title: Some("好友信息".into()),
        author: Some(link.text().collect::<String>()),
        avatar_url,
        forum: None,
        time: None,
        info: Some(div.text().collect::<String>()),
        is_new,
    })
}

fn selector(sel: &str) -> Result<Selector, AdapterError> {
    Selector::parse(sel).map_err(|e| AdapterError::Parse(format!("invalid selector: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_search_title_fixture() {
        let html = include_str!("../../tests/fixtures/search_title_p1.html");
        let urls = ForumUrls::default_4d4y();
        let list = parse_search_title(html, 1, &urls).expect("parse search");

        assert_eq!(list.search_id.as_deref(), Some("12345"));
        assert_eq!(list.max_page, 2);
        assert_eq!(list.items.len(), 1);
        assert_eq!(list.items[0].tid.as_deref(), Some("448060"));
    }

    #[test]
    fn parse_my_threads_fixture() {
        let html = include_str!("../../tests/fixtures/my_threads_p1.html");
        let list = parse_my_threads(html, 1).expect("parse my threads");

        assert_eq!(list.items.len(), 1);
        assert_eq!(list.items[0].tid.as_deref(), Some("100001"));
    }

    #[test]
    fn parse_my_replies_fixture() {
        let html = include_str!("../../tests/fixtures/my_replies_p1.html");
        let list = parse_my_replies(html, 1).expect("parse my replies");

        assert_eq!(list.items.len(), 1);
        assert_eq!(list.items[0].pid.as_deref(), Some("200002"));
        assert_eq!(list.items[0].info.as_deref(), Some("my reply text"));
    }

    #[test]
    fn parse_favorites_fixture() {
        let html = include_str!("../../tests/fixtures/favorites_p1.html");
        let list = parse_favorites(html, 1).expect("parse favorites");

        assert_eq!(list.items.len(), 1);
        assert_eq!(list.items[0].tid.as_deref(), Some("300003"));
    }

    #[test]
    fn parse_pm_list_fixture() {
        let html = include_str!("../../tests/fixtures/pm_list.html");
        let urls = ForumUrls::default_4d4y();
        let list = parse_pm_list(html, &urls).expect("parse pm list");

        assert_eq!(list.items.len(), 1);
        assert_eq!(list.items[0].uid.as_deref(), Some("50005"));
        assert!(list.items[0].is_new);
    }

    #[test]
    fn parse_pm_thread_fixture() {
        let html = include_str!("../../tests/fixtures/pm_thread.html");
        let urls = ForumUrls::default_4d4y();
        let list = parse_pm_thread(html, &urls).expect("parse pm thread");

        assert_eq!(list.items.len(), 2);
        assert!(list.items[0]
            .info
            .as_ref()
            .is_some_and(|s| s.contains("hello")));
        assert_eq!(list.items[0].title.as_deref(), Some("hello from bob"));
    }

    #[test]
    fn parse_check_new_pm_empty() {
        let body = include_str!("../../tests/fixtures/pm_check_new_empty.xml");
        assert!(!parse_check_new_pm(body).expect("parse empty checknewpm"));
    }

    #[test]
    fn parse_pm_new_list_empty() {
        let html = include_str!("../../tests/fixtures/pm_new_empty.html");
        let urls = ForumUrls::default_4d4y();
        let list = parse_pm_new_list(html, &urls).expect("parse empty new pm list");
        assert!(list.items.is_empty());
    }

    #[test]
    fn parse_pm_list_empty() {
        let html = include_str!("../../tests/fixtures/pm_new_empty.html");
        let urls = ForumUrls::default_4d4y();
        let list = parse_pm_list(html, &urls).expect("parse empty pm list");
        assert!(list.items.is_empty());
    }

    #[test]
    fn parse_pm_thread_empty() {
        let html = include_str!("../../tests/fixtures/pm_thread_empty.html");
        let urls = ForumUrls::default_4d4y();
        let list = parse_pm_thread(html, &urls).expect("parse empty pm thread");
        assert!(list.items.is_empty());
    }

    #[test]
    fn parse_notifications_fixture() {
        let html = include_str!("../../tests/fixtures/notifications.html");
        let urls = ForumUrls::default_4d4y();
        let list = parse_notifications(html, &urls).expect("parse notifications");

        assert!(list.items.len() >= 2);
        assert!(list
            .items
            .iter()
            .any(|i| i.title.as_deref() == Some("系统信息")));
    }
}
