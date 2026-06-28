use hiptty_core::{AdapterError, Credentials, SessionInfo};
use md5::{Digest, Md5};

use crate::http::urls::ForumUrls;
use crate::http::HttpClient;
use crate::parser::common::{parse_formhash, parse_session_identity};

const LOGIN_SUCCESS: &str = "欢迎您回来";
const LOGIN_FAIL: &str = "登录失败";

pub async fn login(
    http: &HttpClient,
    urls: &ForumUrls,
    credentials: &Credentials,
) -> Result<SessionInfo, AdapterError> {
    let formhash = fetch_formhash(http, urls).await?;
    let password = processed_password(&credentials.password);

    let params = [
        ("m_formhash", formhash.as_str()),
        ("referer", &format!("{}index.php", urls.base)),
        ("loginfield", "username"),
        ("username", credentials.username.as_str()),
        ("password", password.as_str()),
        (
            "questionid",
            credentials.security_question.as_deref().unwrap_or("0"),
        ),
        (
            "answer",
            credentials.security_answer.as_deref().unwrap_or(""),
        ),
        ("cookietime", "2592000"),
    ];

    let response = http.post_form_gbk(&urls.login_submit(), &params).await?;

    if response.contains(LOGIN_SUCCESS) {
        match session_info(http, urls).await {
            Ok(info) if info.logged_in => Ok(info),
            _ => Ok(SessionInfo {
                logged_in: true,
                username: Some(credentials.username.clone()),
                uid: None,
            }),
        }
    } else if response.contains(LOGIN_FAIL) {
        let message = response
            .split(LOGIN_FAIL)
            .nth(1)
            .and_then(|s| s.split('次').next())
            .map(|s| format!("{LOGIN_FAIL}{s}次"))
            .unwrap_or_else(|| LOGIN_FAIL.to_string());
        Err(AdapterError::AuthFailed(message))
    } else {
        Err(AdapterError::AuthFailed(
            "login failed: unknown response".into(),
        ))
    }
}

pub async fn fetch_formhash(http: &HttpClient, urls: &ForumUrls) -> Result<String, AdapterError> {
    let html = http.get_text(&urls.login_form()).await?;
    parse_formhash(&html).ok_or_else(|| AdapterError::AuthFailed("cannot get formhash".into()))
}

pub async fn session_info(
    http: &HttpClient,
    urls: &ForumUrls,
) -> Result<SessionInfo, AdapterError> {
    let html = http.get_text(&format!("{}index.php", urls.base)).await?;
    if let Some((uid, username)) = parse_session_identity(&html) {
        return Ok(SessionInfo {
            logged_in: true,
            username: Some(username),
            uid: Some(uid),
        });
    }
    Ok(SessionInfo {
        logged_in: false,
        username: None,
        uid: None,
    })
}

fn processed_password(password: &str) -> String {
    if password.len() == 32 && password.chars().all(|c| c.is_ascii_hexdigit()) {
        return password.to_string();
    }

    let escaped = password
        .replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('"', "\\\"");

    let digest = Md5::digest(escaped.as_bytes());
    format!("{digest:x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn md5_password_plaintext() {
        let hash = processed_password("hello");
        assert_eq!(hash.len(), 32);
    }

    #[test]
    fn md5_password_passthrough() {
        let md5 = "d41d8cd98f00b204e9800998ecf8427e";
        assert_eq!(processed_password(md5), md5);
    }
}
