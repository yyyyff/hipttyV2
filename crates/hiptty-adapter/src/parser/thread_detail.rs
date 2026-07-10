use hiptty_core::{AdapterError, ContentNode, ContentSpan, Poll, PollOption, Post, ThreadDetail};
use scraper::{ElementRef, Html, Selector};

use crate::http::urls::ForumUrls;
use crate::parser::common::{extract_param, parse_int, parse_size_text};
use crate::parser::content::{parse_block_image, parse_content};

pub fn parse(html: &str, tid: &str, urls: &ForumUrls) -> Result<ThreadDetail, AdapterError> {
    let document = Html::parse_document(html);

    let (page, last_page) = parse_pagination(&document)?;
    let (title, fid) = parse_nav(&document)?;
    let poll = parse_poll(&document);

    let postlist_sel = Selector::parse("div#postlist > div[id^='post_']")
        .map_err(|e| AdapterError::Parse(e.to_string()))?;

    let mut posts = Vec::new();
    for post_el in document.select(&postlist_sel) {
        let Some(post) = parse_post(post_el, page, poll.as_ref(), urls) else {
            continue;
        };
        posts.push(post);
    }

    if posts.is_empty() {
        return Err(AdapterError::Parse("no posts found in thread page".into()));
    }

    Ok(ThreadDetail {
        tid: tid.to_string(),
        fid,
        title,
        posts,
        page,
        last_page,
    })
}

fn parse_pagination(document: &Html) -> Result<(u32, u32), AdapterError> {
    let pages_sel = Selector::parse("div#wrap div.forumcontrol div.pages")
        .map_err(|e| AdapterError::Parse(e.to_string()))?;

    let mut last_page = 1u32;
    let mut page = 1u32;

    if let Some(pages) = document.select(&pages_sel).next() {
        for child in pages.children() {
            if let scraper::Node::Element(el) = child.value() {
                let n = ElementRef::wrap(child)
                    .map(|el| parse_int(&el.text().collect::<String>()))
                    .unwrap_or(0);
                if n > last_page {
                    last_page = n;
                }
                if el.name() == "strong" {
                    page = n.max(1);
                }
            }
        }
    }

    Ok((page, last_page))
}

fn parse_nav(document: &Html) -> Result<(String, Option<u32>), AdapterError> {
    let nav_sel = Selector::parse("div#nav").map_err(|e| AdapterError::Parse(e.to_string()))?;

    let mut fid = None;
    let mut title = String::new();

    if let Some(nav) = document.select(&nav_sel).next() {
        for a in nav.select(&Selector::parse("a").unwrap()) {
            let href = a.value().attr("href").unwrap_or_default();
            if href.contains("fid=") {
                let f = parse_int(&extract_param(href, "fid=", "&"));
                if f > 0 {
                    fid = Some(f);
                }
            }
        }
    }

    if let Some(h1) = document
        .select(&Selector::parse("div#threadtitle h1").unwrap())
        .next()
    {
        title = h1.text().collect::<String>().trim().to_string();
    }

    if title.is_empty() {
        if let Some(nav) = document.select(&nav_sel).next() {
            let mut nav_text = nav.text().collect::<String>();
            nav_text = nav_text.replace('»', ">").trim().to_string();
            if let Some((_prefix, last)) = nav_text.rsplit_once('>') {
                title = last.trim().to_string();
            } else if !nav_text.is_empty() {
                title = nav_text;
            }
        }
    }

    if title.is_empty() {
        return Err(AdapterError::Parse("thread title not found".into()));
    }

    Ok((title, fid))
}

