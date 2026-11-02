//! The TOML-facing configuration types shared by Linux and the firmware.
//!
//! Every field is optional or defaulted, so partial files deserialize; `validate()`
//! decides what is required. serde names match `docs/config/whether.example.toml`.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};
use whether_core::model::ProviderId;
use whether_core::presentation::{
    AqiScalePref, DailyIcon, DateFormat, DetailKind, HourlyMetric, TemperatureBasis, TimeFormat,
    WidgetKind,
};
use whether_core::units::{
    DistanceUnit, PrecipUnit, PressureUnit, TemperatureUnit, UnitSystem, WindUnit,
};

use crate::duration::DurationStr;
use crate::model_list::ModelList;
use crate::secret::Secret;

/// The sections shared by every configuration file.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CommonConfig {
    /// Schema version; only 1 exists.
    pub version: u32,
    /// `[general]`.
    pub general: General,
    /// `[units]`: per-quantity overrides.
    pub units: UnitsOverride,
    /// `[[location]]` entries.
    pub location: Vec<LocationConfig>,
    /// `[providers]`.
    pub providers: ProvidersConfig,
    /// `[history]`: past days, normals and records.
    pub history: HistoryConfig,
    /// `[presentation]`.
    pub presentation: PresentationToml,
    /// `[strings]`: text overrides by key.
    pub strings: BTreeMap<String, String>,
}

impl Default for CommonConfig {
    fn default() -> Self {
        Self {
            version: 1,
            general: General::default(),
            units: UnitsOverride::default(),
            location: Vec::new(),
            providers: ProvidersConfig::default(),
            history: HistoryConfig::default(),
            presentation: PresentationToml::default(),
            strings: BTreeMap::new(),
        }
    }
}

/// `[general]`.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct General {
    /// Fetch interval (minimum 5 minutes).
    pub refresh: DurationStr,
    /// Unit system; `auto` follows the locale.
    pub units: UnitSystem,
    /// Clock style.
    pub time_format: TimeFormat,
    /// How dates are written.
    pub date_format: DateFormat,
    /// Language of geocoding results.
    pub language: String,
    /// Coordinate decimals sent to APIs (at most 4).
    pub coordinate_precision: u8,
}

/// The shortest allowed refresh interval.
pub const MIN_REFRESH: DurationStr = DurationStr(5 * 60);

impl General {
    /// The refresh interval, raised to [`MIN_REFRESH`].
    pub fn effective_refresh(&self) -> DurationStr {
        self.refresh.max(MIN_REFRESH)
    }
}

impl Default for General {
    fn default() -> Self {
        Self {
            refresh: DurationStr(15 * 60),
            units: UnitSystem::Auto,
            time_format: TimeFormat::H24,
            date_format: DateFormat::MonthNameDay,
            language: "en".to_string(),
            coordinate_precision: 3,
        }
    }
}

/// `[units]`: overrides applied on top of `general.units`.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UnitsOverride {
    /// Temperature unit.
    pub temperature: Option<TemperatureUnit>,
    /// Wind speed unit.
    pub wind: Option<WindUnit>,
    /// Pressure unit.
    pub pressure: Option<PressureUnit>,
    /// Precipitation unit.
    pub precipitation: Option<PrecipUnit>,
    /// Distance unit.
    pub distance: Option<DistanceUnit>,
}

/// One `[[location]]`: coordinates, a postal code, or a free-text query.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LocationConfig {
    /// Display name (required, unique).
    pub name: Option<String>,
    /// Latitude in degrees (with `longitude`).
    pub latitude: Option<f64>,
    /// Longitude in degrees (with `latitude`).
    pub longitude: Option<f64>,
    /// Postal code (with `country`).
    pub zip: Option<String>,
    /// ISO 3166-1 alpha-2 country code.
    pub country: Option<String>,
    /// Free-text place name resolved by geocoding.
    pub query: Option<String>,
    /// IANA zone, for providers that return none.
    pub timezone: Option<String>,
    /// Fixed offset such as `"-04:00"`, for providers that return none.
    pub utc_offset: Option<String>,
    /// Provider override for this location.
    pub provider: Option<ProviderId>,
    /// Elevation in meters.
    pub elevation: Option<f32>,
    /// The unit system here (`"metric"` or `"imperial"`), instead of
    /// `[general] units` and `[units]`.
    pub units: Option<UnitSystem>,
    /// `[presentation]` keys that differ here, e.g. alerts only at home.
    /// They apply over `[presentation]` and a display's own overrides.
    pub presentation: Option<PresentationToml>,
}

