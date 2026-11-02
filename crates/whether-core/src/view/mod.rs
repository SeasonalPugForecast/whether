//! The presentation layer: turns snapshots into display-ready view models.
//!
//! [`build_view`] is the only place values are converted and formatted. Displays
//! arrange and draw what the [`ViewModel`] gives them.

mod build;
pub mod format;
mod history;
mod outlook;
mod radar;
mod summary;
#[cfg(test)]
mod tests;

use alloc::string::String;
use alloc::vec::Vec;

pub use build::build_view;
pub use format::{Formatter, Quantity};
pub use history::plan_vm;
pub use radar::radar_vm;

use crate::condition::Condition;
use crate::history::History;
use crate::icon::{IconId, UiIcon};
use crate::model::WeatherSnapshot;
use crate::presentation::{DetailKind, PresentationConfig, WidgetKind};
use crate::scales::{AqiScale, Level, TempLevel};
use crate::strings::Strings;
use crate::theme::SkyTheme;
use crate::time::UnixTime;

/// Fetch status of one location, as the runtime knows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StatusInfo {
    /// When the last fetch succeeded.
    pub last_success: Option<UnixTime>,
    /// The last error, if the latest fetch failed.
    pub error: Option<String>,
    /// The data is older than the refresh interval.
    pub stale: bool,
    /// A fetch is in progress.
    pub refreshing: bool,
}

/// One location's data for the builder.
#[derive(Clone, Copy, Debug)]
pub struct LocationView<'a> {
    /// The configured location name.
    pub name: &'a str,
    /// The latest snapshot, if any.
    pub snapshot: Option<&'a WeatherSnapshot>,
    /// Its fetch status.
    pub status: &'a StatusInfo,
    /// Its history: past days, normals and records, if fetched.
    pub history: Option<&'a History>,
}

/// Everything [`build_view`] needs.
#[derive(Clone, Copy, Debug)]
pub struct ViewInput<'a> {
    /// All locations, for the tabs.
    pub locations: &'a [LocationView<'a>],
    /// The index of the location to show.
    pub active: usize,
    /// Presentation settings.
    pub presentation: &'a PresentationConfig,
    /// Active strings.
    pub strings: &'a Strings,
    /// The current time.
    pub now: UnixTime,
}

/// The display contract: everything a display shows, already formatted.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ViewModel {
    /// One tab per location.
    pub tabs: Vec<TabVm>,
    /// The shown location.
    pub active: usize,
    /// The shown location's header.
    pub location: LocationHeaderVm,
    /// Fetch status line.
    pub status: StatusVm,
    /// The required data attribution; every display shows it.
    pub attribution: String,
    /// Sky mood for backgrounds and accents.
    pub theme: SkyTheme,
    /// Whether it is day at the location.
    pub is_day: bool,
    /// Whether the temperatures that lead (the big number, the hours' and the
    /// days' `temperature`, `high` and `low`) are feels-like ones, so displays
    /// can say so; the air temperature is then in `actual`, `actual_high` and
    /// `actual_low`.
    pub feels_like_leads: bool,
    /// Enabled widgets with data, in the configured order.
    pub widgets: Vec<WidgetKind>,
    /// Current conditions.
    pub current: Option<CurrentVm>,
    /// A one-sentence outlook.
    pub summary: Option<String>,
    /// Hourly forecast.
    pub hourly: Option<HourlyVm>,
    /// Daily forecast.
    pub daily: Option<DailyVm>,
    /// Detail cards in the configured order.
    pub details: Vec<DetailVm>,
    /// Air quality.
    pub air_quality: Option<AirQualityVm>,
    /// Sunrise, sunset and daylight.
    pub sun: Option<SunVm>,
    /// Moon phase and times.
    pub moon: Option<MoonVm>,
    /// Two-hour precipitation nowcast.
    pub nowcast: Option<NowcastVm>,
    /// Official warnings.
    pub alerts: Vec<AlertVm>,
    /// Radar loop labels, when imagery exists (the pixels travel beside the view).
    pub radar: Option<RadarVm>,
    /// The next hours' rain and snow, hour by hour.
    pub precipitation: Option<PrecipVm>,
    /// The last days as observed, against normal.
    pub history: Option<HistoryVm>,
}

