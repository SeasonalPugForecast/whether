//! Open-Meteo geocoding (<https://open-meteo.com/en/docs/geocoding-api>). Location
//! data is based on GeoNames.

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use serde::Deserialize;
use whether_core::model::ProviderId;

use crate::error::{ProviderError, check_status};
use crate::geocode::{GeocodeQuery, GeocodeResult};
use crate::http::{Endpoint, HttpRequest, HttpResponse, Method};
use crate::url::QueryBuilder;

/// The default geocoding base URL.
pub const DEFAULT_GEOCODING_URL: &str = "https://geocoding-api.open-meteo.com/v1";

/// The search request for `query`. Postal codes are searched as names, limited to
/// their country.
pub fn geocode_request(
    base: &str,
    query: &GeocodeQuery<'_>,
    count: u8,
    language: &str,
) -> HttpRequest {
    let (name, country) = match query {
        GeocodeQuery::Name { text, country } => (*text, *country),
        GeocodeQuery::Zip { zip, country } => (*zip, Some(*country)),
    };
    let mut url = QueryBuilder::new(&alloc::format!("{}/search", base.trim_end_matches('/')))
        .param("name", name)
        .param("count", count.to_string())
        .param("language", language)
        .param("format", "json");
    if let Some(country) = country {
        url = url.param("countryCode", country);
    }
    HttpRequest {
        url: url.finish(),
        headers: vec![],
        endpoint: Endpoint::Geocode,
        required: true,
        metered: false,
        method: Method::Get,
        body: None,
    }
}

#[derive(Deserialize)]
struct Place {
    name: String,
    latitude: f64,
    longitude: f64,
    elevation: Option<f32>,
    country_code: Option<String>,
    admin1: Option<String>,
    timezone: Option<String>,
    population: Option<u64>,
    #[serde(default)]
    postcodes: Vec<String>,
}

#[derive(Deserialize)]
struct Response {
    #[serde(default)]
    results: Vec<Place>,
}

/// Parses a search response; no `results` key means no matches.
pub fn parse_geocode(
    query: &GeocodeQuery<'_>,
    resp: &HttpResponse<'_>,
) -> Result<Vec<GeocodeResult>, ProviderError> {
    check_status(resp)?;
    let r: Response =
        serde_json::from_slice(resp.body).map_err(|e| ProviderError::Malformed(e.to_string()))?;
    let wanted_zip = match query {
        GeocodeQuery::Zip { zip, .. } => Some(*zip),
        GeocodeQuery::Name { .. } => None,
    };
    Ok(r.results
        .into_iter()
        .map(|p| GeocodeResult {
            postcode: match wanted_zip {
                Some(zip) if p.postcodes.iter().any(|z| z == zip) => Some(zip.to_string()),
                _ => p.postcodes.first().cloned(),
            },
            name: p.name,
            latitude: p.latitude,
            longitude: p.longitude,
            country_code: p.country_code,
            admin1: p.admin1,
            timezone: p.timezone,
            elevation: p.elevation,
            population: p.population,
            source: ProviderId::OpenMeteo,
        })
        .collect())
}
