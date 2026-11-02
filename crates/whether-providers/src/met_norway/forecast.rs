//! `locationforecast/2.0`: a `GeoJSON` time series in UTC. Entries have instant
//! values plus `next_1_hours` (about the first 2.5 days) and `next_6_hours`
//! periods; days are aggregated in the location's local time.

use alloc::string::String;
use alloc::vec::Vec;

use serde::Deserialize;
use whether_core::builder::SnapshotBuilder;
use whether_core::model::{Current, Hourly, ResolvedLocation};
use whether_core::time::{UnixTime, UtcOffset, parse_rfc3339};
use whether_core::units::{Celsius, Degrees, Hpa, Mm, Mps, Percent, finite};

use super::symbols;
use crate::aggregate::{Step, daily_from_steps};
use crate::error::ProviderError;

#[derive(Deserialize)]
struct Response {
    geometry: Geometry,
    properties: Properties,
}

#[derive(Deserialize)]
struct Geometry {
    /// `[longitude, latitude, altitude]`.
    coordinates: Vec<f64>,
}

#[derive(Deserialize)]
struct Properties {
    timeseries: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    time: String,
    data: Data,
}

#[derive(Deserialize)]
struct Data {
    instant: Instant,
    #[serde(default)]
    next_1_hours: Option<Period>,
    #[serde(default)]
    next_6_hours: Option<Period>,
}