/// A radar loop's text: one label per frame, oldest first.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RadarVm {
    /// Frame times, "14:35".
    pub frames: Vec<String>,
    /// The required credit (radar source and base map).
    pub attribution: String,
}

/// A location tab.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[allow(missing_docs)]
pub struct TabVm {
    pub name: String,
    pub temp: String,
    pub icon: IconId,
}

/// The header of the shown location.
#[derive(Clone, Debug, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct LocationHeaderVm {
    /// Location name.
    pub name: String,
    /// Region line.
    pub region: Option<String>,
    /// Local weekday and time, e.g. "Mon 11:00".
    pub local_time: String,
}

/// The fetch status line.
#[derive(Clone, Debug, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct StatusVm {
    /// "Updated 3 min ago", "Fetching weather…" or "No weather data yet".
    pub updated: String,
    /// Primary provider name.
    pub provider: String,
    /// The data is old.
    pub stale: bool,
    /// A fetch is running.
    pub refreshing: bool,
    /// The last error.
    pub error: Option<String>,
    /// "Observed at Central Park, 8:55 AM" when a station measured
    /// the current conditions.
    pub observed: Option<String>,
}

/// Current conditions.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[allow(missing_docs)]
pub struct CurrentVm {
    pub temperature: Quantity,
    pub temp_level: TempLevel,
    pub feels_like: Quantity,
    /// "Feels like 14°".
    pub feels_like_label: String,
    /// Today's high.
    pub high: Quantity,
    /// Today's low.
    pub low: Quantity,
    pub condition: Condition,
    pub condition_label: String,
    pub icon: IconId,
}

/// Wind speed and direction.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct WindVm {
    /// Speed.
    pub speed: Quantity,
    /// Gusts.
    pub gust: Quantity,
    /// Direction the wind blows from, in degrees.
    pub direction_deg: Option<f32>,
    /// 16-point compass abbreviation, or empty.
    pub compass: &'static str,
    /// Arrow pointing downwind, or a space.
    pub arrow: char,
}

/// One hourly column.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[allow(missing_docs)]
pub struct HourVm {
    pub time: UnixTime,
    /// "Now", "14:00" or "2 PM".
    pub label: String,
    /// Its day: "Today", "Tomorrow" or the weekday.
    pub day: String,
    /// Its date, "Nov 1" (`date_format`).
    pub date_short: String,
    pub icon: IconId,
    pub is_day: bool,
    pub temperature: Quantity,
    pub temp_level: TempLevel,
    pub feels_like: Quantity,
    /// The air temperature, whichever leads.
    pub actual: Quantity,
    pub precip_probability: Option<u8>,
    pub precipitation: Quantity,
    pub wind: WindVm,
    pub humidity: Option<u8>,
    pub uv: Option<f32>,
    /// Humidity as text, `"62%"`.
    pub humidity_text: Option<String>,
    /// UV index as text, `"3"`.
    pub uv_text: Option<String>,
    /// The hour's condition, `"Light rain"`.
    pub condition_label: String,
    /// Dew point (from the temperature and humidity).
    pub dew_point: Quantity,
    /// Sea-level pressure.
    pub pressure: Quantity,
    /// Visibility.
    pub visibility: Quantity,
    /// Cloud cover, `"40%"`.
    pub cloud_cover: Quantity,
}

/// The hourly forecast.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct HourlyVm {
    /// Columns from the current hour.
    pub hours: Vec<HourVm>,
    /// Lowest shown temperature (display units).
    pub temp_min: f32,
    /// Highest shown temperature (display units).
    pub temp_max: f32,
    /// Largest shown precipitation (display units).
    pub precip_max: f32,
    /// Index of the column containing now.
    pub now_index: usize,
    /// The first 24 hours as stretches of one kind of weather (Dark Sky's bar).
    pub timeline: Vec<TimelineSpanVm>,
}

/// The kind of weather a timeline stretch shows (its color).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "snake_case")
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[allow(missing_docs)]
pub enum TimelineGroup {
    Clear,
    PartlyCloudy,
    Cloudy,
    Fog,
    Drizzle,
    Rain,
    HeavyRain,
    Snow,
    Storm,
}

/// A stretch of hours with one kind of weather.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TimelineSpanVm {
    /// The first hour, an index into `HourlyVm.hours`.
    pub start: usize,
    /// Hours it lasts.
    pub len: usize,
    /// Its kind of weather.
    pub group: TimelineGroup,
    /// Its most severe condition, `"Light rain"`.
    pub label: String,
}

