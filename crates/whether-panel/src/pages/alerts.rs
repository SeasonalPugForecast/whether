//! Alerts on the Now page: one banner per alert in its level color (warning sign,
//! title, period), followed by as much of its text as the room allows. When one-line
//! banners show more alerts than two-line ones, the period moves beside the title.

use embedded_graphics::Drawable;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::{DrawTarget, Point, Primitive, Size};
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle, RoundedRectangle, Triangle};
use u8g2_fonts::FontRenderer;
use whether_core::view::AlertVm;

use super::{paragraph, wrap};
use crate::fonts::{self, Anchor};
use crate::layout::Ctx;

/// Lines of an alert's text shown at most under its banner.
const MAX_BODY_LINES: i32 = 3;

/// The glyphs' extent, ascent to descent: banners are sized by it rather than by the
/// fonts' roomier line height.
fn text_height(font: &FontRenderer) -> i32 {
    i32::from(font.get_ascent()) - i32::from(font.get_descent())
}

/// Space above and below a banner's text.
fn inset_v(ctx: &Ctx) -> i32 {
    ctx.pad() / 3
}

/// The height of one banner, with the period below the title (`two_line`) or not.
fn banner_height(ctx: &Ctx, two_line: bool) -> i32 {
    let period = if two_line {
        text_height(&ctx.fonts.small) + 2
    } else {
        0
    };
    text_height(&ctx.fonts.label) + period + 2 * inset_v(ctx)
}

/// A warning triangle with an exclamation mark, `size` pixels wide, its top-left at
/// `at`.
fn warning_sign<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    at: Point,
    size: i32,
    sign: Rgb565,
    mark: Rgb565,
) -> Result<(), D::Error> {
    let h = size * 7 / 8;
    Triangle::new(
        Point::new(at.x + size / 2, at.y),
        Point::new(at.x, at.y + h),
        Point::new(at.x + size, at.y + h),
    )
    .into_styled(PrimitiveStyle::with_fill(sign))
    .draw(d)?;
    let w = (size / 6).max(2).unsigned_abs();
    let x = at.x + size / 2 - w as i32 / 2;
    d.fill_solid(
        &Rectangle::new(
            Point::new(x, at.y + h * 3 / 8),
            Size::new(w, (h * 5 / 16).max(2).unsigned_abs()),
        ),
        mark,
    )?;
    d.fill_solid(
        &Rectangle::new(Point::new(x, at.y + h * 3 / 4), Size::new(w, w)),
        mark,
    )
}

/// One banner across `area`'s width at `top`.
fn banner<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    ctx: &Ctx,
    alert: &AlertVm,
    area: Rectangle,
    top: i32,
    two_line: bool,
) -> Result<(), D::Error> {
    let f = &ctx.fonts;
    let fill = ctx.pal.level(alert.level);
    let ink = ctx.pal.on_level(alert.level);
    let rect = Rectangle::new(
        Point::new(area.top_left.x, top),
        Size::new(area.size.width, banner_height(ctx, two_line).unsigned_abs()),
    );
    RoundedRectangle::with_equal_corners(rect, Size::new(4, 4))
        .into_styled(PrimitiveStyle::with_fill(fill))
        .draw(d)?;
    let inset = ctx.pad() / 2;
    let y = top + inset_v(ctx);
    let sign = fonts::cap_height(&f.label) as i32 + 2;
    warning_sign(d, Point::new(rect.top_left.x + inset, y), sign, ink, fill)?;
    let x = rect.top_left.x + inset + sign + inset;
    let right = rect.top_left.x + rect.size.width as i32 - inset;
    let room = (right - x).max(0).unsigned_abs();
    // Banner text is drawn without the sky's shadow: the fill is opaque.
    if two_line {
        let title = fonts::fit(&f.label, &alert.title, room);
        fonts::text(d, &f.label, &title, Point::new(x, y), Anchor::TOP_LEFT, ink)?;
        let period = fonts::fit(&f.small, &alert.period, room);
        let below = y + text_height(&f.label) + 2;
        return fonts::text(
            d,
            &f.small,
            &period,
            Point::new(x, below),
            Anchor::TOP_LEFT,
            ink,
        );
    }
    // One line: the period on the right, sharing the title's top, when it takes at
    // most half the room.
    let period = fonts::width(&f.small, &alert.period);
    let title_room = if period <= room / 2 {
        let baseline_gap = i32::from(f.label.get_ascent()) - i32::from(f.small.get_ascent());
        fonts::text(
            d,
            &f.small,
            &alert.period,
            Point::new(right, y + baseline_gap),
            Anchor::TOP_RIGHT,
            ink,
        )?;
        room - period - inset.unsigned_abs()
    } else {
        room
    };
    let title = fonts::fit(&f.label, &alert.title, title_room);
    fonts::text(d, &f.label, &title, Point::new(x, y), Anchor::TOP_LEFT, ink)
}

/// Draws the alerts that fit between `top` and `bottom` inside `area`'s width.
/// Banners come first: an alert's text only gets the room left once every later
/// alert that fits has its banner.
pub(crate) fn draw<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    ctx: &Ctx,
    alerts: &[AlertVm],
    area: Rectangle,
    top: i32,
    bottom: i32,
) -> Result<(), D::Error> {
    let gap = ctx.pad() / 3;
    let fitting = |two_line| {
        let n = (bottom - top + gap) / (banner_height(ctx, two_line) + gap);
        usize::try_from(n).unwrap_or(0).min(alerts.len())
    };
    let two_line = fitting(true) >= fitting(false);
    let height = banner_height(ctx, two_line);
    let shown = &alerts[..fitting(two_line)];
    let font = &ctx.fonts.small;
    let line = fonts::line_height(font) as i32;
    let mut y = top;
    for (i, alert) in shown.iter().enumerate() {
        banner(d, ctx, alert, area, y, two_line)?;
        y += height + gap;
        let later = i32::try_from(shown.len() - i - 1).unwrap_or(0) * (height + gap);
        let lines = ((bottom - y - later) / line).clamp(0, MAX_BODY_LINES);
        if two_line && lines > 0 {
            let wrapped = wrap(
                font,
                &alert.body,
                area.size.width,
                usize::try_from(lines).unwrap_or(0),
            );
            y = paragraph(
                d,
                ctx,
                font,
                &wrapped,
                Point::new(area.top_left.x, y),
                ctx.pal.text,
            )? + gap;
        }
    }
    Ok(())
}