#[derive(Deserialize)]
struct Instant {
    #[serde(default)]
    details: InstantDetails,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct InstantDetails {
    air_temperature: Option<f32>,
    apparent_air_temperature: Option<f32>,
    relative_humidity: Option<f32>,
    dew_point_temperature: Option<f32>,
    air_pressure_at_sea_level: Option<f32>,
    wind_speed: Option<f32>,
    wind_from_direction: Option<f32>,
    wind_speed_of_gust: Option<f32>,
    cloud_area_fraction: Option<f32>,
    ultraviolet_index_clear_sky: Option<f32>,
}

#[derive(Deserialize)]
struct Period {
    #[serde(default)]
    summary: Option<Summary>,
    #[serde(default)]
    details: PeriodDetails,
}

#[derive(Deserialize)]
struct Summary {
    symbol_code: String,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct PeriodDetails {
    precipitation_amount: Option<f32>,
    probability_of_precipitation: Option<f32>,
    air_temperature_max: Option<f32>,
    air_temperature_min: Option<f32>,
}

fn malformed(e: &serde_json::Error) -> ProviderError {
    ProviderError::Malformed(alloc::format!("MET Norway forecast: {e}"))
}

/// Day or night by the local hour, for symbol codes without a suffix.
fn daytime_by_clock(t: UnixTime, offset: UtcOffset) -> bool {
    (6..18).contains(&t.to_local(offset).hour)
}

/// One time series entry as an hourly step, using the 1-hour period when there is
/// one and the 6-hour period otherwise.
fn step(entry: &Entry, time: UnixTime, offset: UtcOffset) -> Step {
    let i = &entry.data.instant.details;
    let one = entry.data.next_1_hours.as_ref();
    let period = one.or(entry.data.next_6_hours.as_ref());
    let (condition, is_day) = period
        .and_then(|p| p.summary.as_ref())
        .map_or((whether_core::condition::Condition::Unknown, None), |s| {
            symbols::condition(&s.symbol_code)
        });
    let details = period.map(|p| &p.details);
    let six = if one.is_none() {
        entry.data.next_6_hours.as_ref().map(|p| &p.details)
    } else {
        None
    };
    Step {
        hourly: Hourly {
            time,
            is_day: is_day.unwrap_or_else(|| daytime_by_clock(time, offset)),
            condition,
            temperature: i.air_temperature.and_then(finite).map(Celsius),
            apparent: i.apparent_air_temperature.and_then(finite).map(Celsius),
            precip_probability: details
                .and_then(|d| d.probability_of_precipitation)
                .and_then(finite)
                .map(Percent),
            precipitation: details
                .and_then(|d| d.precipitation_amount)
                .and_then(finite)
                .map(Mm),
            wind_speed: i.wind_speed.and_then(finite).map(Mps),
            wind_gust: i.wind_speed_of_gust.and_then(finite).map(Mps),
            wind_dir: i.wind_from_direction.and_then(finite).map(Degrees),
            humidity: i.relative_humidity.and_then(finite).map(Percent),
            pressure_msl: i.air_pressure_at_sea_level.and_then(finite).map(Hpa),
            uv_index: i.ultraviolet_index_clear_sky.and_then(finite),
            visibility: None,
            cloud_cover: i.cloud_area_fraction.and_then(finite).map(Percent),
        },
        // Only 6-hour periods give extremes; 1-hour entries use their instants.
        temp_max: six.and_then(|d| d.air_temperature_max).and_then(finite),
        temp_min: six.and_then(|d| d.air_temperature_min).and_then(finite),
    }
}

/// Parses the forecast into current conditions, hourly steps and days. Times are
/// UTC; `offset` (the location's) groups days and labels them, and defaults to
/// UTC when unknown.
pub(super) fn parse(
    body: &[u8],
    now: UnixTime,
    offset: Option<UtcOffset>,
    out: &mut SnapshotBuilder,
) -> Result<(), ProviderError> {
    let r: Response = serde_json::from_slice(body).map_err(|e| malformed(&e))?;
    let offset = offset.unwrap_or(UtcOffset(0));
    if out.location().is_none() {
        let c = &r.geometry.coordinates;
        let (Some(&lon), Some(&lat)) = (c.first(), c.get(1)) else {
            return Err(ProviderError::Malformed(
                "MET Norway forecast: no coordinates".into(),
            ));
        };
        out.set_location(ResolvedLocation {
            name: String::new(),
            region: None,
            latitude: lat,
            longitude: lon,
            elevation_m: c.get(2).map(|&a| a as f32),
            utc_offset: offset,
            tz_name: None,
            tz_abbrev: None,
            station: None,
        });
    }
    let entries: Vec<(&Entry, UnixTime)> = r
        .properties
        .timeseries
        .iter()
        .filter_map(|e| parse_rfc3339(&e.time).map(|t| (e, t)))
        .collect();
    if entries.is_empty() {
        return Err(ProviderError::Malformed(
            "MET Norway forecast: no time series".into(),
        ));
    }
    // Current: the entry whose hour contains `now`, else the nearest.
    let (current_entry, current_time) = entries
        .iter()
        .find(|(_, t)| t.0 <= now.0 && now.0 < t.0 + 3_600)
        .or_else(|| {
            entries
                .iter()
                .min_by_key(|(_, t)| (t.0 - now.0).unsigned_abs())
        })
        .copied()
        .ok_or_else(|| ProviderError::Malformed("MET Norway forecast: empty".into()))?;
    let c = step(current_entry, current_time, offset).hourly;
    let d = &current_entry.data.instant.details;
    out.set_current(Current {
        time: c.time,
        is_day: c.is_day,
        condition: c.condition,
        temperature: c.temperature,
        apparent: c.apparent,
        dew_point: d.dew_point_temperature.and_then(finite).map(Celsius),
        humidity: c.humidity,
        pressure_msl: c.pressure_msl,
        cloud_cover: c.cloud_cover,
        wind_speed: c.wind_speed,
        wind_gust: c.wind_gust,
        wind_dir: c.wind_dir,
        visibility: None,
        uv_index: c.uv_index,
        precipitation: c.precipitation,
    });
    let steps: Vec<Step> = entries.iter().map(|(e, t)| step(e, *t, offset)).collect();
    out.set_daily(daily_from_steps(&steps, offset, None));
    out.set_hourly(
        entries
            .iter()
            .zip(steps)
            .filter(|((e, _), _)| e.data.next_1_hours.is_some())
            .map(|(_, s)| s.hourly)
            .collect(),
    );
    Ok(())
}
