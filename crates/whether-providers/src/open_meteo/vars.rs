//! Open-Meteo variable names for each data-requirement field.

use alloc::string::String;
use alloc::vec::Vec;

use whether_core::requirements::{CurrentFields, DailyFields, HourlyFields};

const CURRENT: &[(CurrentFields, &str)] = &[
    (CurrentFields::TEMPERATURE, "temperature_2m"),
    (CurrentFields::HUMIDITY, "relative_humidity_2m"),
    (CurrentFields::DEW_POINT, "dew_point_2m"),
    (CurrentFields::APPARENT, "apparent_temperature"),
    (CurrentFields::IS_DAY, "is_day"),
    (CurrentFields::PRECIPITATION, "precipitation"),
    (CurrentFields::WEATHER_CODE, "weather_code"),
    (CurrentFields::CLOUD_COVER, "cloud_cover"),
    (CurrentFields::PRESSURE, "pressure_msl"),
    (CurrentFields::WIND_SPEED, "wind_speed_10m"),
    (CurrentFields::WIND_DIRECTION, "wind_direction_10m"),
    (CurrentFields::WIND_GUSTS, "wind_gusts_10m"),
    (CurrentFields::VISIBILITY, "visibility"),
    (CurrentFields::UV_INDEX, "uv_index"),
];

const HOURLY: &[(HourlyFields, &str)] = &[
    (HourlyFields::TEMPERATURE, "temperature_2m"),
    (HourlyFields::HUMIDITY, "relative_humidity_2m"),
    (HourlyFields::APPARENT, "apparent_temperature"),
    (
        HourlyFields::PRECIP_PROBABILITY,
        "precipitation_probability",
    ),
    (HourlyFields::PRECIPITATION, "precipitation"),
    (HourlyFields::WEATHER_CODE, "weather_code"),
    (HourlyFields::IS_DAY, "is_day"),
    (HourlyFields::CLOUD_COVER, "cloud_cover"),
    (HourlyFields::PRESSURE, "pressure_msl"),
    (HourlyFields::WIND_SPEED, "wind_speed_10m"),
    (HourlyFields::WIND_DIRECTION, "wind_direction_10m"),
    (HourlyFields::WIND_GUSTS, "wind_gusts_10m"),
    (HourlyFields::VISIBILITY, "visibility"),
    (HourlyFields::UV_INDEX, "uv_index"),
];

const DAILY: &[(DailyFields, &str)] = &[
    (DailyFields::WEATHER_CODE, "weather_code"),
    (DailyFields::TEMP_MAX, "temperature_2m_max"),
    (DailyFields::TEMP_MIN, "temperature_2m_min"),
    (DailyFields::APPARENT_MAX, "apparent_temperature_max"),
    (DailyFields::APPARENT_MIN, "apparent_temperature_min"),
    (DailyFields::PRECIP_SUM, "precipitation_sum"),
    (
        DailyFields::PRECIP_PROBABILITY_MAX,
        "precipitation_probability_max",
    ),
    (DailyFields::SUNRISE, "sunrise"),
    (DailyFields::SUNSET, "sunset"),
    (DailyFields::DAYLIGHT, "daylight_duration"),
    (DailyFields::UV_INDEX_MAX, "uv_index_max"),
    (DailyFields::WIND_SPEED_MAX, "wind_speed_10m_max"),
    (DailyFields::WIND_GUSTS_MAX, "wind_gusts_10m_max"),
    (
        DailyFields::WIND_DIRECTION_DOMINANT,
        "wind_direction_10m_dominant",
    ),
    (DailyFields::MOON_PHASE, "moon_phase"),
    (DailyFields::MOONRISE, "moonrise"),
    (DailyFields::MOONSET, "moonset"),
];

fn join<F: bitflags::Flags + Copy>(table: &[(F, &str)], wanted: F) -> String {
    let names: Vec<&str> = table
        .iter()
        .filter(|(flag, _)| wanted.contains(*flag))
        .map(|(_, name)| *name)
        .collect();
    names.join(",")
}

/// Comma-separated `current` variables.
pub(crate) fn current(fields: CurrentFields) -> String {
    join(CURRENT, fields)
}

/// Comma-separated `hourly` variables.
pub(crate) fn hourly(fields: HourlyFields) -> String {
    join(HOURLY, fields)
}

/// Comma-separated `daily` variables.
pub(crate) fn daily(fields: DailyFields) -> String {
    join(DAILY, fields)
}

/// Air-quality variables requested for the current hour.
pub(crate) const AIR_QUALITY: &str =
    "us_aqi,european_aqi,pm10,pm2_5,carbon_monoxide,nitrogen_dioxide,sulphur_dioxide,ozone";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_flag_has_a_variable() {
        assert_eq!(CURRENT.len(), CurrentFields::all().iter().count());
        assert_eq!(HOURLY.len(), HourlyFields::all().iter().count());
        assert_eq!(DAILY.len(), DailyFields::all().iter().count());
    }

    #[test]
    fn joins_in_table_order() {
        assert_eq!(
            current(CurrentFields::IS_DAY | CurrentFields::TEMPERATURE),
            "temperature_2m,is_day"
        );
        assert_eq!(daily(DailyFields::empty()), "");
    }
}
