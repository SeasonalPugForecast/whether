//! Now: icon, large temperature, condition, feels like and high/low, then active
//! alerts or else the summary when there is room, and six hourly columns.

use alloc::format;

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::{DrawTarget, Point, Size};
use embedded_graphics::primitives::Rectangle;
use whether_core::view::{CurrentVm, ViewModel};

use super::{paragraph, wrap};
use crate::fonts::{self, Anchor};
use crate::icons::draw_icon;
use crate::layout::{Ctx, meter};

/// Draws the hero block; returns the y below it.
fn hero<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    ctx: &Ctx,
    c: &CurrentVm,
    area: Rectangle,
) -> Result<i32, D::Error> {
    let f = &ctx.fonts;
    let (size, _) = ctx.class.icons();
    // A little air below the header.
    let top = area.top_left + Point::new(0, ctx.pad() / 2);
    let right = top.x + area.size.width as i32;
    draw_icon(d, c.icon, top, size, &ctx.pal)?;
    let mid = top.y + size as i32 / 2;
    let temp_x = top.x + size as i32 + ctx.pad();
    ctx.text(
        d,
        &f.numerals,
        &c.temperature.text,
        Point::new(temp_x, mid),
        Anchor::LEFT,
        ctx.pal.text,
    )?;
    let temp_right = temp_x + fonts::width(&f.numerals, &c.temperature.text) as i32 + ctx.pad();

    let feels = c.feels_like_label.as_str();
    let range = format!("H {}  L {}", c.high.text, c.low.text);
    let lines = [
        (&f.title, c.condition_label.as_str(), ctx.pal.text),
        (&f.label, feels, ctx.pal.muted),
        (&f.label, range.as_str(), ctx.pal.text),
    ];
    let needed = lines
        .iter()
        .map(|(font, s, _)| fonts::width(font, s))
        .max()
        .unwrap_or(0);
    let heights: u32 = lines
        .iter()
        .map(|(font, _, _)| fonts::line_height(font))
        .sum();
    let beside = (right - temp_right).max(0).unsigned_abs() >= needed.min(area.size.width / 2);
    let (x, mut y, anchor, room) = if beside {
        // Right-aligned beside the temperature, centered on the icon.
        let room = (right - temp_right).max(0).unsigned_abs();
        (right, mid - heights as i32 / 2, Anchor::TOP_RIGHT, room)
    } else {
        // Stacked under the icon on narrow panels.
        (
            top.x,
            top.y + size as i32 + ctx.pad() / 2,
            Anchor::TOP_LEFT,
            area.size.width,
        )
    };
    for (font, s, color) in lines {
        let s = fonts::fit(font, s, room);
        ctx.text(d, font, &s, Point::new(x, y), anchor, color)?;
        y += fonts::line_height(font) as i32;
    }
    Ok(if beside {
        y.max(top.y + size as i32)
    } else {
        y
    })
}

/// Six hourly columns along the bottom of `area`; returns their top.
fn hours<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    ctx: &Ctx,
    vm: &ViewModel,
    area: Rectangle,
) -> Result<i32, D::Error> {
    let bottom = area.top_left.y + area.size.height as i32;
    let Some(h) = &vm.hourly else {
        return Ok(bottom);
    };
    let f = &ctx.fonts;
    let (_, icon) = ctx.class.icons();
    let gap = ctx.pad() / 3;
    let bar_h = if ctx.class == crate::fonts::SizeClass::Large {
        5
    } else {
        4
    };
    let height = fonts::line_height(&f.small) as i32
        + icon as i32
        + fonts::line_height(&f.label) as i32
        + bar_h
        + 3 * gap;
    let top = bottom - height;
    let max_cols = (area.size.width / (icon + 16)).max(1) as usize;
    let cols = 6.min(max_cols);
    let col_w = area.size.width as i32 / i32::try_from(cols).unwrap_or(1);
    for (i, hour) in h.hours.iter().skip(h.now_index).take(cols).enumerate() {
        let cx = area.top_left.x + col_w * i32::try_from(i).unwrap_or(0) + col_w / 2;
        let mut y = top;
        ctx.text(
            d,
            &f.small,
            &hour.label,
            Point::new(cx, y),
            Anchor::TOP_CENTER,
            ctx.pal.muted,
        )?;
        y += fonts::line_height(&f.small) as i32 + gap;
        draw_icon(
            d,
            hour.icon,
            Point::new(cx - icon as i32 / 2, y),
            icon,
            &ctx.pal,
        )?;
        y += icon as i32 + gap;
        ctx.text(
            d,
            &f.label,
            &hour.temperature.text,
            Point::new(cx, y),
            Anchor::TOP_CENTER,
            ctx.pal.text,
        )?;
        y += fonts::line_height(&f.label) as i32 + gap;
        if let Some(pop) = hour.precip_probability {
            let w = (col_w - 2 * ctx.pad()).max(8).unsigned_abs();
            let bar = Rectangle::new(
                Point::new(cx - w as i32 / 2, y),
                Size::new(w, bar_h.unsigned_abs()),
            );
            meter(d, ctx, bar, f32::from(pop) / 100.0, ctx.pal.rain)?;
        }
    }
    Ok(top)
}

pub(crate) fn draw<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    ctx: &Ctx,
    vm: &ViewModel,
    c: &CurrentVm,
    area: Rectangle,
) -> Result<(), D::Error> {
    let below_hero = hero(d, ctx, c, area)?;
    let strip_top = hours(d, ctx, vm, area)?;
    // Alerts take the space between; otherwise the summary, when a line fits.
    if !vm.alerts.is_empty() {
        let top = below_hero + ctx.pad() / 2;
        super::alerts::draw(d, ctx, &vm.alerts, area, top, strip_top - ctx.pad() / 2)?;
    } else if let Some(summary) = &vm.summary {
        let font = &ctx.fonts.label;
        let line = fonts::line_height(font) as i32;
        let top = below_hero + ctx.pad() / 2;
        let lines = ((strip_top - ctx.pad() / 2 - top) / line).max(0);
        if lines > 0 {
            let wrapped = wrap(
                font,
                summary,
                area.size.width,
                usize::try_from(lines).unwrap_or(0),
            );
            paragraph(
                d,
                ctx,
                font,
                &wrapped,
                Point::new(area.top_left.x, top),
                ctx.pal.muted,
            )?;
        }
    }
    Ok(())
}
