//! Colors: the shared sky palettes (as gradients) plus fixed colors for icons,
//! levels and badges.

use embedded_graphics::pixelcolor::{Rgb565, Rgb888};
use whether_core::scales::Level;
use whether_core::theme::SkyTheme;

/// An RGB888 triple as an `Rgb565`.
pub fn rgb([r, g, b]: [u8; 3]) -> Rgb565 {
    Rgb565::from(Rgb888::new(r, g, b))
}

/// `a` moved toward `b` by `t` (0 = `a`, 1 = `b`).
pub fn mix(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);
    let channel = |x: u8, y: u8| {
        let v = f32::from(x) + (f32::from(y) - f32::from(x)) * t;
        libm::roundf(v) as u8
    };
    [
        channel(a[0], b[0]),
        channel(a[1], b[1]),
        channel(a[2], b[2]),
    ]
}

/// WCAG relative luminance of an RGB888 color.
pub fn luminance([r, g, b]: [u8; 3]) -> f32 {
    let lin = |c: u8| {
        let c = f32::from(c) / 255.0;
        if c <= 0.039_28 {
            c / 12.92
        } else {
            libm::powf((c + 0.055) / 1.055, 2.4)
        }
    };
    0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
}

/// WCAG contrast ratio of two colors (1–21).
pub fn contrast(a: [u8; 3], b: [u8; 3]) -> f32 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// Below this contrast against any part of the sky, text gets a shadow.
const SHADOW_BELOW: f32 = 3.0;

/// The background when it does not follow the weather.
const NEUTRAL: [u8; 3] = [0x1E, 0x24, 0x30];
const NEUTRAL_TEXT: [u8; 3] = [0xE8, 0xEE, 0xF4];

/// Dark text on level colors.
const ON_LEVEL_DARK: [u8; 3] = [0x1A, 0x1A, 0x1A];

/// A level's color as RGB888.
fn level_rgb(level: Level) -> [u8; 3] {
    match level {
        Level::Good => [0x3F, 0xB9, 0x50],
        Level::Fair => [0xA3, 0xD6, 0x5C],
        Level::Moderate => [0xF2, 0xC9, 0x4C],
        Level::Poor => [0xF2, 0x99, 0x4A],
        Level::VeryPoor => [0xEB, 0x57, 0x57],
        Level::Extreme => [0x9B, 0x51, 0xE0],
    }
}

/// Everything a page draws with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    /// Background gradient top (RGB888 for smooth steps).
    pub top: [u8; 3],
    /// Background gradient bottom.
    pub bottom: [u8; 3],
    /// Primary text.
    pub text: Rgb565,
    /// Secondary text.
    pub muted: Rgb565,
    /// Detail tile fill.
    pub tile: Rgb565,
    /// Empty bar tracks and page dots.
    pub track: Rgb565,
    /// Sun disc and rays.
    pub sun: Rgb565,
    /// Moon.
    pub moon: Rgb565,
    /// Light cloud fill.
    pub cloud: Rgb565,
    /// Storm and overcast cloud fill.
    pub cloud_dark: Rgb565,
    /// Cloud outline.
    pub outline: Rgb565,
    /// Rain drops and precipitation bars.
    pub rain: Rgb565,
    /// Snowflakes and hail.
    pub snow: Rgb565,
    /// Lightning.
    pub bolt: Rgb565,
    /// Fog, haze and smoke bars.
    pub fog: Rgb565,
    /// Dust.
    pub dust: Rgb565,
    /// Cold end of temperature ranges (RGB888, for gradients).
    pub cold: [u8; 3],
    /// Hot end of temperature ranges.
    pub hot: [u8; 3],
    /// Badge fill (stale data, TLS off).
    pub warn: Rgb565,
    /// Text on a badge.
    pub warn_text: Rgb565,
    /// A 1-px text shadow for skies where the text alone lacks contrast (the
    /// bright end of the clear-day gradient).
    pub shadow: Option<Rgb565>,
}

