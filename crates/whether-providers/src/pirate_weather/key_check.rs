//! Checking a Pirate Weather key with the cheapest forecast request there is:
//! every block but `currently` excluded, so the answer is small. Unlike
//! OpenWeather's check, Pirate Weather counts it as a call of the key's month.

use alloc::string::ToString;
use alloc::vec;

use crate::error::check_status;
use crate::http::{Endpoint, HttpRequest, HttpResponse, Method};
use crate::url::QueryBuilder;

use super::{DEFAULT_BASE_URL, PATH_KEY};

pub use crate::openweather::KeyStatus;

/// The request that checks `key`: the forecast for 0° N 0° E with `exclude` set to
/// everything but `currently`, and the key in the `apikey` header (never the URL).
/// It counts as one call against the key's month at Pirate Weather, so `metered` is
/// set; the app's own ledger is not asked, because the check is not a forecast.
pub fn key_check_request(key: &str) -> HttpRequest {
    HttpRequest {
        url: QueryBuilder::new(&alloc::format!(
            "{DEFAULT_BASE_URL}/forecast/{PATH_KEY}/0,0"
        ))
        .param("units", "si")
        .param("exclude", "minutely,hourly,daily,alerts,summary")
        .finish(),
        headers: vec![("apikey", key.trim().to_string())],
        endpoint: Endpoint::Forecast,
        required: true,
        metered: true,
        method: Method::Get,
        body: None,
    }
}

/// The key's status from the response to [`key_check_request`]. A 429 means the
/// key has used its calls for the month.
pub fn parse_key_check(status: u16, body: &[u8]) -> KeyStatus {
    match status {
        200..=299 => KeyStatus::Accepted,
        401 | 403 => KeyStatus::Rejected,
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

    const FORECAST: &[u8] =
        include_bytes!("../../tests/fixtures/pirate_weather/forecast_10001.json");
    const UNAUTHORIZED: &[u8] =
        include_bytes!("../../tests/fixtures/pirate_weather/error_401.json");

    #[test]
    fn the_request_is_small_and_keeps_the_key_in_the_header() {
        let request = key_check_request(" secret123 ");
        assert_eq!(request.method, Method::Get);
        assert!(request.metered, "a call of the key's month");
        assert!(
            request
                .url
                .starts_with("https://api.pirateweather.net/forecast/key/0,0?"),
            "{}",
            request.url
        );
        // Everything but the current conditions is left out.
        let exclude = request
            .url
            .split("exclude=")
            .nth(1)
            .and_then(|rest| rest.split('&').next())
            .unwrap_or_default();
        let mut blocks: alloc::vec::Vec<_> = exclude.split(',').collect();
        blocks.sort_unstable();
        assert_eq!(blocks, ["alerts", "daily", "hourly", "minutely", "summary"]);
        assert!(!blocks.contains(&"currently"));
        // The key is in the header, trimmed, and nowhere a log could print.
        assert_eq!(request.headers, [("apikey", "secret123".to_string())]);
        assert!(!request.url.contains("secret123"));
        assert!(!redact_url(&request.url).contains("secret123"));
        assert!(!alloc::format!("{request:?}").contains("secret123"));
    }

    #[test]
    fn statuses() {
        // A recorded 200 and a recorded 401.
        assert_eq!(parse_key_check(200, FORECAST), KeyStatus::Accepted);
        assert_eq!(parse_key_check(401, UNAUTHORIZED), KeyStatus::Rejected);
        assert_eq!(parse_key_check(403, b""), KeyStatus::Rejected);
        assert_eq!(parse_key_check(429, b""), KeyStatus::Blocked);
        assert_eq!(
            parse_key_check(500, b""),
            KeyStatus::Unavailable("HTTP 500".into())
        );
        assert_eq!(
            parse_key_check(503, br#"{"message":"try later"}"#),
            KeyStatus::Unavailable("HTTP 503: try later".into())
        );
    }
}
