//! Text glyphs for condition icons, shared by the text displays (TUI, line).
//!
//! Nerd Font codepoints are from the *Weather Icons* set (`nf-weather-*`) as listed in
//! nerd-fonts' `glyphnames.json` (version 3.5.1,
//! <https://github.com/ryanoasis/nerd-fonts/blob/master/glyphnames.json>).

use crate::icon::IconId;

/// An emoji for `icon`.
pub fn emoji(icon: IconId) -> &'static str {
    match icon {
        IconId::ClearDay => "☀️",
        IconId::ClearNight => "🌙",
        IconId::PartlyCloudyDay => "⛅",
        IconId::PartlyCloudyNight
        | IconId::Cloudy
        | IconId::OvercastDay
        | IconId::OvercastNight => "☁️",
        IconId::FogDay | IconId::FogNight | IconId::Haze | IconId::Smoke => "🌫️",
        IconId::Dust | IconId::Wind => "💨",
        IconId::Drizzle | IconId::ShowersDay => "🌦️",
        IconId::Rain | IconId::ExtremeRain | IconId::ShowersNight => "🌧️",
        IconId::Sleet
        | IconId::Snow
        | IconId::SnowShowersDay
        | IconId::SnowShowersNight
        | IconId::Hail => "🌨️",
        IconId::ExtremeSnow => "❄️",
        IconId::ThunderstormsDay | IconId::ThunderstormsNight | IconId::ThunderstormsRain => "⛈️",
        IconId::Tornado => "🌪️",
        IconId::NotAvailable => "❔",
    }
}

/// A Nerd Font glyph for `icon` (needs a patched font).
pub fn nerd_font(icon: IconId) -> &'static str {
    match icon {
        IconId::ClearDay => "\u{e30d}",          // weather-day_sunny
        IconId::ClearNight => "\u{e32b}",        // weather-night_clear
        IconId::PartlyCloudyDay => "\u{e302}",   // weather-day_cloudy
        IconId::PartlyCloudyNight => "\u{e37e}", // weather-night_alt_cloudy
        IconId::Cloudy | IconId::OvercastDay | IconId::OvercastNight => "\u{e312}", // weather-cloudy
        IconId::FogDay => "\u{e303}",             // weather-day_fog
        IconId::FogNight => "\u{e346}",           // weather-night_fog
        IconId::Haze => "\u{e3ae}",               // weather-day_haze
        IconId::Smoke => "\u{e35c}",              // weather-smoke
        IconId::Dust => "\u{e35d}",               // weather-dust
        IconId::Drizzle => "\u{e31b}",            // weather-sprinkle
        IconId::Rain => "\u{e318}",               // weather-rain
        IconId::ExtremeRain => "\u{e317}",        // weather-rain_wind
        IconId::ShowersDay => "\u{e309}",         // weather-day_showers
        IconId::ShowersNight => "\u{e326}",       // weather-night_alt_showers
        IconId::Sleet => "\u{e3ad}",              // weather-sleet
        IconId::Snow => "\u{e31a}",               // weather-snow
        IconId::ExtremeSnow => "\u{e36f}",        // weather-snowflake_cold
        IconId::SnowShowersDay => "\u{e30a}",     // weather-day_snow
        IconId::SnowShowersNight => "\u{e327}",   // weather-night_alt_snow
        IconId::ThunderstormsDay => "\u{e30f}",   // weather-day_thunderstorm
        IconId::ThunderstormsNight => "\u{e32a}", // weather-night_alt_thunderstorm
        IconId::ThunderstormsRain => "\u{e31c}",  // weather-storm_showers
        IconId::Hail => "\u{e314}",               // weather-hail
        IconId::Wind => "\u{e34b}",               // weather-strong_wind
        IconId::Tornado => "\u{e351}",            // weather-tornado
        IconId::NotAvailable => "\u{e374}",       // weather-na
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_has_glyphs() {
        for icon in IconId::ALL {
            assert!(!emoji(*icon).is_empty(), "{icon:?}");
            let nerd = nerd_font(*icon).chars().next().unwrap();
            assert!(
                ('\u{e300}'..='\u{e3ff}').contains(&nerd),
                "{icon:?} is outside the Weather Icons range"
            );
        }
    }

    #[test]
    fn day_and_night_differ_where_the_art_does() {
        assert_ne!(nerd_font(IconId::ClearDay), nerd_font(IconId::ClearNight));
        assert_ne!(emoji(IconId::ClearDay), emoji(IconId::ClearNight));
    }
}