/// `[providers]`.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ProvidersConfig {
    /// Fallback chain; the first is the primary.
    pub order: Vec<ProviderId>,
    /// Providers that fill what the primary lacks.
    pub supplement: Vec<ProviderId>,
    /// `[providers.open-meteo]`.
    #[serde(rename = "open-meteo")]
    pub open_meteo: OpenMeteoToml,
    /// `[providers.met-norway]`.
    #[serde(rename = "met-norway")]
    pub met_norway: Option<MetNorwayToml>,
    /// `[providers.openweather]`.
    pub openweather: Option<OpenWeatherToml>,
    /// `[providers.nws]`.
    pub nws: NwsToml,
    /// `[providers.pirate-weather]`.
    #[serde(rename = "pirate-weather")]
    pub pirate_weather: Option<PirateWeatherToml>,
}

/// The environment variable the default OpenWeather key comes from.
pub const OPENWEATHER_KEY_ENV: &str = "OPENWEATHER_API_KEY";

impl Default for ProvidersConfig {
    fn default() -> Self {
        Self {
            order: vec![ProviderId::OpenWeather, ProviderId::OpenMeteo],
            supplement: vec![ProviderId::OpenMeteo],
            open_meteo: OpenMeteoToml::default(),
            met_norway: None,
            nws: NwsToml::default(),
            pirate_weather: None,
            openweather: Some(OpenWeatherToml {
                api_key: Some(Secret::Env {
                    env: OPENWEATHER_KEY_ENV.to_string(),
                }),
                api: Some(OwmApi::Free),
                base_url: None,
                daily_call_limit: Some(DEFAULT_DAILY_CALL_LIMIT),
            }),
        }
    }
}

/// `[providers.open-meteo]`.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OpenMeteoToml {
    /// Forecast API base URL.
    pub base_url: Option<String>,
    /// Air-quality API base URL.
    pub air_quality_url: Option<String>,
    /// Geocoding API base URL.
    pub geocoding_url: Option<String>,
    /// Key for commercial plans.
    pub api_key: Option<Secret>,
    /// Weather models in priority order: one id or a list.
    pub models: Option<ModelList>,
}

/// `[history]`: where past days, normals and records come from.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HistoryConfig {
    /// Sources in fallback order: "acis" (US stations) and "open-meteo"
    /// (anywhere; ERA5 normals, no records).
    pub order: Vec<ProviderId>,
    /// Sources that fill what the first lacks (a day not yet reported).
    pub supplement: Vec<ProviderId>,
    /// `[history.acis]`: NOAA's Regional Climate Centers (US stations).
    pub acis: AcisToml,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            order: vec![ProviderId::Acis, ProviderId::OpenMeteo],
            supplement: vec![ProviderId::OpenMeteo],
            acis: AcisToml::default(),
        }
    }
}

/// `[history.acis]`.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AcisToml {
    /// API base URL.
    pub base_url: Option<String>,
    /// A station id to use instead of the nearest suitable one (`"LGA"`,
    /// `"USW00014732"`; `whether history --station` lists them).
    pub station: Option<String>,
    /// A station id for the records (`"LGAthr"`, a city's threaded record)
    /// instead of the station's own threaded record.
    pub records_station: Option<String>,
}

/// `[providers.nws]`: the US National Weather Service, forecasting in `order`
/// and giving alerts as a supplement.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NwsToml {
    /// API base URL.
    pub base_url: Option<String>,
    /// Where the current conditions come from when the NWS forecasts.
    pub current: Option<NwsCurrent>,
}

/// `[providers.nws] current`.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NwsCurrent {
    /// The nearest station's latest observation, when fresh (else the forecast).
    #[default]
    Observed,
    /// The forecast for the current hour.
    Forecast,
}

/// MET Norway's forecast product.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MetProduct {
    /// All variables.
    #[default]
    Complete,
    /// Fewer variables, smaller responses.
    Compact,
}

/// `[providers.met-norway]`.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MetNorwayToml {
    /// Contact address sent in the User-Agent (required by MET Norway's terms).
    pub contact: Option<String>,
    /// Forecast product.
    pub product: Option<MetProduct>,
    /// API base URL.
    pub base_url: Option<String>,
}

