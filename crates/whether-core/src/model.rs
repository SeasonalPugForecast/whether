//! The normalized weather snapshot that every provider produces.
//!
//! All quantities are canonical units; `None` means the provider did not supply
//! the value.

use alloc::string::String;
use alloc::vec::Vec;

use crate::condition::Condition;
use crate::scales::Level;
use crate::time::{LocalDate, UnixTime, UtcOffset};
use crate::units::{Celsius, Degrees, Hpa, Meters, Mm, Mps, Percent};

/// A weather data provider.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ProviderId {
    /// OpenWeather (openweathermap.org).
    #[cfg_attr(feature = "serde", serde(rename = "openweather"))]
    OpenWeather,
    /// Open-Meteo (open-meteo.com).
    #[cfg_attr(feature = "serde", serde(rename = "open-meteo"))]
    OpenMeteo,
    /// MET Norway (api.met.no).
    #[cfg_attr(feature = "serde", serde(rename = "met-norway"))]
    MetNorway,
    /// The US National Weather Service (api.weather.gov): official alerts as a
    /// supplement, and the forecast as a primary provider.
    #[cfg_attr(feature = "serde", serde(rename = "nws"))]
    Nws,
    /// NOAA's Regional Climate Centers (data.rcc-acis.org): station history,
    /// normals and records for US places; never a forecast.
    #[cfg_attr(feature = "serde", serde(rename = "acis"))]
    Acis,
    /// Pirate Weather (pirateweather.net): a keyed Dark Sky-style forecast from a
    /// blend of NOAA and other models, billed per month.
    #[cfg_attr(feature = "serde", serde(rename = "pirate-weather"))]
    PirateWeather,
}

impl ProviderId {
    /// The identifier used in configuration (`"openweather"`, …).
    pub fn id(self) -> &'static str {
        match self {
            Self::OpenWeather => "openweather",
            Self::OpenMeteo => "open-meteo",
            Self::MetNorway => "met-norway",
            Self::Nws => "nws",
            Self::Acis => "acis",
            Self::PirateWeather => "pirate-weather",
        }
    }

    /// The human-readable provider name.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::OpenWeather => "OpenWeather",
            Self::OpenMeteo => "Open-Meteo",
            Self::MetNorway => "MET Norway",
            Self::Nws => "National Weather Service",
            Self::Acis => "NOAA Regional Climate Centers",
            Self::PirateWeather => "Pirate Weather",
        }
    }

    /// The attribution its data license requires when this provider is the primary
    /// source.
    pub fn attribution(self) -> &'static str {
        match self {
            Self::OpenWeather => "Weather data provided by OpenWeather",
            Self::OpenMeteo => "Weather data by Open-Meteo.com",
            Self::MetNorway => "Data from MET Norway",
            Self::Nws => "Forecast by the NOAA National Weather Service",
            Self::Acis => "History from NOAA's Regional Climate Centers (ACIS)",
            Self::PirateWeather => "Weather data by Pirate Weather",
        }
    }

    /// A short credit used when this provider only filled gaps.
    pub fn short_credit(self) -> &'static str {
        match self {
            Self::OpenWeather => "OpenWeather",
            Self::OpenMeteo => "Open-Meteo.com",
            Self::MetNorway => "MET Norway",
            Self::Nws => "NWS alerts",
            Self::Acis => "NOAA RCC (ACIS)",
            Self::PirateWeather => "Pirate Weather",
        }
    }
}

/// A location resolved to coordinates and a UTC offset.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ResolvedLocation {
    /// Display name, e.g. "New York".
    pub name: String,
    /// Region line, e.g. "New York, US".
    pub region: Option<String>,
    /// Latitude in degrees.
    pub latitude: f64,
    /// Longitude in degrees.
    pub longitude: f64,
    /// Elevation in metres.
    pub elevation_m: Option<f32>,
    /// The location's UTC offset at fetch time.
    pub utc_offset: UtcOffset,
    /// IANA time-zone name, when known.
    pub tz_name: Option<String>,
    /// Time-zone abbreviation, e.g. "EDT".
    pub tz_abbrev: Option<String>,
    /// The weather station whose observation gave the current conditions
    /// (the NWS); `None` when they are a forecast.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub station: Option<Station>,
}

