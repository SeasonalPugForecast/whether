//! Parsing of `/v1/forecast` (requested with `timeformat=unixtime`).

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use serde::Deserialize;
use whether_core::builder::SnapshotBuilder;
use whether_core::condition::Condition;
use whether_core::model::{Current, Daily, Hourly, Minutely, ResolvedLocation};
use whether_core::time::{UnixTime, UtcOffset};
use whether_core::units::{Celsius, Degrees, Hpa, Meters, Mm, Mps, Percent, finite};

use super::models;
use crate::error::ProviderError;
use crate::url::Coordinates;

#[derive(Deserialize)]
struct Response {
    latitude: f64,
    longitude: f64,
    elevation: Option<f32>,
    utc_offset_seconds: i32,
    timezone: Option<String>,
    timezone_abbreviation: Option<String>,
    current: Option<CurrentBlock>,
    hourly: Option<HourlyBlock>,
    daily: Option<DailyBlock>,
    minutely_15: Option<MinutelyBlock>,
}

#[derive(Deserialize)]
struct CurrentBlock {
    time: i64,
    temperature_2m: Option<f32>,
    relative_humidity_2m: Option<f32>,
    dew_point_2m: Option<f32>,
    apparent_temperature: Option<f32>,
    is_day: Option<u8>,
    precipitation: Option<f32>,
    weather_code: Option<u16>,
    cloud_cover: Option<f32>,
    pressure_msl: Option<f32>,
    wind_speed_10m: Option<f32>,
    wind_direction_10m: Option<f32>,
    wind_gusts_10m: Option<f32>,
    visibility: Option<f32>,
    uv_index: Option<f32>,
}

type Column = Vec<Option<f32>>;

#[derive(Deserialize)]
struct HourlyBlock {
    time: Vec<i64>,
    #[serde(default)]
    temperature_2m: Column,
    #[serde(default)]
    relative_humidity_2m: Column,
    #[serde(default)]
    apparent_temperature: Column,
    #[serde(default)]
    precipitation_probability: Column,
    #[serde(default)]
    precipitation: Column,
    #[serde(default)]
    weather_code: Vec<Option<u16>>,
    #[serde(default)]
    is_day: Vec<Option<u8>>,
    #[serde(default)]
    cloud_cover: Column,
    #[serde(default)]
    pressure_msl: Column,
    #[serde(default)]
    wind_speed_10m: Column,
    #[serde(default)]
    wind_direction_10m: Column,
    #[serde(default)]
    wind_gusts_10m: Column,
    #[serde(default)]
    visibility: Column,
    #[serde(default)]
    uv_index: Column,
}

#[derive(Deserialize)]
struct DailyBlock {
    time: Vec<i64>,
    #[serde(default)]
    weather_code: Vec<Option<u16>>,
    #[serde(default)]
    temperature_2m_max: Column,
    #[serde(default)]
    temperature_2m_min: Column,
    #[serde(default)]
    apparent_temperature_max: Column,
    #[serde(default)]
    apparent_temperature_min: Column,
    #[serde(default)]
    precipitation_sum: Column,
    #[serde(default)]
    precipitation_probability_max: Column,
    #[serde(default)]
    sunrise: Vec<Option<i64>>,
    #[serde(default)]
    sunset: Vec<Option<i64>>,
    #[serde(default)]
    daylight_duration: Column,
    #[serde(default)]
    uv_index_max: Column,
    #[serde(default)]
    wind_speed_10m_max: Column,
    #[serde(default)]
    wind_gusts_10m_max: Column,
    #[serde(default)]
    wind_direction_10m_dominant: Column,
    #[serde(default)]
    moon_phase: Column,
    #[serde(default)]
    moonrise: Vec<Option<i64>>,
    #[serde(default)]
    moonset: Vec<Option<i64>>,
}

#[derive(Deserialize)]
struct MinutelyBlock {
    time: Vec<i64>,
    #[serde(default)]
    precipitation: Column,
}