/// Which OpenWeather API a key is used with.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum OwmApi {
    /// The free 2.5 endpoints.
    #[default]
    #[serde(rename = "free")]
    Free,
    /// One Call 3.0 (closed to new subscriptions; rejected by validation).
    #[serde(rename = "onecall-3")]
    OneCall3,
    /// One Call 4.0 ("One Call by Call" subscription), metered by `daily_call_limit`.
    #[serde(rename = "onecall-4")]
    OneCall4,
}

/// Calls per UTC day included free with "One Call by Call"; `daily_call_limit` may
/// not exceed it.
pub const ONECALL_FREE_DAILY_CALLS: u32 = 1000;
/// The default `daily_call_limit`, leaving a margin below the free calls.
pub const DEFAULT_DAILY_CALL_LIMIT: u32 = 900;

/// `[providers.openweather]`.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OpenWeatherToml {
    /// API key.
    pub api_key: Option<Secret>,
    /// API flavor.
    pub api: Option<OwmApi>,
    /// API base URL.
    pub base_url: Option<String>,
    /// One Call calls this device may make per UTC day (1–1000).
    pub daily_call_limit: Option<u32>,
}

/// The environment variable the default Pirate Weather key comes from.
pub const PIRATE_WEATHER_KEY_ENV: &str = "PIRATE_WEATHER_API_KEY";
/// The default `monthly_call_limit`: a margin under the 10,000 calls a month a free
/// key allows.
pub const DEFAULT_PIRATE_WEATHER_MONTHLY_LIMIT: u32 = 9_000;
/// The calls a month a free key allows; a larger `monthly_call_limit` draws a
/// warning (a paid plan may allow more).
pub const PIRATE_WEATHER_FREE_MONTHLY_CALLS: u32 = 10_000;
/// The models `exclude_models` can name (docs.pirateweather.net, 2026-09-24).
pub const PIRATE_WEATHER_MODELS: [&str; 17] = [
    "hrrr",
    "nbm",
    "gefs",
    "gfs",
    "rtma_ru",
    "ecmwf_ifs",
    "dwd_mosmix",
    "ecmwf_aifs",
    "aigefs",
    "aigfs",
    "raqdps",
    "silam",
    "hrdps",
    "gdps",
    "reps",
    "geps",
    "cmc",
];

/// `[providers.pirate-weather]`. Without the section, listing
/// `"pirate-weather"` in `order` uses the defaults and the key from
/// `PIRATE_WEATHER_API_KEY`.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PirateWeatherToml {
    /// API key; default `{ env = "PIRATE_WEATHER_API_KEY" }`.
    pub api_key: Option<Secret>,
    /// Models to leave out of the blend (`"gfs"`, `"hrrr"`, …); the next best
    /// serves instead.
    pub exclude_models: Option<Vec<String>>,
    /// Add the AI models (AIGFS, AIGEFS, ECMWF AIFS) to the blend.
    pub ai_models: Option<bool>,
    /// Calls this device may make per UTC month (default 9000; the free key
    /// allows 10,000). Calls beyond it are never sent.
    pub monthly_call_limit: Option<u32>,
    /// API base URL.
    pub base_url: Option<String>,
}

/// `[presentation]` and `[display.<name>.presentation]`: each key overrides the
/// built-in default. Units and the clock style come from `[general]` and `[units]`.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PresentationToml {
    /// Enabled widgets in order.
    pub widgets: Option<Vec<WidgetKind>>,
    /// Hours in the hourly widget (1–48).
    pub hourly_hours: Option<u8>,
    /// Series in the hourly widget.
    pub hourly_metrics: Option<Vec<HourlyMetric>>,
    /// Days in the daily widget (1–16).
    pub daily_days: Option<u8>,
    /// What a day's icon describes: `daylight` (sunrise to sunset, from the
    /// hourly forecast) or `provider` (the provider's daily code).
    pub daily_icon: Option<DailyIcon>,
    /// Hours in the precipitation table (1–48).
    pub precip_hours: Option<u8>,
    /// Days in the history widget (0–31).
    pub history_days: Option<u8>,
    /// Hours the nowcast covers in 15-minute steps (1–8).
    pub nowcast_hours: Option<u8>,
    /// Which temperature leads: `actual` or `feels_like` (the highs and lows
    /// too, where the data has them).
    pub temperature_basis: Option<TemperatureBasis>,
    /// Give each day its own hours, for displays that expand a day.
    pub daily_hours: Option<bool>,
    /// Detail cards in order.
    pub details: Option<Vec<DetailKind>>,
    /// Preferred air-quality index.
    pub aqi_scale: Option<AqiScalePref>,
    /// Probability (%) treated as "likely".
    pub precip_threshold: Option<u8>,
    /// Decimals for temperatures.
    pub temperature_decimals: Option<u8>,
    /// Show "21 °C" instead of "21°".
    pub show_unit_letter: Option<bool>,
    /// Placeholder for unavailable values.
    pub missing: Option<String>,
    /// Minutes around sunrise and sunset that use the twilight theme.
    pub twilight_minutes: Option<u16>,
}

