use hiptty_core::{AdapterError, AdapterResult, PostAction, PostResult, PrePostInfo};
use scraper::{Html, Selector};

use crate::http::urls::ForumUrls;
use crate::http::HttpClient;
use crate::parser::common::{extract_cdata, extract_param, parse_error_message, parse_html};
use crate::parser::pre_post::{self, PreparedPost};
use crate::parser::thread_detail;

pub async fn prepare_post(
    http: &HttpClient,
    urls: &ForumUrls,
    action: PostAction,
) -> AdapterResult<PrePostInfo> {
    let url = prepare_url(urls, &action);
    let html = http.get_text(&url).await?;
    Ok(pre_post::parse(&html, &action)?.info)
}

pub async fn post(
    http: &HttpClient,
    urls: &ForumUrls,
    action: PostAction,
    content: &str,
    subject: Option<&str>,
    delete: bool,
) -> AdapterResult<PostResult> {
    if crate::post_throttle::requires_throttle(&action) {
        crate::post_throttle::check_post_allowed()?;
    }
    let url = prepare_url(urls, &action);
    let html = http.get_text(&url).await?;
    let prepared = pre_post::parse(&html, &action)?;
    let resolved =
        crate::inline_images::resolve_inline_images(http, urls, &prepared.info, content).await?;
    submit_post(
        http,
        urls,
        &action,
        &prepared,
        &resolved.content,
        subject,
        delete,
        &resolved.new_attaches,
    )
    .await
}

pub async fn pm_delete(http: &HttpClient, urls: &ForumUrls, uid: &str) -> AdapterResult<()> {
    let body = http.get_text(&urls.pm_delete(uid)).await?;
    if let Some(err) = parse_error_message(&body) {
        return Err(AdapterError::ForumMessage(err));
    }
    Ok(())
}

pub async fn send_pm(
    http: &HttpClient,
    urls: &ForumUrls,
    uid: &str,
    content: &str,
) -> AdapterResult<()> {
    let prepare_url = format!("{}pm.php?daterange=1&uid={uid}", urls.base);
    let html = http.get_text(&prepare_url).await?;
    let formhash = parse_html(&html)
        .select(&Selector::parse("input#formhash").map_err(|e| AdapterError::Parse(e.to_string()))?)
        .next()
        .and_then(|el| el.value().attr("value"))
        .map(str::to_string)
        .ok_or_else(|| AdapterError::Parse("pm formhash not found".into()))?;

    let post_url = format!(
        "{}pm.php?action=send&pmsubmit=yes&infloat=yes&inajax=1&uid={uid}",
        urls.base
    );
    let params = [
        ("formhash", formhash.as_str()),
        ("lastdaterange", &timestamp_ms()),
        ("handlekey", "pmreply"),
        ("message", content),
    ];

    let body = http.post_form_gbk(&post_url, &params).await?;
    if body.contains("class=\"summary\"") {
        Ok(())
    } else {
        let message = parse_ajax_root_message(&body).unwrap_or_else(|| "短消息发送失败".into());
        Err(AdapterError::ForumMessage(message))
    }
}

pub async fn favorite_add(http: &HttpClient, urls: &ForumUrls, tid: &str) -> AdapterResult<()> {
    favorite_toggle(http, urls, "favorites", tid, true).await
}

pub async fn favorite_remove(http: &HttpClient, urls: &ForumUrls, tid: &str) -> AdapterResult<()> {
    favorite_toggle(http, urls, "favorites", tid, false).await
}

pub async fn blacklist_add(
    http: &HttpClient,
    urls: &ForumUrls,
    username: &str,
) -> AdapterResult<()> {
    let formhash = fetch_formhash_from_blacklist(http, urls).await?;
    let body = http
        .post_form_gbk(
            &format!("{}pm.php?action=addblack", urls.base),
            &[("formhash", formhash.as_str()), ("user", username)],
        )
        .await?;

    if let Some(err) = parse_error_message(&body) {
        return Err(AdapterError::ForumMessage(err));
    }
    Ok(())
}

pub async fn blacklist_remove(
    http: &HttpClient,
    urls: &ForumUrls,
    username: &str,
) -> AdapterResult<()> {
    let formhash = fetch_formhash_from_blacklist(http, urls).await?;
    let body = http
        .post_form_gbk(
            &format!("{}pm.php?action=delblack", urls.base),
            &[("formhash", formhash.as_str()), ("user", username)],
        )
        .await?;

    if let Some(err) = parse_error_message(&body) {
        return Err(AdapterError::ForumMessage(err));
    }
    Ok(())
}

pub async fn upload_image(
    http: &HttpClient,
    urls: &ForumUrls,
    action: PostAction,
    data: &[u8],
    filename: &str,
) -> AdapterResult<String> {
    let url = prepare_url(urls, &action);
    let html = http.get_text(&url).await?;
    let prepared = pre_post::parse(&html, &action)?;
    let uid = prepared
        .info
        .uid
        .as_deref()
        .ok_or_else(|| AdapterError::Parse("upload uid not found in post form".into()))?;
    let hash = prepared
        .info
        .hash
        .as_deref()
        .ok_or_else(|| AdapterError::Parse("upload hash not found in post form".into()))?;
    upload_image_bytes(http, urls, uid, hash, data, filename).await
}

