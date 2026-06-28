use std::collections::BTreeMap;

use hiptty_core::{AdapterError, PostAction, PrePostInfo};
use scraper::{Html, Selector};

use crate::parser::common::{
    ensure_parseable, extract_param, parse_error_message, parse_formhash, parse_html,
};

#[derive(Debug, Clone)]
pub struct PreparedPost {
    pub info: PrePostInfo,
    pub notice_author: Option<String>,
    pub notice_author_msg: Option<String>,
    pub notice_trim_str: Option<String>,
}

pub fn parse(html: &str, action: &PostAction) -> Result<PreparedPost, AdapterError> {
    ensure_parseable(html)?;

    if html.contains("memcp.php?action=bind") {
        return Err(AdapterError::ForumMessage(
            "需要通过网页完成实名验证才可以发帖".into(),
        ));
    }

    let formhash = parse_formhash(html).ok_or_else(|| {
        parse_error_message(html)
            .map(AdapterError::ForumMessage)
            .unwrap_or_else(|| AdapterError::Parse("post formhash not found".into()))
    })?;

    let document = parse_html(html);
    let quote_mode = matches!(
        action,
        PostAction::ReplyPost { .. } | PostAction::QuotePost { .. }
    );

    let quote_text = document
        .select(&selector("textarea")?)
        .next()
        .map(|el| el.text().collect::<String>())
        .filter(|_| quote_mode);

    let uid = document
        .select(&selector("script")?)
        .next()
        .map(|el| extract_param(&el.text().collect::<String>(), "discuz_uid = ", ","))
        .filter(|s| !s.is_empty());

    let hash = document
        .select(&selector("input[name=hash]")?)
        .next()
        .and_then(|el| el.value().attr("value"))
        .map(str::to_string);

    let subject = document
        .select(&selector("input[name=subject]")?)
        .next()
        .and_then(|el| el.value().attr("value"))
        .map(str::to_string);

    let deletable = document.select(&selector("input#delete")?).next().is_some();

    let mut type_values = BTreeMap::new();
    let mut type_id = None;
    for option in document.select(&selector("#typeid > option")?) {
        let value = option.value().attr("value").unwrap_or_default().to_string();
        let label = option.text().collect::<String>();
        if type_id.is_none() || option.value().attr("selected").is_some() {
            type_id = Some(value.clone());
        }
        type_values.insert(value, label);
    }

    let (notice_author, notice_author_msg, notice_trim_str) = if quote_mode {
        (
            input_value(&document, "input[name=noticeauthor]"),
            input_value(&document, "input[name=noticeauthormsg]"),
            input_value(&document, "input[name=noticetrimstr]"),
        )
    } else {
        (None, None, None)
    };

    Ok(PreparedPost {
        info: PrePostInfo {
            formhash,
            uid,
            hash,
            subject,
            quote_text,
            type_id,
            type_values,
            deletable,
        },
        notice_author,
        notice_author_msg,
        notice_trim_str,
    })
}

fn input_value(document: &Html, sel: &str) -> Option<String> {
    let selector = Selector::parse(sel).ok()?;
    document
        .select(&selector)
        .next()
        .and_then(|el| el.value().attr("value"))
        .map(str::to_string)
        .filter(|s| !s.is_empty())
}

fn selector(sel: &str) -> Result<Selector, AdapterError> {
    Selector::parse(sel).map_err(|e| AdapterError::Parse(format!("invalid selector: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_reply_form_fixture() {
        let html = include_str!("../../tests/fixtures/pre_post_reply.html");
        let prepared = parse(
            html,
            &PostAction::ReplyThread {
                tid: "448060".into(),
            },
        )
        .expect("parse pre post");

        assert_eq!(prepared.info.formhash, "abc123");
        assert_eq!(prepared.info.uid.as_deref(), Some("10001"));
        assert_eq!(prepared.info.hash.as_deref(), Some("def456"));
    }

    #[test]
    fn parse_quote_form_fixture() {
        let html = include_str!("../../tests/fixtures/pre_post_quote.html");
        let prepared = parse(
            html,
            &PostAction::QuotePost {
                tid: "448060".into(),
                pid: "200002".into(),
            },
        )
        .expect("parse quote");

        assert_eq!(
            prepared.info.quote_text.as_deref(),
            Some("[quote]original[/quote]")
        );
        assert_eq!(prepared.notice_author.as_deref(), Some("noticeauthor"));
    }
}
