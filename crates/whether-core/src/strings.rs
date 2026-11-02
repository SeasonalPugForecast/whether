//! Every user-visible word, with English defaults, built-in translations
//! ([`crate::packs`]) and `[strings]` overrides on top.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use crate::condition::{Condition, Intensity};
use crate::derive::Trend;
use crate::icon::UiIcon;
use crate::presentation::DetailKind;
use crate::scales::{AqiLabel, Level, UvLevel};
use crate::time::Weekday;

macro_rules! texts {
    ($($variant:ident => $key:literal, $default:literal;)+) => {
        /// A user-visible text. Keys are the `[strings]` override names.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[allow(missing_docs)]
        pub enum Text { $($variant),+ }

        /// `(text, key, English default)`, in [`Text`] declaration order.
        const TABLE: &[(Text, &str, &str)] = &[$((Text::$variant, $key, $default)),+];
    };
}

texts! {
    Today => "today", "Today";
    Tomorrow => "tomorrow", "Tomorrow";
    Now => "now", "Now";
    FeelsLike => "feels_like", "Feels like {temp}";
    ActualTemp => "actual_temp", "Actual {temp}";
    High => "high", "H";
    Low => "low", "L";
    UpdatedJustNow => "updated_just_now", "Updated just now";
    UpdatedMinutesAgo => "updated_minutes_ago", "Updated {n} min ago";
    UpdatedHoursAgo => "updated_hours_ago", "Updated {n} h ago";
    ObservedAt => "observed_at", "Observed at {station}, {time}";
    Yesterday => "yesterday", "Yesterday";
    Trace => "trace", "Trace";
    NormalHighLow => "normal_high_low", "Normal {high} / {low}";
    RecordIn => "record_in", "{value} in {year}";
    PrecipChance => "precip_chance", "Rain or snow on this date in {n}% of years";
    MonthToDate => "month_to_date", "{amount} this month, normal {normal}";
    MonthToDateOnly => "month_to_date_only", "{amount} this month";
    WarmerThanYesterday => "warmer_than_yesterday", "{n} warmer than yesterday";
    CoolerThanYesterday => "cooler_than_yesterday", "{n} cooler than yesterday";
    SameAsYesterday => "same_as_yesterday", "As warm as yesterday";
    NormalsPeriod => "normals_period", "Normals {years}";
    SkySunny => "sky_sunny", "sunny";
    SkyMostlySunny => "sky_mostly_sunny", "mostly sunny";
    SkyPartlyCloudy => "sky_partly_cloudy", "partly cloudy";
    SkyMostlyCloudy => "sky_mostly_cloudy", "mostly cloudy";
    SkyCloudy => "sky_cloudy", "cloudy";
    SkyFoggy => "sky_foggy", "foggy";
    TempBitter => "temp_bitter", "bitterly cold";
    TempFreezing => "temp_freezing", "freezing";
    TempCold => "temp_cold", "cold";
    TempCool => "temp_cool", "cool";
    TempMild => "temp_mild", "mild";
    TempWarm => "temp_warm", "warm";
    TempHot => "temp_hot", "hot";
    TempVeryHot => "temp_very_hot", "very hot";
    DayAnd => "day_and", "{a} and {b}";
    DayMorningThen => "day_morning_then", "{am} in the morning, then {pm} in the afternoon";
    DayBreezy => "day_breezy", "Breezy.";
    DayWindy => "day_windy", "Windy, with gusts to {gust}.";
    DayHumid => "day_humid", "Humid.";
    DayFalling => "day_falling", "Temperatures falling through the day.";
    PastWarmer => "past_warmer", "{n} warmer than normal.";
    PastCooler => "past_cooler", "{n} cooler than normal.";
    PastNormal => "past_normal", "Near normal.";
    PastDry => "past_dry", "Dry.";
    PastRain => "past_rain", "{amount} of rain.";
    PastTrace => "past_trace", "A trace of rain.";
    PastSnow => "past_snow", "{amount} of snow.";
    PastRecordHigh => "past_record_high", "A record high for the date.";
    PastRecordLow => "past_record_low", "A record low for the date.";
    PastRecordWet => "past_record_wet", "The wettest on record for the date.";
    DayLikely => "day_likely", "{kind} likely.";
    DayPossible => "day_possible", "{kind} possible.";
    PlanTitle => "plan_title", "{dates}, as in {years}";
    PlanAverage => "plan_average", "On average {high} / {low}";
    PlanWetDays => "plan_wet_days", "Rain or snow on {n}% of days";
    Reanalysis => "reanalysis", "ERA5 reanalysis (contains modified Copernicus Climate Change Service information)";
    RecordsFrom => "records_from", "records: {station}";
    Stale => "stale", "Showing older data";
    Refreshing => "refreshing", "Refreshing…";
    Offline => "offline", "Offline";
    Fetching => "fetching", "Fetching weather…";
    NoData => "no_data", "No weather data yet";
    DetailWind => "detail_wind", "Wind";
    DetailGusts => "detail_gusts", "Gusts";
    DetailHumidity => "detail_humidity", "Humidity";
    DetailDewPoint => "detail_dew_point", "Dew point";
    DetailPressure => "detail_pressure", "Pressure";
    DetailUv => "detail_uv", "UV index";
    DetailVisibility => "detail_visibility", "Visibility";
    DetailCloudCover => "detail_cloud_cover", "Cloud cover";
    DetailPrecipitation => "detail_precipitation", "Precipitation";
    DetailFeelsLike => "detail_feels_like", "Feels like";
    DetailNormals => "detail_normals", "Normal";
    DetailRecords => "detail_records", "Records";
    DewPointCaption => "dew_point_caption", "Dew point {temp}";
    GustsCaption => "gusts_caption", "Gusts {speed}";
    TodayTotalCaption => "today_total_caption", "Today {amount}";
    Sunrise => "sunrise", "Sunrise";
    Sunset => "sunset", "Sunset";
    Daylight => "daylight", "Daylight";
    Night => "night", "Night";
    Moon => "moon", "Moon";
    Moonrise => "moonrise", "Moonrise";
    Moonset => "moonset", "Moonset";
    SolarNoon => "solar_noon", "Solar noon";
    AstronomicalDawn => "astronomical_dawn", "Astronomical dawn";
    NauticalDawn => "nautical_dawn", "Nautical dawn";
    CivilDawn => "civil_dawn", "Civil dawn";
    GoldenHourEnds => "golden_hour_ends", "Golden hour ends";
    GoldenHourBegins => "golden_hour_begins", "Golden hour begins";
    CivilDusk => "civil_dusk", "Civil dusk";
    NauticalDusk => "nautical_dusk", "Nautical dusk";
    AstronomicalDusk => "astronomical_dusk", "Astronomical dusk";
    MoonIlluminated => "moon_illuminated", "{n}% illuminated";
    AlertAsOf => "alert_as_of", "As of {time}";
    AirQuality => "air_quality", "Air quality";
    PressureRising => "pressure_rising", "Rising";
    PressureFalling => "pressure_falling", "Falling";
    PressureSteady => "pressure_steady", "Steady";
    UvLow => "uv_low", "Low";
    UvModerate => "uv_moderate", "Moderate";
    UvHigh => "uv_high", "High";
    UvVeryHigh => "uv_very_high", "Very high";
    UvExtreme => "uv_extreme", "Extreme";
    AqiGood => "aqi_good", "Good";
    AqiFair => "aqi_fair", "Fair";
    AqiModerate => "aqi_moderate", "Moderate";
    AqiUnhealthySensitive => "aqi_unhealthy_sensitive", "Unhealthy for sensitive groups";
    AqiUnhealthy => "aqi_unhealthy", "Unhealthy";
    AqiVeryUnhealthy => "aqi_very_unhealthy", "Very unhealthy";
    AqiHazardous => "aqi_hazardous", "Hazardous";
    AqiPoor => "aqi_poor", "Poor";
    AqiVeryPoor => "aqi_very_poor", "Very poor";
    AqiExtremelyPoor => "aqi_extremely_poor", "Extremely poor";
    CondClear => "cond_clear", "Clear";
    CondMainlyClear => "cond_mainly_clear", "Mainly clear";
    CondPartlyCloudy => "cond_partly_cloudy", "Partly cloudy";
    CondCloudy => "cond_cloudy", "Cloudy";
    CondOvercast => "cond_overcast", "Overcast";
    CondFog => "cond_fog", "Fog";
    CondHaze => "cond_haze", "Haze";
    CondSmoke => "cond_smoke", "Smoke";
    CondDust => "cond_dust", "Dust";
    CondLightDrizzle => "cond_light_drizzle", "Light drizzle";
    CondDrizzle => "cond_drizzle", "Drizzle";
    CondHeavyDrizzle => "cond_heavy_drizzle", "Heavy drizzle";
    CondFreezingDrizzle => "cond_freezing_drizzle", "Freezing drizzle";
    CondLightRain => "cond_light_rain", "Light rain";
    CondRain => "cond_rain", "Rain";
    CondHeavyRain => "cond_heavy_rain", "Heavy rain";
    CondFreezingRain => "cond_freezing_rain", "Freezing rain";
    CondLightShowers => "cond_light_showers", "Light showers";
    CondShowers => "cond_showers", "Showers";
    CondHeavyShowers => "cond_heavy_showers", "Heavy showers";
    CondLightSleet => "cond_light_sleet", "Light sleet";
    CondSleet => "cond_sleet", "Sleet";
    CondHeavySleet => "cond_heavy_sleet", "Heavy sleet";
    CondSleetShowers => "cond_sleet_showers", "Sleet showers";
    CondLightSnow => "cond_light_snow", "Light snow";
    CondSnow => "cond_snow", "Snow";
    CondHeavySnow => "cond_heavy_snow", "Heavy snow";
    CondSnowShowers => "cond_snow_showers", "Snow showers";
    CondSnowGrains => "cond_snow_grains", "Snow grains";
    CondThunderstorm => "cond_thunderstorm", "Thunderstorm";
    CondSevereThunderstorm => "cond_severe_thunderstorm", "Severe thunderstorm";
    CondThunderstormHail => "cond_thunderstorm_hail", "Thunderstorm with hail";
    CondSquall => "cond_squall", "Squalls";
    CondTornado => "cond_tornado", "Tornado";
    CondUnknown => "cond_unknown", "Unknown";
    WeekdayMon => "weekday_mon", "Mon";
    WeekdayTue => "weekday_tue", "Tue";
    WeekdayWed => "weekday_wed", "Wed";
    WeekdayThu => "weekday_thu", "Thu";
    WeekdayFri => "weekday_fri", "Fri";
    WeekdaySat => "weekday_sat", "Sat";
    WeekdaySun => "weekday_sun", "Sun";
    MonthJan => "month_jan", "Jan";
    MonthFeb => "month_feb", "Feb";
    MonthMar => "month_mar", "Mar";
    MonthApr => "month_apr", "Apr";
    MonthMay => "month_may", "May";
    MonthJun => "month_jun", "Jun";
    MonthJul => "month_jul", "Jul";
    MonthAug => "month_aug", "Aug";
    MonthSep => "month_sep", "Sep";
    MonthOct => "month_oct", "Oct";
    MonthNov => "month_nov", "Nov";
    MonthDec => "month_dec", "Dec";
    MoonNew => "moon_new", "New moon";
    MoonWaxingCrescent => "moon_waxing_crescent", "Waxing crescent";
    MoonFirstQuarter => "moon_first_quarter", "First quarter";
    MoonWaxingGibbous => "moon_waxing_gibbous", "Waxing gibbous";
    MoonFull => "moon_full", "Full moon";
    MoonWaningGibbous => "moon_waning_gibbous", "Waning gibbous";
    MoonLastQuarter => "moon_last_quarter", "Last quarter";
    MoonWaningCrescent => "moon_waning_crescent", "Waning crescent";
    SummaryHighLow => "summary_high_low", "High {hi}, low {lo}.";
    SummaryHighLowFeels => "summary_high_low_feels", "Feels-like high {hi}, low {lo}.";
    RainLikelyAt => "rain_likely_at", "{kind} likely around {time} ({pct}%).";
    NoPrecipToday => "no_precip_today", "No precipitation expected today.";
    NowcastStarting => "nowcast_starting", "{kind} starting in ~{n} min.";
    NowcastNow => "nowcast_now", "{kind} now.";
    NowcastStartingAt => "nowcast_starting_at", "{kind} starting around {time}.";
    NowcastStopping => "nowcast_stopping", "{kind} stopping in ~{n} min.";
    NowcastStoppingAt => "nowcast_stopping_at", "{kind} stopping around {time}.";
    NowcastDry => "nowcast_dry", "No precipitation in the next {n} hours.";
    NowcastDryHour => "nowcast_dry_hour", "No precipitation in the next hour.";
    DayWhen => "day_when", "{kind} {when}.";
    DayFromTo => "day_from_to", "{kind} from {start} to {end}.";
    DayAllDay => "day_all_day", "{kind} throughout the day.";
    PartOvernight => "part_overnight", "overnight";
    PartEarlyMorning => "part_early_morning", "in the early morning";
    PartMorning => "part_morning", "in the morning";
    PartLateMorning => "part_late_morning", "in the late morning";
    PartMidday => "part_midday", "around midday";
    PartEarlyAfternoon => "part_early_afternoon", "in the early afternoon";
    PartAfternoon => "part_afternoon", "in the afternoon";
    PartLateAfternoon => "part_late_afternoon", "in the late afternoon";
    PartEvening => "part_evening", "in the evening";
    PartLateEvening => "part_late_evening", "late in the evening";
    IntensityLight => "intensity_light", "Light";
    IntensityModerate => "intensity_moderate", "Moderate";
    IntensityHeavy => "intensity_heavy", "Heavy";
    MinutesShort => "minutes_short", "{n} min";
    HoursShort => "hours_short", "{n} h";
    PrecipDryHours => "precip_dry_hours", "No precipitation in the next {n} hours.";
    PrecipRain => "precip_rain", "Rain";
    PrecipSnow => "precip_snow", "Snow";
    PrecipGeneric => "precip_generic", "Precipitation";
    SeverityMinor => "severity_minor", "Minor";
    SeverityModerate => "severity_moderate", "Moderate";
    SeveritySevere => "severity_severe", "Severe";
    SeverityExtreme => "severity_extreme", "Extreme";
    RadarLight => "radar_light", "Light";
    RadarModerate => "radar_moderate", "Moderate";
    RadarHeavy => "radar_heavy", "Heavy";
    RadarVeryHeavy => "radar_very_heavy", "Very heavy";
    RadarExtreme => "radar_extreme", "Extreme";
}

