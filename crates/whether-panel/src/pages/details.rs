//! Details: 2×2 tiles for wind (with a direction arrow), humidity, UV (in its
//! level color) and pressure; other details fill in when one is missing.

use alloc::vec::Vec;

use embedded_graphics::Drawable;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::{DrawTarget, Point, Primitive, Size};
use embedded_graphics::primitives::{
    Circle, Line, PrimitiveStyle, Rectangle, RoundedRectangle, Triangle,
};
use whether_core::derive::Trend;
use whether_core::presentation::DetailKind;
use whether_core::view::{DetailVm, ViewModel};

use crate::fonts::{self, Anchor};
use crate::layout::{Ctx, meter};
use crate::theme::rgb;

const PREFERRED: [DetailKind; 4] = [
    DetailKind::Wind,
    DetailKind::Humidity,
    DetailKind::Uv,
    DetailKind::Pressure,
];

/// The four tiles: the preferred kinds that exist, then other details in order.
fn pick(vm: &ViewModel) -> Vec<&DetailVm> {
    let mut tiles: Vec<&DetailVm> = PREFERRED
        .iter()
        .filter_map(|k| vm.details.iter().find(|d| d.kind == *k))
        .collect();
    for detail in &vm.details {
        if tiles.len() == 4 {
            break;
        }
        if !tiles.iter().any(|t| t.kind == detail.kind) {
            tiles.push(detail);
        }
    }
    tiles
}

/// A compass ring with an arrow pointing where the wind blows to.
fn wind_arrow<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    ctx: &Ctx,
    center: Point,
    radius: i32,
    from_deg: f32,
) -> Result<(), D::Error> {
    Circle::with_center(center, (2 * radius).unsigned_abs())
        .into_styled(PrimitiveStyle::with_stroke(ctx.pal.track, 2))
        .draw(d)?;
    // Wind direction is where it comes from; the arrow shows where it goes.
    let a = (from_deg + 180.0).to_radians();
    let (s, c) = (libm::sinf(a), -libm::cosf(a));
    let r = radius as f32 * 0.8;
    let at = |dist: f32, side: f32| {
        Point::new(
            center.x + libm::roundf(s * dist + c * side) as i32,
            center.y + libm::roundf(c * dist - s * side) as i32,
        )
    };
    let color = ctx.pal.text;
    Line::new(at(-r, 0.0), at(r * 0.3, 0.0))
        .into_styled(PrimitiveStyle::with_stroke(color, 3))
        .draw(d)?;
    Triangle::new(at(r, 0.0), at(r * 0.25, r * 0.4), at(r * 0.25, -r * 0.4))
        .into_styled(PrimitiveStyle::with_fill(color))
        .draw(d)
}

/// A filled triangle in a `size` square at `top_left`: up for rising, down for
/// falling, right for steady.
pub(crate) fn trend_arrow<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    ctx: &Ctx,
    top_left: Point,
    size: i32,
    trend: Trend,
) -> Result<(), D::Error> {
    let p = |fx: i32, fy: i32| Point::new(top_left.x + fx * size / 4, top_left.y + fy * size / 4);
    let (a, b, c) = match trend {
        Trend::Rising => (p(2, 0), p(0, 4), p(4, 4)),
        Trend::Falling => (p(0, 0), p(4, 0), p(2, 4)),
        Trend::Steady => (p(0, 0), p(4, 2), p(0, 4)),
    };
    Triangle::new(a, b, c)
        .into_styled(PrimitiveStyle::with_fill(ctx.pal.text))
        .draw(d)
}

