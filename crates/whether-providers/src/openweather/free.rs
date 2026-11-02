//! Parsers for the free 2.5 endpoints.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use serde::Deserialize;
use whether_core::builder::SnapshotBuilder;
use whether_core::model::{AirQuality, Current, Hourly, ResolvedLocation};
use whether_core::time::{UnixTime, UtcOffset};
use whether_core::units::{Celsius, Degrees, Hpa, Meters, Mm, Mps, Percent, finite};

use super::codes::from_owm_id;
use crate::aggregate::{Step, daily_from_steps};
use crate::error::ProviderError;

#[derive(Deserialize)]
struct Coord {
    lat: f64,
    lon: f64,
}

#[derive(Deserialize)]
struct Weather {
    id: u16,
    #[serde(default)]
    icon: String,
}

#[derive(Deserialize)]
struct Main {
    temp: Option<f32>,
    feels_like: Option<f32>,
    temp_min: Option<f32>,
    temp_max: Option<f32>,
    pressure: Option<f32>,
    sea_level: Option<f32>,
    humidity: Option<f32>,
}

#[derive(Deserialize, Default)]
struct Wind {
    speed: Option<f32>,
    deg: Option<f32>,
    gust: Option<f32>,
}

#[derive(Deserialize, Default)]
struct Clouds {
    all: Option<f32>,
}

/// Rain or snow volume over the last 1 or 3 hours.
#[derive(Deserialize, Default)]
struct Volume {
    #[serde(rename = "1h")]
    one_hour: Option<f32>,
    #[serde(rename = "3h")]
    three_hours: Option<f32>,
}

impl Volume {
    fn amount(&self) -> Option<f32> {
        self.one_hour.or(self.three_hours)
    }
}

#[derive(Deserialize)]
struct Sys {
    country: Option<String>,
}

#[derive(Deserialize)]
struct CurrentResponse {
    coord: Coord,
    weather: Vec<Weather>,
    main: Main,
    visibility: Option<f32>,
    #[serde(default)]
    wind: Wind,
    #[serde(default)]
    clouds: Clouds,
    rain: Option<Volume>,
    snow: Option<Volume>,
    dt: i64,
    sys: Sys,
    timezone: i32,
    #[serde(default)]
    name: String,
}

fn malformed(e: &serde_json::Error) -> ProviderError {
    ProviderError::Malformed(e.to_string())
}

/// The precipitation total when either volume is reported; 0 when neither is.
fn precipitation(rain: Option<&Volume>, snow: Option<&Volume>) -> f32 {
    rain.and_then(Volume::amount).unwrap_or(0.0) + snow.and_then(Volume::amount).unwrap_or(0.0)
}

fn location(name: String, coord: &Coord, country: Option<String>, offset: i32) -> ResolvedLocation {
    ResolvedLocation {
        name,
        region: country,
        latitude: coord.lat,
        longitude: coord.lon,
        elevation_m: None,
        utc_offset: UtcOffset(offset),
        tz_name: None,
        tz_abbrev: None,
        station: None,
    }
}

fn weather_of(list: &[Weather]) -> (whether_core::condition::Condition, bool) {
    list.first()
        .map_or((whether_core::condition::Condition::Unknown, true), |w| {
            (from_owm_id(w.id), !w.icon.ends_with('n'))
        })
}

/// `/data/2.5/weather`.
pub(super) fn parse_current(body: &[u8], out: &mut SnapshotBuilder) -> Result<(), ProviderError> {
    let r: CurrentResponse = serde_json::from_slice(body).map_err(|e| malformed(&e))?;
    let (condition, is_day) = weather_of(&r.weather);
    let m = &r.main;
    if out.location().is_none() {
        out.set_location(location(r.name, &r.coord, r.sys.country, r.timezone));
    }
    out.set_current(Current {
        time: UnixTime(r.dt),
        is_day,
        condition,
        temperature: m.temp.and_then(finite).map(Celsius),
        apparent: m.feels_like.and_then(finite).map(Celsius),
        dew_point: None,
        humidity: m.humidity.and_then(finite).map(Percent),
        pressure_msl: m.sea_level.or(m.pressure).and_then(finite).map(Hpa),
        cloud_cover: r.clouds.all.and_then(finite).map(Percent),
        wind_speed: r.wind.speed.and_then(finite).map(Mps),
        wind_gust: r.wind.gust.and_then(finite).map(Mps),
        wind_dir: r.wind.deg.and_then(finite).map(Degrees),
        visibility: r.visibility.and_then(finite).map(Meters),
        uv_index: None,
        precipitation: Some(Mm(precipitation(r.rain.as_ref(), r.snow.as_ref()))),
    });
    Ok(())
}

