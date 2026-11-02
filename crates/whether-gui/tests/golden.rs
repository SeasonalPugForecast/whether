//! Screenshot goldens for the desktop window. A golden fails when more than 0.5 % of
//! pixels differ by more than 8/255 in any channel; `UPDATE_GOLDEN=1` rewrites them.
//! Every render is also written to `target/screenshots/` for inspection.

use std::path::{Path, PathBuf};

use whether_core::model::WeatherSnapshot;
use whether_core::presentation::PresentationConfig;
use whether_core::strings::Strings;
use whether_core::testdata;
use whether_core::units::UnitPrefs;
use whether_core::view::{LocationView, StatusInfo, ViewInput, ViewModel, build_view};
use whether_gui::desktop::glue::WindowOptions;
use whether_gui::desktop::screenshot::{Which, render_png};
use whether_gui::model::{UiModel, to_ui};

fn vm(
    snapshot: Option<&WeatherSnapshot>,
    status: &StatusInfo,
    presentation: &PresentationConfig,
) -> ViewModel {
    vm_in(snapshot, status, presentation, &Strings::default())
}

fn vm_in(
    snapshot: Option<&WeatherSnapshot>,
    status: &StatusInfo,
    presentation: &PresentationConfig,
    strings: &Strings,
) -> ViewModel {
    let locations = [
        LocationView {
            name: "New York",
            snapshot,
            status,
            history: None,
        },
        LocationView {
            name: "Tromsø",
            snapshot: None,
            status,
            history: None,
        },
    ];
    build_view(&ViewInput {
        locations: &locations,
        active: 0,
        presentation,
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

fn sample() -> UiModel {
    to_ui(&vm(
        Some(&testdata::sample_snapshot()),
        &fresh(),
        &PresentationConfig::default(),
    ))
}

fn rain_at_night() -> UiModel {
    let mut m = sample();
    m.sky = whether_core::theme::SkyTheme::Rain as i32;
    m.current.icon = i32::try_from(whether_core::icon::IconId::ShowersNight.index()).unwrap();
    m.current.condition = "Rain showers".into();
    m
}

/// The sample with a flood warning and a wind advisory (alerts widget on).
fn alerts() -> UiModel {
    to_ui(&vm(
        Some(&testdata::sample_snapshot_with_alerts()),
        &fresh(),
        &testdata::presentation_with_alerts(),
    ))
}

/// The sample shown after refreshes failed: stale, with an error.
fn failing() -> UiModel {
    let status = StatusInfo {
        last_success: Some(testdata::SAMPLE_NOW),
        error: Some("every provider failed: connection timed out".into()),
        stale: true,
        refreshing: false,
    };
    to_ui(&vm(
        Some(&testdata::sample_snapshot()),
        &status,
        &PresentationConfig::default(),
    ))
}

fn imperial() -> UiModel {
    let p = PresentationConfig {
        units: UnitPrefs::IMPERIAL,
        ..PresentationConfig::default()
    };
    to_ui(&vm(Some(&testdata::sample_snapshot()), &fresh(), &p))
}

fn sparse() -> UiModel {
    to_ui(&vm(
        Some(&testdata::sample_snapshot_sparse()),
        &fresh(),
        &PresentationConfig::default(),
    ))
}

fn no_data() -> UiModel {
    let status = StatusInfo {
        last_success: None,
        error: Some("every provider failed".into()),
        stale: false,
        refreshing: false,
    };
    to_ui(&vm(None, &status, &PresentationConfig::default()))
}

fn decode(path: &Path) -> (u32, u32, Vec<u8>) {
    let decoder = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).unwrap()));
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    buf.truncate(info.buffer_size());
    (info.width, info.height, buf)
}

/// The share of pixels where any channel differs by more than 8.
fn difference(a: &Path, b: &Path) -> f64 {
    let (wa, ha, pa) = decode(a);
    let (wb, hb, pb) = decode(b);
    if (wa, ha) != (wb, hb) {
        return 1.0;
    }
    let differing = pa
        .chunks(3)
        .zip(pb.chunks(3))
        .filter(|(x, y)| x.iter().zip(y.iter()).any(|(p, q)| p.abs_diff(*q) > 8))
        .count();
    differing as f64 / f64::from(wa * ha)
}

fn check(name: &str, m: &UiModel, opts: &WindowOptions, size: (u32, u32)) {
    check_which(Which::Desktop, &format!("desktop-{name}"), m, opts, size);
}