fn parse_poll(document: &Html) -> Option<Poll> {
    let poll_form = document
        .select(&Selector::parse("form#poll").unwrap())
        .next()?;
    let title_el = poll_form
        .select(&Selector::parse("div.pollinfo").unwrap())
        .next()?;
    let mut title = title_el.text().collect::<String>().trim().to_string();

    if let Some(timer) = poll_form
        .select(&Selector::parse("p.polltimer").unwrap())
        .next()
    {
        title.push('\n');
        title.push_str(&timer.text().collect::<String>());
    }

    let mut max_answers = 1u32;
    if title.contains("多选投票") {
        let part = extract_param(&title, "最多可选", "项");
        let n = parse_int(&part);
        if n > 1 {
            max_answers = n;
        }
    }

    let mut options = Vec::new();
    for row in poll_form.select(&Selector::parse("div.pollchart > table > tbody > tr").unwrap()) {
        if let Some(label) = row
            .select(&Selector::parse("td.polloption label").unwrap())
            .next()
        {
            let id = row
                .select(&Selector::parse("td.selector input").unwrap())
                .next()
                .and_then(|input| input.value().attr("value"))
                .unwrap_or_default()
                .to_string();
            options.push(PollOption {
                id,
                label: label.text().collect(),
                votes: None,
                percent: None,
            });
        } else if let Some(last_td) = row.select(&Selector::parse("td").unwrap()).last() {
            if let Some(last) = options.last_mut() {
                let rate_text = last_td.text().collect::<String>();
                if last_td
                    .select(&Selector::parse("em").unwrap())
                    .next()
                    .is_some()
                {
                    last.percent = Some(rate_text.trim().to_string());
                }
            }
        }
    }

    let footer = poll_form
        .select(&Selector::parse("div.pollchart > table > tbody > tr").unwrap())
        .last()
        .and_then(|row| row.select(&Selector::parse("td").unwrap()).last())
        .map(|td| td.text().collect::<String>().trim().to_string())
        .filter(|s| !s.is_empty());

    Some(Poll {
        title,
        footer,
        max_answers,
        options,
    })
}

fn parse_post(
    post_el: ElementRef<'_>,
    page: u32,
    thread_poll: Option<&Poll>,
    urls: &ForumUrls,
) -> Option<Post> {
    let id = post_el.value().attr("id")?;
    let pid = id.strip_prefix("post_")?.to_string();

    let time = post_el
        .select(
            &Selector::parse("td.postcontent div.postinfo div.posterinfo div.authorinfo em")
                .unwrap(),
        )
        .next()
        .map(|el| normalize_post_time(&el.text().collect::<String>()))?;

    let floor = post_el
        .select(&Selector::parse("td.postcontent div.postinfo strong a em").unwrap())
        .next()
        .map(|el| parse_int(&el.text().collect::<String>()))
        .filter(|&f| f > 0)?;

    let warned = post_el
        .select(&Selector::parse("td.postcontent span.postratings a").unwrap())
        .next()
        .and_then(|a| a.value().attr("href"))
        .map(|href| href.contains("viewwarning"))
        .unwrap_or(false);

    let author_link = post_el
        .select(&Selector::parse("td.postauthor div.postinfo a").unwrap())
        .next()?;
    let uid = extract_param(
        author_link.value().attr("href").unwrap_or_default(),
        "uid=",
        "&",
    );
    if uid.is_empty() {
        return None;
    }
    let author = author_link.text().collect::<String>();

    let content_sel = Selector::parse(
        "td.postcontent div.defaultpost div.postmessage div.t_msgfontfix table tbody tr td.t_msgfont",
    )
    .unwrap();

    let mut content = Vec::new();
    let mut poll = None;

    if floor == 1 {
        poll = thread_poll.cloned();
    }

    let mut edited_by = None;
    let mut edited_at = None;

    if let Some(content_el) = post_el.select(&content_sel).next() {
        content = parse_content(content_el, urls);
        if let Some((by, at)) = extract_and_strip_edit_notice(&mut content) {
            edited_by = Some(by);
            edited_at = Some(at);
        }
        append_attachments(post_el, urls, &mut content);
    } else if let Some(locked) = post_el
        .select(
            &Selector::parse("td.postcontent div.defaultpost div.postmessage div.locked").unwrap(),
        )
        .next()
    {
        content.push(hiptty_core::ContentNode::Text {
            spans: vec![hiptty_core::ContentSpan::Text {
                text: locked.text().collect(),
                style: hiptty_core::Style {
                    fg: Some("gray".into()),
                    ..Default::default()
                },
                url: None,
            }],
        });
    } else if floor == 1 && thread_poll.is_some() {
        content.push(hiptty_core::ContentNode::Text {
            spans: vec![hiptty_core::ContentSpan::Text {
                text: "[投票主题]".into(),
                style: hiptty_core::Style::default(),
                url: None,
            }],
        });
    } else {
        content.push(hiptty_core::ContentNode::Text {
            spans: vec![hiptty_core::ContentSpan::Text {
                text: "[[无法解析帖子内容]]".into(),
                style: hiptty_core::Style {
                    fg: Some("gray".into()),
                    ..Default::default()
                },
                url: None,
            }],
        });
    }

    if content.is_empty() {
        content.push(hiptty_core::ContentNode::Text {
            spans: vec![hiptty_core::ContentSpan::Text {
                text: "[[无内容]]".into(),
                style: hiptty_core::Style::default(),
                url: None,
            }],
        });
    }

    let signature = post_el
        .select(&Selector::parse("td.postbottom div.signatures").unwrap())
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .filter(|s| !s.is_empty());

    Some(Post {
        pid,
        floor,
        author,
        uid: Some(uid.clone()),
        avatar_url: urls.avatar_by_uid(&uid),
        time,
        content,
        poll,
        page,
        warned,
        signature,
        edited_by,
        edited_at,
    })
}

