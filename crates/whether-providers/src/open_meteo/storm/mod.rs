//! Open-Meteo requests and parsers for the storm outlook: several models'
//! hours at a place, ensemble members, each model's latest run, an earlier
//! run, and a pressure grid around the place.
//!
//! Like every provider here this is sans-IO: builders return [`HttpRequest`]s
//! and parsers take the response bytes. Every storm request carries
//! [`Endpoint::Forecast`]; the caller picks the parser by the builder it used.
//! An Open-Meteo error body (`{"error":true,"reason":…}`) parses to
//! [`ProviderError::Rejected`] with the reason, whatever the status.
//!
//! Coordinates are sent with [`COORDINATE_DECIMALS`] decimals; grid points are
//! snapped to a quarter degree. Nothing secret goes into these URLs.

mod ensemble;
mod meta;
mod point;

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use serde::Deserialize;
use whether_core::model::ResolvedLocation;
use whether_core::storm::DETERMINISTIC_MODELS;
use whether_core::time::{UnixTime, UtcOffset};
use whether_core::units::finite;

use super::models::MAX_MODELS;
use crate::error::ProviderError;
use crate::http::{Endpoint, HttpRequest, Method};
use crate::url::{Coordinates, QueryBuilder};

pub use ensemble::{ENSEMBLE_HOURLY, ensemble_request, parse_ensembles};
pub use meta::{meta_request, meta_url, parse_run_meta, parse_update_interval};
pub use point::{
    POINT_HOURLY, PointForecast, parse_points, parse_single_run, point_request, single_run_request,
};

/// The forecast API, for the point and grid requests.
pub const FORECAST_URL: &str = "https://api.open-meteo.com/v1/forecast";
/// The Ensemble API.
pub const ENSEMBLE_URL: &str = "https://ensemble-api.open-meteo.com/v1/ensemble";
/// The Single Runs API, for one earlier run of a model.
pub const SINGLE_RUNS_URL: &str = "https://single-runs-api.open-meteo.com/v1/forecast";
/// The base of the per-domain `meta.json` files.
pub const META_BASE_URL: &str = "https://api.open-meteo.com/data";

/// Days the point and grid requests cover.
pub const FORECAST_DAYS: u8 = 10;
/// The most days one request may cover.
pub const MAX_FORECAST_DAYS: u8 = 16;
/// Decimals of the place's coordinates in a request.
pub const COORDINATE_DECIMALS: u8 = 2;

/// The `meta.json` domain of each deterministic forecast model, which says
/// when its latest run started.
///
/// GEM's global model runs in the `cmc_gem_gdps_15km` domain; the older
/// `cmc_gem_gdps` file stopped updating in May 2026.
pub const RUN_DOMAINS: [(&str, &str); 4] = [
    ("ecmwf_ifs025", "ecmwf_ifs025"),
    ("gfs_seamless", "ncep_gfs025"),
    ("gem_seamless", "cmc_gem_gdps_15km"),
    ("icon_seamless", "dwd_icon"),
];

/// The domain of each Ensemble API model. It names both the model's
/// `meta.json` and the suffix of its columns in an ensemble response
/// (`precipitation_member07_ncep_gefs025`).
pub const ENSEMBLE_DOMAINS: [(&str, &str); 2] = [
    ("ecmwf_ifs025", "ecmwf_ifs025_ensemble"),
    ("gfs025", "ncep_gefs025"),
];

fn lookup(map: &[(&'static str, &'static str)], model: &str) -> Option<&'static str> {
    map.iter().find(|(id, _)| *id == model).map(|(_, d)| *d)
}

/// The `meta.json` domain of deterministic model `model`, if known.
pub fn run_domain(model: &str) -> Option<&'static str> {
    lookup(&RUN_DOMAINS, model)
}

/// The domain of Ensemble API model `model`, if known.
pub fn ensemble_domain(model: &str) -> Option<&'static str> {
    lookup(&ENSEMBLE_DOMAINS, model)
}

/// A GET of `url` that the outlook needs.
fn get(url: String) -> HttpRequest {
    HttpRequest {
        url,
        headers: vec![],
        endpoint: Endpoint::Forecast,
        required: true,
        metered: false,
        method: Method::Get,
        body: None,
    }
}

