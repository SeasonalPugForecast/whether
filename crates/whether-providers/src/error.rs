//! Provider errors and the shared HTTP status mapping.

use alloc::string::{String, ToString};

use crate::http::HttpResponse;
use crate::usage::QuotaError;

/// Why a provider request failed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ProviderError {
    /// A non-2xx status without a more specific meaning.
    #[error("HTTP {status}{}", message.as_ref().map(|m| alloc::format!(": {m}")).unwrap_or_default())]
    Http {
        /// Status code.
        status: u16,
        /// Message from the body, if any.
        message: Option<String>,
    },
    /// The request was rejected (400) with this reason.
    #[error("request rejected: {0}")]
    Rejected(String),
    /// 401 or 403, with a hint for the user.
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    /// 429 Too Many Requests.
    #[error("rate limited")]
    RateLimited {
        /// Seconds to wait, when the server said.
        retry_after_s: Option<u32>,
    },
    /// 304 Not Modified: the cached data is still current.
    #[error("not modified")]
    NotModified,
    /// The body did not have the expected shape.
    #[error("malformed response: {0}")]
    Malformed(String),
    /// The provider is misconfigured (missing key or contact).
    #[error("configuration: {0}")]
    Config(String),
    /// A metered request was not sent because of the daily call budget.
    #[error("call budget: {0}")]
    Quota(QuotaError),
    /// The request never got a response (DNS, TLS, timeout, connection reset).
    #[error("network: {0}")]
    Transport(String),
    /// The provider does not cover the place (NWS outside the US); nothing was sent.
    #[error("{0} does not cover this place")]
    OutOfArea(&'static str),
}

/// Longest message kept from an error body.
const MAX_MESSAGE: usize = 200;

/// The `reason` or `message` field of a JSON error body, else the body text.
fn body_message(body: &[u8]) -> Option<String> {
    #[derive(serde::Deserialize)]
    struct ErrorBody {
        reason: Option<String>,
        message: Option<String>,
    }
    let text = match serde_json::from_slice::<ErrorBody>(body) {
        Ok(e) => e.reason.or(e.message)?,
        Err(_) => core::str::from_utf8(body).ok()?.trim().to_string(),
    };
    if text.is_empty() {
        return None;
    }
    let cut = text
        .char_indices()
        .nth(MAX_MESSAGE)
        .map_or(text.len(), |(i, _)| i);
    Some(text[..cut].to_string())
}

/// Maps a response status to `Ok` (2xx) or a [`ProviderError`].
pub fn check_status(resp: &HttpResponse<'_>) -> Result<(), ProviderError> {
    match resp.status {
        200..=299 => Ok(()),
        304 => Err(ProviderError::NotModified),
        400 => Err(ProviderError::Rejected(
            body_message(resp.body).unwrap_or_else(|| "bad request".to_string()),
        )),
        401 | 403 => Err(ProviderError::Unauthorized(
            body_message(resp.body).unwrap_or_else(|| "access denied".to_string()),
        )),
        429 => Err(ProviderError::RateLimited {
            retry_after_s: None,
        }),
        status => Err(ProviderError::Http {
            status,
            message: body_message(resp.body),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(code: u16, body: &str) -> Result<(), ProviderError> {
        check_status(&HttpResponse::new(code, body.as_bytes()))
    }

    #[test]
    fn status_mapping() {
        assert_eq!(status(200, ""), Ok(()));
        assert_eq!(status(204, ""), Ok(()));
        assert_eq!(status(304, ""), Err(ProviderError::NotModified));
        assert_eq!(
            status(
                400,
                r#"{"error":true,"reason":"Latitude must be in range of -90 to 90°."}"#
            ),
            Err(ProviderError::Rejected(
                "Latitude must be in range of -90 to 90°.".into()
            ))
        );
        assert_eq!(
            status(401, r#"{"cod":401, "message": "Invalid API key."}"#),
            Err(ProviderError::Unauthorized("Invalid API key.".into()))
        );
        assert_eq!(
            status(403, ""),
            Err(ProviderError::Unauthorized("access denied".into()))
        );
        assert_eq!(
            status(429, ""),
            Err(ProviderError::RateLimited {
                retry_after_s: None
            })
        );
        assert_eq!(
            status(503, "Service Unavailable"),
            Err(ProviderError::Http {
                status: 503,
                message: Some("Service Unavailable".into())
            })
        );
        assert_eq!(
            status(500, ""),
            Err(ProviderError::Http {
                status: 500,
                message: None
            })
        );
    }

    #[test]
    fn long_messages_are_truncated() {
        let long = "x".repeat(1_000);
        let Err(ProviderError::Http {
            message: Some(m), ..
        }) = status(502, &long)
        else {
            panic!("expected Http error");
        };
        assert_eq!(m.len(), MAX_MESSAGE);
    }

    #[test]
    fn error_messages_read_well() {
        let e = ProviderError::Http {
            status: 502,
            message: Some("bad gateway".into()),
        };
        assert_eq!(alloc::format!("{e}"), "HTTP 502: bad gateway");
        assert_eq!(
            alloc::format!("{}", ProviderError::NotModified),
            "not modified"
        );
    }
}
