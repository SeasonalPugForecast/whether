//! Applying a `UiModel` to the desktop window (on the UI thread).

use slint::{
    Color, ComponentHandle, Image, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, VecModel,
};
use whether_display::RadarImages;

use super::config::Section;
use crate::model::{UiModel, title};
use crate::{DesktopWindow, Theme};

/// One sky palette: gradient top and bottom, and the text color on it.
pub use whether_core::theme::SkyColors as Sky;

/// The default palettes, in `SkyTheme` order (clear day, clear night, cloudy day,
/// cloudy night, rain, snow, storm, fog, twilight), shared with the panel display.
pub const DEFAULT_SKIES: [Sky; 9] = whether_core::theme::SKY_COLORS;

fn rgb([r, g, b]: [u8; 3]) -> Color {
    Color::from_rgb_u8(r, g, b)
}

/// The palette when the background does not follow the weather.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum FixedTheme {
    /// Light by day, dark by night.
    #[default]
    Auto,
    /// The fog palette (light background, dark text).
    Light,
    /// The clear-night palette.
    Dark,
}

/// Palette indices used for the fixed themes.
const LIGHT_SKY: usize = 7;
const DARK_SKY: usize = 1;

/// How the window presents the model (from `[display.gui]`).
#[derive(Clone, Debug, PartialEq)]
pub struct WindowOptions {
    /// Sky palettes in `SkyTheme` order.
    pub skies: [Sky; 9],
    /// Follow the sky; otherwise use `theme`.
    pub dynamic_background: bool,
    /// The palette when the background is not dynamic.
    pub theme: FixedTheme,
    /// Scale fonts with the window height (full screen, kiosk).
    pub auto_scale: bool,
    /// The font scale otherwise.
    pub fixed_scale: f32,
    /// Label of the units button (the units it switches to).
    pub units_label: SharedString,
    /// The cards per window width, in order.
    pub cards: super::config::CardLayout,
    /// The accent color.
    pub accent: [u8; 3],
}

impl Default for WindowOptions {
    fn default() -> Self {
        Self {
            skies: DEFAULT_SKIES,
            dynamic_background: true,
            theme: FixedTheme::Auto,
            auto_scale: false,
            fixed_scale: 1.0,
            units_label: "°F".into(),
            cards: super::config::CardLayout::default(),
            accent: super::config::DEFAULT_ACCENT,
        }
    }
}

fn model<T: Clone + 'static>(items: &[T]) -> ModelRc<T> {
    ModelRc::new(VecModel::from(items.to_vec()))
}

/// Line and area commands for temperature, precipitation and wind over the next
/// 24 hours, plus the first, middle and last hour labels.
pub fn chart(m: &UiModel) -> ([SharedString; 3], [SharedString; 3], [SharedString; 3]) {
    let hours = &m.hours[..m.hours.len().min(24)];
    let series: [Vec<f32>; 3] = [
        hours.iter().map(|h| h.temp_frac).collect(),
        hours.iter().map(|h| h.pop_frac).collect(),
        m.wind_fracs.iter().take(hours.len()).copied().collect(),
    ];
    let mut lines: [SharedString; 3] = Default::default();
    let mut areas: [SharedString; 3] = Default::default();
    for (i, values) in series.iter().enumerate() {
        let (line, area) = crate::charts::commands(values);
        lines[i] = line.into();
        areas[i] = area.into();
    }
    let label = |i: usize| hours.get(i).map(|h| h.label.clone()).unwrap_or_default();
    let labels = [
        label(0),
        label(hours.len() / 2),
        label(hours.len().saturating_sub(1)),
    ];
    (lines, areas, labels)
}

/// The sky for `m` as (top, bottom, text) colors.
pub fn sky_for(m: &UiModel, opts: &WindowOptions) -> (Color, Color, Color) {
    let night = m.sky == 1 || m.sky == 3;
    let index = match (opts.dynamic_background, opts.theme) {
        (true, _) => usize::try_from(m.sky).unwrap_or(0),
        (false, FixedTheme::Light) => LIGHT_SKY,
        (false, FixedTheme::Dark) => DARK_SKY,
        (false, FixedTheme::Auto) => {
            if night {
                DARK_SKY
            } else {
                LIGHT_SKY
            }
        }
    };
    let sky = opts.skies.get(index).copied().unwrap_or(DEFAULT_SKIES[0]);
    (rgb(sky.top), rgb(sky.bottom), rgb(sky.text))
}

