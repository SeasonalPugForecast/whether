//! The device panel GUI: `whether_panel::draw_page`, as the firmware runs it.

use anyhow::bail;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::Size;
use embedded_graphics_simulator::SimulatorDisplay;
use whether_core::view::ViewModel;
use whether_panel::{PanelConfig, PanelPage, PanelUi, draw_page};

use crate::Page;
use crate::board::Board;

/// The panel page behind each sim page.
pub fn page(page: Page) -> anyhow::Result<PanelPage> {
    Ok(match page {
        Page::Now => PanelPage::Now,
        Page::Daily => PanelPage::Forecast,
        Page::Details => PanelPage::Details,
        Page::Hourly => bail!("the panel has no hourly page; the next hours are on `now`"),
    })
}

/// Draws `page` of `vm` on a display shaped like `board`, turned upright with
/// `portrait`.
pub fn render(
    board: &Board,
    vm: &ViewModel,
    page: PanelPage,
    portrait: bool,
) -> SimulatorDisplay<Rgb565> {
    let (w, h) = if portrait {
        (board.height, board.width)
    } else {
        (board.width, board.height)
    };
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(w, h));
    // The simulator display's error type is `Infallible`.
    let Ok(()) = draw_page(
        &mut display,
        vm,
        &PanelUi::new(page),
        &PanelConfig::default(),
    );
    display
}
