//! The storm outlooks the app's debug launches (`-storms`, `-hurricane`) show,
//! and the storms views the Swift package decodes, kept in step with the core:
//! regenerate with `UPDATE_SAMPLE=1 cargo test -p whether-ffi`.

use std::path::{Path, PathBuf};

use whether_core::model::ResolvedLocation;
use whether_core::presentation::{PresentationConfig, TimeFormat};
use whether_core::storm::hurricane::DRAWN_AIDS;
use whether_core::storm::testdata::{dry_week, noreaster, noreaster_past_runs};
use whether_core::storm::{StormOutlook, hurricane};
use whether_core::strings::Strings;
use whether_core::time::{UnixTime, UtcOffset};
use whether_core::units::UnitPrefs;
use whether_core::view::storms::storm_ahead;
use whether_providers::nhc::{parse_adeck, parse_bdeck, parse_current_storms};

/// The app's debug-only resources (left out of release builds).
const APP: &str = "../../ios/Whether/Whether/Debug";
/// The Swift package's test resources.
const SWIFT_TESTS: &str = "../../ios/WhetherKit/Tests/WhetherKitTests/Resources";
/// The core's insta snapshots of the storms view.
const SNAPSHOTS: &str = "../whether-core/src/view/snapshots";
/// The providers' insta snapshots of the storms view of NHC's recordings.
const NHC_SNAPSHOTS: &str = "../whether-providers/tests/snapshots";

fn path(dir: &str, file: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(dir).join(file)
}

/// Fails unless `path` holds `wanted`; writes it first with `UPDATE_SAMPLE`.
fn check(path: &Path, wanted: &str) {
    if std::env::var_os("UPDATE_SAMPLE").is_some() {
        std::fs::write(path, wanted).unwrap();
    }
    let saved = std::fs::read_to_string(path).unwrap_or_default();
    assert!(
        saved == wanted,
        "{} is out of date; run UPDATE_SAMPLE=1 cargo test -p whether-ffi",
        path.display()
    );
}

fn compact<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap() + "\n"
}

#[test]
fn the_app_storm_samples_are_current() {
    check(&path(APP, "storms-noreaster.json"), &compact(&noreaster()));
    check(
        &path(APP, "storms-noreaster-past-runs.json"),
        &compact(&noreaster_past_runs()),
    );
    check(&path(APP, "storms-dry-week.json"), &compact(&dry_week()));
}

/// NHC's recordings of Josephine (AL10 2026).
const NHC: &str = "../whether-providers/tests/fixtures/nhc";
/// Josephine's last official forecast started at 2026-10-22 18:00 UTC; the
/// sample is half an hour after its advisory, three hours later.
const JOSEPHINE_NOW: UnixTime = UnixTime(1_792_692_000 + 3 * 3_600 + 1_800);

/// A place on Josephine's official track (her 36-hour position), out at sea:
/// no coast lies in her cone. Its clock is New York's (EDT).
fn on_josephines_track() -> ResolvedLocation {
    ResolvedLocation {
        name: "Test Point".to_string(),
        latitude: 43.5,
        longitude: -55.9,
        utc_offset: UtcOffset(-4 * 3_600),
        tz_name: Some("America/New_York".to_string()),
        ..ResolvedLocation::default()
    }
}

/// Josephine from the recorded summary (hand-made) and decks, alone in an
/// outlook for the place on her track, fetched at the sample's time.
fn josephine_outlook() -> StormOutlook {
    let read = |file: &str| std::fs::read(path(NHC, file)).unwrap();
    let storms = parse_current_storms(&read("current_storms_josephine.synthetic.json")).unwrap();
    let forecast = parse_adeck(&read("aal102026.dat.gz"), &DRAWN_AIDS).unwrap();
    let past = parse_bdeck(&read("bal102026.dat")).unwrap();
    StormOutlook {
        place: on_josephines_track(),
        hurricanes: vec![storms[0].hurricane(forecast, past)],
        fetched_at: JOSEPHINE_NOW,
        ..StormOutlook::default()
    }
}

/// The app's `-hurricane` outlook and the Storm ahead card it gives
/// (`-stormahead hurricane`), in US units and a 12-hour clock as the app's
/// screenshots show them.
#[test]
fn the_app_hurricane_samples_are_current() {
    let outlook = josephine_outlook();
    let place = on_josephines_track();
    let cone = hurricane::cone(&outlook.hurricanes[0].official, outlook.hurricanes[0].basin);
    assert!(hurricane::ring_contains(
        &cone,
        (place.latitude, place.longitude)
    ));
    check(
        &path(APP, "storms-hurricane-josephine.json"),
        &compact(&outlook),
    );
    let presentation = PresentationConfig {
        units: UnitPrefs::IMPERIAL,
        time_format: TimeFormat::H12,
        ..PresentationConfig::default()
    };
    let card = storm_ahead(&outlook, &presentation, &Strings::default(), JOSEPHINE_NOW).unwrap();
    assert_eq!(card.hurricane.as_deref(), Some("al102026"));
    check(&path(APP, "storms-ahead-josephine.json"), &compact(&card));
}

/// The JSON of the insta snapshot `file` in `dir`.
fn body(dir: &str, file: &str) -> String {
    let snapshot = std::fs::read_to_string(path(dir, file)).unwrap();
    // `---`, the header, `---`, then the JSON.
    snapshot.splitn(3, "---\n").nth(2).unwrap().to_string()
}

/// The JSON of the core's insta snapshot `name` of the storms view.
fn snapshot_body(name: &str) -> String {
    body(
        SNAPSHOTS,
        &format!("whether_core__view__storms__tests__{name}.snap"),
    )
}

#[test]
fn the_swift_storms_views_are_the_core_snapshots() {
    for name in ["noreaster_imperial", "noreaster_metric", "dry_week"] {
        check(
            &path(
                SWIFT_TESTS,
                &format!("storms-{}.json", name.replace('_', "-")),
            ),
            &snapshot_body(name),
        );
    }
    // The Now tab's "Storm ahead" card for the nor'easter.
    check(
        &path(SWIFT_TESTS, "storm-ahead-noreaster-imperial.json"),
        &snapshot_body("storm_ahead_noreaster_imperial"),
    );
    // A hurricane's card.
    check(
        &path(SWIFT_TESTS, "storm-ahead-hurricane.json"),
        &snapshot_body("storm_ahead_hurricane"),
    );
}

#[test]
fn the_swift_hurricane_views_are_the_nhc_snapshots() {
    for name in [
        "josephine_imperial",
        "josephine_metric",
        "josephine_storm_ahead",
    ] {
        check(
            &path(
                SWIFT_TESTS,
                &format!("storms-{}.json", name.replace('_', "-")),
            ),
            &body(NHC_SNAPSHOTS, &format!("nhc__view__{name}.snap")),
        );
    }
}
