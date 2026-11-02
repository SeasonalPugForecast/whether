//! Icon glyphs per style and 5-line ASCII art for the current panel.
//!
//! Nerd Font codepoints are from the *Weather Icons* set (`nf-weather-*`) in
//! nerd-fonts' `glyphnames.json` (version 3.5.1,
//! <https://github.com/ryanoasis/nerd-fonts/blob/master/glyphnames.json>). Emoji avoid
//! the VS16 selector where the plain character exists, so terminals agree on width.
//! The ASCII art is original and uses only ASCII, so embedded fonts can draw it.

use whether_core::glyph;
use whether_core::icon::{IconId, UiIcon};

use crate::config::IconStyle;

/// The glyph for a condition icon.
pub fn condition(icon: IconId, style: IconStyle) -> &'static str {
    match style {
        IconStyle::NerdFont => glyph::nerd_font(icon),
        IconStyle::Emoji => emoji(icon),
        IconStyle::Ascii => ascii(icon),
    }
}

fn emoji(icon: IconId) -> &'static str {
    match icon {
        IconId::ClearDay => "☀",
        IconId::ClearNight => "🌙",
        IconId::PartlyCloudyDay => "⛅",
        IconId::PartlyCloudyNight
        | IconId::Cloudy
        | IconId::OvercastDay
        | IconId::OvercastNight => "☁",
        IconId::FogDay | IconId::FogNight | IconId::Haze | IconId::Smoke => "🌫",
        IconId::Dust | IconId::Wind => "💨",
        IconId::Drizzle | IconId::ShowersDay => "🌦",
        IconId::Rain | IconId::ExtremeRain | IconId::ShowersNight => "🌧",
        IconId::Sleet
        | IconId::Snow
        | IconId::SnowShowersDay
        | IconId::SnowShowersNight
        | IconId::Hail => "🌨",
        IconId::ExtremeSnow => "❄",
        IconId::ThunderstormsDay | IconId::ThunderstormsNight | IconId::ThunderstormsRain => "⛈",
        IconId::Tornado => "🌪",
        IconId::NotAvailable => "?",
    }
}

fn ascii(icon: IconId) -> &'static str {
    match icon {
        IconId::ClearDay => "SUN",
        IconId::ClearNight => "CLR",
        IconId::PartlyCloudyDay | IconId::PartlyCloudyNight => "PCLD",
        IconId::Cloudy => "CLD",
        IconId::OvercastDay | IconId::OvercastNight => "OVC",
        IconId::FogDay | IconId::FogNight => "FOG",
        IconId::Haze => "HAZE",
        IconId::Smoke => "SMK",
        IconId::Dust => "DUST",
        IconId::Drizzle => "DRZL",
        IconId::Rain => "RAIN",
        IconId::ExtremeRain => "HVY",
        IconId::ShowersDay | IconId::ShowersNight => "SHWR",
        IconId::Sleet => "SLT",
        IconId::Snow | IconId::SnowShowersDay | IconId::SnowShowersNight => "SNOW",
        IconId::ExtremeSnow => "HSNW",
        IconId::ThunderstormsDay | IconId::ThunderstormsNight | IconId::ThunderstormsRain => "TSTM",
        IconId::Hail => "HAIL",
        IconId::Wind => "WIND",
        IconId::Tornado => "TORN",
        IconId::NotAvailable => "N/A",
    }
}

