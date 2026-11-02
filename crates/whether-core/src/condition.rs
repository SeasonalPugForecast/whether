//! The normalized weather condition that every provider maps onto.

/// Strength of precipitation or a storm.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "snake_case")
)]
#[allow(missing_docs)]
pub enum Intensity {
    Light,
    Moderate,
    Heavy,
}

/// Sky and precipitation state, normalized across WMO codes, OpenWeather ids and
/// MET Norway symbol codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "snake_case")
)]
#[allow(missing_docs)]
pub enum Condition {
    Clear,
    MainlyClear,
    PartlyCloudy,
    Cloudy,
    Overcast,
    Fog,
    Haze,
    Smoke,
    Dust,
    Drizzle(Intensity),
    FreezingDrizzle(Intensity),
    Rain(Intensity),
    FreezingRain(Intensity),
    RainShowers(Intensity),
    Sleet(Intensity),
    SleetShowers(Intensity),
    Snow(Intensity),
    SnowShowers(Intensity),
    SnowGrains,
    Thunderstorm(Intensity),
    ThunderstormHail(Intensity),
    Squall,
    Tornado,
    #[default]
    Unknown,
}

impl Condition {
    /// Maps a WMO weather interpretation code (as used by Open-Meteo).
    pub fn from_wmo(code: u16) -> Self {
        use Intensity::{Heavy, Light, Moderate};
        match code {
            0 => Self::Clear,
            1 => Self::MainlyClear,
            2 => Self::PartlyCloudy,
            3 => Self::Overcast,
            45 | 48 => Self::Fog,
            51 => Self::Drizzle(Light),
            53 => Self::Drizzle(Moderate),
            55 => Self::Drizzle(Heavy),
            56 => Self::FreezingDrizzle(Light),
            57 => Self::FreezingDrizzle(Heavy),
            61 => Self::Rain(Light),
            63 => Self::Rain(Moderate),
            65 => Self::Rain(Heavy),
            66 => Self::FreezingRain(Light),
            67 => Self::FreezingRain(Heavy),
            71 => Self::Snow(Light),
            73 => Self::Snow(Moderate),
            75 => Self::Snow(Heavy),
            77 => Self::SnowGrains,
            80 => Self::RainShowers(Light),
            81 => Self::RainShowers(Moderate),
            82 => Self::RainShowers(Heavy),
            85 => Self::SnowShowers(Light),
            86 => Self::SnowShowers(Heavy),
            95 => Self::Thunderstorm(Moderate),
            96 => Self::ThunderstormHail(Light),
            99 => Self::ThunderstormHail(Heavy),
            _ => Self::Unknown,
        }
    }

    /// The intensity carried by precipitation and storm variants.
    pub fn intensity(self) -> Option<Intensity> {
        match self {
            Self::Drizzle(i)
            | Self::FreezingDrizzle(i)
            | Self::Rain(i)
            | Self::FreezingRain(i)
            | Self::RainShowers(i)
            | Self::Sleet(i)
            | Self::SleetShowers(i)
            | Self::Snow(i)
            | Self::SnowShowers(i)
            | Self::Thunderstorm(i)
            | Self::ThunderstormHail(i) => Some(i),
            _ => None,
        }
    }

    /// How significant the condition is; used to pick a day's dominant condition.
    /// Higher is more significant.
    pub fn severity_rank(self) -> u8 {
        let step = |base: u8| base + self.intensity().map_or(0, |i| i as u8);
        match self {
            Self::Unknown | Self::Clear => 0,
            Self::MainlyClear => 1,
            Self::PartlyCloudy => 2,
            Self::Cloudy => 3,
            Self::Overcast => 4,
            Self::Haze => 5,
            Self::Smoke => 6,
            Self::Dust => 7,
            Self::Fog => 8,
            Self::Drizzle(_) => step(10),
            Self::RainShowers(_) => step(13),
            Self::Rain(_) => step(16),
            Self::FreezingDrizzle(_) => step(19),
            Self::FreezingRain(_) => step(22),
            Self::SleetShowers(_) => step(25),
            Self::Sleet(_) => step(28),
            Self::SnowGrains => 31,
            Self::SnowShowers(_) => step(32),
            Self::Snow(_) => step(35),
            Self::Thunderstorm(_) => step(40),
            Self::ThunderstormHail(_) => step(43),
            Self::Squall => 46,
            Self::Tornado => 50,
        }
    }

