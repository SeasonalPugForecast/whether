//! Which data fields the enabled widgets need, so providers request no more
//! than necessary (a trimmed Open-Meteo request is about 2 KB instead of 24 KB).

use bitflags::bitflags;

use crate::presentation::{
    DailyIcon, DetailKind, HourlyMetric, PresentationConfig, TemperatureBasis, WidgetKind,
};

/// Days that get daylight icons at most: 240 hours of hourly data, which is
/// what `hourly_hours` (a `u8`) holds; later days use the provider's icon.
pub const DAYLIGHT_ICON_DAYS: u8 = 10;

bitflags! {
    /// Fields of the current conditions.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
    pub struct CurrentFields: u32 {
        /// Air temperature.
        const TEMPERATURE = 1 << 0;
        /// Apparent ("feels like") temperature.
        const APPARENT = 1 << 1;
        /// Relative humidity.
        const HUMIDITY = 1 << 2;
        /// Dew point.
        const DEW_POINT = 1 << 3;
        /// Day or night flag.
        const IS_DAY = 1 << 4;
        /// Precipitation now.
        const PRECIPITATION = 1 << 5;
        /// Weather condition code.
        const WEATHER_CODE = 1 << 6;
        /// Cloud cover.
        const CLOUD_COVER = 1 << 7;
        /// Sea-level pressure.
        const PRESSURE = 1 << 8;
        /// Wind speed.
        const WIND_SPEED = 1 << 9;
        /// Wind direction.
        const WIND_DIRECTION = 1 << 10;
        /// Wind gusts.
        const WIND_GUSTS = 1 << 11;
        /// Visibility.
        const VISIBILITY = 1 << 12;
        /// UV index.
        const UV_INDEX = 1 << 13;
    }

    /// Fields of each hourly step.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
    pub struct HourlyFields: u32 {
        /// Air temperature.
        const TEMPERATURE = 1 << 0;
        /// Apparent temperature.
        const APPARENT = 1 << 1;
        /// Precipitation probability.
        const PRECIP_PROBABILITY = 1 << 2;
        /// Precipitation amount.
        const PRECIPITATION = 1 << 3;
        /// Weather condition code.
        const WEATHER_CODE = 1 << 4;
        /// Day or night flag.
        const IS_DAY = 1 << 5;
        /// Wind speed.
        const WIND_SPEED = 1 << 6;
        /// Wind direction.
        const WIND_DIRECTION = 1 << 7;
        /// Wind gusts.
        const WIND_GUSTS = 1 << 8;
        /// Relative humidity.
        const HUMIDITY = 1 << 9;
        /// Sea-level pressure.
        const PRESSURE = 1 << 10;
        /// UV index.
        const UV_INDEX = 1 << 11;
        /// Visibility.
        const VISIBILITY = 1 << 12;
        /// Cloud cover.
        const CLOUD_COVER = 1 << 13;
    }

    /// Fields of each daily entry.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
    pub struct DailyFields: u32 {
        /// Dominant condition code.
        const WEATHER_CODE = 1 << 0;
        /// Maximum temperature.
        const TEMP_MAX = 1 << 1;
        /// Minimum temperature.
        const TEMP_MIN = 1 << 2;
        /// Precipitation total.
        const PRECIP_SUM = 1 << 3;
        /// Maximum precipitation probability.
        const PRECIP_PROBABILITY_MAX = 1 << 4;
        /// Sunrise time.
        const SUNRISE = 1 << 5;
        /// Sunset time.
        const SUNSET = 1 << 6;
        /// Daylight duration.
        const DAYLIGHT = 1 << 7;
        /// Maximum UV index.
        const UV_INDEX_MAX = 1 << 8;
        /// Maximum wind speed.
        const WIND_SPEED_MAX = 1 << 9;
        /// Maximum gusts.
        const WIND_GUSTS_MAX = 1 << 10;
        /// Dominant wind direction.
        const WIND_DIRECTION_DOMINANT = 1 << 11;
        /// Moon phase.
        const MOON_PHASE = 1 << 12;
        /// Moonrise time.
        const MOONRISE = 1 << 13;
        /// Moonset time.
        const MOONSET = 1 << 14;
        /// Highest "feels like" temperature.
        const APPARENT_MAX = 1 << 15;
        /// Lowest "feels like" temperature.
        const APPARENT_MIN = 1 << 16;
    }
}

/// The data a set of widgets needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct DataRequirements {
    /// Current-condition fields.
    pub current: CurrentFields,
    /// Hourly fields.
    pub hourly: HourlyFields,
    /// Hours of hourly data, from now.
    pub hourly_hours: u8,
    /// Hours of hourly data before now: today's daylight, so a day icon after
    /// sunset still describes the whole day.
    pub past_hours: u8,
    /// Daily fields.
    pub daily: DailyFields,
    /// Days of daily data, from today.
    pub daily_days: u8,
    /// Hours of 15-minute precipitation for the nowcast (0: none).
    pub minutely_hours: u8,
    /// Air quality.
    pub air_quality: bool,
    /// Official alerts, including details that cost extra requests.
    pub alerts: bool,
}