/// Discuz `<em>发表于 2026-6-7 13:13</em>` → bare datetime for clients.
fn normalize_post_time(raw: &str) -> String {
    let t = raw.trim();
    t.strip_prefix("发表于")
        .map(str::trim)
        .unwrap_or(t)
        .to_string()
}

/// Pull Discuz `本帖最后由 AUTHOR 于 TIME 编辑` out of body into structured fields.
///
/// Appears as prefix (glued to body) or suffix (`[<i>…</i>]` at end of post).
fn extract_and_strip_edit_notice(content: &mut Vec<ContentNode>) -> Option<(String, String)> {
    // Prefer first text node (prefix glued to content), then last (classic suffix).
    let indices: Vec<usize> = content
        .iter()
        .enumerate()
        .filter_map(|(i, n)| matches!(n, ContentNode::Text { .. }).then_some(i))
        .collect();
    if indices.is_empty() {
        return None;
    }

    let try_order = {
        let mut v = Vec::new();
        if let Some(&first) = indices.first() {
            v.push(first);
        }
        if let Some(&last) = indices.last() {
            if Some(last) != indices.first().copied() {
                v.push(last);
            }
        }
        v
    };

    for idx in try_order {
        let ContentNode::Text { spans } = &mut content[idx] else {
            continue;
        };
        if let Some((by, at, new_spans)) = strip_edit_notice_from_spans(spans) {
            if new_spans.is_empty()
                || new_spans.iter().all(|s| match s {
                    ContentSpan::Text { text, .. } => text.trim().is_empty(),
                    ContentSpan::Smiley { .. } => false,
                })
            {
                content.remove(idx);
            } else {
                *spans = new_spans;
            }
            return Some((by, at));
        }
    }
    None
}

fn strip_edit_notice_from_spans(
    spans: &[ContentSpan],
) -> Option<(String, String, Vec<ContentSpan>)> {
    // Never flatten Smiley spans away. Edit notices sit in text runs; smileys (often
    // right after a prefix notice) must remain ContentSpan::Smiley.
    if let Some(result) = strip_edit_in_leading_text_run(spans) {
        return Some(result);
    }
    if let Some(result) = strip_edit_in_trailing_text_run(spans) {
        return Some(result);
    }
    // Text-only node (classic `[本帖最后由…]` suffix with no smileys).
    if spans.iter().all(|s| matches!(s, ContentSpan::Text { .. })) {
        let mut flat = String::new();
        for span in spans {
            if let ContentSpan::Text { text, .. } = span {
                flat.push_str(text);
            }
        }
        let (by, at, before, after) = split_edit_notice(&flat)?;
        let style = spans
            .iter()
            .find_map(|s| match s {
                ContentSpan::Text { style, .. } => Some(style.clone()),
                _ => None,
            })
            .unwrap_or_default();
        let text = join_before_after(&before, &after);
        let new_spans = if text.is_empty() {
            Vec::new()
        } else {
            vec![ContentSpan::Text {
                text,
                style,
                url: None,
            }]
        };
        return Some((by, at, new_spans));
    }
    None
}

