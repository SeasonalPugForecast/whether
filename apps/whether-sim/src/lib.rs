//! Renders the firmware's display pages on the host, per board profile, to PNG.

pub mod board;
pub mod panel;
pub mod tui;

use std::path::Path;

use anyhow::Context;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics_simulator::{OutputSettingsBuilder, SimulatorDisplay};
use whether_core::icon::IconId;
use whether_core::model::WeatherSnapshot;
use whether_core::testdata;
use whether_core::theme::SkyTheme;
use whether_core::time::UnixTime;
use whether_core::view::ViewModel;
use whether_runtime::CachedSnapshot;

/// A firmware page.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Page {
    /// Current conditions.
    Now,
    /// The next hours.
    Hourly,
    /// The next days.
    Daily,
    /// Details (air quality in the TUI).
    Details,
}

impl Page {
    /// Every page in display order.
    pub const ALL: [Page; 4] = [Page::Now, Page::Hourly, Page::Daily, Page::Details];

    /// The page's name on the command line and in golden file names.
    pub fn name(self) -> &'static str {
        match self {
            Page::Now => "now",
            Page::Hourly => "hourly",
            Page::Daily => "daily",
            Page::Details => "details",
        }
    }
}

/// A weather mood for previews and goldens: the sample data restyled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Sky {
    /// The sample as it is (partly cloudy, by day).
    ClearDay,
    /// Rain showers at night.
    RainNight,
    /// Snow.
    Snow,
}

impl Sky {
    /// Every sky.
    pub const ALL: [Sky; 3] = [Sky::ClearDay, Sky::RainNight, Sky::Snow];

    /// The name in golden file names.
    pub fn name(self) -> &'static str {
        match self {
            Sky::ClearDay => "clear-day",
            Sky::RainNight => "rain-night",
            Sky::Snow => "snow",
        }
    }

    /// `vm` with this sky's theme, icon and condition.
    pub fn apply(self, mut vm: ViewModel) -> ViewModel {
        let (theme, icon, label, day) = match self {
            Sky::ClearDay => (SkyTheme::ClearDay, IconId::ClearDay, "Clear", true),
            Sky::RainNight => (SkyTheme::Rain, IconId::ShowersNight, "Rain showers", false),
            Sky::Snow => (SkyTheme::Snow, IconId::Snow, "Snow", true),
        };
        vm.theme = theme;
        vm.is_day = day;
        if let Some(c) = vm.current.as_mut() {
            c.icon = icon;
            c.condition_label = label.into();
        }
        vm
    }
}

/// A snapshot to show and the time to show it at.
#[derive(Clone, Debug)]
pub struct Scene {
    /// Location name in the header.
    pub name: String,
    /// The data.
    pub snapshot: WeatherSnapshot,
    /// "Now" for relative times (fixed, so renders are deterministic).
    pub now: UnixTime,
}

impl Scene {
    /// The built-in sample (New York, fixed time).
    pub fn sample() -> Self {
        Self {
            name: "New York".into(),
            snapshot: testdata::sample_snapshot(),
            now: testdata::SAMPLE_NOW,
        }
    }

    /// The sample with a flood warning and a wind advisory.
    pub fn sample_with_alerts() -> Self {
        Self {
            snapshot: testdata::sample_snapshot_with_alerts(),
            ..Self::sample()
        }
    }

    /// A cache file written by `whether`, shown as of the time it was saved.
    pub fn from_cache(path: &Path) -> anyhow::Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let cached: CachedSnapshot = serde_json::from_str(&text)
            .with_context(|| format!("{} is not a cache file", path.display()))?;
        Ok(Self {
            name: cached.snapshot.location.name.clone(),
            now: cached.saved_at,
            snapshot: cached.snapshot,
        })
    }

    /// The view model the firmware would build: the default presentation with the
    /// alerts widget on, so a snapshot's alerts show.
    pub fn view(&self) -> ViewModel {
        testdata::view_with(
            &self.name,
            &self.snapshot,
            self.now,
            &testdata::presentation_with_alerts(),
        )
    }
}

/// Writes `display` as a PNG at 1:1 scale.
pub fn save_png(display: &SimulatorDisplay<Rgb565>, out: &Path) -> anyhow::Result<()> {
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    display
        .to_rgb_output_image(&OutputSettingsBuilder::new().build())
        .save_png(out)
        .with_context(|| format!("writing {}", out.display()))
}