/// Hours before now that hold today's daylight, whatever the time.
const PAST_HOURS: u8 = 24;

/// The longest nowcast, in hours (32 steps of 15 minutes).
pub const MAX_NOWCAST_HOURS: u8 = 8;

/// Hours of look-ahead the summary sentence needs.
const SUMMARY_HOURS: u8 = 12;
/// Hours the pressure trend needs (now and three hours later).
const TREND_HOURS: u8 = 4;

impl DataRequirements {
    /// Everything any widget could show: 48 hours and 7 days.
    pub fn full() -> Self {
        Self {
            current: CurrentFields::all(),
            hourly: HourlyFields::all(),
            hourly_hours: 48,
            past_hours: PAST_HOURS,
            daily: DailyFields::all(),
            daily_days: 7,
            minutely_hours: MAX_NOWCAST_HOURS,
            air_quality: true,
            alerts: true,
        }
    }

    /// The data needed by the widgets enabled in `p`.
    pub fn from_presentation(p: &PresentationConfig) -> Self {
        let mut r = Self {
            current: CurrentFields::IS_DAY | CurrentFields::WEATHER_CODE,
            ..Self::default()
        };
        for widget in &p.widgets {
            match widget {
                WidgetKind::Current => {
                    r.current |= CurrentFields::TEMPERATURE | CurrentFields::APPARENT;
                    r.daily |= DailyFields::TEMP_MAX | DailyFields::TEMP_MIN;
                }
                WidgetKind::Summary => {
                    r.current |= CurrentFields::TEMPERATURE;
                    r.daily |= DailyFields::TEMP_MAX | DailyFields::TEMP_MIN;
                    r.hourly |= HourlyFields::PRECIP_PROBABILITY | HourlyFields::WEATHER_CODE;
                    r.hourly_hours = r.hourly_hours.max(SUMMARY_HOURS);
                }
                WidgetKind::Hourly => {
                    r.hourly |= HourlyFields::TEMPERATURE
                        | HourlyFields::WEATHER_CODE
                        | HourlyFields::IS_DAY;
                    for metric in &p.hourly_metrics {
                        r.hourly |= hourly_metric_fields(*metric);
                    }
                    r.hourly_hours = r.hourly_hours.max(p.hourly_hours.clamp(1, 48));
                }
                WidgetKind::Daily => {
                    r.daily |= DailyFields::WEATHER_CODE
                        | DailyFields::TEMP_MAX
                        | DailyFields::TEMP_MIN
                        | DailyFields::PRECIP_PROBABILITY_MAX
                        | DailyFields::PRECIP_SUM;
                    r.daily_days = r.daily_days.max(p.daily_days.clamp(1, 16));
                    if p.daily_hours {
                        // Each shown day's hours, for expanding it.
                        r.hourly |= HourlyFields::TEMPERATURE
                            | HourlyFields::WEATHER_CODE
                            | HourlyFields::IS_DAY
                            | HourlyFields::PRECIP_PROBABILITY;
                        r.hourly_hours = r
                            .hourly_hours
                            .max(p.daily_days.clamp(1, DAYLIGHT_ICON_DAYS) * 24);
                    }
                    if p.daily_icon == DailyIcon::Daylight {
                        // Each shown day's daylight hours, plus sunrise and sunset.
                        r.hourly |= HourlyFields::WEATHER_CODE | HourlyFields::IS_DAY;
                        r.daily |= DailyFields::SUNRISE | DailyFields::SUNSET;
                        r.past_hours = PAST_HOURS;
                        r.hourly_hours = r
                            .hourly_hours
                            .max(p.daily_days.clamp(1, DAYLIGHT_ICON_DAYS) * 24);
                    }
                }
                WidgetKind::Details => {
                    for kind in &p.details {
                        r.add_detail(*kind);
                    }
                }
                WidgetKind::Sun => {
                    r.daily |= DailyFields::SUNRISE | DailyFields::SUNSET | DailyFields::DAYLIGHT;
                }
                WidgetKind::Moon => {
                    r.daily |=
                        DailyFields::MOON_PHASE | DailyFields::MOONRISE | DailyFields::MOONSET;
                }
                WidgetKind::AirQuality => r.air_quality = true,
                WidgetKind::Nowcast => {
                    r.minutely_hours = r
                        .minutely_hours
                        .max(p.nowcast_hours.clamp(1, MAX_NOWCAST_HOURS));
                }
                WidgetKind::Alerts => r.alerts = true,
                // Not provider data: the runtime fetches radar on its own.
                WidgetKind::Radar => {}
                WidgetKind::Precipitation => {
                    r.hourly |= HourlyFields::PRECIPITATION
                        | HourlyFields::PRECIP_PROBABILITY
                        | HourlyFields::WEATHER_CODE;
                    r.hourly_hours = r.hourly_hours.max(p.precip_hours.clamp(1, 48));
                }
                // Today's forecast high, against yesterday's; the past
                // itself is history (`HistoryRequirements`).
                WidgetKind::History => r.daily |= DailyFields::TEMP_MAX | DailyFields::TEMP_MIN,
            }
        }
        if !r.daily.is_empty() {
            r.daily_days = r.daily_days.max(1);
        }
        if p.temperature_basis == TemperatureBasis::FeelsLike {
            // The "feels like" values beside each temperature asked for.
            if r.current.contains(CurrentFields::TEMPERATURE) {
                r.current |= CurrentFields::APPARENT;
            }
            if r.hourly.contains(HourlyFields::TEMPERATURE) {
                r.hourly |= HourlyFields::APPARENT;
            }
            if r.daily
                .intersects(DailyFields::TEMP_MAX | DailyFields::TEMP_MIN)
            {
                r.daily |= DailyFields::APPARENT_MAX | DailyFields::APPARENT_MIN;
            }
        }
        r
    }

