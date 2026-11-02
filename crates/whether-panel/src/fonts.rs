//! u8g2 fonts per panel size class and the text helpers. Licenses: `FONTS.md`.

use alloc::borrow::Cow;

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::{DrawTarget, Point, Size};
use embedded_graphics::primitives::Rectangle;
use u8g2_fonts::types::{FontColor, HorizontalAlignment, VerticalPosition};
use u8g2_fonts::{Error, FontRenderer, fonts};

/// Font sizes follow the panel: `Large` for 480×320 and 320×480, `Small` for
/// 320×240 and smaller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SizeClass {
    /// Panels up to 240 px on the short side.
    Small,
    /// Panels with at least 300 px on the short side.
    Large,
}

impl SizeClass {
    /// The class for a display of `size`.
    pub fn for_size(size: Size) -> Self {
        if size.width.min(size.height) >= 300 {
            Self::Large
        } else {
            Self::Small
        }
    }

    /// Outer padding in pixels.
    pub fn pad(self) -> i32 {
        match self {
            Self::Small => 8,
            Self::Large => 12,
        }
    }

    /// Weather icon sizes: the current conditions and the hourly and daily rows.
    pub fn icons(self) -> (u32, u32) {
        match self {
            Self::Small => (48, 24),
            Self::Large => (64, 32),
        }
    }
}

/// The four text styles of a page (`FreeUniversal`, OFL; Adobe Helvetica, X11).
#[derive(Clone, Debug)]
pub struct Fonts {
    /// Large numerals for the temperature.
    pub numerals: FontRenderer,
    /// Condition and tile values.
    pub title: FontRenderer,
    /// Rows and labels.
    pub label: FontRenderer,
    /// Status line, times and captions.
    pub small: FontRenderer,
}

impl Fonts {
    /// The fonts for `class`. Characters a font lacks are skipped, not errors.
    pub fn new(class: SizeClass) -> Self {
        let f = |r: FontRenderer| r.with_ignore_unknown_chars(true);
        match class {
            SizeClass::Large => Self {
                numerals: f(FontRenderer::new::<fonts::u8g2_font_fub42_tf>()),
                title: f(FontRenderer::new::<fonts::u8g2_font_fur20_tf>()),
                label: f(FontRenderer::new::<fonts::u8g2_font_fur14_tf>()),
                small: f(FontRenderer::new::<fonts::u8g2_font_helvR10_tf>()),
            },
            SizeClass::Small => Self {
                numerals: f(FontRenderer::new::<fonts::u8g2_font_fub30_tf>()),
                title: f(FontRenderer::new::<fonts::u8g2_font_fur14_tf>()),
                label: f(FontRenderer::new::<fonts::u8g2_font_fur11_tf>()),
                small: f(FontRenderer::new::<fonts::u8g2_font_helvR08_tf>()),
            },
        }
    }
}

/// Only display errors matter: unknown glyphs are skipped and no background
/// color is ever requested.
fn display_error<E>(e: Error<E>) -> Result<(), E> {
    match e {
        Error::DisplayError(e) => Err(e),
        Error::BackgroundColorNotSupported | Error::GlyphNotFound(_) => Ok(()),
    }
}

/// Where a text is anchored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Anchor {
    /// Horizontal alignment at the point.
    pub h: HorizontalAlignment,
    /// Vertical position at the point.
    pub v: VerticalPosition,
}

impl Anchor {
    /// Left edge, top.
    pub const TOP_LEFT: Self = Self {
        h: HorizontalAlignment::Left,
        v: VerticalPosition::Top,
    };
    /// Right edge, top.
    pub const TOP_RIGHT: Self = Self {
        h: HorizontalAlignment::Right,
        v: VerticalPosition::Top,
    };
    /// Horizontal center, top.
    pub const TOP_CENTER: Self = Self {
        h: HorizontalAlignment::Center,
        v: VerticalPosition::Top,
    };
    /// Left edge, vertical center.
    pub const LEFT: Self = Self {
        h: HorizontalAlignment::Left,
        v: VerticalPosition::Center,
    };
    /// Right edge, vertical center.
    pub const RIGHT: Self = Self {
        h: HorizontalAlignment::Right,
        v: VerticalPosition::Center,
    };
    /// Centered both ways.
    pub const CENTER: Self = Self {
        h: HorizontalAlignment::Center,
        v: VerticalPosition::Center,
    };
}

