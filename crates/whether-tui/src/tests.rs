//! Rendering snapshots on test buffers, for every preset and view.

use alloc::string::String;

use proptest::prelude::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use whether_core::model::WeatherSnapshot;
use whether_core::presentation::PresentationConfig;
use whether_core::strings::Strings;
use whether_core::testdata;
use whether_core::view::{LocationView, StatusInfo, ViewInput, ViewModel, build_view};

use super::*;
use crate::config::{CellSpec, IconStyle, LayoutPreset, RowSpec, Size};

fn vm_for(snapshot: Option<&WeatherSnapshot>, status: &StatusInfo) -> ViewModel {
    vm_in(snapshot, status, &Strings::default())
}

fn vm_in(snapshot: Option<&WeatherSnapshot>, status: &StatusInfo, strings: &Strings) -> ViewModel {
    let presentation = PresentationConfig {
        widgets: alloc::vec![
            WidgetKind::Current,
            WidgetKind::Summary,
            WidgetKind::Hourly,
            WidgetKind::Daily,
            WidgetKind::Details,
            WidgetKind::Sun,
            WidgetKind::Moon,
            WidgetKind::AirQuality,
            WidgetKind::Nowcast,
            WidgetKind::Alerts,
        ],
        ..PresentationConfig::default()
    };
    let locations = [LocationView {
        name: "New York",
        snapshot,
        status,
        history: None,
    }];
    build_view(&ViewInput {
        locations: &locations,
        active: 0,
        presentation: &presentation,
        strings,
        now: testdata::SAMPLE_NOW,
    })
}

fn fresh() -> StatusInfo {
    StatusInfo {
        last_success: Some(testdata::SAMPLE_NOW),
        error: None,
        stale: false,
        refreshing: false,
    }
}

fn sample() -> ViewModel {
    vm_for(Some(&testdata::sample_snapshot()), &fresh())
}

/// ASCII icons keep snapshots readable; other styles are covered by the icon tests.
fn cfg(layout: LayoutPreset) -> TuiConfig {
    TuiConfig {
        layout,
        icons: IconStyle::Ascii,
        ..TuiConfig::default()
    }
}

fn ui() -> UiState {
    UiState::new(&TuiConfig::default(), 0, ColorDepth::TrueColor)
}

fn render_to(w: u16, h: u16, vm: &ViewModel, ui: &UiState, cfg: &TuiConfig) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| draw(f, vm, ui, cfg)).unwrap();
    terminal.backend().buffer().clone()
}