impl Text {
    /// The `[strings]` key.
    pub fn key(self) -> &'static str {
        TABLE[self as usize].1
    }

    /// The English default.
    pub fn default_text(self) -> &'static str {
        TABLE[self as usize].2
    }

    /// The text for a key, if it exists.
    pub fn from_key(key: &str) -> Option<Self> {
        TABLE.iter().find(|(_, k, _)| *k == key).map(|(t, _, _)| *t)
    }
}

/// The active strings: English defaults plus user overrides.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Strings {
    overrides: BTreeMap<Text, String>,
}

impl Strings {
    /// Applies `[strings]` overrides. Returns the strings and any unknown keys.
    pub fn with_overrides(overrides: &BTreeMap<String, String>) -> (Self, Vec<String>) {
        let mut strings = Self::default();
        let mut unknown = Vec::new();
        for (key, value) in overrides {
            match Text::from_key(key) {
                Some(text) => {
                    strings.overrides.insert(text, value.clone());
                }
                None => unknown.push(key.clone()),
            }
        }
        (strings, unknown)
    }

    /// The pack for `language` (`general.language`), then `[strings]` overrides.
    /// Languages without a pack use English. Returns the unknown override keys too.
    pub fn localized(language: &str, overrides: &BTreeMap<String, String>) -> (Self, Vec<String>) {
        let (mut strings, unknown) = Self::with_overrides(overrides);
        for (text, translated) in crate::packs::pack(language).unwrap_or_default() {
            strings
                .overrides
                .entry(*text)
                .or_insert_with(|| String::from(*translated));
        }
        (strings, unknown)
    }

