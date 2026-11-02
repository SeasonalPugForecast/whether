//! What to show and how to format it: the display-independent presentation settings.
//!
//! Field names match the TOML keys of `[presentation]`; `whether-config` resolves the
//! TOML (including units) into a [`PresentationConfig`].

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use crate::units::UnitPrefs;

/// Clock style for times of day.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum TimeFormat {
    /// `14:00`.
    #[default]
    #[cfg_attr(feature = "serde", serde(rename = "24h"))]
    H24,
    /// `2 PM`.
    #[cfg_attr(feature = "serde", serde(rename = "12h"))]
    H12,
}

/// How a calendar date is written, shown for 1 November (month names come from
/// the active strings).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum DateFormat {
    /// `Nov 1`.
    #[default]
    #[cfg_attr(feature = "serde", serde(rename = "mmm d"))]
    MonthNameDay,
    /// `1 Nov`.
    #[cfg_attr(feature = "serde", serde(rename = "d mmm"))]
    DayMonthName,
    /// `NOV1`.
    #[cfg_attr(feature = "serde", serde(rename = "MMMd"))]
    Compact,
    /// `11/1`.
    #[cfg_attr(feature = "serde", serde(rename = "m/d"))]
    MonthDay,
    /// `11/01`.
    #[cfg_attr(feature = "serde", serde(rename = "mm/dd"))]
    MonthDayPadded,
    /// `1/11`.
    #[cfg_attr(feature = "serde", serde(rename = "d/m"))]
    DayMonth,
    /// `01/11`.
    #[cfg_attr(feature = "serde", serde(rename = "dd/mm"))]
    DayMonthPadded,
    /// `01.11.`.
    #[cfg_attr(feature = "serde", serde(rename = "dd.mm."))]
    DayMonthDots,
    /// `2026-11-01`.
    #[cfg_attr(feature = "serde", serde(rename = "yyyy-mm-dd"))]
    Iso,
}

/// A block of information that displays can show. Displays arrange them; the order
/// here is the user's preferred order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "snake_case")
)]
#[allow(missing_docs)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum WidgetKind {
    Current,
    Summary,
    Hourly,
    Daily,
    Details,
    AirQuality,
    Sun,
    Moon,
    Nowcast,
    Alerts,
    // Radar imagery (US); the runtime fetches it, displays draw it.
    Radar,
    // The next hours' rain and snow, hour by hour (`precip_hours`).
    Precipitation,
    // The last days as observed, against normal (`history_days`).
    History,
}

/// One card in the details widget.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "snake_case")
)]
#[allow(missing_docs)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum DetailKind {
    Wind,
    Gusts,
    Humidity,
    DewPoint,
    Pressure,
    Uv,
    Visibility,
    CloudCover,
    Precipitation,
    FeelsLike,
    // Today's normal high and low.
    Normals,
    // Today's record high and low, with their years.
    Records,
}

/// A series shown in the hourly widget.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "snake_case")
)]
#[allow(missing_docs)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum HourlyMetric {
    Temperature,
    FeelsLike,
    PrecipProbability,
    Precipitation,
    Wind,
    Humidity,
    Uv,
}

/// What a day's icon and condition describe.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "snake_case")
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum DailyIcon {
    /// The hours between sunrise and sunset, from the hourly forecast: rain at
    /// 1 AM does not make a sunny day rainy. Days without hourly data use the
    /// provider's.
    #[default]
    Daylight,
    /// The provider's daily code (often the worst weather of all 24 hours).
    Provider,
}

/// Which temperature displays lead with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "snake_case")
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum TemperatureBasis {
    /// The air temperature; "Feels like" beside it.
    #[default]
    Actual,
    /// The "feels like" (apparent) temperature, the highs and lows too, where the
    /// data has it; "Actual" beside it.
    FeelsLike,
}

/// The preferred air-quality index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "snake_case")
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum AqiScalePref {
    /// US EPA AQI.
    #[default]
    Us,
    /// European AQI.
    European,
}

