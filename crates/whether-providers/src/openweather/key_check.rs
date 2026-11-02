//! Checking an OpenWeather key without spending a metered call: one request to
//! the free current-weather endpoint at a fixed point.

use alloc::string::{String, ToString};
use alloc::vec;

use crate::error::check_status;
use crate::http::{Endpoint, HttpRequest, HttpResponse, Method};
use crate::url::QueryBuilder;

use super::DEFAULT_BASE_URL;

/// What OpenWeather said about a key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyStatus {
    /// The key works (2xx).
    Accepted,
    /// OpenWeather does not know the key, or it is not active yet (401).
    Rejected,
    /// The key made too many calls (429).
    Blocked,
    /// No answer about the key: another status, with what the body said.
    Unavailable(String),
}

/// The request that checks `key`: current weather at 0° N 0° E from the free
/// 2.5 API, which is never metered. The URL carries the key: log it only through
/// [`crate::redact_url`].
pub fn key_check_request(key: &str) -> HttpRequest {
    HttpRequest {
        url: QueryBuilder::new(&alloc::format!("{DEFAULT_BASE_URL}/data/2.5/weather"))
            .param("lat", "0")
            .param("lon", "0")
            .param("appid", key.trim())
            .finish(),
        headers: vec![],
        endpoint: Endpoint::Current,
        required: true,
        metered: false,
        method: Method::Get,
        body: None,
    }
}

/// The key's status from the response to [`key_check_request`].
pub fn parse_key_check(status: u16, body: &[u8]) -> KeyStatus {
    match status {
        200..=299 => KeyStatus::Accepted,
        401 => KeyStatus::Rejected,
        429 => KeyStatus::Blocked,
        _ => match check_status(&HttpResponse::new(status, body)) {
            Ok(()) => KeyStatus::Accepted,
            Err(e) => KeyStatus::Unavailable(e.to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::redact_url;

    #[test]
    fn the_request_is_free_and_hides_the_key() {
        let request = key_check_request(" secret123 ");
        assert!(!request.metered);
        assert_eq!(request.method, Method::Get);
        assert!(
            request
                .url
                .starts_with("https://api.openweathermap.org/data/2.5/weather?"),
            "{}",
            redact_url(&request.url)
        );
        assert!(request.url.contains("appid=secret123"));
        let redacted = redact_url(&request.url);
        assert!(!redacted.contains("secret123"), "{redacted}");
        assert!(redacted.contains("appid=***"), "{redacted}");
        assert!(!alloc::format!("{request:?}").contains("secret123"));
    }

    #[test]
    fn statuses() {
        assert_eq!(parse_key_check(429, b""), KeyStatus::Blocked);
        assert_eq!(
            parse_key_check(503, br#"{"cod":503,"message":"try later"}"#),
            KeyStatus::Unavailable("HTTP 503: try later".into())
        );
        assert_eq!(
            parse_key_check(500, b""),
            KeyStatus::Unavailable("HTTP 500".into())
        );
    }
}
