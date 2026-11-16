//! The nearest station's latest observation (`/stations/{id}/observations/latest`):
//! measured current conditions over the forecast hour.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use serde::Deserialize;
use whether_core::builder::SnapshotBuilder;
use whether_core::model::{Current, Station};
use whether_core::time::{UnixTime, parse_rfc3339};
use whether_core::units::{Celsius, Degrees, Hpa, Meters, Mm, Mps, Percent};

use super::grid::canonical;
use super::icons::icon;
use crate::error::ProviderError;
use crate::http::FetchContext;

/// An observation older than this is not "now".
pub(super) const FRESH_FOR_S: i64 = 90 * 60;

#[derive(Deserialize)]
struct StationList {
    #[serde(default)]
    features: Vec<StationFeature>,
}

#[derive(Deserialize)]
struct StationFeature {
    properties: StationProperties,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StationProperties {
    station_identifier: String,
}

/// The first (nearest) station in a `/stations` list.
pub(super) fn first_station(body: &[u8]) -> Result<Option<String>, ProviderError> {
    let list: StationList =
        serde_json::from_slice(body).map_err(|e| ProviderError::Malformed(e.to_string()))?;
    Ok(list
        .features
        .into_iter()
        .map(|f| f.properties.station_identifier)
        // Station ids are letters and digits; anything else must not reach a URL.
        .find(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric())))
}

#[derive(Deserialize)]
struct Observation {
    geometry: Option<Geometry>,
    properties: Properties,
}