/// The error body Open-Meteo sends with a 400.
#[derive(Deserialize)]
struct ErrorBody {
    error: bool,
    reason: Option<String>,
}

/// The error for a body that failed to parse with `e`: the reason of an
/// Open-Meteo error body, else [`ProviderError::Malformed`].
fn parse_failure(body: &[u8], e: &serde_json::Error) -> ProviderError {
    match serde_json::from_slice::<ErrorBody>(body) {
        Ok(ErrorBody {
            error: true,
            reason,
        }) => ProviderError::Rejected(reason.unwrap_or_else(|| "request rejected".to_string())),
        _ => ProviderError::Malformed(e.to_string()),
    }
}

/// Deserializes `body`, mapping an error body to [`ProviderError::Rejected`].
fn from_body<'a, T: Deserialize<'a>>(body: &'a [u8]) -> Result<T, ProviderError> {
    serde_json::from_slice(body).map_err(|e| parse_failure(body, &e))
}

/// `base` with the place's rounded coordinates.
fn at_place(base: &str, loc: &Coordinates) -> QueryBuilder {
    let (lat, lon) = loc.fmt_param(COORDINATE_DECIMALS);
    QueryBuilder::new(base)
        .param("latitude", lat)
        .param("longitude", lon)
}

/// The deterministic models a request carries: the first [`MAX_MODELS`] of
/// `models`, or [`DETERMINISTIC_MODELS`] when it is empty.
fn model_list<'a>(models: &[&'a str]) -> Vec<&'a str> {
    if models.is_empty() {
        DETERMINISTIC_MODELS.to_vec()
    } else {
        models.iter().take(MAX_MODELS).copied().collect()
    }
}

/// One place's response from the forecast, ensemble or single-runs API.
#[derive(Deserialize)]
struct PlaceResponse {
    latitude: f64,
    longitude: f64,
    elevation: Option<f32>,
    #[serde(default)]
    utc_offset_seconds: i32,
    timezone: Option<String>,
    timezone_abbreviation: Option<String>,
    hourly: Option<HourlyBlock>,
}

impl PlaceResponse {
    /// The place as Open-Meteo resolved it, without a name.
    fn location(&self) -> ResolvedLocation {
        ResolvedLocation {
            name: String::new(),
            region: None,
            latitude: self.latitude,
            longitude: self.longitude,
            elevation_m: self.elevation.and_then(finite),
            utc_offset: UtcOffset(self.utc_offset_seconds),
            tz_name: self.timezone.clone(),
            tz_abbrev: self.timezone_abbreviation.clone(),
            station: None,
        }
    }

    /// The hourly block, checked to have ascending times.
    fn hourly(self) -> Result<HourlyBlock, ProviderError> {
        let hourly = self
            .hourly
            .ok_or_else(|| ProviderError::Malformed("no hourly block".to_string()))?;
        if hourly.time.windows(2).any(|w| w[0] >= w[1]) {
            return Err(ProviderError::Malformed(
                "hourly times are not ascending".to_string(),
            ));
        }
        Ok(hourly)
    }
}

/// An hourly block: the times and every column by key.
#[derive(Deserialize)]
struct HourlyBlock {
    time: Vec<i64>,
    #[serde(flatten)]
    columns: BTreeMap<String, Vec<Option<f32>>>,
}

impl HourlyBlock {
    fn times(&self) -> Vec<UnixTime> {
        self.time.iter().copied().map(UnixTime).collect()
    }

    /// Removes column `key`, checked to have one value per time; values that
    /// are not finite become `None`.
    fn take_key(&mut self, key: &str) -> Result<Option<Vec<Option<f32>>>, ProviderError> {
        let Some(column) = self.columns.remove(key) else {
            return Ok(None);
        };
        if column.len() != self.time.len() {
            return Err(ProviderError::Malformed(alloc::format!(
                "hourly.{key} has {} values for {} time steps",
                column.len(),
                self.time.len()
            )));
        }
        Ok(Some(
            column.into_iter().map(|v| v.and_then(finite)).collect(),
        ))
    }