fn check_which(which: Which, name: &str, m: &UiModel, opts: &WindowOptions, size: (u32, u32)) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out = root.join(format!("../../target/screenshots/{name}.png"));
    render_png(which, m, opts, size, &out).unwrap();
    let golden = root.join(format!("tests/golden/{name}.png"));
    if std::env::var_os("UPDATE_GOLDEN").is_some() || !golden.exists() {
        std::fs::copy(&out, &golden).unwrap();
        return;
    }
    let diff = difference(&out, &golden);
    assert!(
        diff <= 0.005,
        "{name}: {:.2} % of pixels differ (see {})",
        diff * 100.0,
        out.display()
    );
}

#[test]
fn desktop_goldens() {
    let window = WindowOptions::default();
    let kiosk = WindowOptions {
        auto_scale: true,
        ..WindowOptions::default()
    };
    check("clear-day-1000x720", &sample(), &window, (1000, 720));
    check(
        "rain-night-1000x720",
        &rain_at_night(),
        &window,
        (1000, 720),
    );
    check("narrow-760x900", &sample(), &window, (760, 900));
    check("1280x720", &sample(), &window, (1280, 720));
    check("1920x1080", &sample(), &window, (1920, 1080));
    check("kiosk-1920x1080", &sample(), &kiosk, (1920, 1080));
    check("2560x1440", &sample(), &window, (2560, 1440));
    check("imperial-1000x720", &imperial(), &window, (1000, 720));
    check("sparse-1000x720", &sparse(), &window, (1000, 720));
    check("no-data-1000x720", &no_data(), &window, (1000, 720));
}

/// A synthetic radar loop: pale land crossed by a river, and a storm south-west of
/// the marker with green edges and a red core (the latest frame is shown).
fn radar_images() -> &'static whether_display::RadarImages {
    use std::sync::Arc;
    use whether_core::time::UnixTime;
    use whether_display::{RadarFrame, RadarImages};
    let size = 192_i32;
    let mut base = Vec::new();
    for y in 0..size {
        for x in 0..size {
            let river = (y - x / 3 - 40).abs() < 4;
            base.extend_from_slice(if river {
                &[170, 211, 223, 255]
            } else {
                &[242, 239, 233, 255]
            });
        }
    }
    let frame = |shift: i32| {
        let mut rgba = Vec::new();
        for y in 0..size {
            for x in 0..size {
                let d2 = (x - 60 - shift).pow(2) + (y - 130).pow(2);
                rgba.extend_from_slice(match d2 {
                    d if d < 150 => &[230, 0, 0, 255],
                    d if d < 500 => &[255, 220, 0, 255],
                    d if d < 1400 => &[0, 190, 0, 200],
                    _ => &[0, 0, 0, 0],
                });
            }
        }
        Arc::<[u8]>::from(rgba)
    };
    Box::leak(Box::new(RadarImages {
        width: 192,
        height: 192,
        base: base.into(),
        frames: (0..6)
            .map(|i| RadarFrame {
                valid: UnixTime(testdata::SAMPLE_NOW.0 - (5 - i) * 300),
                rgba: frame(i32::try_from(i).unwrap() * 4),
            })
            .collect(),
        marker: (96, 96),
        attribution: whether_providers::radar::ATTRIBUTION,
    }))
}

/// The sample with radar labels for [`radar_images`].
fn with_radar() -> UiModel {
    let mut m = sample();
    m.radar_labels = ["10:35", "10:40", "10:45", "10:50", "10:55", "11:00"]
        .map(Into::into)
        .to_vec();
    m.radar_attribution = whether_providers::radar::ATTRIBUTION.into();
    m
}

/// Appearance from settings: a fixed dark sky, muted, with a mint accent.
#[test]
fn appearance_golden() {
    let gui: whether_gui::desktop::config::GuiConfig = toml::from_str(
        "dynamic_background = false\ntheme = \"dark\"\nsky_style = \"muted\"\naccent = \"#4DD0A6\"\n",
    )
    .unwrap();
    check(
        "appearance-dark-1000x720",
        &sample(),
        &gui.window_options(false),
        (1000, 720),
    );
}

/// A wide layout: radar first, the forecast in the middle,
/// everything else on the right, sun before details.
#[test]
fn custom_layout_golden() {
    let gui: whether_gui::desktop::config::GuiConfig = toml::from_str(
        "sections = [\"radar\", \"current\", \"hourly\", \"daily\", \"sun\", \"details\", \"air_quality\"]\n\
         [layout]\nwide = [[\"radar\", \"current\"], [\"hourly\", \"daily\"]]\n",
    )
    .unwrap();
    let window = gui.window_options(false);
    check_which(
        Which::DesktopRadar(radar_images()),
        "desktop-layout-1920x1080",
        &with_radar(),
        &window,
        (1920, 1080),
    );
}