/// A radar loop as Slint images: the base and the frames.
pub fn radar_images(r: &RadarImages) -> (Image, Vec<Image>) {
    let image = |rgba: &[u8]| {
        Image::from_rgba8(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
            rgba, r.width, r.height,
        ))
    };
    (
        image(&r.base),
        r.frames.iter().map(|f| image(&f.rgba)).collect(),
    )
}

/// Shows converted radar images and the location marker.
pub fn apply_radar(ui: &DesktopWindow, r: &RadarImages, images: &(Image, Vec<Image>)) {
    ui.set_radar_base(images.0.clone());
    ui.set_radar_frames(model(&images.1));
    #[allow(clippy::cast_precision_loss)]
    {
        ui.set_radar_marker_x(r.marker.0 as f32 / r.width.max(1) as f32);
        ui.set_radar_marker_y(r.marker.1 as f32 / r.height.max(1) as f32);
    }
}

/// Card ids for the window's `CardStack`.
mod card {
    pub(super) const CURRENT: i32 = 0;
    pub(super) const ALERTS: i32 = 1;
    pub(super) const HOURLY: i32 = 2;
    pub(super) const DAILY: i32 = 3;
    pub(super) const DETAILS: i32 = 4;
    pub(super) const SUN: i32 = 5;
    pub(super) const AIR: i32 = 6;
    pub(super) const RADAR: i32 = 7;
    pub(super) const PRECIP: i32 = 8;
    pub(super) const NOWCAST: i32 = 9;
    pub(super) const HISTORY: i32 = 10;
}

/// A tier's card ids, column by column: the cards with something to show, and the
/// alerts card right after the current conditions (or at the top of the first
/// column when they are hidden).
pub fn card_ids(m: &UiModel, columns: &[&[Section]]) -> Vec<Vec<i32>> {
    let mut out: Vec<Vec<i32>> = columns.iter().map(|c| column_ids(m, c)).collect();
    let current = columns.iter().any(|c| c.contains(&Section::Current));
    if !current
        && !m.alerts.is_empty()
        && let Some(first) = out.first_mut()
    {
        first.insert(0, card::ALERTS);
    }
    out
}

fn column_ids(m: &UiModel, column: &[Section]) -> Vec<i32> {
    let mut out = Vec::new();
    for s in column {
        let (id, shown) = match s {
            Section::Current => (card::CURRENT, true),
            Section::Hourly => (card::HOURLY, true),
            Section::Daily => (card::DAILY, !m.days.is_empty()),
            Section::Details => (card::DETAILS, true),
            Section::Sun => (card::SUN, m.sun.present),
            Section::AirQuality => (card::AIR, m.air.present),
            Section::Radar => (card::RADAR, !m.radar_labels.is_empty()),
            Section::Precipitation => (card::PRECIP, !m.precip.is_empty()),
            Section::Nowcast => (card::NOWCAST, !m.nowcast_text.is_empty()),
            Section::History => (card::HISTORY, m.history.present),
        };
        if shown {
            out.push(id);
        }
        if id == card::CURRENT && !m.alerts.is_empty() {
            out.push(card::ALERTS);
        }
    }
    out
}

