//! HTTP transport: performs the requests providers plan.

use std::time::Duration;

use whether_core::time::{UnixTime, parse_http_date};
use whether_providers::{HttpRequest, HttpResponse, Method, redact_url, secret_values};

/// Largest response body accepted (a cap against hostile servers).
pub const MAX_BODY: u64 = 4 * 1024 * 1024;

/// A response that owns its bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedResponse {
    /// Status code.
    pub status: u16,
    /// Body bytes.
    pub body: Vec<u8>,
    /// The `Date` header.
    pub date: Option<UnixTime>,
    /// The `Expires` header.
    pub expires: Option<UnixTime>,
    /// The `Last-Modified` header.
    pub last_modified: Option<String>,
}

impl OwnedResponse {
    /// A response with just a status and body.
    pub fn new(status: u16, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status,
            body: body.into(),
            date: None,
            expires: None,
            last_modified: None,
        }
    }

    /// A borrowed view for parsers.
    pub fn as_response(&self) -> HttpResponse<'_> {
        HttpResponse {
            status: self.status,
            body: &self.body,
            date: self.date,
            expires: self.expires,
            last_modified: self.last_modified.as_deref(),
        }
    }
}

/// Why a request got no response. Messages never contain secrets.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TransportError {
    /// The request or connection timed out.
    #[error("timed out: {0}")]
    Timeout(String),
    /// The body was larger than [`MAX_BODY`].
    #[error("response larger than {MAX_BODY} bytes")]
    TooLarge,
    /// DNS, TLS, connection or protocol failure.
    #[error("{0}")]
    Failed(String),
}

/// Performs HTTP requests. Implemented by [`UreqTransport`] and by test fakes.
pub trait Transport: Send + Sync {
    /// Sends `req` (GET, or POST with its JSON body) and returns any status with
    /// its body.
    fn send(&self, req: &HttpRequest) -> Result<OwnedResponse, TransportError>;
}

/// The User-Agent: `whether/<version>`, plus the repository URL once there is one.
pub fn user_agent() -> String {
    let repository = env!("CARGO_PKG_REPOSITORY");
    // A placeholder `.invalid` URL (before the project has a real one) is no contact.
    if repository.is_empty() || repository.contains(".invalid") {
        format!("whether/{}", env!("CARGO_PKG_VERSION"))
    } else {
        format!("whether/{} (+{repository})", env!("CARGO_PKG_VERSION"))
    }
}

/// HTTPS with ureq (rustls, webpki roots, gzip).
#[derive(Debug)]
pub struct UreqTransport {
    agent: ureq::Agent,
}

impl UreqTransport {
    /// A transport with the standard timeouts (30 s total, 10 s to connect).
    pub fn new(user_agent: &str) -> Self {
        Self::with_timeouts(user_agent, Duration::from_secs(30), Duration::from_secs(10))
    }

    /// A transport with custom timeouts (tests use short ones).
    pub fn with_timeouts(user_agent: &str, total: Duration, connect: Duration) -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(total))
            .timeout_connect(Some(connect))
            .http_status_as_error(false)
            .user_agent(user_agent)
            .build();
        Self {
            agent: ureq::Agent::new_with_config(config),
        }
    }
}

/// `message` with every secret query value of `url` masked.
fn scrub(message: &str, url: &str) -> String {
    let mut out = message.replace(url, &redact_url(url));
    for secret in secret_values(url) {
        out = out.replace(secret, "***");
    }
    out
}

fn header(resp: &ureq::http::Response<ureq::Body>, name: &str) -> Option<String> {
    resp.headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

impl Transport for UreqTransport {
    fn send(&self, req: &HttpRequest) -> Result<OwnedResponse, TransportError> {
        let sent = match req.method {
            Method::Get => {
                let mut request = self.agent.get(&req.url);
                for (name, value) in &req.headers {
                    request = request.header(*name, value);
                }
                request.call()
            }
            Method::Post => {
                let mut request = self.agent.post(&req.url).content_type("application/json");
                for (name, value) in &req.headers {
                    request = request.header(*name, value);
                }
                request.send(req.body.as_deref().unwrap_or(""))
            }
        };
        let mut resp = sent.map_err(|e| map_error(&e, &req.url))?;
        let status = resp.status().as_u16();
        let date = header(&resp, "date").and_then(|d| parse_http_date(&d));
        let expires = header(&resp, "expires").and_then(|d| parse_http_date(&d));
        let last_modified = header(&resp, "last-modified");
        let body = resp
            .body_mut()
            .with_config()
            .limit(MAX_BODY)
            .read_to_vec()
            .map_err(|e| map_error(&e, &req.url))?;
        Ok(OwnedResponse {
            status,
            body,
            date,
            expires,
            last_modified,
        })
    }
}

fn map_error(e: &ureq::Error, url: &str) -> TransportError {
    match e {
        ureq::Error::Timeout(which) => TransportError::Timeout(format!("{which:?}")),
        ureq::Error::BodyExceedsLimit(_) => TransportError::TooLarge,
        other => TransportError::Failed(scrub(&other.to_string(), url)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrub_masks_urls_and_keys() {
        let url = "https://api.example/x?lat=1&appid=SECRET";
        let message = format!("failed to fetch {url}: bad SECRET");
        let scrubbed = scrub(&message, url);
        assert!(!scrubbed.contains("SECRET"), "{scrubbed}");
        assert!(scrubbed.contains("appid=***"));
    }

    #[test]
    fn user_agent_names_the_version() {
        assert!(user_agent().starts_with("whether/"));
        assert!(!user_agent().contains("(+)"));
        assert!(!user_agent().contains(".invalid"));
    }

    #[test]
    fn owned_responses_lend_their_parts() {
        let mut owned = OwnedResponse::new(200, "{}");
        owned.last_modified = Some("Mon, 05 Oct 2026 17:00:00 GMT".into());
        let r = owned.as_response();
        assert_eq!(
            (r.status, r.body, r.last_modified),
            (200, &b"{}"[..], owned.last_modified.as_deref())
        );
    }
}