/// Prefix case: `本帖最后由…编辑` then smileys/body in the same Text node.
fn strip_edit_in_leading_text_run(
    spans: &[ContentSpan],
) -> Option<(String, String, Vec<ContentSpan>)> {
    let mut lead_end = 0usize;
    let mut lead_flat = String::new();
    for (i, span) in spans.iter().enumerate() {
        match span {
            ContentSpan::Text { text, .. } => {
                lead_flat.push_str(text);
                lead_end = i + 1;
            }
            ContentSpan::Smiley { .. } => break,
        }
    }
    if lead_end == 0 {
        return None;
    }
    let (by, at, before, after) = split_edit_notice(&lead_flat)?;
    // Only treat as a leading-run hit when the notice is not purely a trailing
    // bracketed footer of a longer text-only node handled elsewhere — always OK.
    let style = spans[..lead_end]
        .iter()
        .find_map(|s| match s {
            ContentSpan::Text { style, .. } => Some(style.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let text = join_before_after(&before, &after);
    let mut new_spans = Vec::new();
    if !text.is_empty() {
        new_spans.push(ContentSpan::Text {
            text,
            style,
            url: None,
        });
    }
    new_spans.extend(spans[lead_end..].iter().cloned());
    Some((by, at, new_spans))
}

/// Suffix case: body/smileys then trailing text `本帖最后由…编辑`.
fn strip_edit_in_trailing_text_run(
    spans: &[ContentSpan],
) -> Option<(String, String, Vec<ContentSpan>)> {
    if spans.is_empty() {
        return None;
    }
    let mut trail_start = spans.len();
    let mut trail_flat = String::new();
    for i in (0..spans.len()).rev() {
        match &spans[i] {
            ContentSpan::Text { text, .. } => {
                trail_flat = format!("{text}{trail_flat}");
                trail_start = i;
            }
            ContentSpan::Smiley { .. } => break,
        }
    }
    if trail_start >= spans.len() {
        return None;
    }
    // Avoid double-applying the same leading run.
    if trail_start == 0 {
        return None;
    }
    let (by, at, before, after) = split_edit_notice(&trail_flat)?;
    let style = spans[trail_start..]
        .iter()
        .find_map(|s| match s {
            ContentSpan::Text { style, .. } => Some(style.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let text = join_before_after(&before, &after);
    let mut new_spans = spans[..trail_start].to_vec();
    if !text.is_empty() {
        new_spans.push(ContentSpan::Text {
            text,
            style,
            url: None,
        });
    }
    Some((by, at, new_spans))
}

fn join_before_after(before: &str, after: &str) -> String {
    let mut text = String::new();
    if !before.is_empty() {
        text.push_str(before);
    }
    if !before.is_empty()
        && !after.is_empty()
        && !before.ends_with('\n')
        && !after.starts_with('\n')
    {
        text.push('\n');
    }
    text.push_str(after);
    text.trim().to_string()
}

/// Split `…[ 本帖最后由 A 于 T 编辑 ]…` → (author, time, before, after).
fn split_edit_notice(s: &str) -> Option<(String, String, String, String)> {
    const MARK: &str = "本帖最后由";
    let idx = s.find(MARK)?;
    let after_mark = s[idx + MARK.len()..].trim_start();
    let (author, rest) = after_mark.split_once(" 于 ")?;
    let author = author.trim();
    if author.is_empty() || author.chars().count() > 30 {
        return None;
    }
    let rest = rest.trim_start();
    let (time, rest) = rest.split_once("编辑")?;
    let time = time.trim();
    if time.is_empty() {
        return None;
    }
    let mut before = s[..idx].to_string();
    let mut after = rest.to_string();
    // Optional surrounding brackets Discuz wraps the notice with.
    before = before
        .trim_end_matches(|c: char| c == '[' || c.is_whitespace())
        .to_string();
    after = after
        .trim_start_matches(|c: char| c == ']' || c.is_whitespace())
        .to_string();
    Some((author.to_string(), time.to_string(), before, after))
}

#[cfg(test)]
mod edit_notice_tests {
    use super::*;

    #[test]
    fn split_prefix_edit_glued_to_body() {
        let s = "本帖最后由 yalelynn 于 2026-6-20 16:18 编辑 ATTACHIMG-1 就看你了";
        let (by, at, before, after) = split_edit_notice(s).expect("parse");
        assert_eq!(by, "yalelynn");
        assert_eq!(at, "2026-6-20 16:18");
        assert!(before.is_empty());
        assert!(after.contains("ATTACHIMG"));
        assert!(after.contains("就看你了"));
    }

    #[test]
    fn split_bracketed_suffix_edit() {
        let s = "正文内容\n\n[ 本帖最后由 fbscell 于 2009-3-15 19:23 编辑 ]";
        let (by, at, before, after) = split_edit_notice(s).expect("parse");
        assert_eq!(by, "fbscell");
        assert_eq!(at, "2009-3-15 19:23");
        assert!(before.contains("正文内容"));
        assert!(after.trim().is_empty());
    }

    #[test]
    fn normalize_time_strips_published_label() {
        assert_eq!(
            normalize_post_time("发表于 2026-6-7 13:13"),
            "2026-6-7 13:13"
        );
        assert_eq!(normalize_post_time("2026-6-7 13:13"), "2026-6-7 13:13");
    }

    #[test]
    fn strip_edit_keeps_trailing_smileys() {
        let spans = vec![
            ContentSpan::Text {
                text: "本帖最后由 yalelynn 于 2026-6-16 21:24 编辑 ".into(),
                style: Default::default(),
                url: None,
            },
            ContentSpan::Smiley {
                url: "https://img02.4d4y.com/forum/images/smilies/default/lol.gif".into(),
                code: Some("default_lol".into()),
                smilie_id: Some("9".into()),
            },
            ContentSpan::Smiley {
                url: "https://img02.4d4y.com/forum/images/smilies/default/smile.gif".into(),
                code: Some("default_smile".into()),
                smilie_id: Some("1".into()),
            },
            ContentSpan::Text {
                text: " - DBer".into(),
                style: Default::default(),
                url: None,
            },
        ];
        let (by, at, out) = strip_edit_notice_from_spans(&spans).expect("strip");
        assert_eq!(by, "yalelynn");
        assert_eq!(at, "2026-6-16 21:24");
        assert_eq!(
            out.iter()
                .filter(|s| matches!(s, ContentSpan::Smiley { .. }))
                .count(),
            2,
            "smileys must survive edit-notice strip: {out:?}"
        );
        assert!(
            !out.iter().any(|s| matches!(
                s,
                ContentSpan::Text { text, .. } if text.contains("本帖最后由")
            )),
            "edit notice still present: {out:?}"
        );
        assert!(
            out.iter().any(|s| matches!(
                s,
                ContentSpan::Text { text, .. } if text.contains("DBer")
            )),
            "body tail lost: {out:?}"
        );
    }
}

fn append_attachments(
    post_el: ElementRef<'_>,
    urls: &ForumUrls,
    content: &mut Vec<hiptty_core::ContentNode>,
) {
    for dl in
        post_el.select(&Selector::parse("td.postcontent div.postattachlist dl.attachimg").unwrap())
    {
        let size = dl
            .select(&Selector::parse("em").unwrap())
            .next()
            .and_then(|em| parse_size_text(&em.text().collect::<String>()));
        if let Some(img) = dl.select(&Selector::parse("img").unwrap()).next() {
            let Some(ContentNode::Image { url, .. }) = parse_block_image(img, urls) else {
                continue;
            };
            if content.iter().any(|existing| {
                matches!(existing, ContentNode::Image { url: existing_url, .. } if existing_url == &url)
            }) {
                continue;
            }
            content.push(ContentNode::Image {
                url,
                thumb_url: None,
                size,
            });
        }
    }

    for attach in post_el.select(&Selector::parse("dl.t_attachlist p.attachname").unwrap()) {
        if let Some(link) = attach.select(&Selector::parse("a[href]").unwrap()).next() {
            let href = link.value().attr("href").unwrap_or_default();
            if href.starts_with("attachment.php?") {
                let size = attach
                    .select(&Selector::parse("em").unwrap())
                    .next()
                    .and_then(|em| parse_size_text(&em.text().collect::<String>()));
                content.push(hiptty_core::ContentNode::Attachment {
                    name: link.text().collect(),
                    url: format!("{}{}", urls.base, href.trim_start_matches('/')),
                    size,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_thread_detail_fixture() {
        let html = include_str!("../../tests/fixtures/thread_detail_448060_p1.html");
        let urls = ForumUrls::default_4d4y();
        let detail = parse(html, "448060", &urls).expect("parse thread detail");

        assert_eq!(detail.tid, "448060");
        assert!(!detail.title.is_empty());
        let with_sig = detail
            .posts
            .iter()
            .find(|p| p.signature.is_some())
            .expect("fixture should include at least one signature");
        assert!(!with_sig.signature.as_ref().unwrap().is_empty());
        let with_img_size = detail.posts.iter().find_map(|p| {
            p.content.iter().find_map(|node| match node {
                hiptty_core::ContentNode::Image { size, .. } if size.is_some() => Some(*size),
                _ => None,
            })
        });
        assert!(with_img_size.is_some());
        assert!(detail.posts.len() >= 5);
        assert!(detail.posts[0].floor == 1);
        assert!(!detail.posts[0].content.is_empty());
        assert!(detail.posts.iter().any(|p| p
            .content
            .iter()
            .any(|n| matches!(n, hiptty_core::ContentNode::Quote { .. }))));
        // Quote chrome owns author/time; body must not re-print the Discuz header line.
        for post in &detail.posts {
            for node in &post.content {
                if let hiptty_core::ContentNode::Quote {
                    author, time, text, ..
                } = node
                {
                    assert!(author.is_some(), "quote author on floor {}", post.floor);
                    assert!(time.is_some(), "quote time on floor {}", post.floor);
                    assert!(
                        !text.contains("原帖由") && !text.contains("发表于"),
                        "quote body still has header on floor {}: {text}",
                        post.floor
                    );
                }
            }
        }
        // Publish time is bare datetime (no duplicated 发表于 prefix).
        assert!(
            !detail.posts[0].time.contains("发表于"),
            "time still has 发表于: {}",
            detail.posts[0].time
        );
        // Edit notice is structured, not left in body.
        let with_edit = detail
            .posts
            .iter()
            .find(|p| p.edited_by.is_some())
            .expect("fixture has edited posts");
        assert!(with_edit.edited_at.is_some());
        for post in &detail.posts {
            for node in &post.content {
                if let hiptty_core::ContentNode::Text { spans } = node {
                    for span in spans {
                        if let hiptty_core::ContentSpan::Text { text, .. } = span {
                            assert!(
                                !text.contains("本帖最后由"),
                                "edit notice still in body floor {}: {text}",
                                post.floor
                            );
                        }
                    }
                }
            }
        }
        assert!(detail.posts.iter().any(|p| {
            p.content.iter().any(|n| {
                matches!(
                    n,
                    hiptty_core::ContentNode::Text { spans }
                        if spans.iter().any(|span| matches!(
                            span,
                            hiptty_core::ContentSpan::Smiley {
                                code: Some(code),
                                ..
                            } if code == "default_lol"
                        ))
                )
            })
        }));
    }

    #[test]
    fn thread_3455616_floor_images() {
        let path = "/tmp/thread_3455616_p1.html";
        if !std::path::Path::new(path).exists() {
            return;
        }
        let html = std::fs::read_to_string(path).expect("read thread fixture");
        let urls = ForumUrls::default_4d4y();
        let detail = parse(&html, "3455616", &urls).expect("parse thread detail");
        for floor in [7u32, 10] {
            let post = detail
                .posts
                .iter()
                .find(|p| p.floor == floor)
                .unwrap_or_else(|| panic!("missing floor {floor}"));
            let imgs: Vec<_> = post
                .content
                .iter()
                .filter_map(|n| match n {
                    hiptty_core::ContentNode::Image { url, thumb_url, .. } => {
                        Some((url.as_str(), thumb_url.as_deref()))
                    }
                    _ => None,
                })
                .collect();
            assert!(
                !imgs.is_empty(),
                "floor {floor} should have inline images: {:?}",
                post.content
            );
            for (url, thumb) in &imgs {
                assert!(
                    url.contains("img02.4d4y.com"),
                    "floor {floor} full url should stay on CDN: {url}"
                );
                if let Some(t) = thumb {
                    assert!(t.contains("img02.4d4y.com"), "floor {floor} thumb: {t}");
                }
            }
        }
    }

    #[test]
    fn attachment_images_use_file_url_not_placeholder() {
        let html = include_str!("../../tests/fixtures/thread_detail_448060_p1.html");
        let urls = ForumUrls::default_4d4y();
        let detail = parse(html, "448060", &urls).expect("parse thread detail");
        let first = &detail.posts[0];
        let img_urls: Vec<&str> = first
            .content
            .iter()
            .filter_map(|node| match node {
                hiptty_core::ContentNode::Image { url, .. } => Some(url.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            !img_urls.is_empty(),
            "expected attachment images on first floor"
        );
        assert!(
            img_urls.iter().any(|url| url.contains(".jpg")),
            "expected jpg attachment urls, got {img_urls:?}"
        );
        assert!(
            !img_urls.iter().any(|url| url.contains("none.gif")),
            "must not fetch placeholder gif, got {img_urls:?}"
        );
    }
}
