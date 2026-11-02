//! The sample view model the Swift package decodes (`ios/WhetherKit` tests), kept
//! in step with the core: regenerate with `UPDATE_SAMPLE=1 cargo test -p whether-ffi`.

use whether_core::testdata;
use whether_ffi::{LocationEntry, Place, Settings, default_settings, view_model};

const SAMPLE: &str = "../../ios/WhetherKit/Tests/WhetherKitTests/Resources/sample-view.json";

#[test]
fn the_swift_sample_is_current() {
    let snapshot = serde_json::to_string(&testdata::sample_snapshot_with_alerts()).unwrap();
    let settings = Settings {
        locale: Some("en_US".into()),
        ..default_settings()
    };
    // The history kept as `fetch_history` returns it, so the Swift mirrors of
    // the history section decode too.
    let kept = serde_json::json!({
        "today": testdata::SAMPLE_NOW.local_date(whether_core::time::UtcOffset(-4 * 3600)),
        "past_days": 7,
        "climate_days": 8,
        "records": true,
        "month_to_date": true,
        "history": testdata::history_10001(),
    })
    .to_string();
    let entry = |name: &str, snapshot_json: Option<String>| LocationEntry {
        history_json: snapshot_json.as_ref().map(|_| kept.clone()),
        place: Place {
            name: name.into(),
            latitude: 40.7143,
            longitude: -74.005_97,
            units: None,
            utc_offset_seconds: None,
            offset_changes: Vec::new(),
        },
        snapshot_json,
        last_success: Some(testdata::SAMPLE_NOW.0),
        error: None,
        refreshing: false,
    };
    let json = view_model(
        vec![entry("New York", Some(snapshot)), entry("Brooklyn", None)],
        0,
        settings,
        testdata::SAMPLE_NOW.0,
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let pretty = serde_json::to_string_pretty(&value).unwrap() + "\n";
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(SAMPLE);
    if std::env::var_os("UPDATE_SAMPLE").is_some() {
        std::fs::write(&path, &pretty).unwrap();
    }
    let saved = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        saved == pretty,
        "{} is out of date; run UPDATE_SAMPLE=1 cargo test -p whether-ffi",
        path.display()
    );
}
