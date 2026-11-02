//! Pirate Weather's Dark Sky-style icon names as normalized conditions.

use whether_core::condition::{Condition, Intensity};

/// Liquid-equivalent rates (mm/h) where rain turns moderate and heavy.
const RAIN_MODERATE: f32 = 2.5;
const RAIN_HEAVY: f32 = 7.6;
/// The same for snow and mixed precipitation, which pile up more slowly per
/// millimetre of liquid.
const SNOW_MODERATE: f32 = 0.5;
const SNOW_HEAVY: f32 = 1.5;
/// Below this rate (mm/h) rain is drizzle.
const DRIZZLE_BELOW: f32 = 0.3;
/// Cloud cover (a fraction) from which a clear sky is only mainly clear.
const MAINLY_CLEAR_FROM: f32 = 0.1;
/// Cloud cover from which "cloudy" is overcast.
const OVERCAST_FROM: f32 = 0.9;

fn intensity(rate: Option<f32>, moderate: f32, heavy: f32) -> Intensity {
    match rate {
        Some(r) if r >= heavy => Intensity::Heavy,
        Some(r) if r >= moderate => Intensity::Moderate,
        _ => Intensity::Light,
    }
}

/// The sky when nothing falls: the icon first, then the cloud cover, which also
/// serves icons that say nothing about it (`wind`) or that are new.
fn sky(icon: &str, cloud_cover: Option<f32>) -> Condition {
    let cover = cloud_cover.filter(|c| c.is_finite());
    match icon {
        "clear-day" | "clear-night" => match cover {
            Some(c) if c >= MAINLY_CLEAR_FROM => Condition::MainlyClear,
            _ => Condition::Clear,
        },
        "partly-cloudy-day" | "partly-cloudy-night" => Condition::PartlyCloudy,
        "cloudy" => match cover {
            Some(c) if c >= OVERCAST_FROM => Condition::Overcast,
            _ => Condition::Cloudy,
        },
        _ => match cover {
            Some(c) if c < 0.1 => Condition::Clear,
            Some(c) if c < 0.3 => Condition::MainlyClear,
            Some(c) if c < 0.6 => Condition::PartlyCloudy,
            Some(c) if c < OVERCAST_FROM => Condition::Cloudy,
            Some(_) => Condition::Overcast,
            None => Condition::Unknown,
        },
    }
}

/// The condition for a Pirate Weather `icon`. `rate` is the liquid-equivalent
/// precipitation intensity in mm/h (a day uses its maximum) and `cloud_cover` a
/// fraction. The icon names only the kind of weather, so the rate gives the strength.
/// `hail` becomes a hailstorm: hail forms in thunderstorms and the model has no
/// plain hail.
pub(super) fn condition(
    icon: Option<&str>,
    rate: Option<f32>,
    cloud_cover: Option<f32>,
) -> Condition {
    let Some(icon) = icon else {
        return sky("", cloud_cover);
    };
    match icon {
        "rain" => match rate {
            Some(r) if r < DRIZZLE_BELOW => Condition::Drizzle(Intensity::Light),
            _ => Condition::Rain(intensity(rate, RAIN_MODERATE, RAIN_HEAVY)),
        },
        "snow" => Condition::Snow(intensity(rate, SNOW_MODERATE, SNOW_HEAVY)),
        "sleet" => Condition::Sleet(intensity(rate, SNOW_MODERATE, SNOW_HEAVY)),
        "thunderstorm" => Condition::Thunderstorm(intensity(rate, RAIN_MODERATE, RAIN_HEAVY)),
        "hail" => Condition::ThunderstormHail(intensity(rate, RAIN_MODERATE, RAIN_HEAVY)),
        "fog" => Condition::Fog,
        "tornado" => Condition::Tornado,
        other => sky(other, cloud_cover),
    }
}

