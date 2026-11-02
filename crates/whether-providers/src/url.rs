//! URL building, coordinate formatting and redaction of secrets in URLs.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};

/// Characters encoded in query values: everything except unreserved characters and
/// commas (Open-Meteo variable lists use literal commas).
const QUERY_VALUE: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~')
    .remove(b',');

/// Characters encoded in a path segment: everything except unreserved characters.
const PATH_SEGMENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// Percent-encodes `segment` for use as one URL path segment, so a value from a
/// server (such as an alert id) cannot add path segments, a query or a fragment.
pub fn encode_path_segment(segment: &str) -> String {
    utf8_percent_encode(segment, PATH_SEGMENT).to_string()
}

/// Query parameters whose values are secrets.
const SECRET_KEYS: [&str; 5] = ["appid", "apikey", "api_key", "key", "token"];

/// Request headers whose values are secrets (Pirate Weather sends its key in
/// `apikey`).
const SECRET_HEADERS: [&str; 4] = ["apikey", "api-key", "x-api-key", "authorization"];

/// Whether the value of request header `name` is a secret, so that it must never be
/// logged.
pub fn is_secret_header(name: &str) -> bool {
    SECRET_HEADERS.iter().any(|s| name.eq_ignore_ascii_case(s))
}

/// Builds `base?key=value&…` with encoded values.
#[derive(Clone, Debug)]
pub struct QueryBuilder {
    url: String,
    has_query: bool,
}

impl QueryBuilder {
    /// Starts from `base`, which may already contain a query.
    pub fn new(base: &str) -> Self {
        Self {
            url: base.to_string(),
            has_query: base.contains('?'),
        }
    }

    /// Appends `key=value`, percent-encoding the value.
    #[must_use]
    pub fn param(mut self, key: &'static str, value: impl AsRef<str>) -> Self {
        self.url.push(if self.has_query { '&' } else { '?' });
        self.has_query = true;
        self.url.push_str(key);
        self.url.push('=');
        self.url
            .extend(utf8_percent_encode(value.as_ref(), QUERY_VALUE));
        self
    }

    /// The finished URL.
    pub fn finish(self) -> String {
        self.url
    }
}

/// Masks the values of secret query parameters (`appid`, `apikey`, `api_key`,
/// `key`, `token`) as `***`. Use it for every URL that is logged or displayed.
pub fn redact_url(url: &str) -> String {
    let Some((base, query)) = url.split_once('?') else {
        return url.to_string();
    };
    let mut out = String::with_capacity(url.len());
    out.push_str(base);
    out.push('?');
    for (i, pair) in query.split('&').enumerate() {
        if i > 0 {
            out.push('&');
        }
        match pair.split_once('=') {
            Some((key, _)) if SECRET_KEYS.iter().any(|s| key.eq_ignore_ascii_case(s)) => {
                out.push_str(key);
                out.push_str("=***");
            }
            _ => out.push_str(pair),
        }
    }
    out
}

/// The values of secret query parameters in `url`, for scrubbing them from error
/// messages that may quote the URL.
pub fn secret_values(url: &str) -> Vec<&str> {
    let Some((_, query)) = url.split_once('?') else {
        return Vec::new();
    };
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .filter(|(key, value)| {
            !value.is_empty() && SECRET_KEYS.iter().any(|s| key.eq_ignore_ascii_case(s))
        })
        .map(|(_, value)| value)
        .collect()
}

/// A WGS84 position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Coordinates {
    /// Latitude in degrees.
    pub latitude: f64,
    /// Longitude in degrees.
    pub longitude: f64,
}

impl Coordinates {
    /// Rounded to `decimals` places (privacy and cache hits; MET Norway allows 4).
    #[must_use]
    pub fn rounded(self, decimals: u8) -> Self {
        let scale = libm::pow(10.0, f64::from(decimals.min(8)));
        let round = |x: f64| libm::round(x * scale) / scale;
        Self {
            latitude: round(self.latitude),
            longitude: round(self.longitude),
        }
    }

    /// `(latitude, longitude)` as query values with at most `decimals` places and no
    /// trailing zeros, e.g. `("40.714", "-74.006")`.
    pub fn fmt_param(self, decimals: u8) -> (String, String) {
        let r = self.rounded(decimals);
        (
            fmt_decimal(r.latitude, decimals),
            fmt_decimal(r.longitude, decimals),
        )
    }
}

fn fmt_decimal(x: f64, decimals: u8) -> String {
    let mut s = format!("{x:.prec$}", prec = usize::from(decimals.min(8)));
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    if s == "-0" { "0".to_string() } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_values_are_encoded_but_commas_stay() {
        let url = QueryBuilder::new("https://example.com/v1/search")
            .param("name", "Tromsø, Norway")
            .param("current", "temperature_2m,weather_code")
            .finish();
        assert_eq!(
            url,
            "https://example.com/v1/search?name=Troms%C3%B8,%20Norway&current=temperature_2m,weather_code"
        );
        let url = QueryBuilder::new("https://x/y?a=1")
            .param("b", "2")
            .finish();
        assert_eq!(url, "https://x/y?a=1&b=2");
    }

    #[test]
    fn redaction() {
        assert_eq!(
            redact_url("https://x/w?lat=1&appid=abc&lon=2"),
            "https://x/w?lat=1&appid=***&lon=2"
        );
        assert_eq!(
            redact_url("https://x/w?APIKEY=abc"),
            "https://x/w?APIKEY=***"
        );
        assert_eq!(
            redact_url("https://x/w?api_key=a&token=b&key=c"),
            "https://x/w?api_key=***&token=***&key=***"
        );
        assert_eq!(redact_url("https://x/w?monkey=a"), "https://x/w?monkey=a");
        assert_eq!(redact_url("https://x/w"), "https://x/w");
    }

    #[test]
    fn coordinates() {
        let c = Coordinates {
            latitude: 40.7143,
            longitude: -74.005_97,
        };
        assert_eq!(
            c.fmt_param(3),
            ("40.714".to_string(), "-74.006".to_string())
        );
        // -74.00597 rounds to -74.0060, and the trailing zero goes.
        assert_eq!(
            c.fmt_param(4),
            ("40.7143".to_string(), "-74.006".to_string())
        );
        let berlin = Coordinates {
            latitude: 52.52,
            longitude: 13.41,
        };
        assert_eq!(
            berlin.fmt_param(4),
            ("52.52".to_string(), "13.41".to_string())
        );
        let zero = Coordinates {
            latitude: -0.0001,
            longitude: 0.0,
        };
        assert_eq!(zero.fmt_param(2), ("0".to_string(), "0".to_string()));
        let met = Coordinates {
            latitude: 59.913_94,
            longitude: 10.75,
        }
        .rounded(4);
        assert!((met.latitude - 59.9139).abs() < 1e-9);
    }

    #[test]
    fn finds_secret_values() {
        assert_eq!(
            secret_values("https://x/a?lat=1&appid=K1&apikey=K2&token="),
            vec!["K1", "K2"]
        );
        assert_eq!(secret_values("https://x/a"), Vec::<&str>::new());
    }

    #[test]
    fn path_segments_cannot_escape() {
        assert_eq!(
            encode_path_segment("2.49.0:FR615:a5/../x?y#z"),
            "2.49.0%3AFR615%3Aa5%2F..%2Fx%3Fy%23z"
        );
        assert_eq!(
            encode_path_segment("8B46C632-DCA7-44D7"),
            "8B46C632-DCA7-44D7"
        );
    }
}