/// One daily row.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[allow(missing_docs)]
pub struct DayVm {
    /// "Today", "Tomorrow" or a weekday.
    pub label: String,
    /// "Oct 7".
    pub date_short: String,
    pub icon: IconId,
    pub condition_label: String,
    pub high: Quantity,
    pub low: Quantity,
    /// The air temperature's high and low, whichever leads.
    pub actual_high: Quantity,
    pub actual_low: Quantity,
    pub temp_level_high: TempLevel,
    pub precip_probability: Option<u8>,
    pub precip_sum: Quantity,
    /// Low and high positions in 0..=1 relative to the period's range.
    pub range: (f32, f32),
    pub sunrise: Option<String>,
    pub sunset: Option<String>,
    pub uv_max: Option<f32>,
    pub wind_max: Quantity,
    /// The strongest gust: the day's own, else its hours' strongest.
    pub gust_max: Quantity,
    /// When its precipitation falls, from its hours: "Light drizzle in the
    /// early afternoon." `None` when dry or without hourly data.
    pub precip_text: Option<String>,
    /// The day in a forecaster's words: "Cold in the morning, then sunny and
    /// warm in the afternoon. Breezy." (with `precip_text` in it). `None`
    /// without daytime hours.
    pub outlook: Option<String>,
    /// Its hours, midnight to midnight as far as the forecast goes, when
    /// `daily_hours` is set (for expanding the day).
    pub hours: Vec<HourVm>,
    /// Those hours as stretches of one kind of weather.
    pub timeline: Vec<TimelineSpanVm>,
    /// The day's normal high, when history is fetched.
    pub normal_high: Quantity,
    /// The day's normal low.
    pub normal_low: Quantity,
}

/// The daily forecast.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct DailyVm {
    /// Rows from today.
    pub days: Vec<DayVm>,
    /// Lowest temperature of the period (display units).
    pub period_min: f32,
    /// Highest temperature of the period (display units).
    pub period_max: f32,
}

/// One details card.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct DetailVm {
    /// Which card.
    pub kind: DetailKind,
    /// Card title.
    pub title: String,
    /// Main value.
    pub value: String,
    /// Secondary line.
    pub caption: Option<String>,
    /// Severity for coloring.
    pub level: Option<Level>,
    /// Gauge fill in 0..=1.
    pub gauge: Option<f32>,
    /// Direction for compass cards.
    pub direction_deg: Option<f32>,
    /// Where the value is heading (pressure over the next three hours), for an
    /// arrow; the caption says the same in words.
    pub trend: Option<crate::derive::Trend>,
}

/// Air quality.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct AirQualityVm {
    /// The index shown.
    pub scale: AqiScale,
    /// Index value.
    pub index: u16,
    /// Category name.
    pub category_label: String,
    /// Severity.
    pub level: Level,
    /// Gauge fill in 0..=1.
    pub gauge: f32,
    /// `(name, value)` pairs, e.g. `("PM2.5", "8.4 µg/m³")`.
    pub pollutants: Vec<(&'static str, String)>,
}

/// Sun times.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SunVm {
    /// Sunrise time or the missing placeholder.
    pub sunrise: String,
    /// Sunset time or the missing placeholder.
    pub sunset: String,
    /// "11h 35m".
    pub daylight: String,
    /// Fraction of daylight elapsed, during the day.
    pub progress: Option<f32>,
    /// Whether the sun is up.
    pub is_day: bool,
    /// Solar noon, when the sun is highest.
    pub solar_noon: Option<String>,
    /// The day's light from dawn to dusk, in time order: astronomical,
    /// nautical and civil dawn, the morning golden hour's end, the evening
    /// golden hour's start, civil, nautical and astronomical dusk. Only those
    /// the place has today (none of the deep ones in a northern summer night).
    pub twilight: Vec<SunEventVm>,
    /// Today's sunrise with its label and instant, when the place has one.
    pub rise: Option<RiseSetVm>,
    /// Today's sunset, likewise.
    pub set: Option<RiseSetVm>,
    /// Solar noon with its label and instant ("Solar noon", "1:15 PM").
    pub noon: Option<RiseSetVm>,
}

