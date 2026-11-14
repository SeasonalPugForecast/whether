//! The storms views the Swift package decodes, kept in step with the core:
//! regenerate with `UPDATE_SAMPLE=1 cargo test -p whether-ffi`.

use std::path::{Path, PathBuf};

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

#[test]
fn the_swift_storms_views_are_the_core_snapshots() {
    for name in ["noreaster_imperial", "noreaster_metric", "dry_week"] {
        let snapshot = std::fs::read_to_string(path(
            SNAPSHOTS,
            &format!("whether_core__view__storms__tests__{name}.snap"),
        ))
        .unwrap();
        // `---`, the header, `---`, then the JSON.
        let body = snapshot.splitn(3, "---\n").nth(2).unwrap();
        check(
            &path(
                SWIFT_TESTS,
                &format!("storms-{}.json", name.replace('_', "-")),
            ),
            body,
        );
    }
}