/// A weather station that reported the current conditions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Station {
    /// Its identifier, e.g. "KNYC".
    pub id: String,
    /// Its name, e.g. "Central Park".
    pub name: String,
    /// How far it is from the place, in metres.
    pub distance_m: Option<u32>,
}

/// Conditions now.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[allow(missing_docs)]
pub struct Current {
    pub time: UnixTime,
    pub is_day: bool,
    pub condition: Condition,
    pub temperature: Option<Celsius>,
    pub apparent: Option<Celsius>,
    pub dew_point: Option<Celsius>,
    pub humidity: Option<Percent>,
    pub pressure_msl: Option<Hpa>,
    pub cloud_cover: Option<Percent>,
    pub wind_speed: Option<Mps>,
    pub wind_gust: Option<Mps>,
    pub wind_dir: Option<Degrees>,
    pub visibility: Option<Meters>,
    pub uv_index: Option<f32>,
    pub precipitation: Option<Mm>,
}

/// One forecast step (1 h; 3 h for OpenWeather's free API).
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[allow(missing_docs)]
pub struct Hourly {
    pub time: UnixTime,
    pub is_day: bool,
    pub condition: Condition,
    pub temperature: Option<Celsius>,
    pub apparent: Option<Celsius>,
    pub precip_probability: Option<Percent>,
    pub precipitation: Option<Mm>,
    pub wind_speed: Option<Mps>,
    pub wind_gust: Option<Mps>,
    pub wind_dir: Option<Degrees>,
    pub humidity: Option<Percent>,
    pub pressure_msl: Option<Hpa>,
    pub uv_index: Option<f32>,
    pub visibility: Option<Meters>,
    pub cloud_cover: Option<Percent>,
}

/// One local calendar day.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[allow(missing_docs)]
pub struct Daily {
    pub date: LocalDate,
    pub condition: Condition,
    pub temp_max: Option<Celsius>,
    pub temp_min: Option<Celsius>,
    /// The day's highest and lowest "feels like" temperatures.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub apparent_max: Option<Celsius>,
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub apparent_min: Option<Celsius>,
    pub precip_sum: Option<Mm>,
    pub precip_probability_max: Option<Percent>,
    pub sunrise: Option<UnixTime>,
    pub sunset: Option<UnixTime>,
    pub daylight_s: Option<u32>,
    pub uv_index_max: Option<f32>,
    pub wind_speed_max: Option<Mps>,
    pub wind_gust_max: Option<Mps>,
    pub wind_dir_dominant: Option<Degrees>,
    /// Phase fraction: 0 new, 0.5 full.
    pub moon_phase: Option<f32>,
    pub moonrise: Option<UnixTime>,
    pub moonset: Option<UnixTime>,
}

/// One 15-minute precipitation step (nowcast).
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[allow(missing_docs)]
pub struct Minutely {
    pub time: UnixTime,
    pub precipitation: Option<Mm>,
}

/// Air quality now. Pollutants are in µg/m³.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[allow(missing_docs)]
pub struct AirQuality {
    pub time: UnixTime,
    pub us_aqi: Option<u16>,
    pub european_aqi: Option<u16>,
    /// OpenWeather's 1–5 index.
    pub owm_index: Option<u8>,
    pub pm2_5: Option<f32>,
    pub pm10: Option<f32>,
    pub o3: Option<f32>,
    pub no2: Option<f32>,
    pub so2: Option<f32>,
    pub co: Option<f32>,
}

/// An official weather warning.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[allow(missing_docs)]
pub struct Alert {
    /// The provider's identifier, when it has one: deduplicates alerts and lets a
    /// later fetch reuse details it already has.
    pub id: Option<String>,
    pub title: String,
    pub description: String,
    pub severity: Level,
    pub start: UnixTime,
    pub end: UnixTime,
    pub source: String,
    /// When the provider last sent this alert's details (`None` when unknown, as in
    /// caches written before this change). A fetch re-requests details older than an hour
    /// so updates reach the app while the alert lasts.
    pub checked_at: Option<UnixTime>,
    /// A page with the alert's details, when the source gives one.
    /// Caches written earlier have none.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub url: Option<String>,
}

/// The prefix of ids of alerts the app adds from Apple Weather. Apple
/// gives no start or end for an alert, so these show when they were checked
/// rather than a period.
pub const APPLE_ALERT_PREFIX: &str = "apple:";