    fn add_detail(&mut self, kind: DetailKind) {
        match kind {
            DetailKind::Wind => {
                self.current |= CurrentFields::WIND_SPEED
                    | CurrentFields::WIND_DIRECTION
                    | CurrentFields::WIND_GUSTS;
            }
            DetailKind::Gusts => self.current |= CurrentFields::WIND_GUSTS,
            DetailKind::Humidity => {
                self.current |= CurrentFields::HUMIDITY | CurrentFields::DEW_POINT;
            }
            DetailKind::DewPoint => {
                self.current |=
                    CurrentFields::DEW_POINT | CurrentFields::TEMPERATURE | CurrentFields::HUMIDITY;
            }
            DetailKind::Pressure => {
                self.current |= CurrentFields::PRESSURE;
                self.hourly |= HourlyFields::PRESSURE;
                self.hourly_hours = self.hourly_hours.max(TREND_HOURS);
            }
            DetailKind::Uv => {
                self.current |= CurrentFields::UV_INDEX;
                self.daily |= DailyFields::UV_INDEX_MAX;
            }
            DetailKind::Visibility => self.current |= CurrentFields::VISIBILITY,
            DetailKind::CloudCover => self.current |= CurrentFields::CLOUD_COVER,
            DetailKind::Precipitation => {
                self.current |= CurrentFields::PRECIPITATION;
                self.daily |= DailyFields::PRECIP_SUM;
            }
            DetailKind::FeelsLike => self.current |= CurrentFields::APPARENT,
            // History (`HistoryRequirements`), not forecast data.
            DetailKind::Normals | DetailKind::Records => {}
        }
    }

    /// Everything either set of requirements needs.
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        Self {
            current: self.current | other.current,
            hourly: self.hourly | other.hourly,
            hourly_hours: self.hourly_hours.max(other.hourly_hours),
            past_hours: self.past_hours.max(other.past_hours),
            daily: self.daily | other.daily,
            daily_days: self.daily_days.max(other.daily_days),
            minutely_hours: self.minutely_hours.max(other.minutely_hours),
            air_quality: self.air_quality || other.air_quality,
            alerts: self.alerts || other.alerts,
        }
    }
}

fn hourly_metric_fields(metric: HourlyMetric) -> HourlyFields {
    match metric {
        HourlyMetric::Temperature => HourlyFields::TEMPERATURE,
        HourlyMetric::FeelsLike => HourlyFields::APPARENT,
        HourlyMetric::PrecipProbability => HourlyFields::PRECIP_PROBABILITY,
        HourlyMetric::Precipitation => HourlyFields::PRECIPITATION,
        HourlyMetric::Wind => HourlyFields::WIND_SPEED | HourlyFields::WIND_DIRECTION,
        HourlyMetric::Humidity => HourlyFields::HUMIDITY,
        HourlyMetric::Uv => HourlyFields::UV_INDEX,
    }
}

/// What the history widget and cards need. History is fetched apart
/// from the forecast, about once a day.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HistoryRequirements {
    /// Observed days before today.
    pub past_days: u8,
    /// Calendar dates from today on whose climate is shown.
    pub climate_days: u8,
    /// Records with the climate.
    pub records: bool,
    /// The days since the 1st of the month (and their normals), for
    /// month-to-date totals.
    pub month_to_date: bool,
}

/// The most observed days the history widget shows.
pub const MAX_HISTORY_DAYS: u8 = 31;