/// Resolved presentation settings shared by every display.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PresentationConfig {
    /// Display units, one per quantity.
    pub units: UnitPrefs,
    /// Clock style.
    pub time_format: TimeFormat,
    /// How dates are written.
    pub date_format: DateFormat,
    /// Enabled widgets in the preferred order.
    pub widgets: Vec<WidgetKind>,
    /// Hours shown by the hourly widget (1–48).
    pub hourly_hours: u8,
    /// Series shown by the hourly widget.
    pub hourly_metrics: Vec<HourlyMetric>,
    /// Days shown by the daily widget (1–16).
    pub daily_days: u8,
    /// What a day's icon describes.
    pub daily_icon: DailyIcon,
    /// Hours in the precipitation table (1–48).
    pub precip_hours: u8,
    /// Days in the history widget (0–31).
    pub history_days: u8,
    /// Hours the nowcast covers in 15-minute steps (1–8).
    pub nowcast_hours: u8,
    /// Which temperature leads.
    pub temperature_basis: TemperatureBasis,
    /// Each day row carries its own hours and timeline, for expanding a day.
    pub daily_hours: bool,
    /// Cards shown by the details widget, in order.
    pub details: Vec<DetailKind>,
    /// Preferred air-quality index.
    pub aqi_scale: AqiScalePref,
    /// Precipitation probability (%) treated as "likely" in summaries.
    pub precip_threshold: u8,
    /// Decimals shown for temperatures.
    pub temperature_decimals: u8,
    /// `true` shows "21 °C" instead of "21°".
    pub show_unit_letter: bool,
    /// Placeholder for unavailable values.
    pub missing: String,
    /// Minutes around sunrise and sunset that use the twilight theme.
    pub twilight_minutes: u16,
}

impl Default for PresentationConfig {
    /// The defaults documented in `docs/config/whether.example.toml`.
    fn default() -> Self {
        Self {
            units: UnitPrefs::METRIC,
            time_format: TimeFormat::H24,
            date_format: DateFormat::MonthNameDay,
            widgets: vec![
                WidgetKind::Current,
                WidgetKind::Summary,
                WidgetKind::Hourly,
                WidgetKind::Precipitation,
                WidgetKind::Daily,
                WidgetKind::Details,
                WidgetKind::Sun,
                WidgetKind::AirQuality,
            ],
            hourly_hours: 24,
            hourly_metrics: vec![HourlyMetric::Temperature, HourlyMetric::PrecipProbability],
            daily_days: 7,
            daily_icon: DailyIcon::Daylight,
            precip_hours: 12,
            history_days: 7,
            nowcast_hours: 2,
            temperature_basis: TemperatureBasis::Actual,
            daily_hours: false,
            details: vec![
                DetailKind::Wind,
                DetailKind::Humidity,
                DetailKind::Pressure,
                DetailKind::Uv,
                DetailKind::Visibility,
                DetailKind::DewPoint,
                DetailKind::CloudCover,
                DetailKind::Precipitation,
            ],
            aqi_scale: AqiScalePref::Us,
            precip_threshold: 50,
            temperature_decimals: 0,
            show_unit_letter: false,
            missing: "—".to_string(),
            twilight_minutes: 45,
        }
    }
}

impl PresentationConfig {
    /// Whether `widget` is enabled.
    pub fn shows(&self, widget: WidgetKind) -> bool {
        self.widgets.contains(&widget)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_example_config() {
        let p = PresentationConfig::default();
        assert_eq!(p.hourly_hours, 24);
        assert_eq!(p.daily_days, 7);
        assert_eq!(p.precip_threshold, 50);
        assert_eq!(p.temperature_decimals, 0);
        assert!(!p.show_unit_letter);
        assert_eq!(p.missing, "—");
        assert_eq!(p.twilight_minutes, 45);
        assert_eq!(p.aqi_scale, AqiScalePref::Us);
        assert_eq!(p.widgets.len(), 8);
        assert!(p.shows(WidgetKind::Precipitation));
        assert_eq!((p.precip_hours, p.daily_icon), (12, DailyIcon::Daylight));
        assert_eq!((p.nowcast_hours, p.daily_hours), (2, false));
        assert_eq!(p.details.len(), 8);
        assert!(p.shows(WidgetKind::Sun) && !p.shows(WidgetKind::Moon));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_names_match_the_example_config() {
        let names = serde_json::to_string(&(
            WidgetKind::AirQuality,
            DetailKind::DewPoint,
            HourlyMetric::PrecipProbability,
            TimeFormat::H12,
            AqiScalePref::European,
            [DateFormat::Compact, DateFormat::DayMonthDots],
        ))
        .unwrap();
        assert_eq!(
            names,
            r#"["air_quality","dew_point","precip_probability","12h","european",["MMMd","dd.mm."]]"#
        );
    }
}
