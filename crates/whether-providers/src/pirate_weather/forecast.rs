//! The forecast response: Dark Sky's shape (`currently`, `minutely`, `hourly`,
//! `daily`, `alerts`) with `units=si` and `version=2`.
//!
//! Units under `units=si`: °C, hPa, m/s, kilometres of visibility, precipitation
//! intensity in mm/h of liquid equivalent, and fractions (0–1) for humidity, cloud
//! cover and probabilities. Times are Unix seconds; a day's `time` is its local
//! midnight, and `offset` is the location's UTC offset in hours.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use serde::Deserialize;
use whether_core::builder::SnapshotBuilder;
use whether_core::model::{Alert, Current, Daily, Hourly, Minutely, ResolvedLocation};
use whether_core::scales::Level;
use whether_core::time::{UnixTime, UtcOffset};
use whether_core::units::{Celsius, Degrees, Hpa, Meters, Mm, Mps, Percent, finite};

use super::icons;
use crate::error::ProviderError;

/// Steps of the nowcast: the `minutely` block covers the next hour, as four
/// 15-minute precipitation amounts.
const NOWCAST_STEPS: usize = 4;
const MINUTES_PER_STEP: usize = 15;
/// `expires` when an alert has no end.
const NO_EXPIRY: i64 = -999;
/// How long an alert without an end stays up: until a fetch no longer lists it.
const OPEN_ENDED_ALERT_S: i64 = 24 * 3600;

#[derive(Deserialize)]
struct Response {
    #[serde(default)]
    latitude: Option<f64>,
    #[serde(default)]
    longitude: Option<f64>,
    #[serde(default)]
    timezone: Option<String>,
    /// Hours east of UTC, possibly fractional (5.5).
    #[serde(default)]
    offset: Option<f64>,
    #[serde(default)]
    elevation: Option<f32>,
    #[serde(default)]
    currently: Option<Step>,
    #[serde(default)]
    minutely: Option<Block<Minute>>,
    #[serde(default)]
    hourly: Option<Block<Step>>,
    #[serde(default)]
    daily: Option<Block<Day>>,
    #[serde(default)]
    alerts: Option<Vec<AlertRecord>>,
}

#[derive(Deserialize)]
struct Block<T> {
    #[serde(default = "Vec::new")]
    data: Vec<T>,
}