    /// Whether the condition involves falling precipitation.
    pub fn is_precipitation(self) -> bool {
        self.intensity().is_some() || self == Self::SnowGrains
    }

    /// Whether the precipitation is frozen (snow, sleet, freezing rain or snow grains).
    pub fn is_frozen(self) -> bool {
        matches!(
            self,
            Self::Snow(_)
                | Self::SnowShowers(_)
                | Self::SnowGrains
                | Self::Sleet(_)
                | Self::SleetShowers(_)
                | Self::FreezingDrizzle(_)
                | Self::FreezingRain(_)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::Intensity::{Heavy, Light, Moderate};
    use super::*;

    #[test]
    fn every_documented_wmo_code() {
        let table = [
            (0, Condition::Clear),
            (1, Condition::MainlyClear),
            (2, Condition::PartlyCloudy),
            (3, Condition::Overcast),
            (45, Condition::Fog),
            (48, Condition::Fog),
            (51, Condition::Drizzle(Light)),
            (53, Condition::Drizzle(Moderate)),
            (55, Condition::Drizzle(Heavy)),
            (56, Condition::FreezingDrizzle(Light)),
            (57, Condition::FreezingDrizzle(Heavy)),
            (61, Condition::Rain(Light)),
            (63, Condition::Rain(Moderate)),
            (65, Condition::Rain(Heavy)),
            (66, Condition::FreezingRain(Light)),
            (67, Condition::FreezingRain(Heavy)),
            (71, Condition::Snow(Light)),
            (73, Condition::Snow(Moderate)),
            (75, Condition::Snow(Heavy)),
            (77, Condition::SnowGrains),
            (80, Condition::RainShowers(Light)),
            (81, Condition::RainShowers(Moderate)),
            (82, Condition::RainShowers(Heavy)),
            (85, Condition::SnowShowers(Light)),
            (86, Condition::SnowShowers(Heavy)),
            (95, Condition::Thunderstorm(Moderate)),
            (96, Condition::ThunderstormHail(Light)),
            (99, Condition::ThunderstormHail(Heavy)),
        ];
        for (code, expected) in table {
            assert_eq!(Condition::from_wmo(code), expected, "WMO {code}");
        }
    }

    #[test]
    fn unknown_wmo_codes() {
        for code in [4, 100, 1000] {
            assert_eq!(Condition::from_wmo(code), Condition::Unknown);
        }
    }

    #[test]
    fn severity_orders_storms_above_rain_above_clouds() {
        assert!(
            Condition::Tornado.severity_rank() > Condition::Thunderstorm(Heavy).severity_rank()
        );
        assert!(
            Condition::Thunderstorm(Light).severity_rank() > Condition::Snow(Heavy).severity_rank()
        );
        assert!(Condition::Rain(Heavy).severity_rank() > Condition::Rain(Light).severity_rank());
        assert!(Condition::Rain(Light).severity_rank() > Condition::Overcast.severity_rank());
        assert!(Condition::Overcast.severity_rank() > Condition::Clear.severity_rank());
    }

    #[test]
    fn precipitation_classification() {
        assert!(Condition::Drizzle(Light).is_precipitation());
        assert!(Condition::SnowGrains.is_precipitation());
        assert!(!Condition::Fog.is_precipitation());
        assert!(Condition::FreezingRain(Light).is_frozen());
        assert!(!Condition::Rain(Heavy).is_frozen());
    }
}