    /// The text, overridden or default.
    pub fn get(&self, text: Text) -> &str {
        self.overrides
            .get(&text)
            .map_or_else(|| text.default_text(), String::as_str)
    }

    /// The text with `{name}` placeholders replaced.
    pub fn fmt(&self, text: Text, args: &[(&str, &str)]) -> String {
        let mut out = String::from(self.get(text));
        for (name, value) in args {
            let mut pattern = String::with_capacity(name.len() + 2);
            pattern.push('{');
            pattern.push_str(name);
            pattern.push('}');
            out = out.replace(&pattern, value);
        }
        out
    }

    /// Every `(key, English default)` pair, for `whether config strings`.
    pub fn keys() -> impl Iterator<Item = (&'static str, &'static str)> {
        TABLE.iter().map(|(_, k, d)| (*k, *d))
    }
}

impl Condition {
    /// The label text for this condition.
    pub fn label(self) -> Text {
        use Intensity::{Heavy, Light, Moderate};
        match self {
            Self::Clear => Text::CondClear,
            Self::MainlyClear => Text::CondMainlyClear,
            Self::PartlyCloudy => Text::CondPartlyCloudy,
            Self::Cloudy => Text::CondCloudy,
            Self::Overcast => Text::CondOvercast,
            Self::Fog => Text::CondFog,
            Self::Haze => Text::CondHaze,
            Self::Smoke => Text::CondSmoke,
            Self::Dust => Text::CondDust,
            Self::Drizzle(Light) => Text::CondLightDrizzle,
            Self::Drizzle(Moderate) => Text::CondDrizzle,
            Self::Drizzle(Heavy) => Text::CondHeavyDrizzle,
            Self::FreezingDrizzle(_) => Text::CondFreezingDrizzle,
            Self::Rain(Light) => Text::CondLightRain,
            Self::Rain(Moderate) => Text::CondRain,
            Self::Rain(Heavy) => Text::CondHeavyRain,
            Self::FreezingRain(_) => Text::CondFreezingRain,
            Self::RainShowers(Light) => Text::CondLightShowers,
            Self::RainShowers(Moderate) => Text::CondShowers,
            Self::RainShowers(Heavy) => Text::CondHeavyShowers,
            Self::Sleet(Light) => Text::CondLightSleet,
            Self::Sleet(Moderate) => Text::CondSleet,
            Self::Sleet(Heavy) => Text::CondHeavySleet,
            Self::SleetShowers(_) => Text::CondSleetShowers,
            Self::Snow(Light) => Text::CondLightSnow,
            Self::Snow(Moderate) => Text::CondSnow,
            Self::Snow(Heavy) => Text::CondHeavySnow,
            Self::SnowShowers(_) => Text::CondSnowShowers,
            Self::SnowGrains => Text::CondSnowGrains,
            Self::Thunderstorm(Heavy) => Text::CondSevereThunderstorm,
            Self::Thunderstorm(_) => Text::CondThunderstorm,
            Self::ThunderstormHail(_) => Text::CondThunderstormHail,
            Self::Squall => Text::CondSquall,
            Self::Tornado => Text::CondTornado,
            Self::Unknown => Text::CondUnknown,
        }
    }
}

