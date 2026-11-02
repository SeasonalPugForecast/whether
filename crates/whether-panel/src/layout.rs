//! Page frame: background, header, status line and the areas between them.

use alloc::string::String;

use embedded_graphics::Drawable;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::{DrawTarget, Point, Primitive, Size};
use embedded_graphics::primitives::{Circle, Line, PrimitiveStyle, Rectangle, RoundedRectangle};
use whether_core::view::ViewModel;

use crate::config::{PanelConfig, PanelUi};
use crate::fonts::{self, Anchor, Fonts, SizeClass};
use crate::theme::Palette;

/// Everything a page needs besides the view model.
#[derive(Clone, Debug)]
pub struct Ctx {
    /// The whole display.
    pub screen: Rectangle,
    /// Font and padding sizes.
    pub class: SizeClass,
    /// The fonts for `class`.
    pub fonts: Fonts,
    /// Colors for the current sky.
    pub pal: Palette,
    /// The status line needs two rows (narrow panels, long attributions).
    pub two_row_status: bool,
}

impl Ctx {
    /// The context for a display covering `screen` and showing `vm`.
    pub fn new(screen: Rectangle, vm: &ViewModel, ui: &PanelUi, cfg: &PanelConfig) -> Self {
        let class = SizeClass::for_size(screen.size);
        let fonts = Fonts::new(class);
        let pad = class.pad().unsigned_abs();
        let one_row = badges(*ui, vm)
            .iter()
            .map(|b| fonts::width(&fonts.small, b) + 12)
            .sum::<u32>()
            + fonts::width(&fonts.small, &status_text(vm))
            + fonts::width(&fonts.small, &vm.attribution)
            + 3 * pad;
        Self {
            screen,
            class,
            pal: Palette::new(vm.theme, cfg.dynamic_background),
            two_row_status: one_row > screen.size.width,
            fonts,
        }
    }

    /// Draws `s` with the sky's text shadow, if it has one.
    pub fn text<D: DrawTarget<Color = Rgb565>>(
        &self,
        d: &mut D,
        font: &u8g2_fonts::FontRenderer,
        s: &str,
        at: Point,
        anchor: Anchor,
        color: Rgb565,
    ) -> Result<(), D::Error> {
        if let Some(shadow) = self.pal.shadow {
            fonts::text(d, font, s, at + Point::new(1, 1), anchor, shadow)?;
        }
        fonts::text(d, font, s, at, anchor, color)
    }

    /// Padding in pixels.
    pub fn pad(&self) -> i32 {
        self.class.pad()
    }

    /// The header line at the top.
    pub fn header(&self) -> Rectangle {
        let h = fonts::line_height(&self.fonts.label) + self.pad().unsigned_abs();
        Rectangle::new(self.screen.top_left, Size::new(self.screen.size.width, h))
    }

    /// The status line at the bottom.
    pub fn status(&self) -> Rectangle {
        let rows = if self.two_row_status { 2 } else { 1 };
        let h = rows * fonts::line_height(&self.fonts.small) + self.pad().unsigned_abs();
        Rectangle::new(
            Point::new(
                self.screen.top_left.x,
                self.screen.top_left.y + (self.screen.size.height.saturating_sub(h)) as i32,
            ),
            Size::new(self.screen.size.width, h),
        )
    }

    /// The page body between header and status line, inside the side padding.
    pub fn body(&self) -> Rectangle {
        let top = self.header().bottom_right().map_or(0, |p| p.y + 1);
        let bottom = self.status().top_left.y;
        Rectangle::new(
            Point::new(self.screen.top_left.x + self.pad(), top),
            Size::new(
                self.screen
                    .size
                    .width
                    .saturating_sub(2 * self.pad().unsigned_abs()),
                (bottom - top).max(0).unsigned_abs(),
            ),
        )
    }
}

/// Paints the background of `area`, one line per row (no framebuffer needed).
pub fn background<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    ctx: &Ctx,
    area: Rectangle,
) -> Result<(), D::Error> {
    let Some(end) = area.bottom_right() else {
        return Ok(());
    };
    let height = ctx.screen.size.height;
    for y in area.top_left.y..=end.y {
        let color = ctx.pal.background(y - ctx.screen.top_left.y, height);
        d.fill_solid(
            &Rectangle::new(
                Point::new(area.top_left.x, y),
                Size::new(area.size.width, 1),
            ),
            color,
        )?;
    }
    Ok(())
}

/// Location name left, page dots centered, local time right.
pub fn header<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    ctx: &Ctx,
    vm: &ViewModel,
    ui: &PanelUi,
    cfg: &PanelConfig,
) -> Result<(), D::Error> {
    let area = ctx.header();
    let y = area.top_left.y + ctx.pad() * 2 / 3;
    let right = area.top_left.x + area.size.width as i32 - ctx.pad();
    let time = &vm.location.local_time;
    ctx.text(
        d,
        &ctx.fonts.label,
        time,
        Point::new(right, y),
        Anchor::TOP_RIGHT,
        ctx.pal.muted,
    )?;
    let room = area
        .size
        .width
        .saturating_sub(fonts::width(&ctx.fonts.label, time) + 3 * ctx.pad().unsigned_abs());
    let name = fonts::fit(&ctx.fonts.label, &vm.location.name, room);
    // Page dots in the gap between name and time, when there is one.
    let name_right = area.top_left.x + ctx.pad() + fonts::width(&ctx.fonts.label, &name) as i32;
    let time_left = right - fonts::width(&ctx.fonts.label, time) as i32;
    let dots = dots_width(ctx, cfg.pages.len());
    let center = area.center().x;
    if cfg.pages.len() > 1
        && center - dots / 2 > name_right + 8
        && center + dots / 2 < time_left - 8
    {
        let mid = y + fonts::cap_height(&ctx.fonts.label) as i32 / 2;
        let active = cfg.pages.iter().position(|p| *p == ui.page);
        page_dots(d, ctx, Point::new(center, mid), cfg.pages.len(), active)?;
    }
    ctx.text(
        d,
        &ctx.fonts.label,
        &name,
        Point::new(area.top_left.x + ctx.pad(), y),
        Anchor::TOP_LEFT,
        ctx.pal.text,
    )
}