/// `currently` and each `hourly` step.
#[derive(Default, Deserialize)]
#[serde(default)]
struct Step {
    time: i64,
    icon: Option<String>,
    #[serde(rename = "precipIntensity")]
    precip_intensity: Option<f32>,
    #[serde(rename = "precipProbability")]
    precip_probability: Option<f32>,
    temperature: Option<f32>,
    #[serde(rename = "apparentTemperature")]
    apparent_temperature: Option<f32>,
    #[serde(rename = "dewPoint")]
    dew_point: Option<f32>,
    humidity: Option<f32>,
    pressure: Option<f32>,
    #[serde(rename = "windSpeed")]
    wind_speed: Option<f32>,
    #[serde(rename = "windGust")]
    wind_gust: Option<f32>,
    #[serde(rename = "windBearing")]
    wind_bearing: Option<f32>,
    #[serde(rename = "cloudCover")]
    cloud_cover: Option<f32>,
    #[serde(rename = "uvIndex")]
    uv_index: Option<f32>,
    /// Kilometres.
    visibility: Option<f32>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Minute {
    time: i64,
    #[serde(rename = "precipIntensity")]
    precip_intensity: Option<f32>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Day {
    time: i64,
    icon: Option<String>,
    #[serde(rename = "sunriseTime")]
    sunrise: Option<i64>,
    #[serde(rename = "sunsetTime")]
    sunset: Option<i64>,
    #[serde(rename = "moonPhase")]
    moon_phase: Option<f32>,
    #[serde(rename = "precipIntensity")]
    precip_intensity: Option<f32>,
    #[serde(rename = "precipIntensityMax")]
    precip_intensity_max: Option<f32>,
    #[serde(rename = "precipProbability")]
    precip_probability: Option<f32>,
    #[serde(rename = "temperatureMin")]
    temperature_min: Option<f32>,
    #[serde(rename = "temperatureMax")]
    temperature_max: Option<f32>,
    #[serde(rename = "apparentTemperatureMin")]
    apparent_min: Option<f32>,
    #[serde(rename = "apparentTemperatureMax")]
    apparent_max: Option<f32>,
    #[serde(rename = "windGust")]
    wind_gust: Option<f32>,
    #[serde(rename = "windBearing")]
    wind_bearing: Option<f32>,
    #[serde(rename = "uvIndex")]
    uv_index: Option<f32>,
    #[serde(rename = "cloudCover")]
    cloud_cover: Option<f32>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct AlertRecord {
    title: String,
    severity: Option<String>,
    time: Option<i64>,
    expires: Option<i64>,
    description: Option<String>,
    uri: Option<String>,
}

fn malformed(message: impl core::fmt::Display) -> ProviderError {
    ProviderError::Malformed(alloc::format!("Pirate Weather forecast: {message}"))
}

/// A 0–1 fraction as a percentage.
fn percent(fraction: Option<f32>) -> Option<Percent> {
    fraction
        .and_then(finite)
        .map(|f| Percent((f * 100.0).clamp(0.0, 100.0)))
}

/// A precipitation rate in mm/h as the millimetres that fall in `hours`.
fn millimetres(rate: Option<f32>, hours: f32) -> Option<Mm> {
    rate.and_then(finite).map(|r| Mm(r.max(0.0) * hours))
}

fn alert_level(severity: Option<&str>) -> Level {
    match severity {
        Some("Extreme") => Level::Extreme,
        Some("Severe") => Level::VeryPoor,
        Some("Minor") => Level::Fair,
        // Moderate and Unknown.
        _ => Level::Moderate,
    }
}

/// The local offset: the response's hours, else the caller's hint, else UTC.
fn offset_of(r: &Response, hint: Option<UtcOffset>) -> UtcOffset {
    match r.offset.filter(|h| h.is_finite() && h.abs() <= 18.0) {
        Some(hours) => UtcOffset(libm::round(hours * 3600.0) as i32),
        None => hint.unwrap_or(UtcOffset(0)),
    }
}

/// Whether `t` is daylight by the days' sun times: between a sunrise and the sunset
/// after it. `None` when `suns` (empty, or not covering `t`) cannot tell.
fn by_sun(t: i64, suns: &[(i64, i64)]) -> Option<bool> {
    if suns.iter().any(|&(rise, set)| (rise..set).contains(&t)) {
        return Some(true);
    }
    let (first, last) = (suns.first()?, suns.last()?);
    (first.0 - 86_400..last.1).contains(&t).then_some(false)
}

fn is_day(t: i64, icon: Option<&str>, suns: &[(i64, i64)], offset: UtcOffset) -> bool {
    by_sun(t, suns)
        .or_else(|| icons::icon_is_day(icon))
        .unwrap_or_else(|| (6..18).contains(&UnixTime(t).to_local(offset).hour))
}

fn hourly(p: &Step, suns: &[(i64, i64)], offset: UtcOffset) -> Hourly {
    Hourly {
        time: UnixTime(p.time),
        is_day: is_day(p.time, p.icon.as_deref(), suns, offset),
        condition: icons::condition(p.icon.as_deref(), p.precip_intensity, p.cloud_cover),
        temperature: p.temperature.and_then(finite).map(Celsius),
        apparent: p.apparent_temperature.and_then(finite).map(Celsius),
        precip_probability: percent(p.precip_probability),
        precipitation: millimetres(p.precip_intensity, 1.0),
        wind_speed: p.wind_speed.and_then(finite).map(Mps),
        wind_gust: p.wind_gust.and_then(finite).map(Mps),
        wind_dir: p.wind_bearing.and_then(finite).map(Degrees),
        humidity: percent(p.humidity),
        pressure_msl: p.pressure.and_then(finite).map(Hpa),
        uv_index: p.uv_index.and_then(finite),
        visibility: p.visibility.and_then(finite).map(|km| Meters(km * 1000.0)),
        cloud_cover: percent(p.cloud_cover),
    }
}

fn current(p: &Step, suns: &[(i64, i64)], offset: UtcOffset) -> Current {
    let h = hourly(p, suns, offset);
    Current {
        time: h.time,
        is_day: h.is_day,
        condition: h.condition,
        temperature: h.temperature,
        apparent: h.apparent,
        dew_point: p.dew_point.and_then(finite).map(Celsius),
        humidity: h.humidity,
        pressure_msl: h.pressure_msl,
        cloud_cover: h.cloud_cover,
        wind_speed: h.wind_speed,
        wind_gust: h.wind_gust,
        wind_dir: h.wind_dir,
        visibility: h.visibility,
        uv_index: h.uv_index,
        precipitation: h.precipitation,
    }
}

fn daily(d: &Day, offset: UtcOffset) -> Daily {
    // The stamp is the day's local midnight; reading its noon keeps the date right
    // when the clocks change that night.
    let date = UnixTime(d.time + 12 * 3600).local_date(offset);
    let rate = d.precip_intensity_max.or(d.precip_intensity);
    let daylight = match (d.sunrise, d.sunset) {
        (Some(rise), Some(set)) if set > rise => u32::try_from(set - rise).ok(),
        _ => None,
    };
    Daily {
        date,
        condition: icons::condition(d.icon.as_deref(), rate, d.cloud_cover),
        temp_max: d.temperature_max.and_then(finite).map(Celsius),
        temp_min: d.temperature_min.and_then(finite).map(Celsius),
        apparent_max: d.apparent_max.and_then(finite).map(Celsius),
        apparent_min: d.apparent_min.and_then(finite).map(Celsius),
        // The day's average rate over its 24 hours. (The recording's one wet day also
        // gives `liquidAccumulation`, 40% higher; which is right waits for a rainy day
        // to compare against the hourly steps.)
        precip_sum: millimetres(d.precip_intensity, 24.0),
        precip_probability_max: percent(d.precip_probability),
        sunrise: d.sunrise.map(UnixTime),
        sunset: d.sunset.map(UnixTime),
        daylight_s: daylight,
        uv_index_max: d.uv_index.and_then(finite),
        // Only the day's average speed is given, which is not a maximum.
        wind_speed_max: None,
        wind_gust_max: d.wind_gust.and_then(finite).map(Mps),
        wind_dir_dominant: d.wind_bearing.and_then(finite).map(Degrees),
        moon_phase: d.moon_phase.and_then(finite),
        moonrise: None,
        moonset: None,
    }
}

/// The next hour's minutes as four 15-minute amounts (mm per step).
fn nowcast(minutes: &[Minute]) -> Vec<Minutely> {
    minutes
        .as_chunks::<MINUTES_PER_STEP>()
        .0
        .iter()
        .take(NOWCAST_STEPS)
        .map(|chunk| Minutely {
            time: UnixTime(chunk[0].time),
            precipitation: Some(Mm(chunk
                .iter()
                .filter_map(|m| m.precip_intensity.and_then(finite))
                .map(|rate| rate.max(0.0) / 60.0)
                .sum())),
        })
        .collect()
}

/// The alert's link when it is a web address; any other scheme is dropped, as the
/// iPhone does for Apple's links.
fn web_link(uri: Option<String>) -> Option<String> {
    uri.map(|u| u.trim().to_string())
        .filter(|u| u.starts_with("https://") || u.starts_with("http://"))
}

fn alerts(records: Vec<AlertRecord>, now: UnixTime, out: &mut SnapshotBuilder) {
    for a in records {
        let title = a.title.trim();
        let Some(start) = a.time else { continue };
        let end = match a.expires {
            Some(NO_EXPIRY) | None => now.0 + OPEN_ENDED_ALERT_S,
            Some(end) => end,
        };
        if title.is_empty() || end <= now.0 {
            continue;
        }
        out.push_alert(Alert {
            id: None,
            title: title.to_string(),
            description: a.description.unwrap_or_default().trim().to_string(),
            severity: alert_level(a.severity.as_deref()),
            start: UnixTime(start),
            end: UnixTime(end),
            source: "Pirate Weather".to_string(),
            checked_at: Some(now),
            url: web_link(a.uri),
        });
    }
}

/// Parses a forecast into the builder: current conditions, the next hour as a
/// nowcast, hourly steps, days and alerts, whichever blocks the request kept.
pub(super) fn parse(
    body: &[u8],
    now: UnixTime,
    offset_hint: Option<UtcOffset>,
    out: &mut SnapshotBuilder,
) -> Result<(), ProviderError> {
    let r: Response = serde_json::from_slice(body).map_err(malformed)?;
    let (Some(latitude), Some(longitude)) = (r.latitude, r.longitude) else {
        return Err(malformed("no coordinates"));
    };
    let currently = r
        .currently
        .as_ref()
        .ok_or_else(|| malformed("no current conditions"))?;
    let offset = offset_of(&r, offset_hint);
    out.set_location(ResolvedLocation {
        name: String::new(),
        region: None,
        latitude,
        longitude,
        elevation_m: r.elevation.and_then(finite),
        utc_offset: offset,
        tz_name: r.timezone.clone().filter(|z| !z.is_empty()),
        tz_abbrev: None,
        station: None,
    });
    // Every listed day needs both times: a polar day or night has neither, and the
    // hours then fall back to the icon.
    let suns: Vec<(i64, i64)> = r
        .daily
        .iter()
        .flat_map(|b| &b.data)
        .map(|d| d.sunrise.zip(d.sunset))
        .collect::<Option<_>>()
        .unwrap_or_default();
    out.set_current(current(currently, &suns, offset));
    if let Some(block) = &r.minutely {
        out.set_minutely(nowcast(&block.data));
    }
    if let Some(block) = &r.hourly {
        out.set_hourly(
            block
                .data
                .iter()
                .map(|p| hourly(p, &suns, offset))
                .collect(),
        );
    }
    if let Some(block) = &r.daily {
        out.set_daily(block.data.iter().map(|d| daily(d, offset)).collect());
    }
    if let Some(records) = r.alerts {
        alerts(records, now, out);
    }
    Ok(())
}
