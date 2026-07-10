pub mod decode;
pub mod urls;

use std::sync::Arc;
use std::time::Duration;

use hiptty_core::AdapterError;
use reqwest::header::{HeaderMap, HeaderValue, COOKIE, USER_AGENT};
use reqwest::{Client, Response, Url};
use reqwest_cookie_store::CookieStoreMutex;

/// TCP connect phase deadline.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// Idle gap between successful response-body reads (not upload duration).
pub const READ_IDLE_TIMEOUT: Duration = Duration::from_secs(10);
/// Default total request deadline (GET / default client timeout).
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
/// Form POST (including GBK login posts).
pub const FORM_POST_TIMEOUT: Duration = Duration::from_secs(30);
/// Image / attachment byte downloads via `get_bytes`.
pub const IMAGE_TIMEOUT: Duration = Duration::from_secs(15);
/// Multipart image upload total deadline (covers slow 8 MiB uploads).
pub const UPLOAD_TIMEOUT: Duration = Duration::from_secs(90);

/// Hard cap for `get_bytes` (avatars / smilies / content images). Prevents unbounded
/// memory when Content-Length is missing or hostile.
pub const MAX_DOWNLOAD_BYTES: u64 = 8 * 1024 * 1024;

/// Injectable timeouts for production and fast tests.
#[derive(Debug, Clone, Copy)]
pub struct HttpTimeouts {
    pub connect: Duration,
    pub read_idle: Duration,
    pub request: Duration,
    pub form_post: Duration,
    pub image: Duration,
    pub upload: Duration,
}

impl HttpTimeouts {
    pub const fn production() -> Self {
        Self {
            connect: CONNECT_TIMEOUT,
            read_idle: READ_IDLE_TIMEOUT,
            request: REQUEST_TIMEOUT,
            form_post: FORM_POST_TIMEOUT,
            image: IMAGE_TIMEOUT,
            upload: UPLOAD_TIMEOUT,
        }
    }
}

#[derive(Clone)]
pub struct HttpClient {
    /// Session-aware client (login, threads, posts). Set-Cookie updates the jar.
    inner: Client,
    /// No cookie_provider: image fetches attach a Cookie *snapshot* header only, so
    /// Set-Cookie on the response cannot revive or pollute the session jar.
    bare: Client,
    pub cookie_store: Arc<CookieStoreMutex>,
    timeouts: HttpTimeouts,
}

impl HttpClient {
    pub fn new(cookie_store: Arc<CookieStoreMutex>) -> Result<Self, AdapterError> {
        Self::with_timeouts(cookie_store, HttpTimeouts::production())
    }

    pub fn with_timeouts(
        cookie_store: Arc<CookieStoreMutex>,
        timeouts: HttpTimeouts,
    ) -> Result<Self, AdapterError> {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static(urls::USER_AGENT));

        // `timeout` is the default total deadline (overridable per request).
        // `read_timeout` is idle-between-reads on the response body — not upload time.
        // Form POST / multipart must set RequestBuilder::timeout explicitly.
        let inner = Client::builder()
            .cookie_provider(Arc::clone(&cookie_store))
            .default_headers(headers.clone())
            .connect_timeout(timeouts.connect)
            .read_timeout(timeouts.read_idle)
            .timeout(timeouts.request)
            .build()
            .map_err(map_reqwest_error)?;

        // No cookie_provider: image CDN traffic must not revive or pollute sessions.
        let bare = Client::builder()
            .default_headers(headers)
            .connect_timeout(timeouts.connect)
            .read_timeout(timeouts.read_idle)
            .timeout(timeouts.request)
            .build()
            .map_err(map_reqwest_error)?;