/// The short weekday name.
pub fn weekday_text(day: Weekday) -> Text {
    match day {
        Weekday::Mon => Text::WeekdayMon,
        Weekday::Tue => Text::WeekdayTue,
        Weekday::Wed => Text::WeekdayWed,
        Weekday::Thu => Text::WeekdayThu,
        Weekday::Fri => Text::WeekdayFri,
        Weekday::Sat => Text::WeekdaySat,
        Weekday::Sun => Text::WeekdaySun,
    }
}

/// The short month name for month 1–12 (anything else gives December).
pub fn month_text(month: u8) -> Text {
    const MONTHS: [Text; 12] = [
        Text::MonthJan,
        Text::MonthFeb,
        Text::MonthMar,
        Text::MonthApr,
        Text::MonthMay,
        Text::MonthJun,
        Text::MonthJul,
        Text::MonthAug,
        Text::MonthSep,
        Text::MonthOct,
        Text::MonthNov,
        Text::MonthDec,
    ];
    MONTHS[usize::from(month.clamp(1, 12)) - 1]
}

/// The title of a details card.
pub fn detail_title(kind: DetailKind) -> Text {
    match kind {
        DetailKind::Wind => Text::DetailWind,
        DetailKind::Gusts => Text::DetailGusts,
        DetailKind::Humidity => Text::DetailHumidity,
        DetailKind::DewPoint => Text::DetailDewPoint,
        DetailKind::Pressure => Text::DetailPressure,
        DetailKind::Uv => Text::DetailUv,
        DetailKind::Visibility => Text::DetailVisibility,
        DetailKind::CloudCover => Text::DetailCloudCover,
        DetailKind::Precipitation => Text::DetailPrecipitation,
        DetailKind::FeelsLike => Text::DetailFeelsLike,
        DetailKind::Normals => Text::DetailNormals,
        DetailKind::Records => Text::DetailRecords,
    }
}