/// Checks that each present column has one value per time step.
fn check_len<T>(block: &str, name: &str, column: &[T], len: usize) -> Result<(), ProviderError> {
    if column.is_empty() || column.len() == len {
        Ok(())
    } else {
        Err(ProviderError::Malformed(alloc::format!(
            "{block}.{name} has {} values for {len} time steps",
            column.len()
        )))
    }
}

/// The value at `i`, if the column is present and the value finite.
fn at(column: &[Option<f32>], i: usize) -> Option<f32> {
    column.get(i).copied().flatten().and_then(finite)
}

fn at_copy<T: Copy>(column: &[Option<T>], i: usize) -> Option<T> {
    column.get(i).copied().flatten()
}

macro_rules! check_columns {
    ($block:literal, $b:expr, $len:expr; $($col:ident),+) => {
        $(check_len($block, stringify!($col), &$b.$col, $len)?;)+
    };
}

fn malformed(e: &serde_json::Error) -> ProviderError {
    ProviderError::Malformed(e.to_string())
}

/// Parses a forecast response to a request for the `configured` models into `out`
///: a multi-model response is merged in the list's order, and `out`
/// records the models that supplied data.
pub(crate) fn parse(
    body: &[u8],
    out: &mut SnapshotBuilder,
    configured: &[String],
) -> Result<(), ProviderError> {
    let r: Response = if configured.is_empty() {
        serde_json::from_slice(body).map_err(|e| malformed(&e))?
    } else {
        let mut value: serde_json::Value =
            serde_json::from_slice(body).map_err(|e| malformed(&e))?;
        let order = models::candidates(configured);
        if models::is_multi_model(&value, &order) {
            out.set_models(models::merge(&mut value, &order));
            serde_json::from_value(value).map_err(|e| malformed(&e))?
        } else {
            let r: Response = serde_json::from_value(value).map_err(|e| malformed(&e))?;
            // One model or none was sent: the plan's choice for this place.
            let place = Coordinates {
                latitude: r.latitude,
                longitude: r.longitude,
            };
            out.set_models(models::for_location(configured, &place));
            r
        }
    };
    let offset = UtcOffset(r.utc_offset_seconds);
    out.set_location(ResolvedLocation {
        name: String::new(),
        region: None,
        latitude: r.latitude,
        longitude: r.longitude,
        elevation_m: r.elevation.and_then(finite),
        utc_offset: offset,
        tz_name: r.timezone,
        tz_abbrev: r.timezone_abbreviation,
        station: None,
    });
    if let Some(c) = r.current {
        out.set_current(current(&c));
    }
    if let Some(h) = r.hourly {
        out.set_hourly(hourly(&h)?);
    }
    if let Some(d) = r.daily {
        out.set_daily(daily(&d, offset)?);
    }
    if let Some(m) = r.minutely_15 {
        out.set_minutely(minutely(&m)?);
    }
    Ok(())
}

fn condition(code: Option<u16>) -> Condition {
    code.map_or(Condition::Unknown, Condition::from_wmo)
}

fn current(c: &CurrentBlock) -> Current {
    Current {
        time: UnixTime(c.time),
        is_day: c.is_day.is_none_or(|d| d != 0),
        condition: condition(c.weather_code),
        temperature: c.temperature_2m.and_then(finite).map(Celsius),
        apparent: c.apparent_temperature.and_then(finite).map(Celsius),
        dew_point: c.dew_point_2m.and_then(finite).map(Celsius),
        humidity: c.relative_humidity_2m.and_then(finite).map(Percent),
        pressure_msl: c.pressure_msl.and_then(finite).map(Hpa),
        cloud_cover: c.cloud_cover.and_then(finite).map(Percent),
        wind_speed: c.wind_speed_10m.and_then(finite).map(Mps),
        wind_gust: c.wind_gusts_10m.and_then(finite).map(Mps),
        wind_dir: c.wind_direction_10m.and_then(finite).map(Degrees),
        visibility: c.visibility.and_then(finite).map(Meters),
        uv_index: c.uv_index.and_then(finite),
        precipitation: c.precipitation.and_then(finite).map(Mm),
    }
}

