//! The storm outlooks the app's debug launch (`-storms`) shows, and the storms
//! views the Swift package decodes, kept in step with the core: regenerate
//! with `UPDATE_SAMPLE=1 cargo test -p whether-ffi`.

use std::path::{Path, PathBuf};

use whether_core::storm::testdata::{dry_week, noreaster, noreaster_past_runs};

/// The app's debug-only resources (left out of release builds).
const APP: &str = "../../ios/Whether/Whether/Debug";
/// The Swift package's test resources.
const SWIFT_TESTS: &str = "../../ios/WhetherKit/Tests/WhetherKitTests/Resources";
/// The core's insta snapshots of the storms view.
const SNAPSHOTS: &str = "../whether-core/src/view/snapshots";

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

/// The JSON of the core's insta snapshot `name` of the storms view.
fn snapshot_body(name: &str) -> String {
    let snapshot = std::fs::read_to_string(path(
        SNAPSHOTS,
        &format!("whether_core__view__storms__tests__{name}.snap"),
    ))
    .unwrap();
    // `---`, the header, `---`, then the JSON.
    snapshot.splitn(3, "---\n").nth(2).unwrap().to_string()
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
}
