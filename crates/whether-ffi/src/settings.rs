//! What the app passes in: its settings and a place, and the providers and
//! presentation they make.

use crate::units::UnitOverrides;
use std::collections::HashMap;
use whether_config::{General, LocationConfig, PresentationToml};
use whether_core::model::OffsetChange as CoreOffsetChange;
use whether_core::models::PRESETS;
use whether_core::presentation::{
    DailyIcon, DateFormat, DetailKind, HourlyMetric, PresentationConfig, TemperatureBasis,
    TimeFormat, WidgetKind,
};
use whether_core::time::{UnixTime, UtcOffset};
use whether_core::units::UnitSystem;
use whether_providers::Coordinates;
use whether_providers::Provider;
use whether_providers::met_norway::{MetNorway, MetNorwaySettings};
use whether_providers::nws::Nws;
use whether_providers::open_meteo::models::for_location;
use whether_providers::open_meteo::{OpenMeteo, OpenMeteoSettings};
use whether_providers::openweather::{DEFAULT_BASE_URL, OpenWeather, OpenWeatherSettings, OwmApi};
use whether_providers::pirate_weather::{PirateWeather, PirateWeatherSettings};

/// Headroom under OpenWeather's 1,000 free One Call calls a day.
pub const DEFAULT_DAILY_CALLS: u32 = 800;

/// Calls a month the phone may make with a Pirate Weather key: a margin under the
/// free 10,000.
const DEFAULT_MONTHLY_CALLS: u32 = whether_providers::pirate_weather::DEFAULT_MONTHLY_CALL_LIMIT;

/// Open-Meteo's name for NOAA's National Blend of Models over the CONUS.
const NOAA_BLEND: &str = "ncep_nbm_conus";

/// A named list of weather models for the Sources screen.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ModelPreset {
    /// What the screen calls it.
    pub name: String,
    /// The models in priority order; empty is Open-Meteo's best match.
    pub models: Vec<String>,
}

/// The model presets, best match first.
#[uniffi::export]
pub fn model_presets() -> Vec<ModelPreset> {
    PRESETS
        .iter()
        .map(|p| ModelPreset {
            name: p.name.to_string(),
            models: p.models.iter().map(ToString::to_string).collect(),
        })
        .collect()
}

/// The models for preferences saved before model lists, which had only a "NOAA
/// blend" switch.
#[uniffi::export]
pub fn models_from_noaa_blend(noaa_blend: bool) -> Vec<String> {
    if noaa_blend {
        vec![NOAA_BLEND.to_string()]
    } else {
        Vec::new()
    }
}

/// Whether the US National Weather Service covers a place, so its alerts can
/// arrive without an OpenWeather key.
#[uniffi::export]
pub fn nws_covers(latitude: f64, longitude: f64) -> bool {
    whether_providers::regions::covers_us(&Coordinates {
        latitude,
        longitude,
    })
}

/// The source ids of [`SourceChoice::provider`]: NWS forecast (US only).
const SOURCE_NWS: &str = "nws";
/// The Open-Meteo weather models of `Settings::models`.
const SOURCE_MODELS: &str = "open-meteo-models";
/// OpenWeather, with a key.
const SOURCE_OPENWEATHER: &str = "openweather";
/// Pirate Weather, with a key.
const SOURCE_PIRATE_WEATHER: &str = "pirate-weather";
/// Open-Meteo's best match: always present, always the fallback.
const SOURCE_OPEN_METEO: &str = "open-meteo";
/// MET Norway, the last resort after Open-Meteo.
const SOURCE_MET_NORWAY: &str = "met-norway";

/// One row of the phone's ordered sources: a provider id and whether
/// the user wants it. Ids this build does not know are ignored, so a list saved
/// by a newer build still works here.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct SourceChoice {
    /// `nws`, `open-meteo-models`, `openweather`, `pirate-weather`, `open-meteo` or
    /// `met-norway`.
    pub provider: String,
    /// Whether the source may serve.
    pub enabled: bool,
}

impl SourceChoice {
    fn new(provider: &str, enabled: bool) -> Self {
        Self {
            provider: provider.to_string(),
            enabled,
        }
    }
}

/// The sources in today's order, as a list a user can reorder: the NWS (off, as
/// it is for a new install), the weather models, OpenWeather and Pirate Weather
/// (each serves once its key is saved, so no key changes the chain), then
/// Open-Meteo and MET Norway, which always end the chain. Pirate Weather follows
/// OpenWeather, so a key added to an existing install never replaces the source
/// that served before.
#[uniffi::export]
pub fn default_sources() -> Vec<SourceChoice> {
    vec![
        SourceChoice::new(SOURCE_NWS, false),
        SourceChoice::new(SOURCE_MODELS, true),
        SourceChoice::new(SOURCE_OPENWEATHER, true),
        SourceChoice::new(SOURCE_PIRATE_WEATHER, true),
        SourceChoice::new(SOURCE_OPEN_METEO, true),
        SourceChoice::new(SOURCE_MET_NORWAY, true),
    ]
}

/// How dates are written (`[general] date_format`), shown for 1 November.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, uniffi::Enum)]
pub enum DateStyle {
    /// `Nov 1`.
    #[default]
    MonthNameDay,
    /// `1 Nov`.
    DayMonthName,
    /// `NOV1`.
    Compact,
    /// `11/1`.
    MonthDay,
    /// `11/01`.
    MonthDayPadded,
    /// `1/11`.
    DayMonth,
    /// `01/11`.
    DayMonthPadded,
    /// `01.11.`.
    DayMonthDots,
    /// `2026-11-01`.
    Iso,
}

