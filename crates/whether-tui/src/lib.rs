#![cfg_attr(not(any(feature = "std", test)), no_std)]
//! Terminal display: ratatui rendering (`no_std`) and a crossterm host (`host` feature).
//!
//! [`draw`] renders a [`ViewModel`] into any ratatui `Frame`: a terminal, a test
//! buffer, or an ESP32 panel through mousefood.

extern crate alloc;

pub mod config;
#[cfg(feature = "host")]
pub mod host;
pub mod icons;
pub mod layout;
pub mod theme;
pub mod ui_state;
mod widgets;

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use whether_core::presentation::WidgetKind;
use whether_core::view::ViewModel;

pub use config::TuiConfig;
pub use ui_state::{Action, ColorDepth, Key, SearchInput, UiState, View};

use layout::Preset;
use theme::Palette;
use widgets::{
    Ctx, air, chrome, current, daily, details, history, hourly, precip, radar, sky, text,
};

/// Radar pixels for the radar view, borrowed from the host's state: the
/// view model carries only the frames' labels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RadarImage<'a> {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// The base map, RGBA.
    pub base: &'a [u8],
    /// The shown frame, RGBA, transparent where there is no echo.
    pub frame: Option<&'a [u8]>,
    /// The location in pixels.
    pub marker: (u32, u32),
}

/// Draws everything: header, body (layout or view), footer with the attribution,
/// and the help overlay.
pub fn draw(frame: &mut Frame<'_>, vm: &ViewModel, ui: &UiState, cfg: &TuiConfig) {
    draw_with_radar(frame, vm, ui, cfg, None);
}

/// [`draw`] with radar imagery for the radar view and layout cells.
pub fn draw_with_radar(
    frame: &mut Frame<'_>,
    vm: &ViewModel,
    ui: &UiState,
    cfg: &TuiConfig,
    radar: Option<RadarImage<'_>>,
) {
    let area = frame.area();
    render_with_radar(frame.buffer_mut(), area, vm, ui, cfg, radar);
}

/// [`draw`] into a buffer area (for hosts without a `Frame`).
pub fn render(buf: &mut Buffer, area: Rect, vm: &ViewModel, ui: &UiState, cfg: &TuiConfig) {
    render_with_radar(buf, area, vm, ui, cfg, None);
}

/// [`draw_with_radar`] into a buffer area.
pub fn render_with_radar(
    buf: &mut Buffer,
    area: Rect,
    vm: &ViewModel,
    ui: &UiState,
    cfg: &TuiConfig,
    radar: Option<RadarImage<'_>>,
) {
    let pal = Palette::new(cfg.background, &cfg.colors).for_depth(ui.colors);
    let ctx = Ctx {
        vm,
        ui,
        cfg,
        pal,
        radar,
    };
    if area.height < 3 || area.width < 10 {
        text::minimal(&ctx, area, buf);
        return;
    }
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(area);
    chrome::header(&ctx, header, buf);
    chrome::footer(&ctx, footer, buf);
    if vm.current.is_none() {
        chrome::no_data(&ctx, body, buf);
    } else {
        match ui.view {
            View::Overview => overview(&ctx, body, area, buf),
            View::Hourly => {
                let [top, bottom] =
                    Layout::vertical([Constraint::Percentage(50), Constraint::Fill(1)]).areas(body);
                hourly::chart(&ctx, top, buf);
                hourly::table(&ctx, bottom, buf);
            }
            View::Daily => daily::daily(&ctx, body, buf, true),
            View::Air => air::air(&ctx, body, buf, true),
            View::Radar => radar::radar(&ctx, body, buf),
            View::History => history::history(&ctx, body, buf),
        }
    }
    if ui.help {
        chrome::help(&ctx, area, buf);
    }
}

/// What the search overlay shows besides the typed query.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchPanel {
    /// "Searching…", "No places found", an error, or empty.
    pub status: alloc::string::String,
    /// Result names, best first.
    pub results: alloc::vec::Vec<alloc::string::String>,
}

/// Draws the location search over everything else.
pub fn draw_search(
    frame: &mut Frame<'_>,
    input: &SearchInput,
    panel: &SearchPanel,
    ui: &UiState,
    cfg: &TuiConfig,
) {
    let area = frame.area();
    let pal = Palette::new(cfg.background, &cfg.colors).for_depth(ui.colors);
    widgets::chrome::search(&pal, input, panel, area, frame.buffer_mut());
}

fn day_rows(ctx: &Ctx<'_>) -> u16 {
    ctx.vm
        .daily
        .as_ref()
        .map_or(0, |d| u16::try_from(d.days.len()).unwrap_or(u16::MAX))
        .saturating_add(2)
}

