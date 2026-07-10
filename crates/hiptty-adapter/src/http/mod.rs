pub mod decode;
pub mod urls;

use std::sync::Arc;
use std::time::Duration;

use hiptty_core::AdapterError;
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use reqwest::{Client, Response};
use reqwest_cookie_store::CookieStoreMutex;

pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
pub const READ_TIMEOUT: Duration = Duration::from_secs(10);

/// Hard cap for `get_bytes` (avatars / smilies / content images). Prevents unbounded
/// memory when Content-Length is missing or hostile.
pub const MAX_DOWNLOAD_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Clone)]
pub struct HttpClient {
    inner: Client,
    pub cookie_store: Arc<CookieStoreMutex>,
}

impl HttpClient {
    pub fn new(cookie_store: Arc<CookieStoreMutex>) -> Result<Self, AdapterError> {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static(urls::USER_AGENT));

        let inner = Client::builder()
            .cookie_provider(Arc::clone(&cookie_store))
            .default_headers(headers)
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(READ_TIMEOUT)
            .build()
            .map_err(|e| AdapterError::Network(e.to_string()))?;

        Ok(Self {
            inner,
            cookie_store,
        })
    }

    pub async fn get_text(&self, url: &str) -> Result<String, AdapterError> {
        let response = self.get(url).await?;
        let content_type = content_type_header(&response);
        let bytes = response
            .bytes()
            .await
            .map_err(|e| AdapterError::Network(e.to_string()))?;
        decode::decode_response(bytes.as_ref(), content_type.as_deref())
    }

    pub async fn get(&self, url: &str) -> Result<Response, AdapterError> {
        self.inner
            .get(url)
            .send()
            .await
            .map_err(|e| AdapterError::Network(e.to_string()))
            .and_then(Self::check_status)
    }

    pub async fn get_bytes(&self, url: &str) -> Result<Vec<u8>, AdapterError> {
        validate_http_url(url)?;
        let response = self
            .inner
            .get(url)
            .send()
            .await
            .map_err(|e| AdapterError::Network(e.to_string()))?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(AdapterError::NotFound(url.to_string()));
        }
        let mut response = Self::check_status(response)?;
        if let Some(len) = response.content_length() {
            if len > MAX_DOWNLOAD_BYTES {
                return Err(AdapterError::InvalidInput(format!(
                    "image too large: Content-Length {len} > {MAX_DOWNLOAD_BYTES}"
                )));
            }
        }
        // Stream with a hard cap so missing/lying Content-Length cannot OOM us.
        let mut buf = Vec::new();
        if let Some(len) = response.content_length() {
            let cap = usize::try_from(len)
                .unwrap_or(usize::MAX)
                .min(MAX_DOWNLOAD_BYTES as usize);
            buf.reserve(cap);
        }
        loop {
            let chunk = response
                .chunk()
                .await
                .map_err(|e| AdapterError::Network(e.to_string()))?;
            let Some(chunk) = chunk else {
                break;
            };
            let next = buf.len() as u64 + chunk.len() as u64;
            if next > MAX_DOWNLOAD_BYTES {
                return Err(AdapterError::InvalidInput(format!(
                    "image too large: exceeded {MAX_DOWNLOAD_BYTES} bytes while streaming"
                )));
            }
            buf.extend_from_slice(&chunk);
        }
        Ok(buf)
    }

    pub async fn post_form(
        &self,
        url: &str,
        params: &[(&str, &str)],
    ) -> Result<String, AdapterError> {
        let (body, _) = self.post_form_with_url(url, params).await?;
        Ok(body)
    }

    /// Form POST with GBK percent-encoding (required for Chinese usernames on Discuz).
    pub async fn post_form_gbk(
        &self,
        url: &str,
        params: &[(&str, &str)],
    ) -> Result<String, AdapterError> {
        let (body, _) = self.post_form_gbk_with_url(url, params).await?;
        Ok(body)
    }

    pub async fn post_form_gbk_with_url(
        &self,
        url: &str,
        params: &[(&str, &str)],
    ) -> Result<(String, String), AdapterError> {
        use reqwest::header::CONTENT_TYPE;

        let form_body = decode::encode_gbk_form(params);
        let response = self
            .inner
            .post(url)
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(form_body)
            .send()
            .await
            .map_err(|e| AdapterError::Network(e.to_string()))
            .and_then(Self::check_status)?;

        let final_url = response.url().to_string();
        let content_type = content_type_header(&response);
        let bytes = response
            .bytes()
            .await
            .map_err(|e| AdapterError::Network(e.to_string()))?;
        let body = decode::decode_response(bytes.as_ref(), content_type.as_deref())?;
        Ok((body, final_url))
    }

    pub async fn post_form_with_url(
        &self,
        url: &str,
        params: &[(&str, &str)],
    ) -> Result<(String, String), AdapterError> {
        let response = self
            .inner
            .post(url)
            .form(params)
            .send()
            .await
            .map_err(|e| AdapterError::Network(e.to_string()))
            .and_then(Self::check_status)?;

        let final_url = response.url().to_string();
        let content_type = content_type_header(&response);
        let bytes = response
            .bytes()
            .await
            .map_err(|e| AdapterError::Network(e.to_string()))?;
        let body = decode::decode_response(bytes.as_ref(), content_type.as_deref())?;
        Ok((body, final_url))
    }

    pub async fn post_multipart(
        &self,
        url: &str,
        fields: &[(&str, &str)],
        file_field: &str,
        filename: &str,
        data: &[u8],
    ) -> Result<String, AdapterError> {
        use reqwest::multipart;

        let mut form = multipart::Form::new();
        for (key, value) in fields {
            form = form.text((*key).to_string(), (*value).to_string());
        }
        let part = multipart::Part::bytes(data.to_vec())
            .file_name(filename.to_string())
            .mime_str("application/octet-stream")
            .map_err(|e| AdapterError::Network(e.to_string()))?;
        form = form.part(file_field.to_string(), part);

        let response = self
            .inner
            .post(url)
            .multipart(form)
            .send()
            .await
            .map_err(|e| AdapterError::Network(e.to_string()))
            .and_then(Self::check_status)?;

        let content_type = content_type_header(&response);
        let bytes = response
            .bytes()
            .await
            .map_err(|e| AdapterError::Network(e.to_string()))?;
        decode::decode_response(bytes.as_ref(), content_type.as_deref())
    }

    fn check_status(response: Response) -> Result<Response, AdapterError> {
        if response.status().is_success() {
            Ok(response)
        } else {
            Err(AdapterError::Network(format!(
                "unexpected status {}",
                response.status()
            )))
        }
    }
}

fn content_type_header(response: &Response) -> Option<String> {
    response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

/// Image fetches only: reject non-HTTP(S) schemes (file:, data:, etc.).
fn validate_http_url(url: &str) -> Result<(), AdapterError> {
    let trimmed = url.trim();
    if trimmed.starts_with("https://") || trimmed.starts_with("http://") {
        Ok(())
    } else {
        Err(AdapterError::InvalidInput(format!(
            "unsupported image URL scheme (need http/https): {url}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_http_url_accepts_http_https() {
        assert!(validate_http_url("https://img02.4d4y.com/a.jpg").is_ok());
        assert!(validate_http_url("http://example.com/x.png").is_ok());
    }

    #[test]
    fn validate_http_url_rejects_other_schemes() {
        assert!(validate_http_url("file:///etc/passwd").is_err());
        assert!(validate_http_url("data:image/png;base64,xx").is_err());
        assert!(validate_http_url("/relative/path.jpg").is_err());
    }
}