impl From<DateStyle> for DateFormat {
    fn from(d: DateStyle) -> Self {
        match d {
            DateStyle::MonthNameDay => Self::MonthNameDay,
            DateStyle::DayMonthName => Self::DayMonthName,
            DateStyle::Compact => Self::Compact,
            DateStyle::MonthDay => Self::MonthDay,
            DateStyle::MonthDayPadded => Self::MonthDayPadded,
            DateStyle::DayMonth => Self::DayMonth,
            DateStyle::DayMonthPadded => Self::DayMonthPadded,
            DateStyle::DayMonthDots => Self::DayMonthDots,
            DateStyle::Iso => Self::Iso,
        }
    }
}

impl From<DateFormat> for DateStyle {
    fn from(d: DateFormat) -> Self {
        match d {
            DateFormat::MonthNameDay => Self::MonthNameDay,
            DateFormat::DayMonthName => Self::DayMonthName,
            DateFormat::Compact => Self::Compact,
            DateFormat::MonthDay => Self::MonthDay,
            DateFormat::MonthDayPadded => Self::MonthDayPadded,
            DateFormat::DayMonth => Self::DayMonth,
            DateFormat::DayMonthPadded => Self::DayMonthPadded,
            DateFormat::DayMonthDots => Self::DayMonthDots,
            DateFormat::Iso => Self::Iso,
        }
    }
}

/// Owned providers, in order.
pub(crate) type Providers = Vec<Box<dyn Provider>>;

/// A unit system.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum Units {
    /// From the phone's region (`locale`).
    Auto,
    /// °C, km/h, mm, hPa.
    Metric,
    /// °F, mph, in, inHg.
    Imperial,
}

/// The app's settings.
///
/// `Debug` is written by hand: it prints every API key as `***`.
#[derive(Clone, PartialEq, uniffi::Record)]
#[allow(clippy::struct_excessive_bools)] // independent switches on the settings screen
pub struct Settings {
    /// Unit system.
    pub units: Units,
    /// 24-hour clock (else 12-hour).
    pub clock_24h: bool,
    /// How dates are written.
    pub date_style: DateStyle,
    /// Language for provider texts such as alerts (`"en"`).
    pub language: String,
    /// The phone's region as a POSIX locale (`"en_US"`), for `Units::Auto`.
    pub locale: Option<String>,
    /// The OpenWeather key from the Keychain; without one Open-Meteo serves.
    pub openweather_key: Option<String>,
    /// Use One Call 4.0 (metered; alerts) instead of the free API.
    pub one_call: bool,
    /// One Call calls the phone may make per UTC day (its share of 1,000).
    pub daily_call_limit: u32,
    /// Fill what the primary lacks (UV, moon, nowcast, air quality) from Open-Meteo.
    pub fill_gaps: bool,
    /// Open-Meteo weather models that forecast first, in priority order (a
    /// preset from `model_presets`); empty for none. Models without data for a
    /// place are skipped there, and Open-Meteo's best match fills the rest
    ///.
    pub models: Vec<String>,
    /// Hours in the hourly forecast (1–48).
    pub hourly_hours: u8,
    /// Days in the daily forecast (1–16).
    pub daily_days: u8,
    /// Hours in the precipitation table (1–48).
    pub precip_hours: u8,
    /// Hours of the next-hours rain graph (1–8; the app offers 1, 2, 4, 8).
    pub nowcast_hours: u8,
    /// Day icons from sunrise to sunset (else the provider's whole-day code).
    pub daylight_icons: bool,
    /// Lead with the "feels like" temperatures (the highs and lows too).
    pub feels_like: bool,
    /// Minutes between refreshes; older data is marked stale after twice this.
    pub refresh_minutes: u32,
    /// US places forecast from the National Weather Service first,
    /// with its stations' current conditions; Open-Meteo fills what it lacks.
    #[uniffi(default = false)]
    pub nws: bool,
    /// Show history: the last days against normal, today's normals
    /// and records, and normals on the days ahead. Fetched once a day.
    #[uniffi(default = true)]
    pub history: bool,
    /// Past days the history lists (1–31).
    #[uniffi(default = 7)]
    pub history_days: u8,
    /// Units that replace the system's one by one (°C with mph); `None` keeps
    /// the system's own.
    #[uniffi(default)]
    pub unit_overrides: UnitOverrides,
    /// MET Norway is the last resort when Open-Meteo fails. It needs
    /// the app's `api_contact`, so it is off while there is none.
    #[uniffi(default = true)]
    pub met_norway: bool,
    /// The sources in the order they are tried. Empty keeps the
    /// fixed chain, driven by `nws`, `models`, the OpenWeather key and
    /// `met_norway`, so earlier installs behave as they did.
    #[uniffi(default)]
    pub sources: Vec<SourceChoice>,
    /// API keys by provider id (`"openweather"`) from the Keychain. The
    /// OpenWeather key may still come in `openweather_key`.
    #[uniffi(default)]
    pub keys: HashMap<String, String>,
    /// Pirate Weather calls the phone may make per UTC month; its free
    /// key allows 10,000.
    #[uniffi(default = 9000)]
    pub pirate_weather_monthly_calls: u32,
}