#[derive(Deserialize)]
struct ForecastItem {
    dt: i64,
    main: Main,
    weather: Vec<Weather>,
    #[serde(default)]
    clouds: Clouds,
    #[serde(default)]
    wind: Wind,
    visibility: Option<f32>,
    pop: Option<f32>,
    rain: Option<Volume>,
    snow: Option<Volume>,
}

#[derive(Deserialize)]
struct City {
    #[serde(default)]
    name: String,
    coord: Coord,
    country: Option<String>,
    timezone: i32,
    sunrise: Option<i64>,
    sunset: Option<i64>,
}

#[derive(Deserialize)]
struct ForecastResponse {
    list: Vec<ForecastItem>,
    city: City,
}

/// `/data/2.5/forecast`: 3-hour steps plus daily aggregates.
pub(super) fn parse_forecast(body: &[u8], out: &mut SnapshotBuilder) -> Result<(), ProviderError> {
    let r: ForecastResponse = serde_json::from_slice(body).map_err(|e| malformed(&e))?;
    let offset = UtcOffset(r.city.timezone);
    if out.location().is_none() {
        out.set_location(location(
            r.city.name,
            &r.city.coord,
            r.city.country,
            r.city.timezone,
        ));
    }
    let steps: Vec<Step> = r
        .list
        .iter()
        .map(|item| {
            let (condition, is_day) = weather_of(&item.weather);
            let m = &item.main;
            Step {
                hourly: Hourly {
                    time: UnixTime(item.dt),
                    is_day,
                    condition,
                    temperature: m.temp.and_then(finite).map(Celsius),
                    apparent: m.feels_like.and_then(finite).map(Celsius),
                    precip_probability: item.pop.and_then(finite).map(|p| Percent(p * 100.0)),
                    precipitation: Some(Mm(precipitation(item.rain.as_ref(), item.snow.as_ref()))),
                    wind_speed: item.wind.speed.and_then(finite).map(Mps),
                    wind_gust: item.wind.gust.and_then(finite).map(Mps),
                    wind_dir: item.wind.deg.and_then(finite).map(Degrees),
                    humidity: m.humidity.and_then(finite).map(Percent),
                    pressure_msl: m.sea_level.or(m.pressure).and_then(finite).map(Hpa),
                    uv_index: None,
                    visibility: item.visibility.and_then(finite).map(Meters),
                    cloud_cover: item.clouds.all.and_then(finite).map(Percent),
                },
                temp_min: m.temp_min.and_then(finite),
                temp_max: m.temp_max.and_then(finite),
            }
        })
        .collect();
    let sun = r
        .city
        .sunrise
        .zip(r.city.sunset)
        .map(|(a, b)| (UnixTime(a), UnixTime(b)));
    out.set_daily(daily_from_steps(&steps, offset, sun));
    out.set_hourly(steps.into_iter().map(|s| s.hourly).collect());
    Ok(())
}

#[derive(Deserialize)]
struct AirMain {
    aqi: u8,
}

#[derive(Deserialize, Default)]
struct Components {
    co: Option<f32>,
    no2: Option<f32>,
    o3: Option<f32>,
    so2: Option<f32>,
    pm2_5: Option<f32>,
    pm10: Option<f32>,
}

#[derive(Deserialize)]
struct AirItem {
    main: AirMain,
    #[serde(default)]
    components: Components,
    dt: i64,
}

#[derive(Deserialize)]
struct AirResponse {
    list: Vec<AirItem>,
}

/// `/data/2.5/air_pollution`.
pub(super) fn parse_air(body: &[u8], out: &mut SnapshotBuilder) -> Result<(), ProviderError> {
    let r: AirResponse = serde_json::from_slice(body).map_err(|e| malformed(&e))?;
    let item = r
        .list
        .first()
        .ok_or_else(|| ProviderError::Malformed("air pollution list is empty".to_string()))?;
    let c = &item.components;
    out.set_air_quality(AirQuality {
        time: UnixTime(item.dt),
        us_aqi: None,
        european_aqi: None,
        owm_index: Some(item.main.aqi),
        pm2_5: c.pm2_5,
        pm10: c.pm10,
        o3: c.o3,
        no2: c.no2,
        so2: c.so2,
        co: c.co,
    });
    Ok(())
}
