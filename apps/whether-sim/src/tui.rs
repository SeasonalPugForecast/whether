//! The device TUI: `whether_tui::draw` through mousefood, as the firmware runs it.

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::Size;
use embedded_graphics_simulator::SimulatorDisplay;
use mousefood::prelude::{EmbeddedBackend, EmbeddedBackendConfig};
use ratatui::Terminal;
use whether_core::view::ViewModel;
use whether_tui::config::{IconStyle, LayoutPreset};
use whether_tui::{ColorDepth, TuiConfig, UiState, View};

use crate::Page;
use crate::board::{Board, font};

/// The firmware's `[display.tui]`: the compact preset, and ASCII icons because
/// the embedded fonts have no icon glyphs.
pub fn device_config() -> TuiConfig {
    TuiConfig {
        layout: LayoutPreset::Compact,
        icons: IconStyle::Ascii,
        ..TuiConfig::default()
    }
}

/// The TUI view behind each page.
fn view(page: Page) -> View {
    match page {
        Page::Now => View::Overview,
        Page::Hourly => View::Hourly,
        Page::Daily => View::Daily,
        Page::Details => View::Air,
    }
}

/// Draws `page` of `vm` on a display shaped like `board`.
pub fn render(
    board: &Board,
    vm: &ViewModel,
    page: Page,
) -> anyhow::Result<SimulatorDisplay<Rgb565>> {
    let cfg = device_config();
    let mut ui = UiState::new(&cfg, 0, ColorDepth::TrueColor);
    ui.view = view(page);
    ui.key_hints = false;
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(board.width, board.height));
    {
        let config = EmbeddedBackendConfig {
            font_regular: font(&board.tui_font)?,
            ..EmbeddedBackendConfig::default()
        };
        let backend = EmbeddedBackend::new(&mut display, config);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|frame| whether_tui::draw(frame, vm, &ui, &cfg))?;
    }
    Ok(display)
}