impl core::fmt::Debug for Settings {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Every key is masked, and only the providers it is for are named.
        let masked: std::collections::BTreeMap<&str, &str> =
            self.keys.keys().map(|k| (k.as_str(), "***")).collect();
        f.debug_struct("Settings")
            .field("units", &self.units)
            .field("clock_24h", &self.clock_24h)
            .field("date_style", &self.date_style)
            .field("language", &self.language)
            .field("locale", &self.locale)
            .field(
                "openweather_key",
                &self.openweather_key.as_ref().map(|_| "***"),
            )
            .field("one_call", &self.one_call)
            .field("daily_call_limit", &self.daily_call_limit)
            .field("fill_gaps", &self.fill_gaps)
            .field("models", &self.models)
            .field("hourly_hours", &self.hourly_hours)
            .field("daily_days", &self.daily_days)
            .field("precip_hours", &self.precip_hours)
            .field("nowcast_hours", &self.nowcast_hours)
            .field("daylight_icons", &self.daylight_icons)
            .field("feels_like", &self.feels_like)
            .field("refresh_minutes", &self.refresh_minutes)
            .field("nws", &self.nws)
            .field("history", &self.history)
            .field("history_days", &self.history_days)
            .field("unit_overrides", &self.unit_overrides)
            .field("met_norway", &self.met_norway)
            .field("sources", &self.sources)
            .field("keys", &masked)
            .field(
                "pirate_weather_monthly_calls",
                &self.pirate_weather_monthly_calls,
            )
            .finish()
    }
}

/// Defaults in the Dark Sky spirit: 48 hours, 8 days, imperial from the locale.
#[uniffi::export]
pub fn default_settings() -> Settings {
    Settings {
        units: Units::Auto,
        clock_24h: false,
        date_style: DateStyle::MonthNameDay,
        language: "en".into(),
        locale: None,
        openweather_key: None,
        one_call: false,
        daily_call_limit: DEFAULT_DAILY_CALLS,
        fill_gaps: true,
        models: Vec::new(),
        hourly_hours: 48,
        daily_days: 8,
        precip_hours: 12,
        nowcast_hours: 2,
        daylight_icons: true,
        feels_like: false,
        refresh_minutes: 15,
        nws: false,
        history: true,
        history_days: 7,
        unit_overrides: UnitOverrides::default(),
        met_norway: true,
        sources: Vec::new(),
        keys: HashMap::new(),
        pirate_weather_monthly_calls: DEFAULT_MONTHLY_CALLS,
    }
}

/// A UTC offset change (DST) of a place's zone, from Foundation's `TimeZone`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct OffsetChange {
    /// Unix seconds when it takes effect.
    pub at: i64,
    /// The offset from then on, in seconds east of UTC.
    pub offset_seconds: i32,
}

/// A place to forecast.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct Place {
    /// Display name.
    pub name: String,
    /// Latitude in degrees.
    pub latitude: f64,
    /// Longitude in degrees.
    pub longitude: f64,
    /// This place's unit system, instead of the settings'.
    pub units: Option<Units>,
    /// The zone's current offset, for providers that report only UTC.
    pub utc_offset_seconds: Option<i32>,
    /// The zone's offset changes over the forecast period.
    pub offset_changes: Vec<OffsetChange>,
}

impl Place {
    pub(crate) fn core_offset_changes(&self) -> Vec<CoreOffsetChange> {
        self.offset_changes
            .iter()
            .map(|c| CoreOffsetChange {
                at: UnixTime(c.at),
                offset: UtcOffset(c.offset_seconds),
            })
            .collect()
    }
}

fn unit_system(units: Units) -> UnitSystem {
    match units {
        Units::Auto => UnitSystem::Auto,
        Units::Metric => UnitSystem::Metric,
        Units::Imperial => UnitSystem::Imperial,
    }
}

/// The widgets the app shows: Dark Sky's sections.
const WIDGETS: [WidgetKind; 11] = [
    WidgetKind::Current,
    WidgetKind::Summary,
    WidgetKind::Nowcast,
    WidgetKind::Hourly,
    WidgetKind::Precipitation,
    WidgetKind::Daily,
    WidgetKind::Details,
    WidgetKind::Sun,
    WidgetKind::Moon,
    WidgetKind::AirQuality,
    WidgetKind::Alerts,
];

impl Settings {
    /// The presentation for `place`: the settings, with its own units on top.
    pub(crate) fn presentation(&self, place: &Place) -> PresentationConfig {
        let general = General {
            units: unit_system(self.units),
            time_format: if self.clock_24h {
                TimeFormat::H24
            } else {
                TimeFormat::H12
            },
            date_format: self.date_style.into(),
            ..General::default()
        };
        let mut widgets = WIDGETS.to_vec();
        // Every card: the app chooses which show, and their order (Now →
        // Details, edited like the home screen). Gusts beside the wind.
        let mut details = vec![
            DetailKind::Wind,
            DetailKind::Gusts,
            DetailKind::Humidity,
            DetailKind::Pressure,
            DetailKind::Uv,
            DetailKind::Visibility,
            DetailKind::DewPoint,
            DetailKind::CloudCover,
            DetailKind::Precipitation,
            DetailKind::FeelsLike,
        ];
        if self.history {
            widgets.push(WidgetKind::History);
            details.extend([DetailKind::Normals, DetailKind::Records]);
        }
        let toml = PresentationToml {
            widgets: Some(widgets),
            details: Some(details),
            history_days: Some(self.history_days.clamp(1, 31)),
            // Every measure, for the timeline's menu and the hourly table.
            hourly_metrics: Some(vec![
                HourlyMetric::Temperature,
                HourlyMetric::FeelsLike,
                HourlyMetric::PrecipProbability,
                HourlyMetric::Precipitation,
                HourlyMetric::Wind,
                HourlyMetric::Humidity,
                HourlyMetric::Uv,
            ]),
            hourly_hours: Some(self.hourly_hours),
            daily_days: Some(self.daily_days),
            precip_hours: Some(self.precip_hours),
            nowcast_hours: Some(self.nowcast_hours),
            temperature_basis: Some(if self.feels_like {
                TemperatureBasis::FeelsLike
            } else {
                TemperatureBasis::Actual
            }),
            // Days expand to their hours in the app.
            daily_hours: Some(true),
            daily_icon: Some(if self.daylight_icons {
                DailyIcon::Daylight
            } else {
                DailyIcon::Provider
            }),
            ..PresentationToml::default()
        };
        let shared = whether_config::resolve_presentation(
            &general,
            &self.unit_overrides.to_config(),
            &toml,
            self.locale.as_deref(),
        );
        let location = LocationConfig {
            units: place.units.map(unit_system),
            ..LocationConfig::default()
        };
        whether_config::for_location(&shared, &location)
    }