/// The sample with ACIS history for 10001: the History card, the
/// normals and records detail cards, and the normals ticked on each day's range
/// bar. Days opened still show their hours,
/// night rain included.
fn with_history() -> UiModel {
    let snap = testdata::sample_snapshot();
    let history = testdata::history_10001();
    let status = fresh();
    let mut presentation = PresentationConfig::default();
    presentation
        .widgets
        .push(whether_core::presentation::WidgetKind::History);
    presentation.details.extend([
        whether_core::presentation::DetailKind::Normals,
        whether_core::presentation::DetailKind::Records,
    ]);
    let locations = [LocationView {
        name: "New York",
        snapshot: Some(&snap),
        status: &status,
        history: Some(&history),
    }];
    to_ui(&build_view(&ViewInput {
        locations: &locations,
        active: 0,
        presentation: &presentation,
        strings: &Strings::default(),
        now: testdata::SAMPLE_NOW,
    }))
}

#[test]
fn history_goldens() {
    let m = with_history();
    assert!(m.history.present);
    let window = WindowOptions::default();
    check("history-1000x720", &m, &window, (1000, 720));
    check("history-1920x1080", &m, &window, (1920, 1080));
}

#[test]
fn radar_goldens() {
    let window = WindowOptions::default();
    let radar = Which::DesktopRadar(radar_images());
    check_which(
        radar,
        "desktop-radar-1000x720",
        &with_radar(),
        &window,
        (1000, 720),
    );
    check_which(
        radar,
        "desktop-radar-1920x1080",
        &with_radar(),
        &window,
        (1920, 1080),
    );
    check_which(
        radar,
        "desktop-radar-narrow-760x900",
        &with_radar(),
        &window,
        (760, 900),
    );
}

#[test]
fn alert_goldens() {
    let window = WindowOptions::default();
    check("alerts-narrow-760x900", &alerts(), &window, (760, 900));
    check("alerts-1000x720", &alerts(), &window, (1000, 720));
    check("alerts-1920x1080", &alerts(), &window, (1920, 1080));
    check_which(
        Which::Compact(0),
        "compact-now-alerts-320x240",
        &alerts(),
        &window,
        (320, 240),
    );
    check_which(
        Which::Compact(3),
        "compact-alerts-320x240",
        &alerts(),
        &window,
        (320, 240),
    );
    check_which(
        Which::Compact(3),
        "compact-alerts-480x320",
        &alerts(),
        &window,
        (480, 320),
    );
}

#[test]
fn compact_goldens() {
    let window = WindowOptions::default();
    check_which(
        Which::Compact(0),
        "compact-now-320x240",
        &sample(),
        &window,
        (320, 240),
    );
    check_which(
        Which::Compact(1),
        "compact-forecast-320x240",
        &sample(),
        &window,
        (320, 240),
    );
    check_which(
        Which::Compact(2),
        "compact-details-320x240",
        &sample(),
        &window,
        (320, 240),
    );
    check_which(
        Which::Compact(0),
        "compact-now-480x320",
        &sample(),
        &window,
        (480, 320),
    );
    // A long error shortens the status, never the data credit.
    check_which(
        Which::Compact(0),
        "compact-now-error-320x240",
        &failing(),
        &window,
        (320, 240),
    );
}

/// the view in German (card titles and buttons are still English).
#[test]
fn german_goldens() {
    let de = Strings::localized("de", &std::collections::BTreeMap::new()).0;
    let m = to_ui(&vm_in(
        Some(&testdata::sample_snapshot()),
        &fresh(),
        &PresentationConfig::default(),
        &de,
    ));
    let window = WindowOptions::default();
    check("de-1000x720", &m, &window, (1000, 720));
    check_which(
        Which::Compact(1),
        "compact-forecast-de-320x240",
        &m,
        &window,
        (320, 240),
    );
}

#[test]
fn settings_dialog_goldens() {
    for (tab, name) in ["general", "forecast", "cards", "display"]
        .iter()
        .enumerate()
    {
        check_which(
            Which::DesktopSettings(i32::try_from(tab).unwrap()),
            &format!("desktop-settings-{name}-1000x720"),
            &sample(),
            &WindowOptions::default(),
            (1000, 720),
        );
    }
}

#[test]
fn search_overlay_golden() {
    let results: &'static [&'static str] = &[
        "Springfield, Illinois, US",
        "Springfield, Missouri, US",
        "Springfield, Massachusetts, US",
    ];
    check_which(
        Which::DesktopSearch(results),
        "desktop-search-1000x720",
        &sample(),
        &WindowOptions::default(),
        (1000, 720),
    );
}