/// A sunrise, sunset, solar noon, moonrise or moonset: the words, the clock time and the
/// instant, for listing in time order and drawing on a day's bar or arc.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RiseSetVm {
    /// "Sunrise", "Moonset".
    pub label: String,
    /// "6:49 AM".
    pub time: String,
    /// When.
    pub at: UnixTime,
}

/// One moment of the day's light.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SunEventVm {
    /// "Civil dawn".
    pub label: String,
    /// "6:49 AM".
    pub time: String,
    /// When, for ordering and drawing.
    pub at: UnixTime,
    /// Before solar noon.
    pub morning: bool,
    /// Which crossing: how far the sun is below (or above) the horizon.
    pub kind: TwilightKind,
}

/// The sun's altitude at a [`SunEventVm`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "snake_case")
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum TwilightKind {
    /// 18° below: full night beyond it.
    Astronomical,
    /// 12° below.
    Nautical,
    /// 6° below.
    Civil,
    /// 6° above: the golden hour's edge.
    Golden,
}

/// Moon phase and times.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[allow(missing_docs)]
pub struct MoonVm {
    pub phase: f32,
    pub phase_label: String,
    pub icon: UiIcon,
    pub moonrise: Option<String>,
    pub moonset: Option<String>,
    /// How much of the disc is lit: "62% illuminated".
    pub illumination: Option<String>,
    /// Today's moonrise with its label and instant, when the moon rises today.
    pub rise: Option<RiseSetVm>,
    /// Today's moonset, likewise.
    pub set: Option<RiseSetVm>,
}

/// One hour of the precipitation table.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PrecipHourVm {
    /// The hour's start.
    pub time: UnixTime,
    /// "Now", "3 PM".
    pub label: String,
    /// Chance of precipitation, `"40%"`.
    pub chance: Option<String>,
    /// The chance as a number, for drawing.
    pub chance_value: Option<u8>,
    /// The amount in this hour.
    pub amount: Quantity,
    /// How hard it falls, 0–1 (1 at 7.6 mm an hour, heavy rain).
    pub intensity: f32,
    /// Frozen (snow, sleet, freezing rain).
    pub frozen: bool,
}

/// The history widget: the last days as observed, against normal.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct HistoryVm {
    /// Observed days, oldest first; days without a report show "—".
    pub days: Vec<PastDayVm>,
    /// Today's normals, records and chance of precipitation.
    pub today: Option<ClimateVm>,
    /// Precipitation since the 1st against normal.
    pub month_to_date: Option<MonthToDateVm>,
    /// "4° warmer than yesterday": today's forecast high against yesterday's.
    pub yesterday_delta: Option<String>,
    /// Where it comes from: "NY City Central Park, 7.8 km · Normals 1991–2020".
    pub credit: String,
}

/// One observed day.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[allow(missing_docs)]
pub struct PastDayVm {
    /// "Yesterday", "Sat".
    pub label: String,
    /// "Oct 3".
    pub date_short: String,
    pub high: Quantity,
    pub low: Quantity,
    /// Rain and melted snow; "Trace" when too little to measure.
    pub precipitation: Quantity,
    /// The high against normal, in display units: "+5°".
    pub departure: Quantity,
    pub normal_high: Quantity,
    pub normal_low: Quantity,
    /// The low against normal: "+3°".
    pub low_departure: Quantity,
    /// The date's normal precipitation.
    pub normal_precip: Quantity,
    /// Snowfall, when any fell.
    pub snow: Option<Quantity>,
    /// The date's record high and low: "94° in 1941".
    pub record_high: Option<String>,
    pub record_low: Option<String>,
    /// The day in a sentence or two: "9° warmer than normal. 0.24 in of rain."
    /// with "A record high for the date." when it set one. `None` without a
    /// report.
    pub summary: Option<String>,
}

/// A date's climate: averages, records and how often it rained or snowed.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[allow(missing_docs)]
pub struct ClimateVm {
    pub normal_high: Quantity,
    pub normal_low: Quantity,
    /// "Normal 69° / 55°".
    pub normal_text: String,
    /// "94° in 1941".
    pub record_high: Option<String>,
    /// "35° in 1881".
    pub record_low: Option<String>,
    /// "Rain or snow on this date in 23% of years".
    pub precip_chance: Option<String>,
}