fn hourly(h: &HourlyBlock) -> Result<Vec<Hourly>, ProviderError> {
    let n = h.time.len();
    check_columns!("hourly", h, n; temperature_2m, relative_humidity_2m, apparent_temperature,
        precipitation_probability, precipitation, weather_code, is_day, cloud_cover,
        pressure_msl, wind_speed_10m, wind_direction_10m, wind_gusts_10m, visibility, uv_index);
    Ok((0..n)
        .map(|i| Hourly {
            time: UnixTime(h.time[i]),
            is_day: at_copy(&h.is_day, i).is_none_or(|d| d != 0),
            condition: condition(at_copy(&h.weather_code, i)),
            temperature: at(&h.temperature_2m, i).map(Celsius),
            apparent: at(&h.apparent_temperature, i).map(Celsius),
            precip_probability: at(&h.precipitation_probability, i).map(Percent),
            precipitation: at(&h.precipitation, i).map(Mm),
            wind_speed: at(&h.wind_speed_10m, i).map(Mps),
            wind_gust: at(&h.wind_gusts_10m, i).map(Mps),
            wind_dir: at(&h.wind_direction_10m, i).map(Degrees),
            humidity: at(&h.relative_humidity_2m, i).map(Percent),
            pressure_msl: at(&h.pressure_msl, i).map(Hpa),
            uv_index: at(&h.uv_index, i),
            visibility: at(&h.visibility, i).map(Meters),
            cloud_cover: at(&h.cloud_cover, i).map(Percent),
        })
        .collect())
}

fn daily(d: &DailyBlock, offset: UtcOffset) -> Result<Vec<Daily>, ProviderError> {
    let n = d.time.len();
    check_columns!("daily", d, n; weather_code, temperature_2m_max, temperature_2m_min,
        apparent_temperature_max, apparent_temperature_min, precipitation_sum, precipitation_probability_max, sunrise, sunset, daylight_duration,
        uv_index_max, wind_speed_10m_max, wind_gusts_10m_max, wind_direction_10m_dominant,
        moon_phase, moonrise, moonset);
    Ok((0..n)
        .map(|i| Daily {
            // Daily times are local midnight expressed in UTC.
            date: UnixTime(d.time[i]).local_date(offset),
            condition: condition(at_copy(&d.weather_code, i)),
            temp_max: at(&d.temperature_2m_max, i).map(Celsius),
            temp_min: at(&d.temperature_2m_min, i).map(Celsius),
            apparent_max: at(&d.apparent_temperature_max, i).map(Celsius),
            apparent_min: at(&d.apparent_temperature_min, i).map(Celsius),
            precip_sum: at(&d.precipitation_sum, i).map(Mm),
            precip_probability_max: at(&d.precipitation_probability_max, i).map(Percent),
            sunrise: at_copy(&d.sunrise, i).map(UnixTime),
            sunset: at_copy(&d.sunset, i).map(UnixTime),
            daylight_s: at(&d.daylight_duration, i).map(|s| libm::roundf(s.max(0.0)) as u32),
            uv_index_max: at(&d.uv_index_max, i),
            wind_speed_max: at(&d.wind_speed_10m_max, i).map(Mps),
            wind_gust_max: at(&d.wind_gusts_10m_max, i).map(Mps),
            wind_dir_dominant: at(&d.wind_direction_10m_dominant, i).map(Degrees),
            moon_phase: at(&d.moon_phase, i),
            moonrise: at_copy(&d.moonrise, i).map(UnixTime),
            moonset: at_copy(&d.moonset, i).map(UnixTime),
        })
        .collect())
}

fn minutely(m: &MinutelyBlock) -> Result<Vec<Minutely>, ProviderError> {
    let n = m.time.len();
    check_len("minutely_15", "precipitation", &m.precipitation, n)?;
    Ok((0..n)
        .map(|i| Minutely {
            time: UnixTime(m.time[i]),
            precipitation: at(&m.precipitation, i).map(Mm),
        })
        .collect())
}
