use hiptty_core::{processed_password, AdapterError, Credentials, SessionInfo};

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