    /// The key saved for provider `id`: `keys`, else (for OpenWeather) the
    /// earlier `openweather_key`. Blank keys do not count.
    fn key_for(&self, id: &str) -> Option<&str> {
        let legacy = (id == SOURCE_OPENWEATHER)
            .then_some(self.openweather_key.as_deref())
            .flatten();
        self.keys
            .get(id)
            .map(String::as_str)
            .into_iter()
            .chain(legacy)
            .map(str::trim)
            .find(|k| !k.is_empty())
    }

    /// The rows the chain follows: `sources`, or (while it is empty) the fixed
    /// order with the earlier switches as the rows' state. The fixed order has no
    /// `pirate-weather` row: it is the chain of every install before that source
    /// existed, and the app always sends a list.
    fn source_rows(&self) -> Vec<SourceChoice> {
        if !self.sources.is_empty() {
            return self.sources.clone();
        }
        vec![
            SourceChoice::new(SOURCE_NWS, self.nws),
            SourceChoice::new(SOURCE_MODELS, true),
            SourceChoice::new(SOURCE_OPENWEATHER, true),
            SourceChoice::new(SOURCE_OPEN_METEO, true),
            SourceChoice::new(SOURCE_MET_NORWAY, self.met_norway),
        ]
    }

    /// The primary chain and the supplements for `place`: the sources in their
    /// order (by default the NWS for US places, the chosen models when any has
    /// data there, OpenWeather with a key), then Open-Meteo; NWS alerts for
    /// every US place; MET Norway last when the app has a contact.
    pub(crate) fn providers(&self, place: &Place) -> (Providers, Providers) {
        self.providers_with(place, crate::api_contact().as_deref())
    }

