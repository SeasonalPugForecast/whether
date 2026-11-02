//! The pages: each draws into the body area between header and status line.

pub(crate) mod alerts;
pub(crate) mod details;
pub(crate) mod forecast;
pub(crate) mod now;

use alloc::string::String;
use alloc::vec::Vec;

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::{DrawTarget, Point};
use embedded_graphics::primitives::Rectangle;
use u8g2_fonts::FontRenderer;
use whether_core::view::ViewModel;

use crate::fonts::{self, Anchor};
use crate::layout::Ctx;

/// Greedy word wrap of `text` into at most `max_lines` lines of `width` pixels;
/// the last line is shortened when text remains.
pub(crate) fn wrap(font: &FontRenderer, text: &str, width: u32, max_lines: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    let mut words = text.split_whitespace().peekable();
    while let Some(word) = words.next() {
        let candidate = if line.is_empty() {
            String::from(word)
        } else {
            alloc::format!("{line} {word}")
        };
        if fonts::width(font, &candidate) <= width || line.is_empty() {
            line = candidate;
            continue;
        }
        lines.push(core::mem::replace(&mut line, String::from(word)));
        if lines.len() == max_lines {
            // Out of lines: end the last one with what is left, shortened.
            let mut rest = core::mem::take(&mut line);
            for w in words.by_ref() {
                rest.push(' ');
                rest.push_str(w);
            }
            let last = lines.pop().unwrap_or_default();
            lines.push(fonts::fit(font, &alloc::format!("{last} {rest}"), width));
            return lines;
        }
    }
    if !line.is_empty() && lines.len() < max_lines {
        lines.push(fonts::fit(font, &line, width));
    }
    lines
}

/// Draws wrapped lines from `top`; returns the y below the last line.
pub(crate) fn paragraph<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    ctx: &Ctx,
    font: &FontRenderer,
    lines: &[String],
    top_left: Point,
    color: Rgb565,
) -> Result<i32, D::Error> {
    let step = fonts::line_height(font) as i32;
    let mut y = top_left.y;
    for line in lines {
        ctx.text(
            d,
            font,
            line,
            Point::new(top_left.x, y),
            Anchor::TOP_LEFT,
            color,
        )?;
        y += step;
    }
    Ok(y)
}

/// "No data" and the reason, centered.
pub(crate) fn no_data<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    ctx: &Ctx,
    vm: &ViewModel,
    area: Rectangle,
) -> Result<(), D::Error> {
    let c = area.center();
    ctx.text(
        d,
        &ctx.fonts.title,
        "No data",
        c,
        Anchor::CENTER,
        ctx.pal.text,
    )?;
    if let Some(error) = &vm.status.error {
        let lines = wrap(&ctx.fonts.small, error, area.size.width, 3);
        let top = c.y + fonts::line_height(&ctx.fonts.title) as i32;
        let step = fonts::line_height(&ctx.fonts.small) as i32;
        for (i, line) in lines.iter().enumerate() {
            let y = top + step * i32::try_from(i).unwrap_or(0);
            ctx.text(
                d,
                &ctx.fonts.small,
                line,
                Point::new(c.x, y),
                Anchor::TOP_CENTER,
                ctx.pal.muted,
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::{Fonts, SizeClass};

    #[test]
    fn wrapping_respects_width_and_lines() {
        let font = Fonts::new(SizeClass::Small).label;
        let text = "Partly cloudy. High 18°, low 8°. Rain starting in about 45 minutes.";
        let lines = wrap(&font, text, 150, 2);
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(
            lines.iter().all(|l| fonts::width(&font, l) <= 150),
            "{lines:?}"
        );
        assert!(lines[1].ends_with(".."), "{lines:?}");
        assert_eq!(wrap(&font, "Clear", 150, 2), ["Clear"]);
        assert_eq!(wrap(&font, "", 150, 2), Vec::<String>::new());
    }
}