/// Whether an icon says it is night (`clear-night`, `partly-cloudy-night`), day
/// (`…-day`) or nothing.
pub(super) fn icon_is_day(icon: Option<&str>) -> Option<bool> {
    let icon = icon?;
    if icon.ends_with("-night") {
        Some(false)
    } else if icon.ends_with("-day") {
        Some(true)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Intensity::{Heavy, Light, Moderate};

    #[test]
    fn the_sky_follows_the_icon_and_the_cloud_cover() {
        assert_eq!(
            condition(Some("clear-day"), None, Some(0.0)),
            Condition::Clear
        );
        assert_eq!(
            condition(Some("clear-night"), None, Some(0.2)),
            Condition::MainlyClear
        );
        assert_eq!(condition(Some("clear-day"), None, None), Condition::Clear);
        assert_eq!(
            condition(Some("partly-cloudy-day"), None, Some(0.5)),
            Condition::PartlyCloudy
        );
        assert_eq!(
            condition(Some("cloudy"), None, Some(0.7)),
            Condition::Cloudy
        );
        assert_eq!(
            condition(Some("cloudy"), None, Some(0.95)),
            Condition::Overcast
        );
        assert_eq!(condition(Some("cloudy"), None, None), Condition::Cloudy);
        // `wind` says nothing about the sky.
        assert_eq!(condition(Some("wind"), None, Some(0.05)), Condition::Clear);
        assert_eq!(
            condition(Some("wind"), None, Some(0.2)),
            Condition::MainlyClear
        );
        assert_eq!(
            condition(Some("wind"), None, Some(0.5)),
            Condition::PartlyCloudy
        );
        assert_eq!(condition(Some("wind"), None, Some(0.8)), Condition::Cloudy);
        assert_eq!(
            condition(Some("wind"), None, Some(0.99)),
            Condition::Overcast
        );
        assert_eq!(condition(Some("wind"), None, None), Condition::Unknown);
        assert_eq!(condition(None, None, Some(0.05)), Condition::Clear);
        assert_eq!(
            condition(Some("?"), None, Some(f32::NAN)),
            Condition::Unknown
        );
    }

    #[test]
    fn precipitation_takes_its_strength_from_the_rate() {
        assert_eq!(
            condition(Some("rain"), Some(0.1), None),
            Condition::Drizzle(Light)
        );
        assert_eq!(
            condition(Some("rain"), Some(1.0), None),
            Condition::Rain(Light)
        );
        assert_eq!(
            condition(Some("rain"), Some(2.5), None),
            Condition::Rain(Moderate)
        );
        assert_eq!(
            condition(Some("rain"), Some(9.0), None),
            Condition::Rain(Heavy)
        );
        assert_eq!(condition(Some("rain"), None, None), Condition::Rain(Light));
        assert_eq!(
            condition(Some("snow"), Some(0.2), None),
            Condition::Snow(Light)
        );
        assert_eq!(
            condition(Some("snow"), Some(0.8), None),
            Condition::Snow(Moderate)
        );
        assert_eq!(
            condition(Some("snow"), Some(2.0), None),
            Condition::Snow(Heavy)
        );
        assert_eq!(
            condition(Some("sleet"), Some(0.8), None),
            Condition::Sleet(Moderate)
        );
        assert_eq!(
            condition(Some("thunderstorm"), Some(8.0), None),
            Condition::Thunderstorm(Heavy)
        );
        assert_eq!(
            condition(Some("hail"), Some(1.0), None),
            Condition::ThunderstormHail(Light)
        );
        assert_eq!(condition(Some("fog"), None, None), Condition::Fog);
        assert_eq!(condition(Some("tornado"), None, None), Condition::Tornado);
    }

    #[test]
    fn the_icon_suffix_names_day_or_night() {
        assert_eq!(icon_is_day(Some("clear-night")), Some(false));
        assert_eq!(icon_is_day(Some("partly-cloudy-day")), Some(true));
        assert_eq!(icon_is_day(Some("rain")), None);
        assert_eq!(icon_is_day(None), None);
    }
}
