//! Forecast: up to five days with icon, precipitation chance, low, a cold-to-hot
//! range bar and high.

use embedded_graphics::Drawable;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::{DrawTarget, Point, Primitive, Size};
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle, RoundedRectangle};
use whether_core::view::{DailyVm, DayVm};

use crate::fonts::{self, Anchor};
use crate::icons::draw_icon;
use crate::layout::Ctx;

/// The day's range on the period's scale: the track, then a segment whose color
/// runs from cold to hot by position.
fn range_bar<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    ctx: &Ctx,
    day: &DayVm,
    track: Rectangle,
) -> Result<(), D::Error> {
    let h = track.size.height;
    RoundedRectangle::with_equal_corners(track, Size::new(h / 2, h / 2))
        .into_styled(PrimitiveStyle::with_fill(ctx.pal.track))
        .draw(d)?;
    let w = track.size.width as f32;
    let start = libm::floorf(day.range.0.clamp(0.0, 1.0) * w) as i32;
    let end = (libm::ceilf(day.range.1.clamp(0.0, 1.0) * w) as i32).max(start + h as i32);
    let end = end.min(track.size.width as i32);
    let r = h as i32 / 2;
    for x in start..end {
        // Round the segment's ends by shortening the outermost columns.
        let edge = (x - start).min(end - 1 - x);
        let inset = if edge < r {
            let dx = (r - edge) as f32 - 0.5;
            r - libm::roundf(libm::sqrtf((r * r) as f32 - dx * dx).max(0.0)) as i32
        } else {
            0
        };
        let color = ctx.pal.range(x as f32 / w.max(1.0));
        d.fill_solid(
            &Rectangle::new(
                Point::new(track.top_left.x + x, track.top_left.y + inset),
                Size::new(1, (h as i32 - 2 * inset).max(1).unsigned_abs()),
            ),
            color,
        )?;
    }
    Ok(())
}

/// Column positions shared by every row.
struct Columns {
    icon: u32,
    icon_x: i32,
    pop_x: i32,
    low_right: i32,
    high_right: i32,
    bar_x: i32,
    bar_w: i32,
    bar_h: u32,
    /// The bar sits on its own line under the row (narrow panels).
    under: bool,
}

impl Columns {
    fn new(ctx: &Ctx, days: &[DayVm], area: Rectangle) -> Self {
        let f = &ctx.fonts;
        let (_, icon) = ctx.class.icons();
        let gap = ctx.pad();
        let label_w = days
            .iter()
            .map(|day| fonts::width(&f.label, &day.label))
            .max()
            .unwrap_or(0) as i32;
        let pop_w = fonts::width(&f.small, "100%") as i32;
        let temp_w = fonts::width(&f.label, "-88°") as i32;
        let right = area.top_left.x + area.size.width as i32;
        let icon_x = area.top_left.x + label_w + gap;
        let pop_x = icon_x + icon as i32 + gap / 2;
        let low_right = pop_x + pop_w + gap / 2 + temp_w;
        let beside = right - temp_w - gap - (low_right + gap);
        // Narrow panels (320 px upright) put the bar on its own line under the row.
        let under = beside < area.size.width as i32 / 4;
        let (bar_x, bar_w) = if under {
            (icon_x, right - icon_x)
        } else {
            (low_right + gap, beside.max(8))
        };
        Self {
            icon,
            icon_x,
            pop_x,
            low_right,
            high_right: right,
            bar_x,
            bar_w,
            bar_h: if ctx.class == crate::fonts::SizeClass::Large {
                8
            } else {
                6
            },
            under,
        }
    }
}

/// One day centered on `row_mid`.
fn row<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    ctx: &Ctx,
    day: &DayVm,
    cols: &Columns,
    x0: i32,
    row_mid: i32,
) -> Result<(), D::Error> {
    let f = &ctx.fonts;
    let gap = ctx.pad();
    let shift = if cols.under {
        (cols.bar_h as i32).midpoint(gap)
    } else {
        0
    };
    let cy = row_mid - shift;
    ctx.text(
        d,
        &f.label,
        &day.label,
        Point::new(x0, cy),
        Anchor::LEFT,
        ctx.pal.text,
    )?;
    let icon = cols.icon;
    draw_icon(
        d,
        day.icon,
        Point::new(cols.icon_x, cy - icon as i32 / 2),
        icon,
        &ctx.pal,
    )?;
    if let Some(pop) = day.precip_probability.filter(|p| *p > 0) {
        let text = whether_core::view::format::chance(pop);
        ctx.text(
            d,
            &f.small,
            &text,
            Point::new(cols.pop_x, cy),
            Anchor::LEFT,
            ctx.pal.muted,
        )?;
    }
    ctx.text(
        d,
        &f.label,
        &day.low.text,
        Point::new(cols.low_right, cy),
        Anchor::RIGHT,
        ctx.pal.muted,
    )?;
    let bar_y = if cols.under {
        cy + icon as i32 / 2 + gap / 2
    } else {
        cy - cols.bar_h as i32 / 2
    };
    let track = Rectangle::new(
        Point::new(cols.bar_x, bar_y),
        Size::new(cols.bar_w.unsigned_abs(), cols.bar_h),
    );
    range_bar(d, ctx, day, track)?;
    ctx.text(
        d,
        &f.label,
        &day.high.text,
        Point::new(cols.high_right, cy),
        Anchor::RIGHT,
        ctx.pal.text,
    )
}

pub(crate) fn draw<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    ctx: &Ctx,
    daily: &DailyVm,
    area: Rectangle,
) -> Result<(), D::Error> {
    let (_, icon) = ctx.class.icons();
    let min_row = icon as i32 + ctx.pad() / 2;
    let fit_rows = (area.size.height as i32 / min_row).max(1);
    let rows = daily
        .days
        .len()
        .min(5)
        .min(usize::try_from(fit_rows).unwrap_or(1));
    if rows == 0 {
        return Ok(());
    }
    let row_h = area.size.height as i32 / i32::try_from(rows).unwrap_or(1);
    let days = &daily.days[..rows];
    let cols = Columns::new(ctx, days, area);
    for (i, day) in days.iter().enumerate() {
        let row_mid = area.top_left.y + row_h * i32::try_from(i).unwrap_or(0) + row_h / 2;
        row(d, ctx, day, &cols, area.top_left.x, row_mid)?;
    }
    Ok(())
}
