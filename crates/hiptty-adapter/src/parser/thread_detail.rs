use hiptty_core::{AdapterError, Poll, PollOption, Post, ThreadDetail};
use scraper::{ElementRef, Html, Selector};

use crate::http::urls::ForumUrls;
use crate::parser::common::{extract_param, parse_int, parse_size_text};
use crate::parser::content::{parse_block_image, parse_content};
use hiptty_core::ContentNode;

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
        .map(|el| el.text().collect::<String>())?;

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

    if let Some(content_el) = post_el.select(&content_sel).next() {
        content = parse_content(content_el, urls);
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
                    bold: false,
                    italic: false,
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
                    bold: false,
                    italic: false,
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
    })
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
            assert!(!imgs.is_empty(), "floor {floor} should have inline images: {:?}", post.content);
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
        assert!(!img_urls.is_empty(), "expected attachment images on first floor");
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