    /// Removes `var`'s column for `model` (`<var>_<model>`). With `single`, a
    /// request for one model, an unsuffixed `<var>` is the model's too.
    fn take(
        &mut self,
        var: &str,
        model: &str,
        single: bool,
    ) -> Result<Option<Vec<Option<f32>>>, ProviderError> {
        match self.take_key(&alloc::format!("{var}_{model}"))? {
            Some(column) => Ok(Some(column)),
            None if single => self.take_key(var),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use whether_core::storm::{DETERMINISTIC_MODELS, ENSEMBLE_MODELS};

    #[test]
    fn every_default_model_has_a_domain() {
        for model in DETERMINISTIC_MODELS {
            assert!(run_domain(model).is_some(), "{model}");
        }
        for model in ENSEMBLE_MODELS {
            assert!(ensemble_domain(model).is_some(), "{model}");
        }
        assert_eq!(
            DETERMINISTIC_MODELS.map(|m| run_domain(m).unwrap_or_default()),
            [
                "ecmwf_ifs025",
                "ncep_gfs025",
                "cmc_gem_gdps_15km",
                "dwd_icon"
            ]
        );
        assert_eq!(
            ENSEMBLE_MODELS.map(|m| ensemble_domain(m).unwrap_or_default()),
            ["ecmwf_ifs025_ensemble", "ncep_gefs025"]
        );
    }

    #[test]
    fn deterministic_and_ensemble_ids_map_separately() {
        // `ecmwf_ifs025` names a deterministic and an ensemble model.
        assert_eq!(run_domain("ecmwf_ifs025"), Some("ecmwf_ifs025"));
        assert_eq!(
            ensemble_domain("ecmwf_ifs025"),
            Some("ecmwf_ifs025_ensemble")
        );
        assert_eq!(run_domain("gfs025"), None);
        assert_eq!(ensemble_domain("gfs_seamless"), None);
        assert_eq!(run_domain("ukmo_seamless"), None);
    }

    #[test]
    fn error_bodies_carry_their_reason() {
        let body = br#"{"error":true,"reason":"Invalid value: Cannot initialize MultiDomains from invalid String value bogus"}"#;
        let r: Result<Vec<u8>, _> = from_body(body);
        assert_eq!(
            r,
            Err(ProviderError::Rejected(
                "Invalid value: Cannot initialize MultiDomains from invalid String value bogus"
                    .to_string()
            ))
        );
        let r: Result<Vec<u8>, _> = from_body(br#"{"error":true}"#);
        assert_eq!(
            r,
            Err(ProviderError::Rejected("request rejected".to_string()))
        );
        let r: Result<Vec<u8>, _> = from_body(br#"{"error":false,"reason":"x"}"#);
        assert!(matches!(r, Err(ProviderError::Malformed(_))));
        let r: Result<Vec<u8>, _> = from_body(b"<html>");
        assert!(matches!(r, Err(ProviderError::Malformed(_))));
    }

    #[test]
    fn coordinates_are_rounded_to_two_decimals() {
        let loc = Coordinates {
            latitude: 40.7143,
            longitude: -74.005_97,
        };
        assert_eq!(
            at_place(FORECAST_URL, &loc).finish(),
            "https://api.open-meteo.com/v1/forecast?latitude=40.71&longitude=-74.01"
        );
    }

    #[test]
    fn model_lists_default_and_stop_at_four() {
        assert_eq!(model_list(&[]), DETERMINISTIC_MODELS);
        assert_eq!(model_list(&["a", "b", "c", "d", "e"]), ["a", "b", "c", "d"]);
    }

    #[test]
    fn columns_lose_values_that_are_not_finite() {
        let mut block = HourlyBlock {
            time: vec![0, 1],
            columns: BTreeMap::from([("x_a".to_string(), vec![Some(f32::NAN), Some(1.0)])]),
        };
        assert_eq!(block.take("x", "a", false), Ok(Some(vec![None, Some(1.0)])));
        assert_eq!(block.take("x", "a", false), Ok(None));
    }
}
