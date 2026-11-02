//! Colors and styles: AQI/UV levels, temperature bands and sky accents, for dark and
//! light terminals, with `[display.tui.colors]` overrides. Whether to use color at all
//! (`NO_COLOR`, `TERM=dumb`) is decided by the host.

use core::str::FromStr;

use ratatui::style::{Color, Modifier, Style};
use whether_core::scales::{Level, TempLevel};
use whether_core::theme::SkyTheme;

use crate::config::{Background, TuiColors};

/// Every color the widgets use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    /// Borders, titles, highlights.
    pub accent: Color,
    /// Secondary text.
    pub muted: Color,
    /// Below 8 °C.
    pub cold: Color,
    /// 24–30 °C.
    pub warm: Color,
    /// From 30 °C.
    pub hot: Color,
    /// Errors and alerts.
    pub error: Color,
    /// Level colors from good to extreme.
    pub levels: [Color; 6],
    /// When false every style is plain (no color).
    pub colored: bool,
}

/// The 16 ANSI colors with xterm's default RGB values.
const ANSI16: [(Color, (u8, u8, u8)); 16] = [
    (Color::Black, (0, 0, 0)),
    (Color::Red, (205, 0, 0)),
    (Color::Green, (0, 205, 0)),
    (Color::Yellow, (205, 205, 0)),
    (Color::Blue, (0, 0, 238)),
    (Color::Magenta, (205, 0, 205)),
    (Color::Cyan, (0, 205, 205)),
    (Color::Gray, (229, 229, 229)),
    (Color::DarkGray, (127, 127, 127)),
    (Color::LightRed, (255, 0, 0)),
    (Color::LightGreen, (0, 255, 0)),
    (Color::LightYellow, (255, 255, 0)),
    (Color::LightBlue, (92, 92, 255)),
    (Color::LightMagenta, (255, 0, 255)),
    (Color::LightCyan, (0, 255, 255)),
    (Color::White, (255, 255, 255)),
];

/// `color` itself unless it is RGB; then the nearest ANSI color.
pub fn nearest_ansi(color: Color) -> Color {
    let Color::Rgb(r, g, b) = color else {
        return color;
    };
    let distance = |(cr, cg, cb): (u8, u8, u8)| {
        let d = |a: u8, b: u8| (i32::from(a) - i32::from(b)).pow(2);
        d(r, cr) + d(g, cg) + d(b, cb)
    };
    ANSI16
        .iter()
        .min_by_key(|(_, rgb)| distance(*rgb))
        .map_or(color, |(c, _)| *c)
}

fn parse(value: Option<&str>, fallback: Color) -> Color {
    value
        .and_then(|v| Color::from_str(v).ok())
        .unwrap_or(fallback)
}

impl Palette {
    /// The palette for `background`, with `overrides` applied.
    pub fn new(background: Background, overrides: &TuiColors) -> Self {
        let base = match background {
            Background::Dark => Self {
                accent: Color::Cyan,
                muted: Color::DarkGray,
                cold: Color::Rgb(0x5f, 0xaf, 0xff),
                warm: Color::Rgb(0xff, 0xaf, 0x5f),
                hot: Color::Rgb(0xff, 0x5f, 0x5f),
                error: Color::LightRed,
                levels: [
                    Color::Rgb(0x5f, 0xd7, 0x5f),
                    Color::Rgb(0xd7, 0xd7, 0x5f),
                    Color::Rgb(0xff, 0xaf, 0x00),
                    Color::Rgb(0xff, 0x5f, 0x00),
                    Color::Rgb(0xd7, 0x00, 0x5f),
                    Color::Rgb(0xaf, 0x00, 0xd7),
                ],
                colored: true,
            },
            Background::Light => Self {
                accent: Color::Blue,
                muted: Color::Gray,
                cold: Color::Rgb(0x00, 0x5f, 0xaf),
                warm: Color::Rgb(0xaf, 0x5f, 0x00),
                hot: Color::Rgb(0xaf, 0x00, 0x00),
                error: Color::Red,
                levels: [
                    Color::Rgb(0x00, 0x87, 0x00),
                    Color::Rgb(0x87, 0x87, 0x00),
                    Color::Rgb(0xaf, 0x5f, 0x00),
                    Color::Rgb(0xd7, 0x00, 0x00),
                    Color::Rgb(0x87, 0x00, 0x5f),
                    Color::Rgb(0x5f, 0x00, 0x87),
                ],
                colored: true,
            },
        };
        Self {
            accent: parse(overrides.accent.as_deref(), base.accent),
            muted: parse(overrides.muted.as_deref(), base.muted),
            cold: parse(overrides.cold.as_deref(), base.cold),
            warm: parse(overrides.warm.as_deref(), base.warm),
            hot: parse(overrides.hot.as_deref(), base.hot),
            ..base
        }
    }

