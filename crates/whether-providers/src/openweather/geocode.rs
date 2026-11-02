//! OpenWeather geocoding (<https://openweathermap.org/api/geocoding-api>).

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use serde::Deserialize;
use whether_core::model::ProviderId;

use crate::error::{ProviderError, check_status};
use crate::geocode::{GeocodeQuery, GeocodeResult};
use crate::http::{Endpoint, HttpRequest, HttpResponse, Method};
use crate::url::QueryBuilder;

/// The request for `query`: `/geo/1.0/zip` for postal codes, `/geo/1.0/direct` for names.
pub fn geocode_request(base: &str, key: &str, query: &GeocodeQuery<'_>, limit: u8) -> HttpRequest {
    let base = base.trim_end_matches('/');
    let url = match query {
        GeocodeQuery::Zip { zip, country } => {
            QueryBuilder::new(&alloc::format!("{base}/geo/1.0/zip"))
                .param("zip", alloc::format!("{zip},{country}"))
        }
        GeocodeQuery::Name { text, country } => {
            let q = match country {
                Some(c) => alloc::format!("{text},{c}"),
                None => (*text).to_string(),
            };
            QueryBuilder::new(&alloc::format!("{base}/geo/1.0/direct"))
                .param("q", q)
                .param("limit", limit.to_string())
        }
    };
    HttpRequest {
        url: url.param("appid", key).finish(),
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
    lat: f64,
    lon: f64,
    country: Option<String>,
    state: Option<String>,
    zip: Option<String>,
}

impl Place {
    fn into_result(self) -> GeocodeResult {
        GeocodeResult {
            name: self.name,
            latitude: self.lat,
            longitude: self.lon,
            country_code: self.country,
            admin1: self.state,
            postcode: self.zip,
            timezone: None,
            elevation: None,
            population: None,
            source: ProviderId::OpenWeather,
        }
    }
}

/// Parses a response to [`geocode_request`]. A 404 (unknown postal code) is an empty
/// result.
pub fn parse_geocode(
    query: &GeocodeQuery<'_>,
    resp: &HttpResponse<'_>,
) -> Result<Vec<GeocodeResult>, ProviderError> {
    if resp.status == 404 {
        return Ok(Vec::new());
    }
    check_status(resp)?;
    let malformed = |e: serde_json::Error| ProviderError::Malformed(e.to_string());
    Ok(match query {
        GeocodeQuery::Zip { .. } => {
            vec![
                serde_json::from_slice::<Place>(resp.body)
                    .map_err(malformed)?
                    .into_result(),
            ]
        }
        GeocodeQuery::Name { .. } => serde_json::from_slice::<Vec<Place>>(resp.body)
            .map_err(malformed)?
            .into_iter()
            .map(Place::into_result)
            .collect(),
    })
}