/// Punctuation the view uses beyond the fonts' Latin-1 range, with stand-ins.
const STAND_INS: [(char, &str); 3] = [('–', "-"), ('—', "-"), ('…', "...")];

/// `text` with [`STAND_INS`] replaced, so dashes and ellipses are not skipped.
fn printable(text: &str) -> Cow<'_, str> {
    if !text.contains(|c| STAND_INS.iter().any(|(s, _)| *s == c)) {
        return Cow::Borrowed(text);
    }
    let mut out = alloc::string::String::with_capacity(text.len());
    for c in text.chars() {
        match STAND_INS.iter().find(|(s, _)| *s == c) {
            Some((_, r)) => out.push_str(r),
            None => out.push(c),
        }
    }
    Cow::Owned(out)
}

/// Draws `text` at `at`.
pub fn text<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    font: &FontRenderer,
    text: &str,
    at: Point,
    anchor: Anchor,
    color: Rgb565,
) -> Result<(), D::Error> {
    font.render_aligned(
        printable(text).as_ref(),
        at,
        anchor.v,
        anchor.h,
        FontColor::Transparent(color),
        d,
    )
    .map(|_| ())
    .or_else(display_error)
}

/// The rendered width of `text` in pixels.
pub fn width(font: &FontRenderer, text: &str) -> u32 {
    font.get_rendered_dimensions(
        printable(text).as_ref(),
        Point::zero(),
        VerticalPosition::Top,
    )
    .map_or(0, |dim| dim.advance.x.unsigned_abs())
}

/// The font's line height in pixels.
pub fn line_height(font: &FontRenderer) -> u32 {
    font.get_default_line_height()
}

/// The height of capital letters and digits (ascent).
pub fn cap_height(font: &FontRenderer) -> u32 {
    u32::from(font.get_ascent().unsigned_abs())
}

/// `text` cut to fit `max` pixels, ending in ".." when shortened.
pub fn fit(font: &FontRenderer, text: &str, max: u32) -> alloc::string::String {
    if width(font, text) <= max {
        return text.into();
    }
    let mut out = alloc::string::String::from(text);
    while !out.is_empty() {
        out.pop();
        let trimmed = out.trim_end();
        let candidate = alloc::format!("{trimmed}..");
        if width(font, &candidate) <= max {
            return candidate;
        }
    }
    out
}

/// The bounding box `text` would take when drawn at `at`.
pub fn bounds(font: &FontRenderer, text: &str, at: Point, anchor: Anchor) -> Option<Rectangle> {
    font.get_rendered_dimensions_aligned(printable(text).as_ref(), at, anchor.v, anchor.h)
        .ok()
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes_follow_the_short_side() {
        assert_eq!(SizeClass::for_size(Size::new(480, 320)), SizeClass::Large);
        assert_eq!(SizeClass::for_size(Size::new(320, 480)), SizeClass::Large);
        assert_eq!(SizeClass::for_size(Size::new(320, 240)), SizeClass::Small);
    }

    #[test]
    fn fonts_cover_the_view_model_symbols() {
        let fonts = Fonts::new(SizeClass::Large);
        // Latin-1 fonts: degrees, the middle dot and micrograms render.
        for s in ["18°", "·", "µg/m³", "Tromsø"] {
            assert!(width(&fonts.label, s) > 0, "{s}");
        }
        assert!(width(&fonts.numerals, "-12°") > width(&fonts.title, "-12°"));
        // Dashes and ellipses have ASCII stand-ins.
        for (s, stand_in) in [("–", "-"), ("—", "-"), ("…", "...")] {
            assert_eq!(width(&fonts.label, s), width(&fonts.label, stand_in), "{s}");
        }
        assert_eq!(printable("Mon 09:00 – Tue 06:00"), "Mon 09:00 - Tue 06:00");
        // A missing glyph is skipped instead of failing.
        assert_eq!(width(&fonts.label, "東京"), 0);
    }

    #[test]
    fn fitting_shortens_with_dots() {
        let font = Fonts::new(SizeClass::Small).label;
        let long = "Thunderstorm with heavy hail";
        let cut = fit(&font, long, 80);
        assert!(cut.ends_with("..") && width(&font, &cut) <= 80, "{cut}");
        assert_eq!(fit(&font, "Rain", 80), "Rain");
    }
}