impl Alert {
    /// Whether the alert came from Apple Weather ([`APPLE_ALERT_PREFIX`]).
    #[must_use]
    pub fn is_apple(&self) -> bool {
        self.id
            .as_deref()
            .is_some_and(|id| id.starts_with(APPLE_ALERT_PREFIX))
    }
}

/// Everything known about one location's weather from one fetch.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct WeatherSnapshot {
    /// Where the data applies.
    pub location: ResolvedLocation,
    /// The primary source.
    pub provider: ProviderId,
    /// Providers that filled gaps; all are credited.
    pub supplemented_by: Vec<ProviderId>,
    /// When the data was received.
    pub fetched_at: UnixTime,
    /// Conditions now.
    pub current: Option<Current>,
    /// Ascending forecast steps.
    pub hourly: Vec<Hourly>,
    /// Ascending local days.
    pub daily: Vec<Daily>,
    /// Ascending 15-minute precipitation steps; may be empty.
    pub minutely_15: Vec<Minutely>,
    /// Ascending one-minute precipitation steps for the next hour, from a minute
    /// nowcast the app adds (Apple's WeatherKit on the iPhone); usually
    /// empty.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub minutely_1: Vec<Minutely>,
    /// Air quality now.
    pub air_quality: Option<AirQuality>,
    /// Official warnings.
    pub alerts: Vec<Alert>,
    /// UTC offset changes (DST) within the forecast period, ascending; empty when
    /// one offset (`location.utc_offset`) applies throughout.
    #[cfg_attr(feature = "serde", serde(default))]
    pub offset_changes: Vec<OffsetChange>,
    /// The weather models that supplied the forecast, in priority order (Open-Meteo
    /// ids); empty when the provider chose its own.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub models: Vec<String>,
}

/// From `at` on, local times use `offset`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OffsetChange {
    /// When the new offset takes effect.
    pub at: UnixTime,
    /// The offset from then on.
    pub offset: UtcOffset,
}

impl WeatherSnapshot {
    /// The UTC offset local times use at `t`: the last change at or before it, else
    /// the location's.
    pub fn offset_at(&self, t: UnixTime) -> UtcOffset {
        self.offset_changes
            .iter()
            .take_while(|c| c.at <= t)
            .last()
            .map_or(self.location.utc_offset, |c| c.offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_ids_and_names() {
        assert_eq!(ProviderId::OpenWeather.id(), "openweather");
        assert_eq!(ProviderId::OpenMeteo.id(), "open-meteo");
        assert_eq!(ProviderId::MetNorway.id(), "met-norway");
        assert_eq!(ProviderId::MetNorway.display_name(), "MET Norway");
        assert_eq!(ProviderId::PirateWeather.id(), "pirate-weather");
        assert_eq!(ProviderId::PirateWeather.display_name(), "Pirate Weather");
        assert_eq!(
            ProviderId::PirateWeather.attribution(),
            "Weather data by Pirate Weather"
        );
        assert_eq!(ProviderId::PirateWeather.short_credit(), "Pirate Weather");
        assert_eq!(
            ProviderId::OpenMeteo.attribution(),
            "Weather data by Open-Meteo.com"
        );
        assert_eq!(ProviderId::OpenMeteo.short_credit(), "Open-Meteo.com");
    }

    #[cfg(feature = "serde")]
    #[test]
    fn provider_serde_names_match_config_ids() {
        for p in [
            ProviderId::OpenWeather,
            ProviderId::OpenMeteo,
            ProviderId::MetNorway,
            ProviderId::Nws,
            ProviderId::PirateWeather,
        ] {
            let json = serde_json::to_string(&p).unwrap();
            assert_eq!(json, alloc::format!("\"{}\"", p.id()));
        }
    }

    /// Caches written before `id` and `checked_at` still load.
    #[cfg(feature = "serde")]
    #[test]
    fn older_alerts_load_without_the_newer_fields() {
        let old = r#"{"title":"Flood Watch","description":"","severity":"poor",
                      "start":0,"end":3600,"source":"NWS"}"#;
        let alert: Alert = serde_json::from_str(old).unwrap();
        assert_eq!((alert.id, alert.checked_at), (None, None));
    }
}