/// The name of a UV category.
pub fn uv_text(level: UvLevel) -> Text {
    match level {
        UvLevel::Low => Text::UvLow,
        UvLevel::Moderate => Text::UvModerate,
        UvLevel::High => Text::UvHigh,
        UvLevel::VeryHigh => Text::UvVeryHigh,
        UvLevel::Extreme => Text::UvExtreme,
    }
}

/// The name of an air-quality category.
pub fn aqi_text(label: AqiLabel) -> Text {
    match label {
        AqiLabel::Good => Text::AqiGood,
        AqiLabel::Fair => Text::AqiFair,
        AqiLabel::Moderate => Text::AqiModerate,
        AqiLabel::UnhealthyForSensitive => Text::AqiUnhealthySensitive,
        AqiLabel::Unhealthy => Text::AqiUnhealthy,
        AqiLabel::VeryUnhealthy => Text::AqiVeryUnhealthy,
        AqiLabel::Hazardous => Text::AqiHazardous,
        AqiLabel::Poor => Text::AqiPoor,
        AqiLabel::VeryPoor => Text::AqiVeryPoor,
        AqiLabel::ExtremelyPoor => Text::AqiExtremelyPoor,
    }
}

/// An alert's severity in words, the sign beside its color for readers who can't
/// tell the colors apart (WCAG 1.4.1). Alerts carry a [`Level`]; the words are the
/// warning services' own (CAP's minor, moderate, severe, extreme).
pub fn severity_text(level: Level) -> Text {
    match level {
        Level::Good | Level::Fair => Text::SeverityMinor,
        Level::Moderate => Text::SeverityModerate,
        Level::Poor | Level::VeryPoor => Text::SeveritySevere,
        Level::Extreme => Text::SeverityExtreme,
    }
}