fn overview(ctx: &Ctx<'_>, body: Rect, screen: Rect, buf: &mut Buffer) {
    match layout::choose(ctx.cfg.layout, !ctx.cfg.rows.is_empty(), screen) {
        Some(Preset::Wide) => wide(ctx, body, buf),
        Some(Preset::Dashboard) => dashboard(ctx, body, buf),
        Some(Preset::Compact) => compact(ctx, body, buf),
        Some(Preset::Minimal) => text::minimal(ctx, body, buf),
        None => {
            for (kind, area) in layout::custom_areas(&ctx.cfg.rows, body) {
                widget(ctx, kind, area, buf);
            }
        }
    }
}

/// One widget kind in a custom layout cell.
fn widget(ctx: &Ctx<'_>, kind: WidgetKind, area: Rect, buf: &mut Buffer) {
    match kind {
        WidgetKind::Current => current::current(ctx, area, buf),
        WidgetKind::Summary => text::summary(ctx, area, buf),
        WidgetKind::Hourly => {
            let [top, bottom] =
                Layout::vertical([Constraint::Percentage(60), Constraint::Fill(1)]).areas(area);
            hourly::chart(ctx, top, buf);
            hourly::table(ctx, bottom, buf);
        }
        WidgetKind::Daily => daily::daily(ctx, area, buf, area.width >= 100),
        WidgetKind::Details => details::details(ctx, area, buf),
        WidgetKind::AirQuality => air::air(ctx, area, buf, area.height > 6),
        WidgetKind::Sun => sky::sun(ctx, area, buf),
        WidgetKind::Moon => sky::moon(ctx, area, buf),
        WidgetKind::Nowcast => text::nowcast(ctx, area, buf),
        WidgetKind::Alerts => text::alerts(ctx, area, buf),
        WidgetKind::Radar => radar::radar(ctx, area, buf),
        WidgetKind::Precipitation => precip::precip(ctx, area, buf),
        WidgetKind::History => history::history(ctx, area, buf),
    }
}

/// Full-screen terminals on large monitors.
fn wide(ctx: &Ctx<'_>, body: Rect, buf: &mut Buffer) {
    let [top, middle, bottom] = Layout::vertical([
        Constraint::Length(11),
        Constraint::Min(14),
        Constraint::Length(day_rows(ctx)),
    ])
    .areas(body);
    let [now, info, side] = Layout::horizontal([
        Constraint::Percentage(30),
        Constraint::Percentage(40),
        Constraint::Percentage(30),
    ])
    .areas(top);
    current::current(ctx, now, buf);
    details::details(ctx, info, buf);
    let [sun, moon, aq] = Layout::vertical([
        Constraint::Length(5),
        Constraint::Length(4),
        Constraint::Fill(1),
    ])
    .areas(side);
    sky::sun(ctx, sun, buf);
    sky::moon(ctx, moon, buf);
    air::air(ctx, aq, buf, false);
    let [chart, table] =
        Layout::vertical([Constraint::Percentage(60), Constraint::Fill(1)]).areas(middle);
    hourly::chart(ctx, chart, buf);
    hourly::table(ctx, table, buf);
    let [days, notes] =
        Layout::horizontal([Constraint::Percentage(65), Constraint::Percentage(35)]).areas(bottom);
    daily::daily(ctx, days, buf, true);
    text::summary_and_alerts(ctx, notes, buf);
}

/// The default for ordinary terminals. Active alerts get a strip of one line each
/// (at most three) below the top row.
fn dashboard(ctx: &Ctx<'_>, body: Rect, buf: &mut Buffer) {
    let [top, notes, middle, bottom] = Layout::vertical([
        Constraint::Length(9),
        Constraint::Length(text::alert_strip_rows(ctx, 3)),
        Constraint::Min(10),
        Constraint::Length(day_rows(ctx)),
    ])
    .areas(body);
    text::alerts(ctx, notes, buf);
    let [now, info, side] = Layout::horizontal([
        Constraint::Percentage(35),
        Constraint::Percentage(40),
        Constraint::Percentage(25),
    ])
    .areas(top);
    current::current(ctx, now, buf);
    details::details(ctx, info, buf);
    let [sun, moon] = Layout::vertical([Constraint::Length(5), Constraint::Fill(1)]).areas(side);
    sky::sun(ctx, sun, buf);
    sky::moon(ctx, moon, buf);
    let [chart, table] =
        Layout::vertical([Constraint::Percentage(60), Constraint::Fill(1)]).areas(middle);
    hourly::chart(ctx, chart, buf);
    hourly::table(ctx, table, buf);
    daily::daily(ctx, bottom, buf, false);
}

/// Small terminals and the ESP32 panel.
fn compact(ctx: &Ctx<'_>, body: Rect, buf: &mut Buffer) {
    let [now, hours, days, rest] = Layout::vertical([
        Constraint::Length(6),
        Constraint::Length(3),
        Constraint::Length(5),
        Constraint::Fill(1),
    ])
    .areas(body);
    let inner = ctx.panel("Now", now, buf);
    current::body(ctx, inner, buf, false);
    hourly::next_hours(ctx, hours, buf);
    daily::daily(ctx, days, buf, false);
    text::alerts_and_summary(ctx, rest, buf);
}

#[cfg(test)]
mod tests;