fn text(buf: &Buffer) -> String {
    let mut out = String::new();
    for y in 0..buf.area.height {
        let line: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

fn snapshot(w: u16, h: u16, layout: LayoutPreset) -> String {
    let vm = sample();
    let buf = render_to(w, h, &vm, &ui(), &cfg(layout));
    let out = text(&buf);
    assert!(
        out.contains(&vm.attribution),
        "the attribution is always shown at {w}×{h}"
    );
    out
}

#[test]
fn wide_190x55() {
    insta::assert_snapshot!(snapshot(190, 55, LayoutPreset::Wide));
}

#[test]
fn auto_picks_wide_then_dashboard() {
    assert_eq!(
        snapshot(190, 55, LayoutPreset::Auto),
        snapshot(190, 55, LayoutPreset::Wide)
    );
    assert_eq!(
        snapshot(120, 40, LayoutPreset::Auto),
        snapshot(120, 40, LayoutPreset::Dashboard)
    );
}

#[test]
fn dashboard_120x40() {
    insta::assert_snapshot!(snapshot(120, 40, LayoutPreset::Dashboard));
}

#[test]
fn compact_80x24() {
    insta::assert_snapshot!(snapshot(80, 24, LayoutPreset::Compact));
}

#[test]
fn compact_80x26_esp32_480x320() {
    insta::assert_snapshot!(snapshot(80, 26, LayoutPreset::Compact));
}

#[test]
fn compact_53x20_esp32_320x240() {
    insta::assert_snapshot!(snapshot(53, 20, LayoutPreset::Compact));
}

/// The CYD's 53×20 cells: the header keeps the local time, and the daily view
/// drops detail columns instead of truncating the day names.
#[test]
fn narrow_daily_view_53x20() {
    let daily = UiState {
        view: View::Daily,
        ..ui()
    };
    let out = text(&render_to(
        53,
        20,
        &sample(),
        &daily,
        &cfg(LayoutPreset::Compact),
    ));
    let header = out.lines().next().unwrap();
    assert!(header.contains("Mon 11:00"), "{header}");
    assert!(out.contains("Today ") && out.contains("Tomorrow "), "{out}");
    assert!(!out.contains("km/h"), "{out}");
    let overview = text(&render_to(
        53,
        20,
        &sample(),
        &ui(),
        &cfg(LayoutPreset::Compact),
    ));
    let device = UiState {
        key_hints: false,
        ..ui()
    };
    let panel = text(&render_to(
        53,
        20,
        &sample(),
        &device,
        &cfg(LayoutPreset::Compact),
    ));
    assert!(
        !panel.contains("quit") && panel.contains("Open-Meteo.com"),
        "{panel}"
    );
    assert_eq!(
        overview.matches("~45").count(),
        1,
        "the nowcast once: {overview}"
    );
    insta::assert_snapshot!(out);
}

#[test]
fn minimal_40x10() {
    insta::assert_snapshot!(snapshot(40, 10, LayoutPreset::Minimal));
}

fn view(view: View, help: bool) -> String {
    let ui = UiState { view, help, ..ui() };
    text(&render_to(
        120,
        40,
        &sample(),
        &ui,
        &cfg(LayoutPreset::Auto),
    ))
}

#[test]
fn views_at_120x40() {
    insta::assert_snapshot!("view_hourly", view(View::Hourly, false));
    insta::assert_snapshot!("view_daily", view(View::Daily, false));
    insta::assert_snapshot!("view_air", view(View::Air, false));
    insta::assert_snapshot!("view_help", view(View::Overview, true));
}

#[test]
fn custom_layout_from_the_example() {
    let custom = TuiConfig {
        rows: alloc::vec![
            RowSpec {
                height: Size::Cells(10),
                widgets: alloc::vec![
                    CellSpec {
                        kind: WidgetKind::Current,
                        width: Some(Size::Percent(40))
                    },
                    CellSpec {
                        kind: WidgetKind::Details,
                        width: Some(Size::Fill)
                    },
                ],
            },
            RowSpec {
                height: Size::Fill,
                widgets: alloc::vec![CellSpec {
                    kind: WidgetKind::Hourly,
                    width: None
                }]
            },
        ],
        ..cfg(LayoutPreset::Custom)
    };
    insta::assert_snapshot!(text(&render_to(120, 40, &sample(), &ui(), &custom)));
}

#[test]
fn sparse_snapshot() {
    let vm = vm_for(Some(&testdata::sample_snapshot_sparse()), &fresh());
    insta::assert_snapshot!(text(&render_to(
        120,
        40,
        &vm,
        &ui(),
        &cfg(LayoutPreset::Auto)
    )));
}

/// The alerts sample in `layout`; every alert's title must show (only the first in
/// the minimal layout).
fn alert_snapshot(w: u16, h: u16, layout: LayoutPreset) -> String {
    let vm = vm_for(Some(&testdata::sample_snapshot_with_alerts()), &fresh());
    let out = text(&render_to(w, h, &vm, &ui(), &cfg(layout)));
    assert!(out.contains("Flood Warning"), "{layout:?} at {w}×{h}");
    if layout != LayoutPreset::Minimal {
        assert!(out.contains("Wind Advisory"), "{layout:?} at {w}×{h}");
    }
    assert!(out.contains(&vm.attribution));
    out
}

/// the German pack in the compact layout (panel titles stay English).
#[test]
fn german_compact_80x24() {
    let de = Strings::localized("de", &alloc::collections::BTreeMap::new()).0;
    let vm = vm_in(Some(&testdata::sample_snapshot()), &fresh(), &de);
    let out = text(&render_to(80, 24, &vm, &ui(), &cfg(LayoutPreset::Compact)));
    assert!(
        out.contains("Teilweise bewölkt") && out.contains("Heute"),
        "{out}"
    );
    insta::assert_snapshot!(out);
}

#[test]
fn active_alerts_wide_190x55() {
    let out = alert_snapshot(190, 55, LayoutPreset::Wide);
    assert!(out.contains("Mon 09:00 – Tue 06:00"), "the period shows");
    insta::assert_snapshot!(out);
}

#[test]
fn active_alerts_dashboard_120x40() {
    insta::assert_snapshot!(alert_snapshot(120, 40, LayoutPreset::Dashboard));
}

#[test]
fn active_alerts_compact_80x24() {
    insta::assert_snapshot!(alert_snapshot(80, 24, LayoutPreset::Compact));
}

#[test]
fn active_alerts_compact_53x20_esp32_320x240() {
    insta::assert_snapshot!(alert_snapshot(53, 20, LayoutPreset::Compact));
}

#[test]
fn active_alerts_minimal_40x10() {
    insta::assert_snapshot!(alert_snapshot(40, 10, LayoutPreset::Minimal));
}

#[test]
fn no_snapshot_with_an_error() {
    let status = StatusInfo {
        last_success: None,
        error: Some("every provider failed".into()),
        stale: false,
        refreshing: true,
    };
    let vm = vm_for(None, &status);
    let out = text(&render_to(80, 24, &vm, &ui(), &cfg(LayoutPreset::Auto)));
    assert!(out.contains("No weather data yet") && out.contains("every provider failed"));
    insta::assert_snapshot!(out);
}

#[test]
fn stale_status_is_shown_in_the_error_style() {
    let status = StatusInfo {
        stale: true,
        ..fresh()
    };
    let vm = vm_for(Some(&testdata::sample_snapshot()), &status);
    let buf = render_to(120, 40, &vm, &ui(), &cfg(LayoutPreset::Auto));
    let header: String = (0..120).map(|x| buf[(x, 0)].symbol()).collect();
    let x = u16::try_from(header.find(&vm.status.updated).unwrap()).unwrap();
    let palette = theme::Palette::new(config::Background::Dark, &config::TuiColors::default());
    assert_eq!(buf[(x, 0)].fg, palette.error);
}

#[test]
fn temperatures_are_bold_and_colored() {
    let vm = sample();
    let buf = render_to(120, 40, &vm, &ui(), &cfg(LayoutPreset::Auto));
    let out = text(&buf);
    let temp = &vm.current.as_ref().unwrap().temperature.text;
    let (y, line) = out
        .lines()
        .enumerate()
        .find(|(_, l)| l.contains(&alloc::format!("{temp}  ")))
        .unwrap();
    let x = u16::try_from(line[..line.find(temp.as_str()).unwrap()].chars().count()).unwrap();
    let cell = &buf[(x, u16::try_from(y).unwrap())];
    assert!(cell.modifier.contains(ratatui::style::Modifier::BOLD));
    let plain = render_to(
        120,
        40,
        &vm,
        &UiState {
            colors: ColorDepth::Plain,
            ..ui()
        },
        &cfg(LayoutPreset::Auto),
    );
    assert_eq!(
        plain[(x, u16::try_from(y).unwrap())].fg,
        ratatui::style::Color::Reset
    );
}

/// Runs of styled cells per row: `y: x0..x1 fg bg modifiers`.
fn styles(buf: &Buffer) -> String {
    use core::fmt::Write as _;
    let mut out = String::new();
    for y in 0..buf.area.height {
        let mut x = 0;
        while x < buf.area.width {
            let cell = &buf[(x, y)];
            let key = (cell.fg, cell.bg, cell.modifier);
            let start = x;
            while x < buf.area.width && {
                let c = &buf[(x, y)];
                (c.fg, c.bg, c.modifier) == key
            } {
                x += 1;
            }
            if key
                != (
                    ratatui::style::Color::Reset,
                    ratatui::style::Color::Reset,
                    ratatui::style::Modifier::empty(),
                )
            {
                let _ = writeln!(out, "{y}: {start}..{x} {:?} {:?} {:?}", key.0, key.1, key.2);
            }
        }
    }
    out
}

fn variant(cfg: &TuiConfig, colors: ColorDepth) -> Buffer {
    render_to(80, 24, &sample(), &UiState { colors, ..ui() }, cfg)
}

#[test]
fn light_background_80x24() {
    let light = TuiConfig {
        background: config::Background::Light,
        ..cfg(LayoutPreset::Auto)
    };
    insta::assert_snapshot!(styles(&variant(&light, ColorDepth::TrueColor)));
}

#[test]
fn sixteen_colors_80x24() {
    let buf = variant(&cfg(LayoutPreset::Auto), ColorDepth::Ansi16);
    let dump = styles(&buf);
    assert!(!dump.contains("Rgb"), "only named colors");
    insta::assert_snapshot!(dump);
}

#[test]
fn ascii_only_80x24() {
    let status = StatusInfo {
        error: Some("timeout".into()),
        refreshing: true,
        ..fresh()
    };
    let vm = vm_for(Some(&testdata::sample_snapshot()), &status);
    let buf = render_to(80, 24, &vm, &ui(), &cfg(LayoutPreset::Auto));
    let out = text(&buf);
    assert!(out.contains("! timeout") && out.contains("~ "), "{out}");
    assert!(!out.contains('⚠') && !out.contains('⟳'));
    insta::assert_snapshot!(out);
}

#[test]
fn ascii_charts_use_dots() {
    let ui = UiState {
        view: View::Hourly,
        ..ui()
    };
    let out = text(&render_to(
        120,
        40,
        &sample(),
        &ui,
        &cfg(LayoutPreset::Auto),
    ));
    assert!(
        !out.chars().any(|c| ('\u{2800}'..='\u{28ff}').contains(&c)),
        "no Braille in ASCII mode"
    );
    assert!(out.contains('•'));
}

#[test]
fn search_popup() {
    let input = SearchInput {
        query: "Springfield".into(),
        selected: 1,
        submitted: true,
    };
    let panel = SearchPanel {
        status: String::new(),
        results: alloc::vec![
            "Springfield, Illinois, US".into(),
            "Springfield, Missouri, US".into()
        ],
    };
    let ui = ui();
    let cfg = cfg(LayoutPreset::Auto);
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|f| {
            draw(f, &sample(), &ui, &cfg);
            draw_search(f, &input, &panel, &ui, &cfg);
        })
        .unwrap();
    let out = text(terminal.backend().buffer());
    assert!(out.contains("> Springfield_") && out.contains("Missouri"));
    insta::assert_snapshot!(out);
}

/// A 96-pixel map: land, with a storm south-west of the marker (green edges, a red
/// core) and nothing elsewhere.
fn radar_pixels() -> (alloc::vec::Vec<u8>, alloc::vec::Vec<u8>) {
    let size = 96_i32;
    let mut base = alloc::vec::Vec::new();
    let mut frame = alloc::vec::Vec::new();
    for y in 0..size {
        for x in 0..size {
            base.extend_from_slice(&[242, 239, 233, 255]);
            let d2 = (x - 30).pow(2) + (y - 66).pow(2);
            let echo: [u8; 4] = match d2 {
                d if d < 64 => [230, 0, 0, 255],
                d if d < 256 => [0, 200, 0, 255],
                _ => [0, 0, 0, 0],
            };
            frame.extend_from_slice(&echo);
        }
    }
    (base, frame)
}

fn radar_vm() -> ViewModel {
    let mut vm = sample();
    vm.radar = Some(whether_core::view::RadarVm {
        frames: ["10:35", "10:40", "10:45", "10:50", "10:55", "11:00"]
            .map(String::from)
            .to_vec(),
        attribution:
            "Radar: NOAA NEXRAD via Iowa Environmental Mesonet · Map © OpenStreetMap contributors"
                .into(),
    });
    vm
}

fn render_radar(w: u16, h: u16, ui: &UiState, cfg: &TuiConfig) -> Buffer {
    let (base, frame) = radar_pixels();
    let image = RadarImage {
        width: 96,
        height: 96,
        base: &base,
        frame: Some(&frame),
        marker: (48, 48),
    };
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal
        .draw(|f| draw_with_radar(f, &radar_vm(), ui, cfg, Some(image)))
        .unwrap();
    terminal.backend().buffer().clone()
}

fn radar_ui(colors: ColorDepth) -> UiState {
    UiState {
        view: View::Radar,
        radar_step: 5,
        ..UiState::new(&TuiConfig::default(), 0, colors)
    }
}

/// Every cell's foreground and background in the radar panel's map rows.
fn map_colors(
    buf: &Buffer,
) -> alloc::vec::Vec<(&str, ratatui::style::Color, ratatui::style::Color)> {
    let mut out = alloc::vec::Vec::new();
    for y in 2..buf.area.height - 4 {
        for x in 1..buf.area.width - 1 {
            let c = &buf[(x, y)];
            out.push((c.symbol(), c.fg, c.bg));
        }
    }
    out
}

#[test]
fn radar_view_80x24() {
    use ratatui::style::Color;
    let buf = render_radar(
        80,
        24,
        &radar_ui(ColorDepth::TrueColor),
        &cfg(LayoutPreset::Auto),
    );
    let out = text(&buf);
    assert!(
        out.contains("> 11:00") && out.contains("OpenStreetMap") && out.contains("contributors"),
        "{out}"
    );
    assert_eq!(out.matches('+').count(), 1, "one marker: {out}");
    let cells = map_colors(&buf);
    let red = |c: Color| matches!(c, Color::Rgb(r, g, b) if r > 150 && g < 60 && b < 60);
    let green = |c: Color| matches!(c, Color::Rgb(r, g, b) if g > 120 && r < 60 && b < 60);
    assert!(
        cells.iter().any(|(_, fg, bg)| red(*fg) || red(*bg)),
        "the core"
    );
    assert!(
        cells.iter().any(|(_, fg, bg)| green(*fg) || green(*bg)),
        "the edges"
    );
    assert!(
        cells
            .iter()
            .any(|(_, _, bg)| matches!(bg, Color::Rgb(r, _, _) if (90..130).contains(r))),
        "the land, dimmed on a dark background"
    );
    insta::assert_snapshot!(out);
    let blocks = render_radar(
        80,
        24,
        &radar_ui(ColorDepth::TrueColor),
        &TuiConfig::default(),
    );
    assert!(
        map_colors(&blocks)
            .iter()
            .any(|(s, fg, bg)| *s == "▀" && fg != bg),
        "half blocks: two pixels a cell"
    );
}

#[test]
fn radar_without_truecolor_draws_only_echoes() {
    use ratatui::style::Color;
    let buf = render_radar(
        80,
        24,
        &radar_ui(ColorDepth::Ansi16),
        &cfg(LayoutPreset::Auto),
    );
    let cells = map_colors(&buf);
    assert!(
        cells
            .iter()
            .all(|(_, fg, bg)| !matches!(fg, Color::Rgb(..)) && !matches!(bg, Color::Rgb(..)))
    );
    assert!(
        cells
            .iter()
            .any(|(_, fg, bg)| *fg == Color::Red || *bg == Color::Red)
    );
    assert!(
        cells
            .iter()
            .any(|(_, fg, bg)| *fg == Color::Green || *bg == Color::Green)
    );
    let plain = text(&render_radar(
        80,
        24,
        &radar_ui(ColorDepth::Plain),
        &cfg(LayoutPreset::Auto),
    ));
    assert!(
        plain.contains('#') && plain.contains(':'),
        "shades by strength: {plain}"
    );
    let unicode = TuiConfig::default();
    let plain = text(&render_radar(
        80,
        24,
        &radar_ui(ColorDepth::Plain),
        &unicode,
    ));
    assert!(plain.contains('▓') && plain.contains('░'), "{plain}");
}

#[test]
fn radar_view_without_imagery() {
    let ui = UiState {
        view: View::Radar,
        ..ui()
    };
    let out = text(&render_to(80, 12, &sample(), &ui, &cfg(LayoutPreset::Auto)));
    assert!(out.contains("No radar for this location"), "{out}");
    insta::assert_snapshot!(out);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]
    #[test]
    fn any_size_renders(w in 1u16..300, h in 1u16..100, help in any::<bool>(), view_index in 0usize..6) {
        let views = [View::Overview, View::Hourly, View::Daily, View::Air, View::Radar, View::History];
        let ui = UiState { help, view: views[view_index], ..ui() };
        render_to(w, h, &sample(), &ui, &cfg(LayoutPreset::Auto));
        render_to(w, h, &with_history(), &ui, &cfg(LayoutPreset::Auto));
        render_radar(w, h, &ui, &cfg(LayoutPreset::Auto));
    }
}

