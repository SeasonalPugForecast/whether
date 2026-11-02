//! The NWS icon codes (`/icons/land/day/rain_showers,40`) and the twice-daily
//! `/forecast` periods whose daytime icons describe each day.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use serde::Deserialize;
use whether_core::builder::SnapshotBuilder;
use whether_core::condition::{Condition, Intensity};
use whether_core::time::{UtcOffset, parse_rfc3339};

use crate::error::ProviderError;

/// The condition an icon code shows. Codes from api.weather.gov/icons; the
/// `wind_` variants are windy skies.
pub(super) fn code(code: &str) -> Option<Condition> {
    use Intensity::{Heavy, Light, Moderate};
    Some(match code.strip_prefix("wind_").unwrap_or(code) {
        "skc" | "hot" | "cold" => Condition::Clear,
        "few" => Condition::MainlyClear,
        "sct" => Condition::PartlyCloudy,
        "bkn" => Condition::Cloudy,
        "ovc" => Condition::Overcast,
        "snow" => Condition::Snow(Moderate),
        "blizzard" => Condition::Snow(Heavy),
        "rain_snow" | "rain_sleet" | "snow_sleet" | "sleet" => Condition::Sleet(Moderate),
        "fzra" | "rain_fzra" | "snow_fzra" => Condition::FreezingRain(Moderate),
        "rain" => Condition::Rain(Moderate),
        "rain_showers" => Condition::RainShowers(Moderate),
        "rain_showers_hi" => Condition::RainShowers(Light),
        "tsra" | "tsra_sct" => Condition::Thunderstorm(Moderate),
        "tsra_hi" => Condition::Thunderstorm(Light),
        "tornado" => Condition::Tornado,
        "hurricane" | "tropical_storm" => Condition::Squall,
        "dust" => Condition::Dust,
        "smoke" => Condition::Smoke,
        "haze" => Condition::Haze,
        "fog" => Condition::Fog,
        _ => return None,
    })
}

/// The condition and day flag of an icon URL; a split icon
/// (`…/day/sct/rain,30`) gives its more severe half.
pub(super) fn icon(url: &str) -> Option<(Condition, bool)> {
    let path = url.split('?').next()?;
    let mut parts = path.split("/icons/").nth(1)?.split('/');
    let _set = parts.next()?;
    let is_day = match parts.next()? {
        "day" => true,
        "night" => false,
        _ => return None,
    };
    parts
        .filter_map(|part| code(part.split(',').next()?))
        .max_by_key(|c| c.severity_rank())
        .map(|c| (c, is_day))
}

#[derive(Deserialize)]
struct Forecast {
    properties: Periods,
}

#[derive(Deserialize)]
struct Periods {
    periods: Vec<Period>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Period {
    start_time: String,
    is_daytime: bool,
    icon: Option<String>,
}

/// The offset an RFC 3339 time is written in (`…-04:00` → −4 h).
fn written_offset(time: &str) -> Option<UtcOffset> {
    if time.ends_with('Z') {
        return Some(UtcOffset(0));
    }
    let zone = time.get(time.len().checked_sub(6)?..)?;
    let sign = match zone.as_bytes().first()? {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let hours: i32 = zone.get(1..3)?.parse().ok()?;
    let minutes: i32 = zone.get(4..6)?.parse().ok()?;
    Some(UtcOffset(sign * (hours * 3600 + minutes * 60)))
}

/// Sets each day's condition from its daytime period's icon in a `/forecast`
/// response. Days the gridpoint did not cover are left alone.
pub(super) fn parse_periods(body: &[u8], out: &mut SnapshotBuilder) -> Result<(), ProviderError> {
    let forecast: Forecast =
        serde_json::from_slice(body).map_err(|e| ProviderError::Malformed(e.to_string()))?;
    let days: Vec<_> = forecast
        .properties
        .periods
        .iter()
        .filter(|p| p.is_daytime)
        .filter_map(|p| {
            let start = parse_rfc3339(&p.start_time)?;
            let date = start.local_date(written_offset(&p.start_time)?);
            Some((date, icon(p.icon.as_deref()?)?.0))
        })
        .collect();
    for day in out.daily_mut() {
        if let Some((_, condition)) = days.iter().find(|(date, _)| *date == day.date) {
            day.condition = *condition;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every code on api.weather.gov/icons (2026-08-04).
    const CODES: [&str; 34] = [
        "skc",
        "few",
        "sct",
        "bkn",
        "ovc",
        "wind_skc",
        "wind_few",
        "wind_sct",
        "wind_bkn",
        "wind_ovc",
        "snow",
        "rain_snow",
        "rain_sleet",
        "snow_sleet",
        "fzra",
        "rain_fzra",
        "snow_fzra",
        "sleet",
        "rain",
        "rain_showers",
        "rain_showers_hi",
        "tsra",
        "tsra_sct",
        "tsra_hi",
        "tornado",
        "hurricane",
        "tropical_storm",
        "dust",
        "smoke",
        "haze",
        "hot",
        "cold",
        "blizzard",
        "fog",
    ];

    #[test]
    fn every_icon_code_has_a_condition() {
        for c in CODES {
            assert!(code(c).is_some(), "{c}");
        }
        assert_eq!(code("bkn"), Some(Condition::Cloudy));
        assert_eq!(code("wind_bkn"), Some(Condition::Cloudy));
        assert_eq!(code("purple"), None);
    }

    #[test]
    fn icon_urls_give_the_worse_half_and_the_time_of_day() {
        assert_eq!(
            icon("https://api.weather.gov/icons/land/day/rain_showers,40?size=medium"),
            Some((Condition::RainShowers(Intensity::Moderate), true))
        );
        assert_eq!(
            icon("https://api.weather.gov/icons/land/night/sct/tsra,60?size=medium"),
            Some((Condition::Thunderstorm(Intensity::Moderate), false))
        );
        assert_eq!(icon("https://example.com/x.png"), None);
    }

    #[test]
    fn written_offsets() {
        assert_eq!(
            written_offset("2026-10-17T09:00:00-04:00"),
            Some(UtcOffset(-14_400))
        );
        assert_eq!(
            written_offset("2026-10-17T09:00:00+05:30"),
            Some(UtcOffset(19_800))
        );
        assert_eq!(written_offset("2026-10-17T09:00:00Z"), Some(UtcOffset(0)));
        assert_eq!(written_offset("09:00"), None);
    }
}