impl Palette {
    /// The palette for `sky`; without `dynamic` a fixed neutral background.
    pub fn new(sky: SkyTheme, dynamic: bool) -> Self {
        let (top, bottom, text) = if dynamic {
            let c = sky.colors();
            (c.top, c.bottom, c.text)
        } else {
            (NEUTRAL, NEUTRAL, NEUTRAL_TEXT)
        };
        let mid = mix(top, bottom, 0.5);
        let worst = [top, mid, bottom]
            .into_iter()
            .map(|bg| contrast(text, bg))
            .fold(f32::MAX, f32::min);
        let shadow = (worst < SHADOW_BELOW).then(|| {
            if luminance(text) > 0.5 {
                rgb(mix(top, [0, 0, 0], 0.6))
            } else {
                rgb(mix(bottom, [255, 255, 255], 0.6))
            }
        });
        Self {
            top,
            bottom,
            text: rgb(text),
            muted: rgb(mix(text, mid, 0.3)),
            tile: rgb(mix(mid, text, 0.14)),
            track: rgb(mix(mid, text, 0.32)),
            sun: rgb([0xFF, 0xC1, 0x2E]),
            moon: rgb([0xF4, 0xE9, 0xC1]),
            cloud: rgb([0xF5, 0xF7, 0xFA]),
            cloud_dark: rgb([0x9C, 0xA6, 0xB2]),
            outline: rgb([0x6E, 0x7A, 0x88]),
            rain: rgb([0x3B, 0x9C, 0xF2]),
            snow: rgb([0xFF, 0xFF, 0xFF]),
            bolt: rgb([0xFF, 0xD2, 0x3F]),
            fog: rgb([0xC4, 0xCC, 0xD4]),
            dust: rgb([0xC8, 0x9B, 0x5E]),
            cold: [0x56, 0xCC, 0xF2],
            hot: [0xF2, 0x99, 0x4A],
            warn: rgb([0xD6, 0x45, 0x45]),
            warn_text: rgb([0xFF, 0xFF, 0xFF]),
            shadow,
        }
    }

    /// The background color of row `y` on a display `height` rows tall.
    pub fn background(&self, y: i32, height: u32) -> Rgb565 {
        let t = y as f32 / height.saturating_sub(1).max(1) as f32;
        rgb(mix(self.top, self.bottom, t))
    }

    /// The color of a level (UV, air quality, alerts).
    pub fn level(&self, level: Level) -> Rgb565 {
        rgb(level_rgb(level))
    }

    /// Text on a level's color: near-black or white, whichever contrasts more.
    pub fn on_level(&self, level: Level) -> Rgb565 {
        let bg = level_rgb(level);
        if contrast(ON_LEVEL_DARK, bg) >= contrast([0xFF; 3], bg) {
            rgb(ON_LEVEL_DARK)
        } else {
            rgb([0xFF; 3])
        }
    }

    /// A temperature-range color at `t` (0 = cold, 1 = hot).
    pub fn range(&self, t: f32) -> Rgb565 {
        rgb(mix(self.cold, self.hot, t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_readable_on_every_sky() {
        for sky in [
            SkyTheme::ClearDay,
            SkyTheme::ClearNight,
            SkyTheme::CloudyDay,
            SkyTheme::CloudyNight,
            SkyTheme::Rain,
            SkyTheme::Snow,
            SkyTheme::Storm,
            SkyTheme::Fog,
            SkyTheme::Twilight,
        ] {
            let c = sky.colors();
            // At the gradient's worst point.
            let worst = [c.top, c.bottom, mix(c.top, c.bottom, 0.5)]
                .into_iter()
                .map(|bg| contrast(c.text, bg))
                .fold(f32::MAX, f32::min);
            let shadowed = Palette::new(sky, true).shadow.is_some();
            assert!(worst >= SHADOW_BELOW || shadowed, "{sky:?}: {worst}");
            assert!(
                worst >= 1.5,
                "{sky:?}: {worst} is too low even with a shadow"
            );
        }
    }

    #[test]
    fn text_on_level_colors_is_readable() {
        let p = Palette::new(SkyTheme::ClearDay, true);
        for level in [
            Level::Good,
            Level::Fair,
            Level::Moderate,
            Level::Poor,
            Level::VeryPoor,
            Level::Extreme,
        ] {
            let text = if p.on_level(level) == rgb(ON_LEVEL_DARK) {
                ON_LEVEL_DARK
            } else {
                [0xFF; 3]
            };
            let ratio = contrast(text, level_rgb(level));
            assert!(ratio >= 4.5, "{level:?}: {ratio}");
        }
    }

    #[test]
    fn mixing_and_backgrounds() {
        assert_eq!(mix([0, 0, 0], [255, 255, 255], 0.5), [128, 128, 128]);
        let p = Palette::new(SkyTheme::ClearDay, true);
        assert_eq!(p.background(0, 320), rgb(p.top));
        assert_eq!(p.background(319, 320), rgb(p.bottom));
        let flat = Palette::new(SkyTheme::ClearDay, false);
        assert_eq!(flat.background(0, 320), flat.background(319, 320));
    }
}
