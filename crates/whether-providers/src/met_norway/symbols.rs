//! MET Norway symbol codes (`next_1_hours.summary.symbol_code`), e.g.
//! `lightrainshowers_day`, onto [`Condition`].

use whether_core::condition::{Condition, Intensity};

/// The condition for `code`, and whether the code says day (`_day`,
/// `_polartwilight`) or night (`_night`); `None` when it has no suffix.
pub(crate) fn condition(code: &str) -> (Condition, Option<bool>) {
    let (base, is_day) = if let Some(b) = code.strip_suffix("_day") {
        (b, Some(true))
    } else if let Some(b) = code.strip_suffix("_night") {
        (b, Some(false))
    } else if let Some(b) = code.strip_suffix("_polartwilight") {
        (b, Some(true))
    } else {
        (code, None)
    };
    (base_condition(base), is_day)
}

fn base_condition(base: &str) -> Condition {
    // MET's two documented misspellings.
    let base = match base {
        "lightssleetshowersandthunder" => "lightsleetshowersandthunder",
        "lightssnowshowersandthunder" => "lightsnowshowersandthunder",
        other => other,
    };
    match base {
        "clearsky" => return Condition::Clear,
        "fair" => return Condition::MainlyClear,
        "partlycloudy" => return Condition::PartlyCloudy,
        "cloudy" => return Condition::Cloudy,
        "fog" => return Condition::Fog,
        _ => {}
    }
    let (intensity, rest) = if let Some(r) = base.strip_prefix("light") {
        (Intensity::Light, r)
    } else if let Some(r) = base.strip_prefix("heavy") {
        (Intensity::Heavy, r)
    } else {
        (Intensity::Moderate, base)
    };
    precipitation(intensity, rest).unwrap_or(Condition::Unknown)
}

fn precipitation(i: Intensity, rest: &str) -> Option<Condition> {
    if let Some(kind) = rest.strip_suffix("andthunder") {
        return matches!(
            kind,
            "rain" | "rainshowers" | "sleet" | "sleetshowers" | "snow" | "snowshowers"
        )
        .then_some(Condition::Thunderstorm(i));
    }
    Some(match rest {
        "rain" => Condition::Rain(i),
        "rainshowers" => Condition::RainShowers(i),
        "sleet" => Condition::Sleet(i),
        "sleetshowers" => Condition::SleetShowers(i),
        "snow" => Condition::Snow(i),
        "snowshowers" => Condition::SnowShowers(i),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use Intensity::{Heavy, Light, Moderate};

    /// Every code in MET's symbol legend (weatherapi/weathericon/2.0), with the
    /// historical spellings.
    #[test]
    fn every_documented_code() {
        let table: &[(&str, Condition)] = &[
            ("clearsky", Condition::Clear),
            ("fair", Condition::MainlyClear),
            ("partlycloudy", Condition::PartlyCloudy),
            ("cloudy", Condition::Cloudy),
            ("fog", Condition::Fog),
            ("lightrain", Condition::Rain(Light)),
            ("rain", Condition::Rain(Moderate)),
            ("heavyrain", Condition::Rain(Heavy)),
            ("lightrainshowers", Condition::RainShowers(Light)),
            ("rainshowers", Condition::RainShowers(Moderate)),
            ("heavyrainshowers", Condition::RainShowers(Heavy)),
            ("lightsleet", Condition::Sleet(Light)),
            ("sleet", Condition::Sleet(Moderate)),
            ("heavysleet", Condition::Sleet(Heavy)),
            ("lightsleetshowers", Condition::SleetShowers(Light)),
            ("sleetshowers", Condition::SleetShowers(Moderate)),
            ("heavysleetshowers", Condition::SleetShowers(Heavy)),
            ("lightsnow", Condition::Snow(Light)),
            ("snow", Condition::Snow(Moderate)),
            ("heavysnow", Condition::Snow(Heavy)),
            ("lightsnowshowers", Condition::SnowShowers(Light)),
            ("snowshowers", Condition::SnowShowers(Moderate)),
            ("heavysnowshowers", Condition::SnowShowers(Heavy)),
            ("lightrainandthunder", Condition::Thunderstorm(Light)),
            ("rainandthunder", Condition::Thunderstorm(Moderate)),
            ("heavyrainandthunder", Condition::Thunderstorm(Heavy)),
            ("lightrainshowersandthunder", Condition::Thunderstorm(Light)),
            ("rainshowersandthunder", Condition::Thunderstorm(Moderate)),
            ("heavyrainshowersandthunder", Condition::Thunderstorm(Heavy)),
            ("lightsleetandthunder", Condition::Thunderstorm(Light)),
            ("sleetandthunder", Condition::Thunderstorm(Moderate)),
            ("heavysleetandthunder", Condition::Thunderstorm(Heavy)),
            (
                "lightssleetshowersandthunder",
                Condition::Thunderstorm(Light),
            ),
            ("sleetshowersandthunder", Condition::Thunderstorm(Moderate)),
            (
                "heavysleetshowersandthunder",
                Condition::Thunderstorm(Heavy),
            ),
            ("lightsnowandthunder", Condition::Thunderstorm(Light)),
            ("snowandthunder", Condition::Thunderstorm(Moderate)),
            ("heavysnowandthunder", Condition::Thunderstorm(Heavy)),
            (
                "lightssnowshowersandthunder",
                Condition::Thunderstorm(Light),
            ),
            ("snowshowersandthunder", Condition::Thunderstorm(Moderate)),
            ("heavysnowshowersandthunder", Condition::Thunderstorm(Heavy)),
        ];
        for (code, expected) in table {
            assert_eq!(condition(code).0, *expected, "{code}");
        }
    }

    #[test]
    fn suffixes_tell_day_and_night() {
        assert_eq!(condition("clearsky_day"), (Condition::Clear, Some(true)));
        assert_eq!(
            condition("fair_night"),
            (Condition::MainlyClear, Some(false))
        );
        assert_eq!(
            condition("lightsnowshowers_polartwilight"),
            (Condition::SnowShowers(Light), Some(true))
        );
        assert_eq!(condition("cloudy"), (Condition::Cloudy, None));
    }

    #[test]
    fn unknown_codes() {
        for code in [
            "",
            "sunny",
            "heavyfog",
            "lightsnowandhail",
            "rainandthunderstorm",
        ] {
            assert_eq!(condition(code).0, Condition::Unknown, "{code}");
        }
    }
}