#[derive(Deserialize)]
struct Geometry {
    /// `[longitude, latitude]`.
    coordinates: Vec<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Properties {
    station_id: Option<String>,
    station_name: Option<String>,
    timestamp: String,
    icon: Option<String>,
    #[serde(default)]
    temperature: Measured,
    #[serde(default)]
    dewpoint: Measured,
    #[serde(default)]
    relative_humidity: Measured,
    #[serde(default)]
    wind_direction: Measured,
    #[serde(default)]
    wind_speed: Measured,
    #[serde(default)]
    wind_gust: Measured,
    #[serde(default)]
    sea_level_pressure: Measured,
    #[serde(default)]
    visibility: Measured,
    #[serde(default)]
    heat_index: Measured,
    #[serde(default)]
    wind_chill: Measured,
    #[serde(default)]
    precipitation_last_hour: Measured,
}

/// One measured value: `{ unitCode, value, qualityControl }`.
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Measured {
    unit_code: Option<String>,
    value: Option<f64>,
    quality_control: Option<String>,
}

impl Measured {
    /// The value in canonical units, unless missing or failed by quality control
    /// (`X`).
    fn get(&self) -> Option<f32> {
        if self.quality_control.as_deref() == Some("X") {
            return None;
        }
        canonical(self.unit_code.as_deref(), self.value?)
    }
}

/// The distance between two places, in metres (haversine).
fn distance_m(a: (f64, f64), b: (f64, f64)) -> u32 {
    const EARTH_RADIUS_M: f64 = 6_371_000.0;
    let (lat1, lat2) = (a.0.to_radians(), b.0.to_radians());
    let (dlat, dlon) = ((b.0 - a.0).to_radians(), (b.1 - a.1).to_radians());
    let h = libm::sin(dlat / 2.0) * libm::sin(dlat / 2.0)
        + libm::cos(lat1) * libm::cos(lat2) * libm::sin(dlon / 2.0) * libm::sin(dlon / 2.0);
    let d = 2.0 * EARTH_RADIUS_M * libm::asin(libm::sqrt(h).min(1.0));
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 0..20,000 km
    let metres = libm::round(d) as u32;
    metres
}

/// What an observation of `temperature` °C feels like: the station's heat
/// index or wind chill when it reports one. Stations report a wind chill only
/// at 10 °C or colder with at least 5 km/h of wind, and a heat index only
/// when it is hot, so a calm moment in a breezy night reports neither; then
/// the forecast hour's feels-like is moved by how far the measured temperature
/// is from the forecast's, which keeps the forecast's wind and humidity. With
/// no forecast to go by, it feels like the temperature.
fn feels_like(
    heat_index: Option<f32>,
    wind_chill: Option<f32>,
    temperature: f32,
    forecast: &Current,
) -> f32 {
    let shifted = || match (forecast.apparent, forecast.temperature) {
        (Some(apparent), Some(forecast)) => Some(apparent.0 + (temperature - forecast.0)),
        _ => None,
    };
    heat_index
        .or(wind_chill)
        .or_else(shifted)
        .unwrap_or(temperature)
}

/// Puts a station's observation in place of the forecast hour's current
/// conditions when it is fresh and its temperature passed quality control.
/// Values it lacks keep the forecast's; the condition is its icon's, else the
/// forecast hour's (automated stations often report no weather). A stale or
/// failed observation changes nothing.
pub(super) fn parse(
    body: &[u8],
    ctx: &FetchContext,
    out: &mut SnapshotBuilder,
) -> Result<(), ProviderError> {
    let obs: Observation =
        serde_json::from_slice(body).map_err(|e| ProviderError::Malformed(e.to_string()))?;
    let p = obs.properties;
    let Some(time) = parse_rfc3339(&p.timestamp) else {
        return Err(ProviderError::Malformed(alloc::format!(
            "observation time `{}`",
            p.timestamp
        )));
    };
    let Some(temperature) = p.temperature.get() else {
        return Ok(());
    };
    if ctx.now.0 - time.0 >= FRESH_FOR_S || time.0 > ctx.now.0 + 300 {
        return Ok(());
    }
    let forecast = out.current_mut().cloned().unwrap_or(Current {
        time,
        is_day: true,
        ..Current::default()
    });
    let (condition, is_day) = p
        .icon
        .as_deref()
        .and_then(icon)
        .unwrap_or((forecast.condition, forecast.is_day));
    let apparent = feels_like(
        p.heat_index.get(),
        p.wind_chill.get(),
        temperature,
        &forecast,
    );
    let observed = Current {
        time: UnixTime(time.0),
        is_day,
        condition,
        temperature: Some(Celsius(temperature)),
        apparent: Some(Celsius(apparent)),
        dew_point: p.dewpoint.get().map(Celsius).or(forecast.dew_point),
        humidity: p.relative_humidity.get().map(Percent).or(forecast.humidity),
        pressure_msl: p
            .sea_level_pressure
            .get()
            .map(Hpa)
            .or(forecast.pressure_msl),
        cloud_cover: forecast.cloud_cover,
        wind_speed: p.wind_speed.get().map(Mps).or(forecast.wind_speed),
        wind_gust: p.wind_gust.get().map(Mps).or(forecast.wind_gust),
        wind_dir: p.wind_direction.get().map(Degrees).or(forecast.wind_dir),
        visibility: p.visibility.get().map(Meters).or(forecast.visibility),
        uv_index: forecast.uv_index,
        precipitation: p.precipitation_last_hour.get().map(Mm),
    };
    out.set_current(observed);
    let station_at = obs.geometry.as_ref().and_then(|g| match g.coordinates[..] {
        [lon, lat, ..] => Some((lat, lon)),
        _ => None,
    });
    if let Some(location) = out.location_mut() {
        let place = (location.latitude, location.longitude);
        location.station = Some(Station {
            id: p.station_id.unwrap_or_default(),
            name: p.station_name.unwrap_or_default(),
            distance_m: station_at.map(|s| distance_m(place, s)),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distances() {
        // New York (ZIP 10001) to Central Park (KNYC), where its observations
        // place the station: about 6.6 km.
        let d = distance_m((40.7143, -74.005_97), (40.77, -73.98));
        assert!((6_400..6_800).contains(&d), "{d}");
        assert_eq!(distance_m((1.0, 2.0), (1.0, 2.0)), 0);
    }

    #[test]
    fn station_ids_stay_out_of_urls_unless_plain() {
        let list = |id: &str| {
            alloc::format!(
                r#"{{"features":[{{"properties":{{"stationIdentifier":"{id}"}}}},{{"properties":{{"stationIdentifier":"KPTW"}}}}]}}"#
            )
        };
        assert_eq!(
            first_station(list("KNYC").as_bytes()).unwrap().as_deref(),
            Some("KNYC")
        );
        assert_eq!(
            first_station(list("../x").as_bytes()).unwrap().as_deref(),
            Some("KPTW")
        );
        assert_eq!(first_station(br#"{"features":[]}"#).unwrap(), None);
    }

    /// A forecast hour of `temperature` °C that feels like `apparent` °C.
    fn hour(temperature: Option<f32>, apparent: Option<f32>) -> Current {
        Current {
            temperature: temperature.map(Celsius),
            apparent: apparent.map(Celsius),
            ..Current::default()
        }
    }

    #[test]
    fn a_reported_heat_index_or_wind_chill_is_the_feels_like() {
        let breezy = hour(Some(1.1), Some(-2.2));
        assert_eq!(feels_like(Some(33.0), None, 30.0, &breezy), 33.0);
        assert_eq!(feels_like(None, Some(-0.5), 2.2, &breezy), -0.5);
        assert_eq!(feels_like(Some(33.0), Some(-0.5), 2.2, &breezy), 33.0);
    }

    #[test]
    fn a_calm_moment_keeps_the_forecast_hours_wind_chill() {
        // A station reads 1.7 °C and calm, so no wind chill; the forecast hour
        // has 1.1 °C with an 11 km/h wind, feeling like -2.2 °C. Measured 0.6°
        // warmer, it feels like -1.6 °C.
        let breezy = hour(Some(1.1), Some(-2.2));
        let felt = feels_like(None, None, 1.7, &breezy);
        assert!((felt - -1.6).abs() < 1e-4, "{felt}");
    }

    #[test]
    fn a_mild_hour_feels_like_the_measured_temperature() {
        // Neither index applies: the forecast's feels-like is its temperature.
        let mild = hour(Some(16.0), Some(16.0));
        assert_eq!(feels_like(None, None, 15.0, &mild), 15.0);
    }

    #[test]
    fn without_a_forecast_feels_like_is_the_temperature() {
        assert_eq!(feels_like(None, None, 1.7, &hour(None, None)), 1.7);
        assert_eq!(feels_like(None, None, 1.7, &hour(Some(1.1), None)), 1.7);
        assert_eq!(feels_like(None, None, 1.7, &hour(None, Some(-2.2))), 1.7);
    }

    #[test]
    fn failed_quality_control_hides_a_value() {
        let m = |qc: &str| Measured {
            unit_code: Some("wmoUnit:degC".into()),
            value: Some(9.1),
            quality_control: Some(qc.into()),
        };
        assert_eq!(m("V").get(), Some(9.1));
        assert_eq!(m("X").get(), None);
    }
}