    /// [`Self::providers`] with the API contact given, so tests need no real one.
    ///
    /// A row serves when it is enabled and has what it needs (a US place, models
    /// with data, a key). The first row for an id counts. `open-meteo` and
    /// `met-norway` are the tail wherever they sit in the list: Open-Meteo
    /// always follows the rows, then MET Norway when its row is on.
    pub(crate) fn providers_with(
        &self,
        place: &Place,
        contact: Option<&str>,
    ) -> (Providers, Providers) {
        let loc = Coordinates {
            latitude: place.latitude,
            longitude: place.longitude,
        };
        let rows = self.source_rows();
        let on = |id: &str| {
            rows.iter()
                .find(|r| r.provider == id)
                .is_some_and(|r| r.enabled)
        };
        let mut chosen = false;
        let mut nws = false;
        let mut done: Vec<&str> = Vec::new();
        let mut primary: Vec<Box<dyn Provider>> = Vec::new();
        for row in &rows {
            let id = row.provider.as_str();
            if done.contains(&id) {
                continue;
            }
            done.push(id);
            if !row.enabled {
                continue;
            }
            match id {
                SOURCE_NWS if whether_providers::regions::covers_us(&loc) => {
                    nws = true;
                    primary.push(Box::new(Nws::forecaster(
                        whether_providers::nws::DEFAULT_BASE_URL,
                    )));
                }
                SOURCE_MODELS if !for_location(&self.models, &loc).is_empty() => {
                    chosen = true;
                    primary.push(Box::new(OpenMeteo::new(OpenMeteoSettings {
                        models: self.models.clone(),
                        ..OpenMeteoSettings::default()
                    })));
                }
                SOURCE_OPENWEATHER => {
                    if let Some(key) = self.key_for(SOURCE_OPENWEATHER) {
                        primary.push(Box::new(OpenWeather::new(OpenWeatherSettings {
                            api_key: key.to_string(),
                            api: if self.one_call {
                                OwmApi::OneCall4
                            } else {
                                OwmApi::Free
                            },
                            base_url: DEFAULT_BASE_URL.to_string(),
                            daily_call_limit: self.daily_call_limit,
                        })));
                    }
                }
                SOURCE_PIRATE_WEATHER => {
                    if let Some(key) = self.key_for(SOURCE_PIRATE_WEATHER) {
                        primary.push(Box::new(PirateWeather::new(PirateWeatherSettings {
                            monthly_call_limit: self.pirate_weather_monthly_calls,
                            ..PirateWeatherSettings::new(key)
                        })));
                    }
                }
                // The tail, below; other ids are from a newer build.
                _ => {}
            }
        }
        primary.push(Box::new(OpenMeteo::default()));
        let mut supplements: Vec<Box<dyn Provider>> = Vec::new();
        // Models lack some fields (the NOAA blend has no UV or pressure);
        // OpenWeather lacks UV, moon and the nowcast. (A supplement equal to the
        // primary that served is skipped.)
        if chosen || nws || (self.fill_gaps && primary.len() > 1) {
            supplements.push(Box::new(OpenMeteo::default()));
        }
        // Keyless and global, but only when Open-Meteo has failed: it serves
        // after the last Open-Meteo, and fills nothing (it lacks UV, air quality).
        if on(SOURCE_MET_NORWAY)
            && let Some(contact) = contact
        {
            primary.push(Box::new(MetNorway::new(MetNorwaySettings::new(contact))));
        }
        // Official US alerts, free; outside the US it sends nothing.
        supplements.push(Box::new(Nws::default()));
        (primary, supplements)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use whether_core::model::ProviderId;
    use whether_core::units::UnitPrefs;

    #[test]
    fn debug_never_prints_a_key() {
        let mut s = default_settings();
        s.openweather_key = Some("ow-secret-0001".into());
        s.keys = [
            ("openweather".to_string(), "ow-secret-0001".to_string()),
            ("pirate-weather".to_string(), "pw-secret-0002".to_string()),
        ]
        .into();
        let shown = format!("{s:?}");
        assert!(!shown.contains("secret"), "{shown}");
        assert!(shown.contains("openweather_key: Some(\"***\")"), "{shown}");
        assert!(shown.contains("\"pirate-weather\": \"***\""), "{shown}");
        // With no key it still says so.
        s.openweather_key = None;
        assert!(format!("{s:?}").contains("openweather_key: None"));
    }

    #[test]
    fn default_budget_leaves_headroom_under_the_free_limit() {
        assert_eq!(default_settings().daily_call_limit, DEFAULT_DAILY_CALLS);
        const { assert!(DEFAULT_DAILY_CALLS < 1000) };
    }

    fn place() -> Place {
        Place {
            name: "New York".into(),
            latitude: 40.7143,
            longitude: -74.005_97,
            units: None,
            utc_offset_seconds: None,
            offset_changes: Vec::new(),
        }
    }

    #[test]
    fn units_follow_the_locale_then_the_place() {
        let mut s = default_settings();
        s.locale = Some("en_US".into());
        assert_eq!(s.presentation(&place()).units, UnitPrefs::IMPERIAL);
        s.locale = Some("en_GB".into());
        assert_eq!(s.presentation(&place()).units, UnitPrefs::METRIC);
        let abroad = Place {
            units: Some(Units::Imperial),
            ..place()
        };
        assert_eq!(s.presentation(&abroad).units, UnitPrefs::IMPERIAL);
        let p = s.presentation(&place());
        assert_eq!((p.hourly_hours, p.daily_days), (48, 8));
        assert_eq!(&p.details[..2], [DetailKind::Wind, DetailKind::Gusts]);
        assert!(p.shows(WidgetKind::Nowcast) && p.shows(WidgetKind::Alerts));
        assert_eq!(p.time_format, TimeFormat::H12);
        assert_eq!(p.date_format, DateFormat::MonthNameDay);
        let s = Settings {
            date_style: DateStyle::Compact,
            ..s
        };
        assert_eq!(s.presentation(&place()).date_format, DateFormat::Compact);
    }

    #[test]
    fn overrides_change_one_unit_at_a_time() {
        use crate::units::{PrecipitationUnit, TemperatureUnit, WindUnit};
        use whether_core::units as core;
        let mut s = default_settings();
        s.units = Units::Metric;
        let plain = s.presentation(&place()).units;
        // Empty overrides change nothing.
        s.unit_overrides = UnitOverrides::default();
        assert_eq!(s.presentation(&place()).units, plain);
        assert_eq!(plain, UnitPrefs::METRIC);
        // The UK: °C with mph, the rest of the system untouched.
        s.unit_overrides = UnitOverrides {
            temperature: Some(TemperatureUnit::Celsius),
            wind: Some(WindUnit::Mph),
            ..UnitOverrides::default()
        };
        s.units = Units::Imperial;
        let u = s.presentation(&place()).units;
        assert_eq!(
            (u.temperature, u.wind, u.pressure, u.precipitation),
            (
                core::TemperatureUnit::Celsius,
                core::WindUnit::Mph,
                core::PressureUnit::InHg,
                core::PrecipUnit::In
            )
        );
        s.unit_overrides.precipitation = Some(PrecipitationUnit::Mm);
        assert_eq!(
            s.presentation(&place()).units.precipitation,
            core::PrecipUnit::Mm
        );
    }

    fn ids(s: &Settings, place: &Place) -> (Vec<ProviderId>, Vec<ProviderId>) {
        let (p, sup) = s.providers_with(place, None);
        (
            p.iter().map(|p| p.id()).collect(),
            sup.iter().map(|p| p.id()).collect(),
        )
    }

    #[test]
    fn openweather_needs_a_key() {
        let ids = |s: &Settings| ids(s, &place());
        let mut s = default_settings();
        assert_eq!(
            ids(&s),
            (vec![ProviderId::OpenMeteo], vec![ProviderId::Nws]),
            "NWS alerts always"
        );
        s.openweather_key = Some("  ".into());
        assert_eq!(
            ids(&s).0,
            [ProviderId::OpenMeteo],
            "blank keys do not count"
        );
        s.openweather_key = Some("KEY".into());
        assert_eq!(
            ids(&s),
            (
                vec![ProviderId::OpenWeather, ProviderId::OpenMeteo],
                vec![ProviderId::OpenMeteo, ProviderId::Nws]
            )
        );
    }

    #[test]
    fn met_norway_serves_only_when_open_meteo_fails() {
        let contact = Some("https://example.org/whether");
        let mut s = default_settings();
        assert!(s.met_norway, "on unless switched off");
        let chain = |s: &Settings, contact| {
            let (p, sup) = s.providers_with(&place(), contact);
            (
                p.iter().map(|p| p.id()).collect::<Vec<_>>(),
                sup.iter().map(|p| p.id()).collect::<Vec<_>>(),
            )
        };
        // With a contact it ends the chain, and adds no supplement.
        assert_eq!(
            chain(&s, contact),
            (
                vec![ProviderId::OpenMeteo, ProviderId::MetNorway],
                vec![ProviderId::Nws]
            )
        );
        // Without one (the placeholder URL) it is off.
        assert_eq!(chain(&s, None).0, [ProviderId::OpenMeteo]);
        assert_eq!(
            s.providers_with(&place(), None).0.len(),
            1,
            "no real contact yet"
        );
        // Switched off.
        s.met_norway = false;
        assert_eq!(chain(&s, contact).0, [ProviderId::OpenMeteo]);
        // After every other source, a key's OpenWeather included.
        s.met_norway = true;
        s.openweather_key = Some("KEY".into());
        assert_eq!(
            chain(&s, contact).0,
            [
                ProviderId::OpenWeather,
                ProviderId::OpenMeteo,
                ProviderId::MetNorway
            ]
        );
    }

    #[test]
    fn met_norway_is_unmetered() {
        let p = MetNorway::new(MetNorwaySettings::new("https://example.org/whether"));
        assert!(p.budget().is_none(), "no call budget");
    }

    #[test]
    fn the_nws_forecasts_us_places_first() {
        let mut s = default_settings();
        s.nws = true;
        assert_eq!(
            ids(&s, &place()),
            (
                vec![ProviderId::Nws, ProviderId::OpenMeteo],
                vec![ProviderId::OpenMeteo, ProviderId::Nws]
            ),
            "Open-Meteo fills UV, air quality and the days past the 7th"
        );
        let london = Place {
            latitude: 51.5,
            longitude: -0.13,
            ..place()
        };
        assert_eq!(ids(&s, &london).0, [ProviderId::OpenMeteo], "not abroad");
    }

    #[test]
    fn presets_and_saved_switches() {
        let presets = model_presets();
        assert_eq!(presets[0].name, "Best match");
        assert_eq!(presets[0].models, Vec::<String>::new());
        let blend = presets
            .iter()
            .find(|p| p.models == [NOAA_BLEND])
            .expect("a NOAA blend preset");
        // An older phone's "NOAA blend" switch becomes that preset.
        assert_eq!(models_from_noaa_blend(true), blend.models);
        assert_eq!(models_from_noaa_blend(false), presets[0].models);
    }

    #[test]
    fn a_model_list_forecasts_where_it_has_data() {
        let mut s = default_settings();
        s.models = vec![
            "ncep_hrrr_conus".into(),
            "ncep_nbm_conus".into(),
            "gfs_seamless".into(),
        ];
        let (primary, _) = s.providers_with(&place(), None);
        assert_eq!(
            primary[0].variant(),
            Some("ncep_hrrr_conus,ncep_nbm_conus,gfs_seamless")
        );
        let london = Place {
            latitude: 51.5,
            longitude: -0.13,
            ..place()
        };
        let (primary, _) = s.providers_with(&london, None);
        assert_eq!(
            primary[0].variant(),
            Some("ncep_hrrr_conus,ncep_nbm_conus,gfs_seamless"),
            "GFS covers London"
        );
        s.models = vec!["ncep_hrrr_conus".into()];
        let (primary, _) = s.providers_with(&london, None);
        assert_eq!(primary.len(), 1, "no models with data: best match alone");
        assert_eq!(primary[0].variant(), None);
    }

    #[test]
    fn the_noaa_blend_forecasts_the_lower_48_only() {
        let mut s = default_settings();
        s.models = models_from_noaa_blend(true);
        let (primary, supplements) = s.providers_with(&place(), None);
        assert_eq!(primary[0].variant(), Some(NOAA_BLEND));
        assert_eq!(
            ids(&s, &place()),
            (
                vec![ProviderId::OpenMeteo, ProviderId::OpenMeteo],
                vec![ProviderId::OpenMeteo, ProviderId::Nws]
            ),
            "the best match falls back and fills UV and pressure"
        );
        assert_eq!(supplements[0].variant(), None);
        let london = Place {
            latitude: 51.5,
            longitude: -0.13,
            ..place()
        };
        let (primary, _) = s.providers_with(&london, None);
        assert_eq!(primary[0].variant(), None, "no blend abroad");
    }

    fn london() -> Place {
        Place {
            latitude: 51.5,
            longitude: -0.13,
            ..place()
        }
    }

    /// The chain with every variant (a model list tells two Open-Meteos apart).
    fn chain(s: &Settings, place: &Place, contact: Option<&str>) -> Vec<(ProviderId, String)> {
        let (p, sup) = s.providers_with(place, contact);
        p.iter()
            .map(|p| (p.id(), format!("{:?}", p.variant())))
            .chain(
                sup.iter()
                    .map(|p| (p.id(), format!("supp {:?}", p.variant()))),
            )
            .collect()
    }

    fn row(provider: &str, enabled: bool) -> SourceChoice {
        SourceChoice::new(provider, enabled)
    }

    #[test]
    fn the_default_list_is_todays_order() {
        let ids: Vec<_> = default_sources().into_iter().map(|r| r.provider).collect();
        assert_eq!(
            ids,
            [
                "nws",
                "open-meteo-models",
                "openweather",
                "pirate-weather",
                "open-meteo",
                "met-norway"
            ]
        );
        assert!(
            default_sources().iter().skip(1).all(|r| r.enabled),
            "only the NWS is off; the keyed rows serve once they have a key"
        );
        assert!(
            default_settings().sources.is_empty(),
            "earlier chain by default"
        );
    }

    #[test]
    fn a_list_made_from_the_switches_reproduces_the_fixed_chain() {
        const CONTACT: Option<&str> = Some("https://example.org/whether");
        let blend = models_from_noaa_blend(true);
        let hires = vec!["ncep_hrrr_conus".to_string(), "gfs_seamless".to_string()];
        for place in [place(), london()] {
            for contact in [None, CONTACT] {
                for nws in [false, true] {
                    for models in [Vec::new(), blend.clone(), hires.clone()] {
                        for key in [None, Some("KEY".to_string())] {
                            for (fill_gaps, one_call, met) in
                                [(true, false, true), (false, true, false)]
                            {
                                let mut s = default_settings();
                                s.nws = nws;
                                s.models = models.clone();
                                s.openweather_key = key.clone();
                                s.fill_gaps = fill_gaps;
                                s.one_call = one_call;
                                s.met_norway = met;
                                let fixed = chain(&s, &place, contact);
                                // The same choices, as the Sources screen saves them.
                                s.sources = default_sources();
                                s.sources[0].enabled = nws;
                                s.sources[5].enabled = met;
                                assert_eq!(chain(&s, &place, contact), fixed);
                                // The key may come from the Keychain's map instead.
                                s.openweather_key = None;
                                if let Some(k) = &key {
                                    s.keys.insert("openweather".into(), k.clone());
                                }
                                assert_eq!(chain(&s, &place, contact), fixed);
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn openweather_can_come_before_the_models() {
        let mut s = default_settings();
        s.models = vec!["ncep_hrrr_conus".into(), "gfs_seamless".into()];
        s.keys.insert("openweather".into(), "KEY".into());
        s.sources = vec![
            row("openweather", true),
            row("open-meteo-models", true),
            row("open-meteo", true),
        ];
        let (primary, supplements) = s.providers_with(&place(), None);
        assert_eq!(
            primary.iter().map(|p| p.id()).collect::<Vec<_>>(),
            [
                ProviderId::OpenWeather,
                ProviderId::OpenMeteo,
                ProviderId::OpenMeteo
            ]
        );
        assert_eq!(primary[1].variant(), Some("ncep_hrrr_conus,gfs_seamless"));
        assert_eq!(primary[2].variant(), None);
        assert_eq!(supplements.len(), 2, "best match fills, NWS alerts");
        // The earlier order is models first.
        s.sources = default_sources();
        let (primary, _) = s.providers_with(&place(), None);
        assert_eq!(primary[0].variant(), Some("ncep_hrrr_conus,gfs_seamless"));
        assert_eq!(primary[1].id(), ProviderId::OpenWeather);
    }

    #[test]
    fn a_disabled_row_is_skipped() {
        let mut s = default_settings();
        s.models = models_from_noaa_blend(true);
        s.keys.insert("openweather".into(), "KEY".into());
        s.sources = default_sources();
        s.sources[0].enabled = true;
        let contact = Some("https://example.org/whether");
        let ids = |s: &Settings| {
            let (p, _) = s.providers_with(&place(), contact);
            p.iter().map(|p| p.id()).collect::<Vec<_>>()
        };
        assert_eq!(
            ids(&s),
            [
                ProviderId::Nws,
                ProviderId::OpenMeteo,
                ProviderId::OpenWeather,
                ProviderId::OpenMeteo,
                ProviderId::MetNorway
            ]
        );
        s.sources[0].enabled = false;
        s.sources[2].enabled = false;
        s.sources[5].enabled = false;
        assert_eq!(ids(&s), [ProviderId::OpenMeteo, ProviderId::OpenMeteo]);
        s.sources[1].enabled = false;
        assert_eq!(ids(&s), [ProviderId::OpenMeteo]);
        // A row that is on still needs its key, its models, its coverage.
        s.sources = default_sources();
        s.sources[0].enabled = true;
        s.keys.clear();
        s.models.clear();
        assert_eq!(
            chain(&s, &london(), None).len(),
            2,
            "Open-Meteo, NWS alerts"
        );
        assert_eq!(
            ids(&s),
            [
                ProviderId::Nws,
                ProviderId::OpenMeteo,
                ProviderId::MetNorway
            ]
        );
        s.keys.insert("openweather".into(), "  ".into());
        assert_eq!(ids(&s).len(), 3, "blank keys do not count");
    }

    #[test]
    fn open_meteo_always_ends_the_chain() {
        let contact = Some("https://example.org/whether");
        let mut s = default_settings();
        s.keys.insert("openweather".into(), "KEY".into());
        for sources in [
            vec![row("openweather", true)],
            vec![row("openweather", true), row("met-norway", true)],
            // Position is ignored: the tail follows the rows.
            vec![
                row("met-norway", true),
                row("open-meteo", true),
                row("openweather", true),
            ],
            vec![
                row("open-meteo", false),
                row("openweather", true),
                row("met-norway", true),
            ],
        ] {
            s.sources = sources.clone();
            let (p, _) = s.providers_with(&place(), contact);
            let ids: Vec<_> = p.iter().map(|p| p.id()).collect();
            let met = sources.iter().any(|r| r.provider == "met-norway");
            assert_eq!(
                ids,
                if met {
                    vec![
                        ProviderId::OpenWeather,
                        ProviderId::OpenMeteo,
                        ProviderId::MetNorway,
                    ]
                } else {
                    vec![ProviderId::OpenWeather, ProviderId::OpenMeteo]
                },
                "{sources:?}"
            );
        }
        // Nothing enabled still forecasts, from Open-Meteo.
        s.sources = vec![row("openweather", false), row("nws", false)];
        assert_eq!(chain(&s, &place(), None).len(), 2);
        let (p, _) = s.providers_with(&place(), None);
        assert_eq!(
            p.iter().map(|p| p.id()).collect::<Vec<_>>(),
            [ProviderId::OpenMeteo]
        );
    }

    #[test]
    fn met_norway_follows_its_row_not_the_old_switch_once_there_is_a_list() {
        let contact = Some("https://example.org/whether");
        let mut s = default_settings();
        s.met_norway = false;
        s.sources = default_sources();
        let (p, _) = s.providers_with(&place(), contact);
        assert_eq!(p.last().map(|p| p.id()), Some(ProviderId::MetNorway));
        s.sources.retain(|r| r.provider != "met-norway");
        let (p, _) = s.providers_with(&place(), contact);
        assert_eq!(p.last().map(|p| p.id()), Some(ProviderId::OpenMeteo));
    }

    #[test]
    fn unknown_and_repeated_ids_are_ignored() {
        let mut s = default_settings();
        s.keys.insert("openweather".into(), "KEY".into());
        let plain = {
            s.sources = default_sources();
            chain(&s, &place(), None)
        };
        // A newer build's list: ids this one has never heard of, and a key for one.
        s.sources = vec![
            row("visual-crossing", true),
            row("nws", false),
            row("open-meteo-models", true),
            row("openweather", true),
            row("some-future-thing", true),
            row("open-meteo", true),
            row("met-norway", true),
        ];
        s.keys.insert("visual-crossing".into(), "VKEY".into());
        assert_eq!(chain(&s, &place(), None), plain);
        // Only the first row for an id counts.
        s.sources = vec![row("openweather", false), row("openweather", true)];
        let (p, _) = s.providers_with(&place(), None);
        assert_eq!(
            p.iter().map(|p| p.id()).collect::<Vec<_>>(),
            [ProviderId::OpenMeteo]
        );
    }

    fn primary_ids(s: &Settings) -> Vec<ProviderId> {
        let (p, _) = s.providers_with(&place(), None);
        p.iter().map(|p| p.id()).collect()
    }

    #[test]
    fn pirate_weather_serves_with_its_own_key_after_openweather() {
        let mut s = default_settings();
        s.sources = default_sources();
        // No key, or a blank one: the chain is the same as without the row.
        let plain = chain(&s, &place(), None);
        s.keys.insert("pirate-weather".into(), "  ".into());
        assert_eq!(chain(&s, &place(), None), plain);
        // A key: it forecasts, with Open-Meteo filling its gaps (no UV) and behind it.
        s.keys.insert("pirate-weather".into(), "PKEY".into());
        assert_eq!(
            ids(&s, &place()),
            (
                vec![ProviderId::PirateWeather, ProviderId::OpenMeteo],
                vec![ProviderId::OpenMeteo, ProviderId::Nws]
            )
        );
        assert_eq!(
            ids(&s, &london()).0,
            [ProviderId::PirateWeather, ProviderId::OpenMeteo],
            "worldwide"
        );
        // Both keys: OpenWeather first, as it was before Pirate Weather existed.
        s.keys.insert("openweather".into(), "KEY".into());
        assert_eq!(
            primary_ids(&s),
            [
                ProviderId::OpenWeather,
                ProviderId::PirateWeather,
                ProviderId::OpenMeteo
            ]
        );
        // The user moves it first; the OpenWeather key is the other's key, not its own.
        s.sources = vec![
            row("pirate-weather", true),
            row("openweather", true),
            row("open-meteo", true),
        ];
        assert_eq!(
            primary_ids(&s),
            [
                ProviderId::PirateWeather,
                ProviderId::OpenWeather,
                ProviderId::OpenMeteo
            ]
        );
        s.keys.remove("pirate-weather");
        assert_eq!(
            primary_ids(&s),
            [ProviderId::OpenWeather, ProviderId::OpenMeteo],
            "the OpenWeather key is not Pirate Weather's"
        );
        // Switched off, or absent from the list.
        s.keys.insert("pirate-weather".into(), "PKEY".into());
        s.sources[0].enabled = false;
        assert_eq!(
            primary_ids(&s),
            [ProviderId::OpenWeather, ProviderId::OpenMeteo]
        );
        s.sources.remove(0);
        s.sources[0].enabled = false;
        assert_eq!(primary_ids(&s), [ProviderId::OpenMeteo]);
    }

    #[test]
    fn pirate_weather_comes_after_the_models_by_default() {
        let mut s = default_settings();
        s.sources = default_sources();
        s.models = models_from_noaa_blend(true);
        s.keys.insert("pirate-weather".into(), "PKEY".into());
        assert_eq!(
            primary_ids(&s),
            [
                ProviderId::OpenMeteo,
                ProviderId::PirateWeather,
                ProviderId::OpenMeteo
            ],
            "the NOAA blend (Open-Meteo) first, then the keyed source"
        );
    }

    #[test]
    fn the_fixed_chain_never_includes_pirate_weather() {
        // Empty `sources` is exactly the chain of every earlier install.
        let mut s = default_settings();
        s.keys.insert("pirate-weather".into(), "PKEY".into());
        assert_eq!(primary_ids(&s), [ProviderId::OpenMeteo]);
    }

    #[test]
    fn pirate_weather_has_a_month_budget_set_by_the_phone() {
        let mut s = default_settings();
        assert_eq!(s.pirate_weather_monthly_calls, 9000);
        assert_eq!(
            s.pirate_weather_monthly_calls, DEFAULT_MONTHLY_CALLS,
            "the UniFFI default and the constant agree"
        );
        s.sources = default_sources();
        s.keys.insert("pirate-weather".into(), "PKEY".into());
        s.pirate_weather_monthly_calls = 1234;
        let (primary, _) = s.providers_with(&place(), None);
        assert_eq!(
            primary[0].budget(),
            Some(whether_providers::Budget::monthly(1234))
        );
    }

    #[test]
    fn the_nws_covers_new_york_not_london() {
        assert!(nws_covers(40.7143, -74.005_97));
        assert!(!nws_covers(51.5, -0.13));
    }
}
