//! Place search for the app's location list, through the same host as
//! forecasts: Open-Meteo's geocoder, which needs no key.

use std::sync::Arc;

use whether_providers::open_meteo::geocode::{
    DEFAULT_GEOCODING_URL, geocode_request, parse_geocode,
};
use whether_providers::{GeocodeQuery, HttpResponse};

use crate::WhetherError;
use crate::fetch::{FetchHost, host_request};

/// Results a search returns at most.
const RESULTS: u8 = 10;

/// A place a search found.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct FoundPlace {
    /// Its name, `"New York"`.
    pub name: String,
    /// State or province.
    pub region: Option<String>,
    /// ISO country code, `"US"`.
    pub country_code: Option<String>,
    /// `"New York, New York, US"`.
    pub display_name: String,
    /// Latitude in degrees.
    pub latitude: f64,
    /// Longitude in degrees.
    pub longitude: f64,
    /// IANA time zone, `"America/New_York"`.
    pub timezone: Option<String>,
}

/// Whether `text` is a postal code: three to ten digits.
fn is_postal_code(text: &str) -> bool {
    (3..=10).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_digit())
}

/// Places matching `query`, a name or (all digits) a postal code in `country`
/// (the phone's region, `"US"`), with names in `language`.
#[uniffi::export]
#[allow(clippy::needless_pass_by_value)] // UniFFI passes arguments by value
pub async fn search_places(
    query: String,
    country: Option<String>,
    language: String,
    host: Arc<dyn FetchHost>,
) -> Result<Vec<FoundPlace>, WhetherError> {
    let text = query.trim();
    let q = match country.as_deref() {
        Some(country) if is_postal_code(text) => GeocodeQuery::Zip { zip: text, country },
        _ => GeocodeQuery::Name {
            text,
            country: None,
        },
    };
    if !q.is_searchable() {
        return Ok(Vec::new());
    }
    let request = geocode_request(DEFAULT_GEOCODING_URL, &q, RESULTS, &language);
    let resp = host
        .send(host_request(&request))
        .await
        .map_err(|e| WhetherError::Search {
            message: e.to_string(),
        })?;
    let results = parse_geocode(
        &q,
        &HttpResponse {
            status: resp.status,
            body: &resp.body,
            date: None,
            expires: None,
            last_modified: None,
        },
    )
    .map_err(|e| WhetherError::Search {
        message: e.to_string(),
    })?;
    // Places that read the same ("Brooklyn, England, GB" four times) would be
    // indistinguishable in a list; the first is the most relevant.
    let mut seen = std::collections::BTreeSet::new();
    Ok(results
        .into_iter()
        .filter(|r| seen.insert(r.display_name()))
        .map(|r| FoundPlace {
            display_name: r.display_name(),
            name: r.name,
            region: r.admin1,
            country_code: r.country_code,
            latitude: r.latitude,
            longitude: r.longitude,
            timezone: r.timezone,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postal_codes_are_digits() {
        assert!(is_postal_code("10001"));
        assert!(!is_postal_code("19"));
        assert!(!is_postal_code("Brooklyn"));
        assert!(!is_postal_code("193 35"));
    }
}