pub async fn upload_image_bytes(
    http: &HttpClient,
    urls: &ForumUrls,
    uid: &str,
    hash: &str,
    data: &[u8],
    filename: &str,
) -> AdapterResult<String> {
    let upload_url = format!(
        "{}misc.php?action=swfupload&operation=upload&simple=1&type=image",
        urls.base
    );
    let response = http
        .post_multipart(
            &upload_url,
            &[("uid", uid), ("hash", hash)],
            "Filedata",
            filename,
            data,
        )
        .await?;

    parse_upload_response(&response)
}

fn prepare_url(urls: &ForumUrls, action: &PostAction) -> String {
    match action {
        PostAction::ReplyThread { tid } => {
            format!("{}post.php?action=reply&tid={tid}", urls.base)
        }
        PostAction::QuickDelete { tid, pid, fid } => {
            format!(
                "{}post.php?action=edit&fid={fid}&tid={tid}&pid={pid}&page=1",
                urls.base
            )
        }
        PostAction::ReplyPost { tid, pid } => {
            format!("{}post.php?action=reply&tid={tid}&reppost={pid}", urls.base)
        }
        PostAction::QuotePost { tid, pid } => {
            format!(
                "{}post.php?action=reply&tid={tid}&repquote={pid}",
                urls.base
            )
        }
        PostAction::NewThread { fid, .. } => {
            format!("{}post.php?action=newthread&fid={fid}", urls.base)
        }
        PostAction::EditPost {
            tid,
            pid,
            fid,
            page,
        } => {
            format!(
                "{}post.php?action=edit&fid={fid}&tid={tid}&pid={pid}&page={page}",
                urls.base
            )
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn submit_post(
    http: &HttpClient,
    urls: &ForumUrls,
    action: &PostAction,
    prepared: &PreparedPost,
    content: &str,
    subject: Option<&str>,
    delete: bool,
    new_attaches: &[String],
) -> AdapterResult<PostResult> {
    let (post_url, params) = build_post_params(
        urls,
        action,
        prepared,
        content,
        subject,
        delete,
        new_attaches,
    )?;

    let param_refs: Vec<(&str, &str)> = params
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let (body, final_url) = http.post_form_gbk_with_url(&post_url, &param_refs).await?;

    if delete && final_url.contains("forumdisplay.php") {
        return Ok(PostResult {
            success: true,
            message: "删除成功!".into(),
            tid: None,
            floor: None,
            detail: None,
        });
    }

    let tid = extract_param(&final_url, "tid=", "&");
    if (final_url.contains("viewthread.php") || final_url.contains("redirect.php"))
        && !tid.is_empty()
    {
        let detail = thread_detail::parse(&body, &tid, urls).ok();
        let mut message = "发表成功!".to_string();
        if delete {
            message = message.replace("发表", "删除");
        }
        crate::post_throttle::record_post_success(action, delete);
        return Ok(PostResult {
            success: true,
            message,
            tid: Some(tid),
            floor: None,
            detail,
        });
    }

    let mut message: String = if delete {
        "删除失败!".into()
    } else {
        "发表失败!".into()
    };
    if let Some(info) = parse_alert_info(&body) {
        message.push(' ');
        message.push_str(&info);
    }

    Ok(PostResult {
        success: false,
        message,
        tid: None,
        floor: None,
        detail: None,
    })
}

fn build_post_params(
    urls: &ForumUrls,
    action: &PostAction,
    prepared: &PreparedPost,
    content: &str,
    subject: Option<&str>,
    delete: bool,
    new_attaches: &[String],
) -> AdapterResult<(String, Vec<(String, String)>)> {
    let mut params = vec![
        ("formhash".into(), prepared.info.formhash.clone()),
        ("posttime".into(), timestamp_ms()),
        ("wysiwyg".into(), "0".into()),
        ("usesig".into(), "1".into()),
    ];
    for attach in new_attaches {
        params.push(("attachnew[][description]".into(), attach.clone()));
    }

    let (post_url, message) = match action {
        PostAction::ReplyThread { tid } => {
            let message = content.to_string();
            (
                format!(
                    "{}post.php?action=reply&tid={tid}&replysubmit=yes",
                    urls.base
                ),
                message,
            )
        }
        PostAction::ReplyPost { tid, .. } | PostAction::QuotePost { tid, .. } => {
            let quote = prepared.info.quote_text.clone().unwrap_or_default();
            let message = format!("{quote}\n\n    {content}");
            if let Some(author) = &prepared.notice_author {
                params.push(("noticeauthor".into(), author.clone()));
                params.push((
                    "noticeauthormsg".into(),
                    prepared.notice_author_msg.clone().unwrap_or_default(),
                ));
                params.push((
                    "noticetrimstr".into(),
                    prepared.notice_trim_str.clone().unwrap_or_default(),
                ));
            }
            (
                format!(
                    "{}post.php?action=reply&tid={tid}&replysubmit=yes",
                    urls.base
                ),
                message,
            )
        }
        PostAction::NewThread { fid, type_id } => {
            let type_id = type_id
                .clone()
                .or_else(|| prepared.info.type_id.clone())
                .unwrap_or_else(|| "0".into());
            let subject = subject
                .map(str::to_string)
                .or_else(|| prepared.info.subject.clone())
                .ok_or_else(|| AdapterError::InvalidInput("new thread requires subject".into()))?;
            params.push(("subject".into(), subject));
            params.push(("attention_add".into(), "1".into()));
            (
                format!(
                    "{}post.php?action=newthread&fid={fid}&typeid={type_id}&topicsubmit=yes",
                    urls.base
                ),
                content.to_string(),
            )
        }
        PostAction::EditPost {
            tid,
            pid,
            fid,
            page,
        } => {
            if let Some(subject) = subject.or(prepared.info.subject.as_deref()) {
                params.push(("subject".into(), subject.to_string()));
                if let Some(type_id) = &prepared.info.type_id {
                    params.push(("typeid".into(), type_id.clone()));
                }
            }
            if delete {
                params.push(("delete".into(), "1".into()));
            }
            (
                format!(
                    "{}post.php?action=edit&extra=&editsubmit=yes&mod=&fid={fid}&tid={tid}&pid={pid}&page={page}",
                    urls.base
                ),
                content.to_string(),
            )
        }
        PostAction::QuickDelete { tid, pid, fid } => {
            params.push(("delete".into(), "1".into()));
            (
                format!(
                    "{}post.php?action=edit&extra=&editsubmit=yes&mod=&fid={fid}&tid={tid}&pid={pid}&page=1",
                    urls.base
                ),
                String::new(),
            )
        }
    };

    params.push(("message".into(), message));
    Ok((post_url, params))
}

async fn favorite_toggle(
    http: &HttpClient,
    urls: &ForumUrls,
    item: &str,
    tid: &str,
    add: bool,
) -> AdapterResult<()> {
    let action = if add { "add" } else { "remove" };
    let url = format!(
        "{}my.php?item={item}&action={action}&inajax=1&ajaxtarget=favorite_msg&tid={tid}",
        urls.base
    );
    let body = http.get_text(&url).await?;
    if parse_ajax_root_message(&body).is_some() {
        Ok(())
    } else {
        Err(AdapterError::ForumMessage(
            if add {
                "添加收藏失败"
            } else {
                "移除收藏失败"
            }
            .into(),
        ))
    }
}

async fn fetch_formhash_from_blacklist(
    http: &HttpClient,
    urls: &ForumUrls,
) -> AdapterResult<String> {
    let html = http.get_text(&urls.blacklist()).await?;
    crate::parser::common::parse_formhash(&html)
        .ok_or_else(|| AdapterError::Parse("blacklist formhash not found".into()))
}

fn parse_ajax_root_message(body: &str) -> Option<String> {
    if let Some(text) = extract_cdata(body) {
        return Some(text);
    }

    let document = Html::parse_fragment(body);
    let root = document.select(&Selector::parse("root").ok()?).next()?;
    let mut text = root.text().collect::<String>();
    if text.trim().is_empty() {
        text = root.html();
        if let Some(inner) = extract_cdata(&text) {
            return Some(inner);
        }
    }
    if let Some(idx) = text.find('<') {
        text.truncate(idx);
    }
    let text = text.trim().to_string();
    (!text.is_empty()).then_some(text)
}

fn parse_alert_info(html: &str) -> Option<String> {
    parse_html(html)
        .select(&Selector::parse("div.alert_info").ok()?)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .filter(|s| !s.is_empty())
}

fn parse_upload_response(response: &str) -> AdapterResult<String> {
    if !response.contains("DISCUZUPLOAD") {
        return Err(AdapterError::ForumMessage(format!(
            "无法获取图片ID: {response}"
        )));
    }
    let parts: Vec<_> = response.split('|').collect();
    if parts.len() < 3 || parts[2] == "0" {
        return Err(AdapterError::ForumMessage("无效上传图片ID".into()));
    }
    Ok(parts[2].to_string())
}

fn timestamp_ms() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().to_string())
        .unwrap_or_else(|_| "0".into())
}

#[cfg(test)]
mod ajax_tests {
    use super::parse_ajax_root_message;

    #[test]
    fn parse_root_cdata_xml() {
        let body = r#"<?xml version="1.0" encoding="gbk"?><root><![CDATA[主题已成功从您的收藏夹中移除]]></root>"#;
        let msg = parse_ajax_root_message(body).expect("cdata message");
        assert!(msg.contains("收藏"));
    }
}