        Ok(Self {
            inner,
            bare,
            cookie_store,
            timeouts,
        })
    }

    pub fn timeouts(&self) -> HttpTimeouts {
        self.timeouts
    }

    pub async fn get_text(&self, url: &str) -> Result<String, AdapterError> {
        let response = self.get(url).await?;
        let content_type = content_type_header(&response);
        let bytes = response.bytes().await.map_err(map_reqwest_error)?;
        decode::decode_response(bytes.as_ref(), content_type.as_deref())
    }

    pub async fn get(&self, url: &str) -> Result<Response, AdapterError> {
        self.inner
            .get(url)
            .send()
            .await
            .map_err(map_reqwest_error)
            .and_then(Self::check_status)
    }

    pub async fn get_bytes(&self, url: &str) -> Result<Vec<u8>, AdapterError> {
        validate_http_url(url)?;
        // Snapshot cookies for auth-gated attachments, but never feed Set-Cookie back.
        let mut req = self.bare.get(url).timeout(self.timeouts.image);
        if let Some(cookie) = cookie_header_snapshot(&self.cookie_store, url) {
            if let Ok(val) = HeaderValue::from_str(&cookie) {
                req = req.header(COOKIE, val);
            }
        }
        let response = req.send().await.map_err(map_reqwest_error)?;
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
            let chunk = response.chunk().await.map_err(map_reqwest_error)?;
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
            .timeout(self.timeouts.form_post)
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(form_body)
            .send()
            .await
            .map_err(map_reqwest_error)
            .and_then(Self::check_status)?;

        let final_url = response.url().to_string();
        let content_type = content_type_header(&response);
        let bytes = response.bytes().await.map_err(map_reqwest_error)?;
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
            .timeout(self.timeouts.form_post)
            .form(params)
            .send()
            .await
            .map_err(map_reqwest_error)
            .and_then(Self::check_status)?;

        let final_url = response.url().to_string();
        let content_type = content_type_header(&response);
        let bytes = response.bytes().await.map_err(map_reqwest_error)?;
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

        // Explicit total deadline — must override client default REQUEST_TIMEOUT so
        // large uploads are not killed at 15s. read_timeout remains idle-on-response.
        let response = self
            .inner
            .post(url)
            .timeout(self.timeouts.upload)
            .multipart(form)
            .send()
            .await
            .map_err(map_reqwest_error)
            .and_then(Self::check_status)?;

        let content_type = content_type_header(&response);
        let bytes = response.bytes().await.map_err(map_reqwest_error)?;
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

fn map_reqwest_error(e: reqwest::Error) -> AdapterError {
    if e.is_timeout() {
        AdapterError::Network("request timed out".to_string())
    } else {
        AdapterError::Network(e.to_string())
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

/// Build a `Cookie` header from the current jar without installing a cookie_provider
/// (so response Set-Cookie cannot mutate the session).
fn cookie_header_snapshot(store: &CookieStoreMutex, url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    let guard = store.lock().ok()?;
    let pairs: Vec<String> = guard
        .get_request_values(&parsed)
        .map(|(name, value)| format!("{name}={value}"))
        .collect();
    if pairs.is_empty() {
        None
    } else {
        Some(pairs.join("; "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hiptty_core::ErrorCode;
    use reqwest_cookie_store::CookieStore;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use std::time::Duration;

    fn empty_store() -> Arc<CookieStoreMutex> {
        Arc::new(CookieStoreMutex::new(CookieStore::default()))
    }

    fn test_timeouts(request_ms: u64, form_ms: u64, upload_ms: u64) -> HttpTimeouts {
        HttpTimeouts {
            connect: Duration::from_millis(200),
            read_idle: Duration::from_millis(500),
            request: Duration::from_millis(request_ms),
            form_post: Duration::from_millis(form_ms),
            image: Duration::from_millis(request_ms),
            upload: Duration::from_millis(upload_ms),
        }
    }

    /// Slow HTTP/1.1 server: wait `delay` after reading the request, then respond.
    fn spawn_delayed_server(delay: Duration, body: &'static [u8]) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let ready = std::sync::Arc::new(std::sync::Barrier::new(2));
        let ready2 = Arc::clone(&ready);
        thread::spawn(move || {
            ready2.wait();
            if let Ok((mut stream, _)) = listener.accept() {
                let _ = stream.set_nodelay(true);
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                // Drain the full request so the client can finish sending before we reply.
                let mut buf = [0u8; 8192];
                let mut total = Vec::new();
                let deadline = std::time::Instant::now() + Duration::from_secs(1);
                while std::time::Instant::now() < deadline {
                    match stream.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            total.extend_from_slice(&buf[..n]);
                            if let Some(header_end) =
                                total.windows(4).position(|w| w == b"\r\n\r\n")
                            {
                                let headers = &total[..header_end + 4];
                                let body_start = header_end + 4;
                                let content_len = headers
                                    .split(|&b| b == b'\n')
                                    .find_map(|line| {
                                        let line = line.strip_suffix(b"\r").unwrap_or(line);
                                        let s = std::str::from_utf8(line).ok()?;
                                        let (k, v) = s.split_once(':')?;
                                        if k.eq_ignore_ascii_case("content-length") {
                                            v.trim().parse::<usize>().ok()
                                        } else {
                                            None
                                        }
                                    })
                                    .unwrap_or(0);
                                if total.len().saturating_sub(body_start) >= content_len {
                                    break;
                                }
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(1));
                        }
                        Err(_) => break,
                    }
                }
                thread::sleep(delay);
                let mut resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .into_bytes();
                resp.extend_from_slice(body);
                let _ = stream.write_all(&resp);
                let _ = stream.flush();
                let _ = stream.shutdown(std::net::Shutdown::Both);
            }
        });
        ready.wait();
        format!("http://{addr}/")
    }

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

    #[test]
    fn production_timeouts_match_constants() {
        let t = HttpTimeouts::production();
        assert_eq!(t.connect, CONNECT_TIMEOUT);
        assert_eq!(t.read_idle, READ_IDLE_TIMEOUT);
        assert_eq!(t.request, REQUEST_TIMEOUT);
        assert_eq!(t.form_post, FORM_POST_TIMEOUT);
        assert_eq!(t.image, IMAGE_TIMEOUT);
        assert_eq!(t.upload, UPLOAD_TIMEOUT);
    }

    #[tokio::test]
    async fn get_times_out_with_request_timeout() {
        let url = spawn_delayed_server(Duration::from_millis(200), b"ok");
        let client =
            HttpClient::with_timeouts(empty_store(), test_timeouts(50, 500, 500)).expect("client");
        let err = client.get_text(&url).await.expect_err("should timeout");
        assert_eq!(err.code(), ErrorCode::Network);
        assert!(err.to_string().contains("request timed out"));
        assert!(err.code().retryable());
    }

    #[tokio::test]
    async fn form_post_uses_form_timeout_not_request() {
        // delay 100ms: longer than request (40) but shorter than form (400) → success
        let url = spawn_delayed_server(Duration::from_millis(100), b"ok");
        let client =
            HttpClient::with_timeouts(empty_store(), test_timeouts(40, 400, 500)).expect("client");
        let body = client
            .post_form(&url, &[("a", "b")])
            .await
            .expect("form should use longer timeout");
        assert_eq!(body, "ok");
    }

    #[tokio::test]
    async fn form_post_times_out_beyond_form_timeout() {
        let url = spawn_delayed_server(Duration::from_millis(200), b"ok");
        let client =
            HttpClient::with_timeouts(empty_store(), test_timeouts(50, 50, 500)).expect("client");
        let err = client
            .post_form(&url, &[("a", "b")])
            .await
            .expect_err("should timeout");
        assert_eq!(err.code(), ErrorCode::Network);
        assert!(err.to_string().contains("request timed out"));
        assert!(err.code().retryable());
    }

    #[tokio::test]
    async fn multipart_uses_upload_timeout_not_request() {
        // delay 80ms: longer than request (50) but shorter than upload (200) → success
        let url = spawn_delayed_server(Duration::from_millis(80), b"uploaded");
        let client =
            HttpClient::with_timeouts(empty_store(), test_timeouts(50, 50, 200)).expect("client");
        let body = client
            .post_multipart(&url, &[], "file", "x.jpg", b"data")
            .await
            .expect("multipart should use upload timeout");
        assert_eq!(body, "uploaded");
    }

    #[tokio::test]
    async fn content_length_over_max_rejected() {
        // Minimal server that claims a huge Content-Length.
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf);
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    MAX_DOWNLOAD_BYTES + 1
                );
                let _ = stream.write_all(header.as_bytes());
            }
        });
        let url = format!("http://{addr}/img.jpg");
        let client = HttpClient::with_timeouts(
            empty_store(),
            HttpTimeouts {
                connect: Duration::from_millis(200),
                read_idle: Duration::from_secs(2),
                request: Duration::from_secs(2),
                form_post: Duration::from_secs(2),
                image: Duration::from_secs(2),
                upload: Duration::from_secs(2),
            },
        )
        .expect("client");
        let err = client.get_bytes(&url).await.expect_err("too large");
        assert_eq!(err.code(), ErrorCode::InvalidInput);
        assert!(err.to_string().contains("too large"));
    }

    #[test]
    fn map_timeout_is_retryable_network() {
        // Construct via a real timed-out call is covered above; unit-check ErrorCode mapping path.
        let err = AdapterError::Network("request timed out".to_string());
        assert_eq!(err.code(), ErrorCode::Network);
        assert!(err.code().retryable());
    }
}