/// The radar legend's five steps, light to heavy: the words under a stepped legend.
pub const RADAR_STEPS: [Text; 5] = [
    Text::RadarLight,
    Text::RadarModerate,
    Text::RadarHeavy,
    Text::RadarVeryHeavy,
    Text::RadarExtreme,
];

/// The name of a pressure trend.
pub fn trend_text(trend: Trend) -> Text {
    match trend {
        Trend::Rising => Text::PressureRising,
        Trend::Falling => Text::PressureFalling,
        Trend::Steady => Text::PressureSteady,
    }
}

/// The name of a moon phase, for the moon-phase icons.
pub fn moon_phase_text(icon: UiIcon) -> Option<Text> {
    Some(match icon {
        UiIcon::MoonNew => Text::MoonNew,
        UiIcon::MoonWaxingCrescent => Text::MoonWaxingCrescent,
        UiIcon::MoonFirstQuarter => Text::MoonFirstQuarter,
        UiIcon::MoonWaxingGibbous => Text::MoonWaxingGibbous,
        UiIcon::MoonFull => Text::MoonFull,
        UiIcon::MoonWaningGibbous => Text::MoonWaningGibbous,
        UiIcon::MoonLastQuarter => Text::MoonLastQuarter,
        UiIcon::MoonWaningCrescent => Text::MoonWaningCrescent,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeSet;
    use alloc::string::ToString;

    #[test]
    fn packs_translate_and_overrides_still_win() {
        let mut overrides = BTreeMap::new();
        overrides.insert("today".to_string(), "Heut".to_string());
        let (de, unknown) = Strings::localized("de-DE", &overrides);
        assert_eq!(unknown, Vec::<String>::new());
        assert_eq!(de.get(Text::Now), "Jetzt");
        assert_eq!(de.get(Text::Today), "Heut", "[strings] wins over the pack");
        assert_eq!(de.fmt(Text::FeelsLike, &[("temp", "14°")]), "Gefühlt 14°");
        let (en, _) = Strings::localized("en", &BTreeMap::new());
        assert_eq!(en, Strings::default());
    }

    #[test]
    fn table_rows_line_up_with_variants_and_keys_are_unique() {
        let mut keys = BTreeSet::new();
        for (i, (text, key, default)) in TABLE.iter().enumerate() {
            assert_eq!(*text as usize, i, "{key} is out of order");
            assert!(keys.insert(*key), "duplicate key {key}");
            assert_ne!(*default, "");
            assert!(
                key.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "{key}"
            );
        }
    }

    #[test]
    fn severity_words_follow_the_warning_services() {
        let s = Strings::default();
        let words: Vec<&str> = [
            Level::Good,
            Level::Fair,
            Level::Moderate,
            Level::Poor,
            Level::VeryPoor,
            Level::Extreme,
        ]
        .into_iter()
        .map(|l| s.get(severity_text(l)))
        .collect();
        assert_eq!(
            words,
            ["Minor", "Minor", "Moderate", "Severe", "Severe", "Extreme"]
        );
        let steps: Vec<&str> = RADAR_STEPS.iter().map(|t| s.get(*t)).collect();
        assert_eq!(
            steps,
            ["Light", "Moderate", "Heavy", "Very heavy", "Extreme"]
        );
    }

    #[test]
    fn overrides_and_unknown_keys() {
        let mut map = BTreeMap::new();
        map.insert("today".to_string(), "Heute".to_string());
        map.insert("not_a_key".to_string(), "x".to_string());
        let (s, unknown) = Strings::with_overrides(&map);
        assert_eq!(s.get(Text::Today), "Heute");
        assert_eq!(s.get(Text::Tomorrow), "Tomorrow");
        assert_eq!(unknown, ["not_a_key"]);
    }

    #[test]
    fn placeholders() {
        let s = Strings::default();
        assert_eq!(
            s.fmt(Text::UpdatedMinutesAgo, &[("n", "3")]),
            "Updated 3 min ago"
        );
        assert_eq!(
            s.fmt(
                Text::RainLikelyAt,
                &[("kind", "Rain"), ("time", "18:00"), ("pct", "70")]
            ),
            "Rain likely around 18:00 (70%)."
        );
        assert_eq!(s.fmt(Text::Today, &[("n", "3")]), "Today");
    }

    #[test]
    fn every_condition_has_a_label() {
        let s = Strings::default();
        assert_eq!(
            s.get(Condition::Rain(Intensity::Light).label()),
            "Light rain"
        );
        assert_eq!(
            s.get(Condition::Thunderstorm(Intensity::Heavy).label()),
            "Severe thunderstorm"
        );
        assert_eq!(s.get(month_text(10)), "Oct");
        assert_eq!(s.get(weekday_text(Weekday::Mon)), "Mon");
        assert_eq!(moon_phase_text(UiIcon::Sunrise), None);
    }

    #[test]
    fn keys_list_every_text() {
        assert_eq!(Strings::keys().count(), TABLE.len());
        assert_eq!(Text::from_key("feels_like"), Some(Text::FeelsLike));
        assert_eq!(Text::FeelsLike.key(), "feels_like");
    }
}