    /// The palette for a terminal's color depth.
    #[must_use]
    pub fn for_depth(self, depth: crate::ui_state::ColorDepth) -> Self {
        match depth {
            crate::ui_state::ColorDepth::Plain => self.plain(),
            crate::ui_state::ColorDepth::Ansi16 => self.ansi16(),
            crate::ui_state::ColorDepth::TrueColor => self,
        }
    }

    /// Every RGB color replaced by the nearest of the 16 ANSI colors.
    #[must_use]
    pub fn ansi16(self) -> Self {
        Self {
            accent: nearest_ansi(self.accent),
            muted: nearest_ansi(self.muted),
            cold: nearest_ansi(self.cold),
            warm: nearest_ansi(self.warm),
            hot: nearest_ansi(self.hot),
            error: nearest_ansi(self.error),
            levels: self.levels.map(nearest_ansi),
            colored: self.colored,
        }
    }

    /// A palette that draws without color (`NO_COLOR`, `color = "never"`).
    #[must_use]
    pub fn plain(self) -> Self {
        Self {
            colored: false,
            ..self
        }
    }

    fn fg(&self, color: Color) -> Style {
        if self.colored {
            Style::new().fg(color)
        } else {
            Style::new()
        }
    }

    /// Text in a level's color (AQI category, UV).
    pub fn level_style(&self, level: Level) -> Style {
        let i = match level {
            Level::Good => 0,
            Level::Fair => 1,
            Level::Moderate => 2,
            Level::Poor => 3,
            Level::VeryPoor => 4,
            Level::Extreme => 5,
        };
        self.fg(self.levels[i])
    }

    /// A temperature's color by band.
    pub fn temp_color(&self, level: TempLevel) -> Color {
        match level {
            TempLevel::Freezing | TempLevel::Cold => self.cold,
            TempLevel::Cool => Color::Reset,
            TempLevel::Mild => self.levels[0],
            TempLevel::Warm => self.warm,
            TempLevel::Hot => self.hot,
        }
    }

    /// A temperature in its band's color, bold.
    pub fn temp_style(&self, level: TempLevel) -> Style {
        self.fg(self.temp_color(level)).add_modifier(Modifier::BOLD)
    }

    /// The accent for the current sky (used for the current panel's border).
    pub fn accent(&self, sky: SkyTheme) -> Color {
        match sky {
            SkyTheme::ClearDay | SkyTheme::Twilight => self.warm,
            SkyTheme::ClearNight | SkyTheme::CloudyNight => Color::Blue,
            SkyTheme::Rain | SkyTheme::Storm => self.cold,
            SkyTheme::Snow | SkyTheme::Fog | SkyTheme::CloudyDay => self.accent,
        }
    }

    /// Borders and titles.
    pub fn accent_style(&self) -> Style {
        self.fg(self.accent)
    }

    /// Secondary text.
    pub fn muted_style(&self) -> Style {
        self.fg(self.muted)
    }

    /// Precipitation.
    pub fn cold_style(&self) -> Style {
        self.fg(self.cold)
    }

    /// Errors and alerts.
    pub fn error_style(&self) -> Style {
        self.fg(self.error).add_modifier(Modifier::BOLD)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overrides_win_and_bad_values_fall_back() {
        let overrides = TuiColors {
            accent: Some("magenta".into()),
            hot: Some("#123456".into()),
            cold: Some("nope".into()),
            ..TuiColors::default()
        };
        let p = Palette::new(Background::Dark, &overrides);
        assert_eq!(p.accent, Color::Magenta);
        assert_eq!(p.hot, Color::Rgb(0x12, 0x34, 0x56));
        assert_eq!(
            p.cold,
            Palette::new(Background::Dark, &TuiColors::default()).cold
        );
    }

    #[test]
    fn light_and_dark_differ() {
        let dark = Palette::new(Background::Dark, &TuiColors::default());
        let light = Palette::new(Background::Light, &TuiColors::default());
        assert_ne!(dark.accent, light.accent);
        assert_ne!(
            dark.level_style(Level::Good),
            light.level_style(Level::Good)
        );
    }

    #[test]
    fn ansi16_maps_to_named_colors() {
        assert_eq!(nearest_ansi(Color::Rgb(0xff, 0x5f, 0x5f)), Color::LightRed);
        assert_eq!(nearest_ansi(Color::Rgb(0x5f, 0xaf, 0xff)), Color::LightBlue);
        assert_eq!(nearest_ansi(Color::Cyan), Color::Cyan);
        let p = Palette::new(Background::Dark, &TuiColors::default()).ansi16();
        let rgb = |c: Color| matches!(c, Color::Rgb(..));
        assert!(!rgb(p.hot) && !rgb(p.cold) && p.levels.iter().all(|c| !rgb(*c)));
    }

    #[test]
    fn plain_has_no_colors() {
        let p = Palette::new(Background::Dark, &TuiColors::default()).plain();
        assert_eq!(p.level_style(Level::Extreme), Style::new());
        assert_eq!(
            p.temp_style(TempLevel::Hot),
            Style::new().add_modifier(Modifier::BOLD)
        );
        assert_eq!(p.error_style().fg, None);
    }
}