/// A climate query as shown (the vacation planner): each trip date's
/// averages, chance of rain or snow and extremes over the chosen years.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PlanVm {
    /// "Jul 12 – Jul 19, as in 1995–2024".
    pub title: String,
    /// The dates, in order.
    pub days: Vec<PlanDayVm>,
    /// "On average 86° / 71°" over all the dates.
    pub average: Option<String>,
    /// "Rain or snow on 34% of days": the dates' chances, averaged.
    pub wet_days: Option<String>,
    /// Where it comes from: "NY City Central Park, 4.8 mi · 1995–2024".
    pub credit: String,
    /// The data attribution: "History from NOAA's Regional Climate
    /// Centers (ACIS)".
    pub attribution: String,
}

/// One date of a trip.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PlanDayVm {
    /// The trip date's weekday: "Mon".
    pub label: String,
    /// "Jul 12".
    pub date_short: String,
    /// The typical weather: from how often it was wet, and how cold or hot it
    /// was (sun, some cloud, showers or storms, rain, their snowy forms).
    pub icon: crate::icon::IconId,
    /// The average high.
    pub high: Quantity,
    /// The average low.
    pub low: Quantity,
    /// The average precipitation.
    pub precipitation: Quantity,
    /// The share of years with rain or snow that date.
    pub precip_chance: Quantity,
    /// The highest high: "94° in 2011".
    pub hottest: Option<String>,
    /// The lowest low: "50° in 2002".
    pub coldest: Option<String>,
    /// The highest high, for a graph.
    pub highest: Quantity,
    /// The lowest low, for a graph.
    pub lowest: Quantity,
    /// The average low and high on a scale shared by every date (0–1, `-1`
    /// when unknown).
    pub range: (f32, f32),
    /// The lowest low and highest high on that scale.
    pub extremes: (f32, f32),
}

/// Precipitation since the 1st of the month.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[allow(missing_docs)]
pub struct MonthToDateVm {
    pub precipitation: Quantity,
    pub normal: Quantity,
    /// "2.10 in this month, 2.90 in normal".
    pub text: String,
}

/// The precipitation table: the next `precip_hours` hours.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PrecipVm {
    /// The hours, from the current one.
    pub hours: Vec<PrecipHourVm>,
    /// The amount over all of them.
    pub total: Quantity,
    /// The highest chance among them, `"70%"`.
    pub max_chance: Option<String>,
    /// "No precipitation in the next 12 hours." when every hour is dry (no
    /// amount and a chance under 15%), else `None`.
    pub dry: Option<String>,
}

/// Precipitation nowcast.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct NowcastVm {
    /// "Rain starting in ~45 min."
    pub text: String,
    /// Precipitation per 15-minute step, in millimetres.
    pub series_mm: Vec<f32>,
    /// How hard each step falls, 0–1 on Dark Sky's scale: a third at light
    /// (2.5 mm an hour), two thirds at moderate (7.6), full at heavy (50).
    pub intensity: Vec<f32>,
    /// The scale's lines from a third up: "Light", "Moderate", "Heavy".
    pub levels: Vec<String>,
    /// Time axis labels; `step` 0 is now, `series_mm.len()` the end.
    pub ticks: Vec<NowcastTickVm>,
    /// Hours covered (`nowcast_hours`).
    pub hours: u8,
    /// Minutes per step: 15, or 1 for an hour of minute data (WeatherKit).
    pub step_minutes: u8,
    /// Whether any step has precipitation.
    pub wet: bool,
}

/// A label on the nowcast's time axis.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct NowcastTickVm {
    /// The step it marks.
    pub step: usize,
    /// "Now", "30 min", "2 h".
    pub label: String,
}

/// An official warning.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[allow(missing_docs)]
pub struct AlertVm {
    /// The provider's identifier (`Alert.id`), stable while the alert lasts even when
    /// its text changes: the key for telling alerts apart.
    pub id: Option<String>,
    pub title: String,
    pub body: String,
    pub level: Level,
    /// "Mon 14:00 – Tue 06:00".
    pub period: String,
    /// When the alert ends, for comparing updates (shown only through `period`).
    pub end: UnixTime,
    /// Who issued it (`Alert.source`: "NWS Upton NY"); additive and optional
    /// in plugin protocol v1, absent when the source is unnamed.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub source: Option<String>,
    /// A page with the alert's details (`Alert.url`; additive and optional in
    /// plugin protocol v1).
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub url: Option<String>,
}