/// The glyph for an interface icon.
pub fn ui(icon: UiIcon, style: IconStyle) -> &'static str {
    match style {
        IconStyle::NerdFont => match icon {
            UiIcon::Sunrise => "\u{e34c}",            // weather-sunrise
            UiIcon::Sunset => "\u{e34d}",             // weather-sunset
            UiIcon::Moonrise => "\u{e3c1}",           // weather-moonrise
            UiIcon::Moonset => "\u{e3c2}",            // weather-moonset
            UiIcon::Thermometer => "\u{e350}",        // weather-thermometer
            UiIcon::Humidity => "\u{e373}",           // weather-humidity
            UiIcon::Barometer => "\u{e372}",          // weather-barometer
            UiIcon::Wind => "\u{e34b}",               // weather-strong_wind
            UiIcon::Raindrop => "\u{e371}",           // weather-raindrop
            UiIcon::UvIndex => "\u{e36b}",            // weather-hot
            UiIcon::Visibility => "\u{e3ae}",         // weather-day_haze
            UiIcon::Cloud => "\u{e33d}",              // weather-cloud
            UiIcon::MoonNew => "\u{e38d}",            // weather-moon_new
            UiIcon::MoonWaxingCrescent => "\u{e390}", // weather-moon_waxing_crescent_3
            UiIcon::MoonFirstQuarter => "\u{e394}",   // weather-moon_first_quarter
            UiIcon::MoonWaxingGibbous => "\u{e397}",  // weather-moon_waxing_gibbous_3
            UiIcon::MoonFull => "\u{e39b}",           // weather-moon_full
            UiIcon::MoonWaningGibbous => "\u{e39e}",  // weather-moon_waning_gibbous_3
            UiIcon::MoonLastQuarter => "\u{e3a2}",    // weather-moon_third_quarter
            UiIcon::MoonWaningCrescent => "\u{e3a5}", // weather-moon_waning_crescent_3
        },
        IconStyle::Emoji => match icon {
            UiIcon::Sunrise => "🌅",
            UiIcon::Sunset => "🌇",
            UiIcon::Moonrise | UiIcon::Moonset => "🌙",
            UiIcon::Thermometer => "🌡",
            UiIcon::Humidity | UiIcon::Raindrop => "💧",
            UiIcon::Barometer => "⏲",
            UiIcon::Wind => "💨",
            UiIcon::UvIndex => "☀",
            UiIcon::Visibility => "👁",
            UiIcon::Cloud => "☁",
            UiIcon::MoonNew => "🌑",
            UiIcon::MoonWaxingCrescent => "🌒",
            UiIcon::MoonFirstQuarter => "🌓",
            UiIcon::MoonWaxingGibbous => "🌔",
            UiIcon::MoonFull => "🌕",
            UiIcon::MoonWaningGibbous => "🌖",
            UiIcon::MoonLastQuarter => "🌗",
            UiIcon::MoonWaningCrescent => "🌘",
        },
        IconStyle::Ascii => match icon {
            UiIcon::Sunrise => "rise",
            UiIcon::Sunset => "set",
            UiIcon::Moonrise => "m-rise",
            UiIcon::Moonset => "m-set",
            UiIcon::Thermometer => "T",
            UiIcon::Humidity => "RH",
            UiIcon::Barometer => "P",
            UiIcon::Wind => "W",
            UiIcon::Raindrop => "R",
            UiIcon::UvIndex => "UV",
            UiIcon::Visibility => "VIS",
            UiIcon::Cloud => "CLD",
            UiIcon::MoonNew => "new",
            UiIcon::MoonWaxingCrescent | UiIcon::MoonWaxingGibbous => "wax",
            UiIcon::MoonFirstQuarter => "1st",
            UiIcon::MoonFull => "full",
            UiIcon::MoonWaningGibbous | UiIcon::MoonWaningCrescent => "wane",
            UiIcon::MoonLastQuarter => "3rd",
        },
    }
}