#[test]
fn the_status_line_names_the_observing_station() {
    let mut vm = sample();
    vm.status.observed = Some("Observed at Central Park, 8:55 AM".into());
    let out = text(&render_to(190, 55, &vm, &ui(), &cfg(LayoutPreset::Wide)));
    assert!(out.contains("Observed at Central Park, 8:55 AM"), "{out}");
}

/// The sample with the synthetic 10001 history and the history widget.
fn with_history() -> ViewModel {
    let snapshot = testdata::sample_snapshot();
    let history = testdata::history_10001();
    let status = fresh();
    let presentation = PresentationConfig {
        widgets: alloc::vec![
            WidgetKind::Current,
            WidgetKind::Details,
            WidgetKind::History
        ],
        details: alloc::vec![
            whether_core::presentation::DetailKind::Normals,
            whether_core::presentation::DetailKind::Records,
        ],
        units: whether_core::units::UnitPrefs::IMPERIAL,
        ..PresentationConfig::default()
    };
    let strings = Strings::default();
    let locations = [LocationView {
        name: "New York",
        snapshot: Some(&snapshot),
        status: &status,
        history: Some(&history),
    }];
    build_view(&ViewInput {
        locations: &locations,
        active: 0,
        presentation: &presentation,
        strings: &strings,
        now: testdata::SAMPLE_NOW,
    })
}

#[test]
fn history_view_at_100x30() {
    let ui = UiState {
        view: View::History,
        ..ui()
    };
    let out = text(&render_to(
        100,
        30,
        &with_history(),
        &ui,
        &cfg(LayoutPreset::Auto),
    ));
    assert!(out.contains("Normal 69° / 55°"), "{out}");
    assert!(out.contains("Yesterday"), "{out}");
    insta::assert_snapshot!(out);
    // Without history the view says so.
    let empty = text(&render_to(
        100,
        30,
        &sample(),
        &ui,
        &cfg(LayoutPreset::Auto),
    ));
    assert!(empty.contains("No history yet"), "{empty}");
}
