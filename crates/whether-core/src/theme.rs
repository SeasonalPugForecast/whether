//! Sky themes: the background mood each display derives from the current weather.

use crate::condition::Condition;

/// The sky mood used for gradients (GUI), accents (TUI) and backgrounds (panels).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "snake_case")
)]
#[allow(missing_docs)]
pub enum SkyTheme {
    ClearDay,
    ClearNight,
    CloudyDay,
    CloudyNight,
    Rain,
    Snow,
    Storm,
    Fog,
    Twilight,
}

/// One sky's colors (RGB): gradient top and bottom, and the text drawn on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SkyColors {
    /// Gradient top.
    pub top: [u8; 3],
    /// Gradient bottom.
    pub bottom: [u8; 3],
    /// Text on this sky.
    pub text: [u8; 3],
}

/// The default sky palettes in [`SkyTheme`] order, shared by the desktop GUI and
/// the panel display so both look alike.
pub const SKY_COLORS: [SkyColors; 9] = [
    SkyColors {
        top: [0x2F, 0x80, 0xED],
        bottom: [0x56, 0xCC, 0xF2],
        text: [0xFF, 0xFF, 0xFF],
    },
    SkyColors {
        top: [0x0F, 0x20, 0x27],
        bottom: [0x2C, 0x53, 0x64],
        text: [0xE8, 0xF1, 0xF8],
    },
    SkyColors {
        top: [0x75, 0x7F, 0x9A],
        bottom: [0xD7, 0xDD, 0xE8],
        text: [0x1E, 0x24, 0x30],
    },
    SkyColors {
        top: [0x23, 0x25, 0x26],
        bottom: [0x41, 0x43, 0x45],
        text: [0xE6, 0xE6, 0xE6],
    },
    SkyColors {
        top: [0x3A, 0x60, 0x73],
        bottom: [0x16, 0x22, 0x2A],
        text: [0xEA, 0xF2, 0xF8],
    },
    SkyColors {
        top: [0xA8, 0xC0, 0xD6],
        bottom: [0xE6, 0xEE, 0xF5],
        text: [0x1E, 0x2A, 0x36],
    },
    SkyColors {
        top: [0x1F, 0x1C, 0x2C],
        bottom: [0x92, 0x8D, 0xAB],
        text: [0xF2, 0xF0, 0xFF],
    },
    SkyColors {
        top: [0x8E, 0x9E, 0xAB],
        bottom: [0xEE, 0xF2, 0xF3],
        text: [0x26, 0x32, 0x3A],
    },
    SkyColors {
        top: [0x2C, 0x3E, 0x50],
        bottom: [0xFD, 0x74, 0x6C],
        text: [0xFF, 0xF6, 0xF0],
    },
];

impl SkyTheme {
    /// This sky's default colors.
    pub fn colors(self) -> SkyColors {
        SKY_COLORS[self as usize]
    }
}

/// Chooses the sky theme. `twilight` is true near sunrise or sunset.
pub fn sky_theme(condition: Condition, is_day: bool, twilight: bool) -> SkyTheme {
    match condition {
        Condition::Thunderstorm(_)
        | Condition::ThunderstormHail(_)
        | Condition::Squall
        | Condition::Tornado => SkyTheme::Storm,
        c if c.is_frozen() => SkyTheme::Snow,
        c if c.is_precipitation() => SkyTheme::Rain,
        Condition::Fog | Condition::Haze | Condition::Smoke | Condition::Dust => SkyTheme::Fog,
        Condition::Cloudy | Condition::Overcast => {
            if is_day {
                SkyTheme::CloudyDay
            } else {
                SkyTheme::CloudyNight
            }
        }
        _ if twilight => SkyTheme::Twilight,
        _ if is_day => SkyTheme::ClearDay,
        _ => SkyTheme::ClearNight,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::condition::Intensity::{Heavy, Light};

    #[test]
    fn every_sky_has_colors() {
        assert_eq!(SkyTheme::ClearDay.colors().top, [0x2F, 0x80, 0xED]);
        assert_eq!(SkyTheme::Twilight.colors().bottom, [0xFD, 0x74, 0x6C]);
    }

    #[test]
    fn storms_win_over_everything() {
        assert_eq!(
            sky_theme(Condition::Thunderstorm(Light), true, true),
            SkyTheme::Storm
        );
        assert_eq!(sky_theme(Condition::Tornado, false, false), SkyTheme::Storm);
    }

    #[test]
    fn precipitation_kinds() {
        assert_eq!(
            sky_theme(Condition::Snow(Heavy), true, false),
            SkyTheme::Snow
        );
        assert_eq!(
            sky_theme(Condition::FreezingRain(Light), true, false),
            SkyTheme::Snow
        );
        assert_eq!(
            sky_theme(Condition::Drizzle(Light), false, false),
            SkyTheme::Rain
        );
        assert_eq!(sky_theme(Condition::Haze, true, false), SkyTheme::Fog);
    }

    #[test]
    fn clouds_follow_day_and_night() {
        assert_eq!(
            sky_theme(Condition::Overcast, true, true),
            SkyTheme::CloudyDay
        );
        assert_eq!(
            sky_theme(Condition::Cloudy, false, false),
            SkyTheme::CloudyNight
        );
    }

    #[test]
    fn clear_skies_use_twilight_near_sunrise_and_sunset() {
        assert_eq!(sky_theme(Condition::Clear, true, true), SkyTheme::Twilight);
        assert_eq!(
            sky_theme(Condition::PartlyCloudy, true, false),
            SkyTheme::ClearDay
        );
        assert_eq!(
            sky_theme(Condition::MainlyClear, false, false),
            SkyTheme::ClearNight
        );
    }
}