/// Sets every window property from `m`.
pub fn apply(ui: &DesktopWindow, m: &UiModel, opts: &WindowOptions) {
    let (top, bottom, text) = sky_for(m, opts);
    let theme = ui.global::<Theme>();
    theme.set_accent(rgb(opts.accent));
    theme.set_text(text);
    theme.set_text_muted(text.with_alpha(0.72));
    ui.set_sky_top(top);
    ui.set_sky_bottom(bottom);
    ui.set_auto_scale(opts.auto_scale);
    ui.set_fixed_scale(opts.fixed_scale);
    ui.set_units_label(opts.units_label.clone());
    let ids = |columns: &[&[Section]]| -> Vec<ModelRc<i32>> {
        card_ids(m, columns)
            .into_iter()
            .map(|c| ModelRc::new(VecModel::from(c)))
            .collect()
    };
    let cards = &opts.cards;
    if let [narrow] = ids(&[&cards.narrow]).as_slice() {
        ui.set_narrow_cards(narrow.clone());
    }
    if let [a, b] = ids(&[&cards.standard[0], &cards.standard[1]]).as_slice() {
        ui.set_standard_cards_a(a.clone());
        ui.set_standard_cards_b(b.clone());
    }
    if let [a, b, c] = ids(&[&cards.wide[0], &cards.wide[1], &cards.wide[2]]).as_slice() {
        ui.set_wide_cards_a(a.clone());
        ui.set_wide_cards_b(b.clone());
        ui.set_wide_cards_c(c.clone());
    }
    ui.set_radar_labels(model(&m.radar_labels));
    ui.set_precip(model(&m.precip));
    ui.set_precip_total(m.precip_total.clone());
    ui.set_precip_max_chance(m.precip_max_chance.clone());
    ui.set_precip_dry(m.precip_dry.clone());
    ui.set_history(m.history.clone());
    ui.set_history_days(model(&m.history_days));
    ui.set_day_spans(model(&m.day_spans));
    ui.set_day_marks(model(&m.day_marks));
    ui.set_nowcast_text(m.nowcast_text.clone());
    ui.set_nowcast_wet(m.nowcast_wet);
    ui.set_nowcast_bars(model(&m.nowcast_bars));
    ui.set_nowcast_ticks(model(&m.nowcast_ticks));
    ui.set_nowcast_levels(model(&m.nowcast_levels));
    ui.set_radar_attribution(m.radar_attribution.clone());

    ui.set_window_title(title(m).into());
    let names: Vec<SharedString> = m.tabs.iter().map(|t| t.name.clone()).collect();
    ui.set_locations(model(&names));
    ui.set_current_location(i32::try_from(m.active).unwrap_or(0));
    ui.set_status(m.status.clone());
    ui.set_has_data(m.has_data);
    ui.set_current(m.current.clone());
    ui.set_hours(model(&m.hours));
    ui.set_days(model(&m.days));
    ui.set_details(model(&m.details));
    ui.set_air(m.air.clone());
    ui.set_pollutants(model(&m.pollutants));
    ui.set_sun(m.sun.clone());
    ui.set_alerts(model(&m.alerts));
    ui.set_attribution(m.attribution.clone());
    let (lines, areas, labels) = chart(m);
    ui.set_chart_lines(model(&lines));
    ui.set_chart_areas(model(&areas));
    let [first, mid, last] = labels;
    ui.set_chart_first(first);
    ui.set_chart_mid(mid);
    ui.set_chart_last(last);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cards_without_data_drop_out_and_alerts_follow_now() {
        use Section::{AirQuality, Current, Daily, Hourly, Radar, Sun};
        let mut m = crate::model::to_ui(&whether_core::testdata::sample_view_with_alerts());
        m.air.present = false;
        let ids = card_ids(&m, &[&[Hourly, Current, Radar], &[Sun, AirQuality]]);
        assert_eq!(
            ids,
            [
                vec![card::HOURLY, card::CURRENT, card::ALERTS],
                vec![card::SUN]
            ],
            "no radar imagery, no air quality"
        );
        let ids = card_ids(&m, &[&[Daily], &[Hourly]]);
        assert_eq!(
            ids,
            [vec![card::ALERTS, card::DAILY], vec![card::HOURLY]],
            "without the current card, alerts lead the first column"
        );
        m.alerts.clear();
        assert_eq!(card_ids(&m, &[&[Daily]]), [vec![card::DAILY]]);
    }

    #[test]
    fn chart_has_three_metrics() {
        let m = crate::model::to_ui(&crate::model::tests::sample_vm());
        let (lines, areas, labels) = chart(&m);
        assert!(lines.iter().all(|l| l.starts_with("M ")));
        assert!(areas.iter().all(|a| a.ends_with('Z')));
        assert_eq!(labels[0], m.hours[0].label);
        let empty = chart(&UiModel::default());
        assert!(empty.0.iter().all(SharedString::is_empty));
    }

    #[test]
    fn default_skies_follow_the_plan() {
        assert_eq!(DEFAULT_SKIES[0].top, [0x2F, 0x80, 0xED]);
        assert_eq!(DEFAULT_SKIES[8].bottom, [0xFD, 0x74, 0x6C]);
    }
}