/// A filled badge with `label`; returns its width.
fn badge<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    ctx: &Ctx,
    label: &str,
    left: Point,
) -> Result<i32, D::Error> {
    let font = &ctx.fonts.small;
    let w = fonts::width(font, label) + 8;
    let h = fonts::line_height(font) + 2;
    let rect = Rectangle::new(Point::new(left.x, left.y - h as i32 / 2), Size::new(w, h));
    RoundedRectangle::with_equal_corners(rect, Size::new(3, 3))
        .into_styled(PrimitiveStyle::with_fill(ctx.pal.warn))
        .draw(d)?;
    fonts::text(
        d,
        font,
        label,
        Point::new(left.x + 4, left.y),
        Anchor::LEFT,
        ctx.pal.warn_text,
    )?;
    Ok(w as i32)
}

/// One dot per configured page, the current one filled.
fn page_dots<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    ctx: &Ctx,
    center: Point,
    count: usize,
    active: Option<usize>,
) -> Result<(), D::Error> {
    let size = if ctx.class == SizeClass::Large { 7 } else { 5 };
    let step = size as i32 * 2;
    let count = i32::try_from(count).unwrap_or(0);
    let x0 = center.x - (count - 1) * step / 2;
    for i in 0..count {
        let style = if active == usize::try_from(i).ok() {
            PrimitiveStyle::with_fill(ctx.pal.text)
        } else {
            PrimitiveStyle::with_fill(ctx.pal.track)
        };
        Circle::with_center(Point::new(x0 + i * step, center.y), size)
            .into_styled(style)
            .draw(d)?;
    }
    Ok(())
}

/// The width of the page dots.
fn dots_width(ctx: &Ctx, count: usize) -> i32 {
    let size = if ctx.class == SizeClass::Large { 7 } else { 5 };
    i32::try_from(count).unwrap_or(0) * size * 2
}

/// The badges to show: TLS off, and stale data as the firmware or the view sees it.
/// Measuring the status line and drawing it both use this list.
fn badges(ui: PanelUi, vm: &ViewModel) -> alloc::vec::Vec<&'static str> {
    let mut out = alloc::vec::Vec::new();
    if ui.insecure_tls {
        out.push("TLS OFF");
    }
    if ui.stale || vm.status.stale {
        out.push("STALE");
    }
    out
}

/// "Updated …", or the error when there is no data at all.
fn status_text(vm: &ViewModel) -> String {
    match &vm.status.error {
        Some(error) if vm.current.is_none() => error.clone(),
        _ => vm.status.updated.clone(),
    }
}

/// Badges and "Updated …" left, the attribution right (always shown in full). On
/// narrow panels the attribution gets its own row.
pub fn status_line<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    ctx: &Ctx,
    vm: &ViewModel,
    ui: &PanelUi,
) -> Result<(), D::Error> {
    let area = ctx.status();
    background(d, ctx, area)?;
    let font = &ctx.fonts.small;
    let line = fonts::line_height(font) as i32;
    let left = area.top_left.x + ctx.pad();
    let right = area.top_left.x + area.size.width as i32 - ctx.pad();
    // A thin rule separates the status line from the page.
    Line::new(
        Point::new(left, area.top_left.y),
        Point::new(right, area.top_left.y),
    )
    .into_styled(PrimitiveStyle::with_stroke(ctx.pal.track, 1))
    .draw(d)?;
    let (first, last) = if ctx.two_row_status {
        let top = area.top_left.y + ctx.pad() / 2 + line / 2;
        (top, top + line)
    } else {
        let y = area.center().y;
        (y, y)
    };
    ctx.text(
        d,
        font,
        &vm.attribution,
        Point::new(right, last),
        Anchor::RIGHT,
        ctx.pal.text,
    )?;
    let limit = if ctx.two_row_status {
        right
    } else {
        right - fonts::width(font, &vm.attribution) as i32 - ctx.pad()
    };
    let mut x = left;
    for label in badges(*ui, vm) {
        x += badge(d, ctx, label, Point::new(x, first))? + 4;
    }
    let status = fonts::fit(font, &status_text(vm), (limit - x).max(0).unsigned_abs());
    ctx.text(
        d,
        font,
        &status,
        Point::new(x, first),
        Anchor::LEFT,
        ctx.pal.muted,
    )
}

/// A horizontal bar: the track, then `fraction` of it in `color`.
pub fn meter<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    ctx: &Ctx,
    area: Rectangle,
    fraction: f32,
    color: Rgb565,
) -> Result<(), D::Error> {
    let r = Size::new(area.size.height / 2, area.size.height / 2);
    RoundedRectangle::with_equal_corners(area, r)
        .into_styled(PrimitiveStyle::with_fill(ctx.pal.track))
        .draw(d)?;
    let filled = libm::roundf(area.size.width as f32 * fraction.clamp(0.0, 1.0)) as u32;
    if filled > 0 {
        let fill = Rectangle::new(
            area.top_left,
            Size::new(filled.max(area.size.height), area.size.height),
        );
        RoundedRectangle::with_equal_corners(fill, r)
            .into_styled(PrimitiveStyle::with_fill(color))
            .draw(d)?;
    }
    Ok(())
}