fn tile<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    ctx: &Ctx,
    detail: &DetailVm,
    rect: Rectangle,
) -> Result<(), D::Error> {
    let f = &ctx.fonts;
    RoundedRectangle::with_equal_corners(rect, Size::new(8, 8))
        .into_styled(PrimitiveStyle::with_fill(ctx.pal.tile))
        .draw(d)?;
    let pad = ctx.pad();
    let x = rect.top_left.x + pad;
    let inner_w = rect.size.width.saturating_sub(2 * pad.unsigned_abs());
    let radius = (rect.size.height as i32 / 4).min(rect.size.width as i32 / 6);
    let beside_arrow = inner_w.saturating_sub((2 * radius + pad).unsigned_abs());
    // The arrow gives way when the value would not fit beside it (the value names
    // the direction too).
    let arrow = detail
        .direction_deg
        .filter(|_| detail.kind == DetailKind::Wind)
        .filter(|_| fonts::width(&f.label, &detail.value) <= beside_arrow);
    let text_w = if arrow.is_some() {
        beside_arrow
    } else {
        inner_w
    };
    let mut y = rect.top_left.y + pad * 2 / 3;
    let title = fonts::fit(&f.small, &detail.title, text_w);
    ctx.text(
        d,
        &f.small,
        &title,
        Point::new(x, y),
        Anchor::TOP_LEFT,
        ctx.pal.muted,
    )?;
    y += fonts::line_height(&f.small) as i32 + pad / 3;
    // Long values ("13 km/h NE") drop to the label font before being shortened.
    let font = if fonts::width(&f.title, &detail.value) <= text_w {
        &f.title
    } else {
        &f.label
    };
    let value = fonts::fit(font, &detail.value, text_w);
    let value_y = y + (fonts::cap_height(&f.title) - fonts::cap_height(font)) as i32;
    ctx.text(
        d,
        font,
        &value,
        Point::new(x, value_y),
        Anchor::TOP_LEFT,
        ctx.pal.text,
    )?;
    y += fonts::line_height(&f.title) as i32;
    if let Some(caption) = &detail.caption {
        // A trend arrow leads the caption ("▲ Rising").
        let mut cx = x;
        if let Some(trend) = detail.trend {
            let size = fonts::cap_height(&f.small).max(6) as i32;
            trend_arrow(d, ctx, Point::new(x, y), size, trend)?;
            cx += size + pad / 2;
        }
        let room = text_w.saturating_sub((cx - x).unsigned_abs());
        let caption = fonts::fit(&f.small, caption, room);
        ctx.text(
            d,
            &f.small,
            &caption,
            Point::new(cx, y),
            Anchor::TOP_LEFT,
            ctx.pal.muted,
        )?;
    }
    if let Some(deg) = arrow {
        let center = Point::new(
            rect.top_left.x + rect.size.width as i32 - pad - radius,
            rect.center().y,
        );
        wind_arrow(d, ctx, center, radius, deg)?;
    } else if let Some(gauge) = detail.gauge {
        let h: u32 = if ctx.class == crate::fonts::SizeClass::Large {
            6
        } else {
            4
        };
        let bar = Rectangle::new(
            Point::new(
                x,
                rect.top_left.y + rect.size.height as i32 - pad - h as i32,
            ),
            Size::new(inner_w, h),
        );
        let color = detail.level.map_or(rgb(ctx.pal.cold), |l| ctx.pal.level(l));
        meter(d, ctx, bar, gauge, color)?;
    }
    Ok(())
}

pub(crate) fn draw<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    ctx: &Ctx,
    vm: &ViewModel,
    area: Rectangle,
) -> Result<(), D::Error> {
    let gap = ctx.pad() / 2;
    let w = (area.size.width as i32 - gap) / 2;
    let h = (area.size.height as i32 - gap) / 2;
    for (i, detail) in pick(vm).into_iter().enumerate() {
        let (col, row) = (
            i32::try_from(i % 2).unwrap_or(0),
            i32::try_from(i / 2).unwrap_or(0),
        );
        let rect = Rectangle::new(
            Point::new(
                area.top_left.x + col * (w + gap),
                area.top_left.y + row * (h + gap),
            ),
            Size::new(w.max(1).unsigned_abs(), h.max(1).unsigned_abs()),
        );
        tile(d, ctx, detail, rect)?;
    }
    Ok(())
}