impl PresentationToml {
    /// `over`'s keys on top of `self`'s (for `[display.<name>.presentation]`).
    #[must_use]
    pub fn overlay(&self, over: &Self) -> Self {
        Self {
            widgets: over.widgets.clone().or_else(|| self.widgets.clone()),
            hourly_hours: over.hourly_hours.or(self.hourly_hours),
            hourly_metrics: over
                .hourly_metrics
                .clone()
                .or_else(|| self.hourly_metrics.clone()),
            daily_days: over.daily_days.or(self.daily_days),
            daily_icon: over.daily_icon.or(self.daily_icon),
            precip_hours: over.precip_hours.or(self.precip_hours),
            history_days: over.history_days.or(self.history_days),
            nowcast_hours: over.nowcast_hours.or(self.nowcast_hours),
            temperature_basis: over.temperature_basis.or(self.temperature_basis),
            daily_hours: over.daily_hours.or(self.daily_hours),
            details: over.details.clone().or_else(|| self.details.clone()),
            aqi_scale: over.aqi_scale.or(self.aqi_scale),
            precip_threshold: over.precip_threshold.or(self.precip_threshold),
            temperature_decimals: over.temperature_decimals.or(self.temperature_decimals),
            show_unit_letter: over.show_unit_letter.or(self.show_unit_letter),
            missing: over.missing.clone().or_else(|| self.missing.clone()),
            twilight_minutes: over.twilight_minutes.or(self.twilight_minutes),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The core sections of `docs/config/whether.example.toml`, with its values.
    const EXAMPLE_CORE: &str = r#"
version = 1

[general]
refresh = "15m"
units = "auto"
time_format = "24h"
date_format = "mmm d"
language = "en"
coordinate_precision = 3

[units]

[[location]]
name = "New York"
zip = "10001"
country = "US"

[providers]
order = ["openweather", "open-meteo"]
supplement = ["open-meteo"]

[providers.openweather]
api_key = { env = "OPENWEATHER_API_KEY" }
api = "free"
daily_call_limit = 900

[providers.open-meteo]

[presentation]
widgets = ["current", "summary", "hourly", "precipitation", "daily", "details", "sun", "air_quality"]
hourly_hours = 24
hourly_metrics = ["temperature", "precip_probability"]
daily_days = 7
details = ["wind", "humidity", "pressure", "uv", "visibility", "dew_point", "cloud_cover", "precipitation"]
aqi_scale = "us"
precip_threshold = 50
temperature_decimals = 0
show_unit_letter = false
missing = "—"
twilight_minutes = 45

[strings]
"#;

    fn parse(text: &str) -> Result<CommonConfig, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }

    #[test]
    fn example_values_are_the_defaults() {
        let c = parse(EXAMPLE_CORE).unwrap();
        let defaults = CommonConfig::default();
        assert_eq!(c.version, defaults.version);
        assert_eq!(c.general, defaults.general);
        assert_eq!(c.units, defaults.units);
        assert_eq!(c.providers, defaults.providers);
        assert_eq!(c.location[0].zip.as_deref(), Some("10001"));
        // Every presentation key is set, so it resolves to the core defaults.
        let p = &c.presentation;
        let d = whether_core::presentation::PresentationConfig::default();
        assert_eq!(p.widgets.as_ref(), Some(&d.widgets));
        assert_eq!(p.details.as_ref(), Some(&d.details));
        assert_eq!(p.hourly_metrics.as_ref(), Some(&d.hourly_metrics));
        assert_eq!(
            (
                p.hourly_hours,
                p.daily_days,
                p.precip_threshold,
                p.twilight_minutes
            ),
            (
                Some(d.hourly_hours),
                Some(d.daily_days),
                Some(d.precip_threshold),
                Some(d.twilight_minutes)
            )
        );
        assert_eq!(p.missing.as_deref(), Some(d.missing.as_str()));
    }

    #[test]
    fn empty_and_partial_files_parse() {
        assert_eq!(parse("").unwrap(), CommonConfig::default());
        let c =
            parse("[general]\nrefresh = \"30m\"\n[providers.met-norway]\ncontact = \"x\"").unwrap();
        assert_eq!(c.general.refresh, DurationStr(1800));
        assert_eq!(c.general.language, "en", "other keys keep their defaults");
        assert!(
            c.providers.openweather.is_some(),
            "the default OpenWeather table stays"
        );
        assert_eq!(
            c.providers.met_norway.unwrap().contact.as_deref(),
            Some("x")
        );
    }

    #[test]
    fn pirate_weather_section_parses() {
        let c = parse(
            "[providers]\norder = [\"pirate-weather\", \"open-meteo\"]\n\
             [providers.pirate-weather]\napi_key = { env = \"PIRATE_WEATHER_API_KEY\" }\n\
             exclude_models = [\"gfs\"]\nai_models = true\nmonthly_call_limit = 5000",
        )
        .unwrap();
        assert_eq!(c.providers.order[0], ProviderId::PirateWeather);
        let p = c.providers.pirate_weather.unwrap();
        assert_eq!(
            p.api_key,
            Some(Secret::Env {
                env: PIRATE_WEATHER_KEY_ENV.into()
            })
        );
        assert_eq!(p.exclude_models, Some(vec!["gfs".to_string()]));
        assert_eq!(
            (p.ai_models, p.monthly_call_limit),
            (Some(true), Some(5000))
        );
        assert!(parse("[providers.pirate-weather]\nmodels = []").is_err());
        assert_eq!(parse("").unwrap().providers.pirate_weather, None);
    }

    #[test]
    fn unknown_keys_are_errors() {
        for text in [
            "colour = 1",
            "[general]\nrefersh = \"15m\"",
            "[[location]]\nname = \"a\"\nlat = 1.0",
            "[providers.openweather]\nkey = \"x\"",
            "[presentation]\nhourly = 24",
        ] {
            let err = parse(text).unwrap_err();
            assert!(err.contains("unknown field"), "{text}: {err}");
        }
    }

    #[test]
    fn enums_use_the_documented_names() {
        let c = parse(
            "[general]\nunits = \"custom\"\ntime_format = \"12h\"\ndate_format = \"MMMd\"\n\
             [units]\nwind = \"km/h\"\npressure = \"inHg\"\n\
             [[location]]\nname = \"Cabin\"\nquery = \"Tromsø\"\nprovider = \"met-norway\"\n\
             [providers.openweather]\napi = \"onecall-4\"\ndaily_call_limit = 500\n\
             [providers.met-norway]\nproduct = \"compact\"",
        )
        .unwrap();
        assert_eq!(c.general.units, UnitSystem::Custom);
        assert_eq!(c.general.date_format, DateFormat::Compact);
        assert_eq!(c.units.wind, Some(WindUnit::Kmh));
        assert_eq!(c.location[0].provider, Some(ProviderId::MetNorway));
        let openweather = c.providers.openweather.unwrap();
        assert_eq!(openweather.api, Some(OwmApi::OneCall4));
        assert_eq!(openweather.daily_call_limit, Some(500));
        assert_eq!(
            c.providers.met_norway.unwrap().product,
            Some(MetProduct::Compact)
        );
        assert!(parse("[general]\nunits = \"british\"").is_err());
    }

    #[test]
    fn overlay_prefers_the_override() {
        let base = PresentationToml {
            hourly_hours: Some(24),
            daily_days: Some(5),
            ..Default::default()
        };
        let over = PresentationToml {
            hourly_hours: Some(48),
            missing: Some("-".into()),
            ..Default::default()
        };
        let merged = base.overlay(&over);
        assert_eq!(
            (merged.hourly_hours, merged.daily_days),
            (Some(48), Some(5))
        );
        assert_eq!(merged.missing.as_deref(), Some("-"));
        assert_eq!(base.overlay(&PresentationToml::default()), base);
    }

    #[test]
    fn effective_refresh_is_at_least_five_minutes() {
        let mut g = General::default();
        assert_eq!(g.effective_refresh(), DurationStr(900));
        g.refresh = DurationStr(60);
        assert_eq!(g.effective_refresh(), MIN_REFRESH);
    }
}
