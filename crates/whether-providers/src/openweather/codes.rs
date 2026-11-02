//! OpenWeather condition ids (<https://openweathermap.org/weather-conditions>).

use whether_core::condition::{Condition, Intensity};

/// Maps an OpenWeather condition id to a [`Condition`].
pub fn from_owm_id(id: u16) -> Condition {
    use Intensity::{Heavy, Light, Moderate};
    match id {
        200 | 210 | 230 => Condition::Thunderstorm(Light),
        201 | 211 | 221 | 231 => Condition::Thunderstorm(Moderate),
        202 | 212 | 232 => Condition::Thunderstorm(Heavy),
        300 | 310 => Condition::Drizzle(Light),
        301 | 311 | 313 | 321 => Condition::Drizzle(Moderate),
        302 | 312 | 314 => Condition::Drizzle(Heavy),
        500 => Condition::Rain(Light),
        501 => Condition::Rain(Moderate),
        502..=504 => Condition::Rain(Heavy),
        511 => Condition::FreezingRain(Moderate),
        520 => Condition::RainShowers(Light),
        521 | 531 => Condition::RainShowers(Moderate),
        522 => Condition::RainShowers(Heavy),
        600 => Condition::Snow(Light),
        601 => Condition::Snow(Moderate),
        602 => Condition::Snow(Heavy),
        611 | 616 => Condition::Sleet(Moderate),
        612 => Condition::SleetShowers(Light),
        613 => Condition::SleetShowers(Moderate),
        615 => Condition::Sleet(Light),
        620 => Condition::SnowShowers(Light),
        621 => Condition::SnowShowers(Moderate),
        622 => Condition::SnowShowers(Heavy),
        701 | 741 => Condition::Fog,
        711 => Condition::Smoke,
        721 => Condition::Haze,
        731 | 751 | 761 | 762 => Condition::Dust,
        771 => Condition::Squall,
        781 => Condition::Tornado,
        800 => Condition::Clear,
        801 => Condition::MainlyClear,
        802 => Condition::PartlyCloudy,
        803 => Condition::Cloudy,
        804 => Condition::Overcast,
        _ => Condition::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Intensity::{Heavy, Light, Moderate};

    #[test]
    fn every_documented_id() {
        let table: &[(&[u16], Condition)] = &[
            (&[200, 210, 230], Condition::Thunderstorm(Light)),
            (&[201, 211, 221, 231], Condition::Thunderstorm(Moderate)),
            (&[202, 212, 232], Condition::Thunderstorm(Heavy)),
            (&[300, 310], Condition::Drizzle(Light)),
            (&[301, 311, 313, 321], Condition::Drizzle(Moderate)),
            (&[302, 312, 314], Condition::Drizzle(Heavy)),
            (&[500], Condition::Rain(Light)),
            (&[501], Condition::Rain(Moderate)),
            (&[502, 503, 504], Condition::Rain(Heavy)),
            (&[511], Condition::FreezingRain(Moderate)),
            (&[520], Condition::RainShowers(Light)),
            (&[521, 531], Condition::RainShowers(Moderate)),
            (&[522], Condition::RainShowers(Heavy)),
            (&[600], Condition::Snow(Light)),
            (&[601], Condition::Snow(Moderate)),
            (&[602], Condition::Snow(Heavy)),
            (&[611], Condition::Sleet(Moderate)),
            (&[612], Condition::SleetShowers(Light)),
            (&[613], Condition::SleetShowers(Moderate)),
            (&[615], Condition::Sleet(Light)),
            (&[616], Condition::Sleet(Moderate)),
            (&[620], Condition::SnowShowers(Light)),
            (&[621], Condition::SnowShowers(Moderate)),
            (&[622], Condition::SnowShowers(Heavy)),
            (&[701, 741], Condition::Fog),
            (&[711], Condition::Smoke),
            (&[721], Condition::Haze),
            (&[731, 751, 761, 762], Condition::Dust),
            (&[771], Condition::Squall),
            (&[781], Condition::Tornado),
            (&[800], Condition::Clear),
            (&[801], Condition::MainlyClear),
            (&[802], Condition::PartlyCloudy),
            (&[803], Condition::Cloudy),
            (&[804], Condition::Overcast),
        ];
        for (ids, expected) in table {
            for &id in *ids {
                assert_eq!(from_owm_id(id), *expected, "id {id}");
            }
        }
    }

    #[test]
    fn unknown_ids() {
        for id in [0, 199, 400, 805, 999] {
            assert_eq!(from_owm_id(id), Condition::Unknown);
        }
    }
}
