#![no_std]
//! Panel display: Mousam-style pages drawn straight to any embedded-graphics
//! `DrawTarget<Color = Rgb565>`, without a framebuffer or bitmaps. It fits
//! an ESP32 without PSRAM; `whether-sim` renders the same pages to PNG.

#![allow(
    clippy::cast_possible_wrap,
    reason = "pixel coordinates and sizes are bounded by the panel (a few hundred pixels)"
)]
#![allow(
    clippy::many_single_char_names,
    reason = "geometry reads best with x, y, r, w, h and the draw target d"
)]

extern crate alloc;

pub mod config;
pub mod fonts;
pub mod icons;
pub mod layout;
mod pages;
pub mod theme;

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::DrawTarget;
use whether_core::view::ViewModel;

pub use config::{PanelConfig, PanelPage, PanelUi};
pub use icons::draw_icon;
pub use theme::Palette;

use layout::Ctx;

/// Draws `ui.page` of `vm` over the whole display, status line included.
pub fn draw_page<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    vm: &ViewModel,
    ui: &PanelUi,
    cfg: &PanelConfig,
) -> Result<(), D::Error> {
    let ctx = Ctx::new(d.bounding_box(), vm, ui, cfg);
    layout::background(d, &ctx, ctx.screen)?;
    layout::header(d, &ctx, vm, ui, cfg)?;
    let body = ctx.body();
    match &vm.current {
        None => pages::no_data(d, &ctx, vm, body)?,
        Some(current) => match ui.page {
            PanelPage::Now => pages::now::draw(d, &ctx, vm, current, body)?,
            PanelPage::Forecast => match &vm.daily {
                Some(daily) => pages::forecast::draw(d, &ctx, daily, body)?,
                None => pages::no_data(d, &ctx, vm, body)?,
            },
            PanelPage::Details => pages::details::draw(d, &ctx, vm, body)?,
        },
    }
    layout::status_line(d, &ctx, vm, ui)
}

/// Redraws only the status line (its background included), for the once-a-minute
/// "Updated …" refresh without repainting the page.
pub fn draw_status_line<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    vm: &ViewModel,
    ui: &PanelUi,
    cfg: &PanelConfig,
) -> Result<(), D::Error> {
    let ctx = Ctx::new(d.bounding_box(), vm, ui, cfg);
    layout::status_line(d, &ctx, vm, ui)
}

#[cfg(test)]
mod tests;
