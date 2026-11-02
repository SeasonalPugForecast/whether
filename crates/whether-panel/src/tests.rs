//! Renders into an in-memory framebuffer (pixel-exact goldens live in whether-sim).

extern crate std;

use alloc::vec;
use alloc::vec::Vec;
use core::convert::Infallible;

use embedded_graphics::Pixel;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::{Dimensions, DrawTarget, OriginDimensions, Point, Size};
use whether_core::icon::IconId;
use whether_core::presentation::PresentationConfig;
use whether_core::strings::Strings;
use whether_core::testdata;
use whether_core::theme::SkyTheme;
use whether_core::view::{LocationView, StatusInfo, ViewInput, ViewModel, build_view};

use super::*;

/// A framebuffer that records writes outside its area instead of dropping them.
struct Fb {
    size: Size,
    pixels: Vec<Option<Rgb565>>,
    outside: usize,
}

impl Fb {
    fn new(w: u32, h: u32) -> Self {
        Self {
            size: Size::new(w, h),
            pixels: vec![None; (w * h) as usize],
            outside: 0,
        }
    }

    fn get(&self, x: i32, y: i32) -> Option<Rgb565> {
        self.pixels[(y as u32 * self.size.width + x as u32) as usize]
    }

    fn rows_written(&self) -> Vec<i32> {
        (0..self.size.height as i32)
            .filter(|&y| (0..self.size.width as i32).any(|x| self.get(x, y).is_some()))
            .collect()
    }
}

impl OriginDimensions for Fb {
    fn size(&self) -> Size {
        self.size
    }
}

impl DrawTarget for Fb {
    type Color = Rgb565;
    type Error = Infallible;

    fn draw_iter<I: IntoIterator<Item = Pixel<Rgb565>>>(
        &mut self,
        pixels: I,
    ) -> Result<(), Infallible> {
        for Pixel(p, c) in pixels {
            if p.x >= 0
                && p.y >= 0
                && (p.x as u32) < self.size.width
                && (p.y as u32) < self.size.height
            {
                let i = (p.y as u32 * self.size.width + p.x as u32) as usize;
                self.pixels[i] = Some(c);
            } else {
                self.outside += 1;
            }
        }
        Ok(())
    }
}

fn sample() -> ViewModel {
    testdata::sample_view()
}

#[test]
fn icons_stay_inside_their_box_at_every_size() {
    let pal = Palette::new(SkyTheme::ClearDay, true);
    for &icon in IconId::ALL {
        for size in [24, 32, 64] {
            let mut fb = Fb::new(size + 8, size + 8);
            draw_icon(&mut fb, icon, Point::new(4, 4), size, &pal).unwrap();
            let drawn: Vec<(i32, i32)> = (0..fb.size.height as i32)
                .flat_map(|y| (0..fb.size.width as i32).map(move |x| (x, y)))
                .filter(|&(x, y)| fb.get(x, y).is_some())
                .collect();
            assert!(
                drawn.len() > (size * size / 20) as usize,
                "{icon:?} at {size}: too few pixels"
            );
            // One pixel of slack for anti-aliasing-free rounding at the edges.
            let inside = |v: i32| (3..=(size as i32 + 4)).contains(&v);
            assert!(
                fb.outside == 0 && drawn.iter().all(|&(x, y)| inside(x) && inside(y)),
                "{icon:?} at {size} leaves its box"
            );
        }
    }
}

#[test]
fn every_page_covers_the_screen_at_every_size() {
    let vm = sample();
    let cfg = PanelConfig::default();
    for (w, h) in [(480, 320), (320, 480), (320, 240)] {
        for page in [PanelPage::Now, PanelPage::Forecast, PanelPage::Details] {
            let mut fb = Fb::new(w, h);
            draw_page(&mut fb, &vm, &PanelUi::new(page), &cfg).unwrap();
            assert_eq!(fb.outside, 0, "{page:?} at {w}×{h} draws off screen");
            assert!(
                fb.pixels.iter().all(Option::is_some),
                "{page:?} at {w}×{h} leaves gaps"
            );
        }
    }
}