/// A 5-line picture, at most [`ART_WIDTH`] columns.
pub type Art = [&'static str; 5];

/// The widest art line.
pub const ART_WIDTH: usize = 13;

const SUN: Art = [
    "   \\  |  /   ",
    "    .---.    ",
    " --(     )-- ",
    "    '---'    ",
    "   /  |  \\   ",
];
const MOON: Art = [
    "    .--.     ",
    "   /  .'     ",
    "  |  (       ",
    "   \\  '.     ",
    "    '--'     ",
];
const PARTLY_DAY: Art = [
    "  \\ |        ",
    "-( )- .--.   ",
    " / | (    ). ",
    "   (___.__)  ",
    "             ",
];
const PARTLY_NIGHT: Art = [
    "   .-.       ",
    "  (  ( .--.  ",
    "   '-(    ). ",
    "   (___.__)  ",
    "             ",
];
const CLOUD: Art = [
    "             ",
    "    .--.     ",
    " .-(    ).   ",
    "(___.__)__)  ",
    "             ",
];
const FOG: Art = [
    "             ",
    " _ - _ - _ - ",
    "  - _ - _ -  ",
    " _ - _ - _ - ",
    "             ",
];
const DRIZZLE: Art = [
    "    .--.     ",
    " .-(    ).   ",
    "(___.__)__)  ",
    "  '   '   '  ",
    "             ",
];
const RAIN: Art = [
    "    .--.     ",
    " .-(    ).   ",
    "(___.__)__)  ",
    " ' ' ' ' '   ",
    "  ' ' ' ' '  ",
];
const HEAVY_RAIN: Art = [
    "    .--.     ",
    " .-(    ).   ",
    "(___.__)__)  ",
    " /,/,/,/,/   ",
    " ,/,/,/,/,   ",
];
const SNOW: Art = [
    "    .--.     ",
    " .-(    ).   ",
    "(___.__)__)  ",
    "  *  *  *    ",
    " *  *  *  *  ",
];
const SLEET: Art = [
    "    .--.     ",
    " .-(    ).   ",
    "(___.__)__)  ",
    "  ' *  ' *   ",
    " *  '  *  '  ",
];
const THUNDER: Art = [
    "    .--.     ",
    " .-(    ).   ",
    "(___.__)__)  ",
    "    /_  /_   ",
    "     /   /   ",
];
const UNKNOWN: Art = [
    "    .--.     ",
    "   '   |     ",
    "     .'      ",
    "     |       ",
    "     o       ",
];

/// The ASCII art for a condition icon.
pub fn art(icon: IconId) -> &'static Art {
    match icon {
        IconId::ClearDay => &SUN,
        IconId::ClearNight => &MOON,
        IconId::PartlyCloudyDay => &PARTLY_DAY,
        IconId::PartlyCloudyNight => &PARTLY_NIGHT,
        IconId::Cloudy | IconId::OvercastDay | IconId::OvercastNight => &CLOUD,
        IconId::FogDay
        | IconId::FogNight
        | IconId::Haze
        | IconId::Smoke
        | IconId::Dust
        | IconId::Wind
        | IconId::Tornado => &FOG,
        IconId::Drizzle => &DRIZZLE,
        IconId::Rain | IconId::ShowersDay | IconId::ShowersNight => &RAIN,
        IconId::ExtremeRain => &HEAVY_RAIN,
        IconId::Sleet | IconId::Hail => &SLEET,
        IconId::Snow | IconId::ExtremeSnow | IconId::SnowShowersDay | IconId::SnowShowersNight => {
            &SNOW
        }
        IconId::ThunderstormsDay | IconId::ThunderstormsNight | IconId::ThunderstormsRain => {
            &THUNDER
        }
        IconId::NotAvailable => &UNKNOWN,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STYLES: [IconStyle; 3] = [IconStyle::NerdFont, IconStyle::Emoji, IconStyle::Ascii];

    #[test]
    fn every_icon_has_a_glyph_in_every_style() {
        for icon in IconId::ALL {
            for style in STYLES {
                assert!(!condition(*icon, style).is_empty(), "{icon:?} {style:?}");
            }
            assert!(ascii(*icon).is_ascii() && ascii(*icon).len() <= 4);
        }
        for icon in UiIcon::ALL {
            for style in STYLES {
                assert!(!ui(*icon, style).is_empty(), "{icon:?} {style:?}");
            }
        }
    }

    #[test]
    fn emoji_avoid_the_variation_selector() {
        for icon in IconId::ALL {
            assert!(!emoji(*icon).contains('\u{fe0f}'), "{icon:?}");
        }
    }

    #[test]
    fn nerd_font_ui_glyphs_are_weather_icons() {
        for icon in UiIcon::ALL {
            let c = ui(*icon, IconStyle::NerdFont).chars().next().unwrap();
            assert!(('\u{e300}'..='\u{e3ff}').contains(&c), "{icon:?}");
        }
    }

    #[test]
    fn art_is_five_ascii_lines_within_the_width() {
        for icon in IconId::ALL {
            for line in art(*icon) {
                assert!(line.is_ascii(), "{icon:?}: {line}");
                assert!(
                    line.len() <= ART_WIDTH,
                    "{icon:?}: `{line}` is {} wide",
                    line.len()
                );
            }
        }
    }
}
