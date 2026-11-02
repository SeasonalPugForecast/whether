//! The panels. Each draws into a buffer area from the view model; none formats
//! numbers (that is `whether-core::view`'s job).

pub(crate) mod air;
pub(crate) mod chrome;
pub(crate) mod current;
pub(crate) mod daily;
pub(crate) mod details;
pub(crate) mod history;
pub(crate) mod hourly;
pub(crate) mod precip;
pub(crate) mod radar;
pub(crate) mod sky;
pub(crate) mod text;

use alloc::string::{String, ToString};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Widget};
use whether_core::view::ViewModel;

use crate::config::TuiConfig;
use crate::theme::Palette;
use crate::ui_state::UiState;

/// Shown where a value is unknown.
pub(crate) const MISSING: &str = "-";

/// Everything a panel needs.
pub(crate) struct Ctx<'a> {
    pub vm: &'a ViewModel,
    pub ui: &'a UiState,
    pub cfg: &'a TuiConfig,
    pub pal: Palette,
    /// Radar pixels, when the host has them.
    pub radar: Option<crate::RadarImage<'a>>,
}

impl Ctx<'_> {
    /// ASCII-only mode (`icons = "ascii"`): no symbols that need special fonts.
    pub(crate) fn ascii(&self) -> bool {
        self.cfg.icons == crate::config::IconStyle::Ascii
    }

    /// The warning sign.
    pub(crate) fn warn(&self) -> &'static str {
        if self.ascii() { "!" } else { "⚠" }
    }

    /// The refreshing sign.
    pub(crate) fn refreshing(&self) -> &'static str {
        if self.ascii() { "~" } else { "⟳" }
    }

    /// A bordered panel titled `title`; returns the inner area.
    pub(crate) fn panel(&self, title: &str, area: Rect, buf: &mut Buffer) -> Rect {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.pal.muted_style())
            .title(Line::styled(
                alloc::format!(" {title} "),
                self.pal.accent_style().add_modifier(Modifier::BOLD),
            ));
        let inner = block.inner(area);
        block.render(area, buf);
        inner
    }
}

/// `"42%"`, or the missing marker.
pub(crate) fn percent(value: Option<u8>) -> String {
    value.map_or_else(|| MISSING.to_string(), whether_core::view::format::chance)
}