#[test]
fn the_status_line_redraws_only_its_rows() {
    let vm = sample();
    let cfg = PanelConfig::default();
    let mut fb = Fb::new(480, 320);
    let ui = PanelUi {
        stale: true,
        insecure_tls: true,
        ..PanelUi::new(PanelPage::Now)
    };
    draw_status_line(&mut fb, &vm, &ui, &cfg).unwrap();
    let rows = fb.rows_written();
    let ctx = layout::Ctx::new(fb.bounding_box(), &vm, &ui, &cfg);
    assert!(
        ctx.two_row_status,
        "two badges push the attribution to its own row"
    );
    assert_eq!(*rows.first().unwrap(), ctx.status().top_left.y);
    assert_eq!(*rows.last().unwrap(), 319);
    assert!(ctx.status().top_left.y > 260);
}

#[test]
fn without_data_the_page_explains_why() {
    let status = StatusInfo {
        last_success: None,
        error: Some("every provider failed".into()),
        stale: false,
        refreshing: false,
    };
    let locations = [LocationView {
        name: "New York",
        snapshot: None,
        status: &status,
        history: None,
    }];
    let vm = build_view(&ViewInput {
        locations: &locations,
        active: 0,
        presentation: &PresentationConfig::default(),
        strings: &Strings::default(),
        now: testdata::SAMPLE_NOW,
    });
    let mut fb = Fb::new(320, 240);
    draw_page(
        &mut fb,
        &vm,
        &PanelUi::new(PanelPage::Now),
        &PanelConfig::default(),
    )
    .unwrap();
    assert_eq!(fb.outside, 0);
}

/// The view's stale flag adds a badge, which the status line's measurement counts
/// too: on a 320 px panel the attribution then moves to its own row instead of
/// being drawn over.
#[test]
fn a_stale_view_is_measured_like_it_is_drawn() {
    let mut vm = sample();
    let ui = PanelUi {
        insecure_tls: true,
        ..PanelUi::new(PanelPage::Now)
    };
    let cfg = PanelConfig::default();
    let screen = embedded_graphics::primitives::Rectangle::new(Point::zero(), Size::new(320, 240));
    assert!(!layout::Ctx::new(screen, &vm, &ui, &cfg).two_row_status);
    vm.status.stale = true;
    assert!(layout::Ctx::new(screen, &vm, &ui, &cfg).two_row_status);
}

/// Alerts replace the summary: both banners fit on every panel (one-line banners
/// where two-line ones would not), inside the screen and in their levels' colors.
#[test]
fn alerts_show_as_banners_at_every_size() {
    let vm = testdata::sample_view_with_alerts();
    let cfg = PanelConfig::default();
    let red = Palette::new(vm.theme, true).level(whether_core::scales::Level::VeryPoor);
    let yellow = Palette::new(vm.theme, true).level(whether_core::scales::Level::Moderate);
    for (w, h) in [(480, 320), (320, 480), (320, 240)] {
        let mut fb = Fb::new(w, h);
        draw_page(&mut fb, &vm, &PanelUi::new(PanelPage::Now), &cfg).unwrap();
        assert_eq!(fb.outside, 0, "alerts at {w}×{h} draw off screen");
        let count = |c| fb.pixels.iter().filter(|p| **p == Some(c)).count();
        assert!(count(red) > 1_000, "no flood warning banner at {w}×{h}");
        assert!(count(yellow) > 1_000, "no wind advisory banner at {w}×{h}");
    }
}

/// The pressure arrow points up for rising, down for falling, right for steady.
#[test]
fn trend_arrows_point_the_right_way() {
    use whether_core::derive::Trend;
    let vm = sample();
    let ui = PanelUi::new(PanelPage::Details);
    let cfg = PanelConfig::default();
    let screen = embedded_graphics::primitives::Rectangle::new(Point::zero(), Size::new(480, 320));
    let ctx = layout::Ctx::new(screen, &vm, &ui, &cfg);
    let filled = |trend| {
        let mut fb = Fb::new(16, 16);
        crate::pages::details::trend_arrow(&mut fb, &ctx, Point::zero(), 16, trend).unwrap();
        // Top-left, top-right and bottom-left corners.
        [fb.get(2, 2), fb.get(13, 2), fb.get(2, 13)].map(|p| p.is_some())
    };
    assert_eq!(filled(Trend::Rising), [false, false, true]);
    assert_eq!(filled(Trend::Falling), [true, true, false]);
    assert_eq!(filled(Trend::Steady), [true, false, true]);
}
