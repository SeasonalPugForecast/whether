//! Icon identifiers. Each display maps them to its own artwork (SVG, PNG, glyphs).

use crate::condition::{Condition, Intensity};

macro_rules! icon_enum {
    ($(#[$doc:meta])* $name:ident { $($variant:ident),+ $(,)? }) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize), serde(rename_all = "snake_case"))]
        #[repr(u8)]
        #[allow(missing_docs)]
        pub enum $name { $($variant),+ }

        impl $name {
            /// Every variant, in declaration order (the order of icon tables).
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            /// Position in [`Self::ALL`], used to index icon tables.
            pub fn index(self) -> usize {
                self as usize
            }
        }
    };
}

icon_enum!(
    /// Weather condition icons, with day and night variants where the art differs.
    IconId {
        ClearDay, ClearNight, PartlyCloudyDay, PartlyCloudyNight, Cloudy, OvercastDay,
        OvercastNight, FogDay, FogNight, Haze, Smoke, Dust, Drizzle, Rain, ExtremeRain,
        ShowersDay, ShowersNight, Sleet, Snow, ExtremeSnow, SnowShowersDay, SnowShowersNight,
        ThunderstormsDay, ThunderstormsNight, ThunderstormsRain, Hail, Wind, Tornado,
        NotAvailable,
    }
);

icon_enum!(
    /// Icons for interface elements: sun and moon events, measurements and moon phases.
    UiIcon {
        Sunrise, Sunset, Moonrise, Moonset, Thermometer, Humidity, Barometer, Wind, Raindrop,
        UvIndex, Visibility, Cloud, MoonNew, MoonWaxingCrescent, MoonFirstQuarter,
        MoonWaxingGibbous, MoonFull, MoonWaningGibbous, MoonLastQuarter, MoonWaningCrescent,
    }
);

impl Condition {
    /// The icon for this condition by day or by night.
    pub fn icon(self, is_day: bool) -> IconId {
        let pick = |day: IconId, night: IconId| if is_day { day } else { night };
        match self {
            Self::Clear => pick(IconId::ClearDay, IconId::ClearNight),
            Self::MainlyClear | Self::PartlyCloudy => {
                pick(IconId::PartlyCloudyDay, IconId::PartlyCloudyNight)
            }
            Self::Cloudy => IconId::Cloudy,
            Self::Overcast => pick(IconId::OvercastDay, IconId::OvercastNight),
            Self::Fog => pick(IconId::FogDay, IconId::FogNight),
            Self::Haze => IconId::Haze,
            Self::Smoke => IconId::Smoke,
            Self::Dust => IconId::Dust,
            Self::Drizzle(_) => IconId::Drizzle,
            Self::Rain(Intensity::Heavy) => IconId::ExtremeRain,
            Self::Rain(_) => IconId::Rain,
            Self::RainShowers(_) => pick(IconId::ShowersDay, IconId::ShowersNight),
            Self::FreezingDrizzle(_)
            | Self::FreezingRain(_)
            | Self::Sleet(_)
            | Self::SleetShowers(_) => IconId::Sleet,
            Self::Snow(Intensity::Heavy) => IconId::ExtremeSnow,
            Self::Snow(_) | Self::SnowGrains => IconId::Snow,
            Self::SnowShowers(_) => pick(IconId::SnowShowersDay, IconId::SnowShowersNight),
            Self::Thunderstorm(Intensity::Heavy) => IconId::ThunderstormsRain,
            Self::Thunderstorm(_) => pick(IconId::ThunderstormsDay, IconId::ThunderstormsNight),
            Self::ThunderstormHail(_) => IconId::Hail,
            Self::Squall => IconId::Wind,
            Self::Tornado => IconId::Tornado,
            Self::Unknown => IconId::NotAvailable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::condition::Intensity::{Heavy, Light, Moderate};
    use alloc::format;
    use alloc::string::String;
    use core::fmt::Write;

    const CONDITIONS: &[Condition] = &[
        Condition::Clear,
        Condition::MainlyClear,
        Condition::PartlyCloudy,
        Condition::Cloudy,
        Condition::Overcast,
        Condition::Fog,
        Condition::Haze,
        Condition::Smoke,
        Condition::Dust,
        Condition::Drizzle(Light),
        Condition::FreezingDrizzle(Light),
        Condition::Rain(Light),
        Condition::Rain(Moderate),
        Condition::Rain(Heavy),
        Condition::FreezingRain(Heavy),
        Condition::RainShowers(Moderate),
        Condition::Sleet(Moderate),
        Condition::SleetShowers(Light),
        Condition::Snow(Moderate),
        Condition::Snow(Heavy),
        Condition::SnowShowers(Light),
        Condition::SnowGrains,
        Condition::Thunderstorm(Moderate),
        Condition::Thunderstorm(Heavy),
        Condition::ThunderstormHail(Light),
        Condition::Squall,
        Condition::Tornado,
        Condition::Unknown,
    ];

    #[test]
    fn icon_matrix() {
        let mut out = String::new();
        for &c in CONDITIONS {
            let _ = writeln!(
                out,
                "{:<28} day={:<20} night={:?}",
                format!("{c:?}"),
                format!("{:?}", c.icon(true)),
                c.icon(false)
            );
        }
        insta::assert_snapshot!(out);
    }

    #[test]
    fn all_tables_match_indices() {
        for (i, icon) in IconId::ALL.iter().enumerate() {
            assert_eq!(icon.index(), i);
        }
        for (i, icon) in UiIcon::ALL.iter().enumerate() {
            assert_eq!(icon.index(), i);
        }
        assert_eq!(IconId::ALL.len(), 29);
        assert_eq!(UiIcon::ALL.len(), 20);
    }
}