impl HistoryRequirements {
    /// The history the widgets and cards enabled in `p` show. Once any history
    /// is fetched, the daily widget's days get their normals too.
    pub fn from_presentation(p: &PresentationConfig) -> Self {
        let details = |kind: DetailKind| p.shows(WidgetKind::Details) && p.details.contains(&kind);
        let mut r = Self::default();
        if p.shows(WidgetKind::History) {
            r.past_days = p.history_days.min(MAX_HISTORY_DAYS);
            r.month_to_date = true;
            r.climate_days = 1;
            r.records = true;
        }
        if details(DetailKind::Normals) || details(DetailKind::Records) {
            r.climate_days = r.climate_days.max(1);
        }
        r.records |= details(DetailKind::Records);
        if !r.is_empty() && p.shows(WidgetKind::Daily) {
            r.climate_days = r.climate_days.max(p.daily_days.clamp(1, 16));
        }
        r
    }

    /// Whether no history is needed.
    pub fn is_empty(&self) -> bool {
        self.past_days == 0 && self.climate_days == 0 && !self.month_to_date
    }

    /// Whether history fetched for `self` has everything `other` needs.
    pub fn covers(&self, other: &Self) -> bool {
        self.past_days >= other.past_days
            && self.climate_days >= other.climate_days
            && (self.records || !other.records)
            && (self.month_to_date || !other.month_to_date)
    }

    /// Everything either needs.
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        Self {
            past_days: self.past_days.max(other.past_days),
            climate_days: self.climate_days.max(other.climate_days),
            records: self.records || other.records,
            month_to_date: self.month_to_date || other.month_to_date,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn default_presentation() {
        insta::assert_debug_snapshot!(DataRequirements::from_presentation(
            &PresentationConfig::default()
        ));
    }

    #[test]
    fn current_only() {
        let p = PresentationConfig {
            widgets: vec![WidgetKind::Current],
            ..Default::default()
        };
        let r = DataRequirements::from_presentation(&p);
        assert_eq!(
            r.current,
            CurrentFields::IS_DAY
                | CurrentFields::WEATHER_CODE
                | CurrentFields::TEMPERATURE
                | CurrentFields::APPARENT
        );
        assert_eq!(r.daily, DailyFields::TEMP_MAX | DailyFields::TEMP_MIN);
        assert_eq!((r.hourly_hours, r.daily_days), (0, 1));
        assert!(r.hourly.is_empty() && !r.air_quality && r.minutely_hours == 0);
    }

    #[test]
    fn esp32_like_presentation() {
        let p = PresentationConfig {
            widgets: vec![
                WidgetKind::Current,
                WidgetKind::Hourly,
                WidgetKind::Daily,
                WidgetKind::Details,
            ],
            hourly_hours: 12,
            daily_days: 5,
            // As the firmware asks (its buffers are small).
            daily_icon: DailyIcon::Provider,
            details: vec![
                DetailKind::Wind,
                DetailKind::Humidity,
                DetailKind::Uv,
                DetailKind::Pressure,
            ],
            ..Default::default()
        };
        insta::assert_debug_snapshot!(DataRequirements::from_presentation(&p));
    }

    #[test]
    fn daylight_icons_ask_for_every_shown_day() {
        let p = PresentationConfig {
            widgets: vec![WidgetKind::Daily],
            daily_days: 16,
            ..Default::default()
        };
        let r = DataRequirements::from_presentation(&p);
        assert_eq!(r.hourly_hours, 240, "capped at DAYLIGHT_ICON_DAYS");
        assert!(r.hourly.contains(HourlyFields::WEATHER_CODE));
        assert!(r.daily.contains(DailyFields::SUNRISE | DailyFields::SUNSET));
        let provider = PresentationConfig {
            daily_icon: DailyIcon::Provider,
            ..p
        };
        assert_eq!(
            DataRequirements::from_presentation(&provider).hourly_hours,
            0
        );
    }

    #[test]
    fn union_takes_the_larger_of_everything() {
        let a = DataRequirements {
            hourly_hours: 12,
            daily_days: 3,
            ..Default::default()
        };
        let b = DataRequirements {
            hourly_hours: 4,
            daily_days: 7,
            air_quality: true,
            alerts: true,
            ..Default::default()
        };
        let u = a.union(b);
        assert_eq!(
            (u.hourly_hours, u.daily_days, u.air_quality, u.alerts),
            (12, 7, true, true)
        );
        assert_eq!(DataRequirements::full().union(a), DataRequirements::full());
    }

    #[test]
    fn hours_and_days_are_clamped() {
        let p = PresentationConfig {
            hourly_hours: 200,
            daily_days: 0,
            ..Default::default()
        };
        let r = DataRequirements::from_presentation(&p);
        assert_eq!((r.hourly_hours, r.daily_days), (48, 1));
    }
}
