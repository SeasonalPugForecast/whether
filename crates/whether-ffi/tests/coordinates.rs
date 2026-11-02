//! Every coordinate that leaves the phone has at most 2 decimals: the
//! forecast, history and climate fetches (the plan and the widgets call these) are
//! run against a host that records each URL and POST body.

use std::sync::{Arc, Mutex};

use whether_ffi::{
    Cached, FetchHost, HostError, HostRequest, HostResponse, Place, Settings, Trip,
    default_settings, fetch, fetch_climate, fetch_history,
};

const FORECAST: &str =
    include_str!("../../whether-providers/tests/fixtures/open_meteo/forecast_full_10001.json");
const AIR: &str =
    include_str!("../../whether-providers/tests/fixtures/open_meteo/air_quality_10001.json");
const NWS_NONE: &str = include_str!("../../whether-providers/tests/fixtures/nws/alerts_10001.json");
const OWM_401: &str =
    include_str!("../../whether-providers/tests/fixtures/openweather/error_401_invalid_key.json");
const STATIONS: &str =
    include_str!("../../whether-providers/tests/fixtures/acis/stnmeta_10001.json");
const CLIMATE: &str =
    include_str!("../../whether-providers/tests/fixtures/acis/climate_NYC_0712_0719.json");
const NORMALS: &str = include_str!("../../whether-providers/tests/fixtures/acis/stndata_NYC.json");
const CHANCE: &str = include_str!("../../whether-providers/tests/fixtures/acis/chance_NYC.json");
const RECORDS_THR: &str =
    include_str!("../../whether-providers/tests/fixtures/acis/records_NYCthr.json");
const RECORDS: &str = include_str!("../../whether-providers/tests/fixtures/acis/records_NYC.json");
const PAST: &str =
    include_str!("../../whether-providers/tests/fixtures/open_meteo/history_past_10001.json");
const ARCHIVE: &str =
    include_str!("../../whether-providers/tests/fixtures/open_meteo/climate_10001_0712_0719.json");

/// The recorded Open-Meteo `current.time`.
const RECORDED_AT: i64 = 1_793_637_000;
/// 2026-11-02 10:00 EST, the day the history fixtures were recorded.
const HISTORY_AT: i64 = 1_793_631_600;

/// Answers from the recorded fixtures and keeps every request.
#[derive(Default)]
struct Recording {
    seen: Mutex<Vec<HostRequest>>,
}

#[async_trait::async_trait]
impl FetchHost for Recording {
    async fn send(&self, request: HostRequest) -> Result<HostResponse, HostError> {
        let url = request.url.clone();
        let body = request.body.clone().unwrap_or_default();
        self.seen.lock().unwrap().push(request);
        let text = if url.contains("openweathermap") {
            return Ok(answer(401, OWM_401));
        } else if url.contains("air-quality") {
            AIR
        } else if url.contains("api.weather.gov") {
            NWS_NONE
        } else if url.ends_with("StnMeta") {
            STATIONS
        } else if url.ends_with("StnData") {
            if body.contains("\"mean\"") {
                CLIMATE
            } else if body.contains("normal") {
                NORMALS
            } else if body.contains("cnt_ge") {
                CHANCE
            } else if body.contains("NYCthr") {
                RECORDS_THR
            } else {
                RECORDS
            }
        } else if url.contains("historical-forecast") {
            PAST
        } else if url.contains("archive-api") {
            ARCHIVE
        } else {
            FORECAST
        };
        Ok(answer(200, text))
    }

    fn save_usage(&self, _ledger_json: String) -> Result<(), HostError> {
        Ok(())
    }
}

fn answer(status: u16, body: &str) -> HostResponse {
    HostResponse {
        status,
        body: body.as_bytes().to_vec(),
        date: None,
        expires: None,
        last_modified: None,
    }
}

/// The rounded place (latitude, longitude without its sign).
const PLACE: [f64; 2] = [40.71, 74.01];
/// ACIS looks for stations in a box of 0.6 degrees around the rounded place.
const BOX: [f64; 2] = [40.11, 74.61];

/// New York to six decimals: the saved place keeps all of them.
fn place() -> Place {
    Place {
        name: "New York".into(),
        latitude: 40.714_268,
        longitude: -74.005_973,
        units: None,
        utc_offset_seconds: Some(-5 * 3600),
        offset_changes: Vec::new(),
    }
}

fn settings() -> Settings {
    Settings {
        locale: Some("en_US".into()),
        // Rejected with a 401, so the request that carries coordinates is seen too.
        openweather_key: Some("TESTKEY".into()),
        ..default_settings()
    }
}

/// Every decimal number in `text` as `(value, decimals without trailing zeros)`.
fn decimals(text: &str) -> Vec<(f64, usize)> {
    text.split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .filter_map(|token| {
            let (whole, fraction) = token.split_once('.')?;
            if whole.is_empty() || fraction.is_empty() || fraction.contains('.') {
                return None;
            }
            let value = token.parse::<f64>().ok()?;
            Some((value, fraction.trim_end_matches('0').len()))
        })
        .collect()
}

/// Checks each request of `seen`: no number has more than 2 decimals (the host and
/// the key are skipped), and the numbers `expected` (the rounded place, or the corner
/// of the ACIS search box around it) are among them.
fn assert_rounded(seen: &[HostRequest], what: &str, expected: [f64; 2]) {
    assert!(!seen.is_empty(), "{what}: no requests");
    let mut numbers = Vec::new();
    for request in seen {
        let after_host = request
            .url
            .split_once("://")
            .and_then(|(_, rest)| rest.split_once('/'))
            .map_or("", |(_, path)| path);
        let text = format!(
            "{after_host} {}",
            request.body.as_deref().unwrap_or_default()
        )
        .replace("%2C", ",")
        .replace("%2c", ",");
        for (value, places) in decimals(&text) {
            assert!(
                places <= 2,
                "{what}: {value} has {places} decimals in {}",
                request.url
            );
            numbers.push(value);
        }
    }
    let has = |x: f64| numbers.iter().any(|n| (n - x).abs() < 1e-9);
    assert!(
        has(expected[0]) && has(expected[1]),
        "{what}: {expected:?} is sent: {numbers:?}"
    );
    let all = seen
        .iter()
        .map(|r| format!("{} {}", r.url, r.body.as_deref().unwrap_or_default()))
        .collect::<Vec<_>>()
        .join(" ");
    for full in ["40.714268", "74.005973", "40.7142", "74.0059"] {
        assert!(!all.contains(full), "{what}: {full} leaked");
    }
}

fn trip() -> Trip {
    Trip {
        from_year: 2027,
        from_month: 7,
        from_day: 12,
        to_year: 2027,
        to_month: 7,
        to_day: 19,
        first_year: 1995,
        last_year: 2024,
    }
}

#[test]
fn the_scanner_counts_decimals() {
    assert_eq!(decimals("lat=40.7100&x=3.50"), vec![(40.71, 2), (3.5, 1)]);
    assert_eq!(decimals("-74.6100,40.1"), vec![(74.61, 2), (40.1, 1)]);
    assert_eq!(decimals("a1991-01-01 v2").len(), 0);
    assert_eq!(decimals("40.714268")[0].1, 6, "the scanner sees six");
}

#[test]
fn a_forecast_sends_two_decimals() {
    let host = Arc::new(Recording::default());
    pollster::block_on(fetch(
        place(),
        settings(),
        Cached::default(),
        RECORDED_AT,
        Arc::clone(&host) as Arc<dyn FetchHost>,
    ))
    .unwrap();
    let seen = host.seen.lock().unwrap().clone();
    assert!(seen.iter().any(|r| r.url.contains("openweathermap")));
    assert!(seen.iter().any(|r| r.url.contains("open-meteo")));
    assert!(seen.iter().any(|r| r.url.contains("api.weather.gov")));
    assert_rounded(&seen, "forecast", PLACE);
}

#[test]
fn history_sends_two_decimals() {
    let host = Arc::new(Recording::default());
    pollster::block_on(fetch_history(
        place(),
        default_settings(),
        None,
        HISTORY_AT,
        Arc::clone(&host) as Arc<dyn FetchHost>,
    ))
    .unwrap();
    let seen = host.seen.lock().unwrap().clone();
    assert!(
        seen.iter().any(|r| r.url.ends_with("StnMeta")),
        "ACIS asked"
    );
    assert_rounded(&seen, "history", BOX);
}

#[test]
fn history_falls_back_to_open_meteo_with_two_decimals() {
    // ACIS answering nothing usable makes Open-Meteo's archive the source.
    struct NoAcis(Recording);
    #[async_trait::async_trait]
    impl FetchHost for NoAcis {
        async fn send(&self, request: HostRequest) -> Result<HostResponse, HostError> {
            if request.url.contains("rcc-acis") {
                self.0.seen.lock().unwrap().push(request);
                return Ok(answer(503, ""));
            }
            self.0.send(request).await
        }
        fn save_usage(&self, _: String) -> Result<(), HostError> {
            Ok(())
        }
    }
    let host = Arc::new(NoAcis(Recording::default()));
    pollster::block_on(fetch_history(
        place(),
        default_settings(),
        None,
        HISTORY_AT,
        Arc::clone(&host) as Arc<dyn FetchHost>,
    ))
    .unwrap();
    let seen = host.0.seen.lock().unwrap().clone();
    assert!(seen.iter().any(|r| r.url.contains("archive-api")));
    assert_rounded(&seen, "history via Open-Meteo", PLACE);
}

#[test]
fn a_climate_query_sends_two_decimals() {
    let host = Arc::new(Recording::default());
    pollster::block_on(fetch_climate(
        place(),
        default_settings(),
        trip(),
        HISTORY_AT,
        Arc::clone(&host) as Arc<dyn FetchHost>,
    ))
    .unwrap();
    let seen = host.seen.lock().unwrap().clone();
    assert!(
        seen.iter().any(|r| r.url.ends_with("StnMeta")),
        "ACIS asked"
    );
    assert_rounded(&seen, "climate", BOX);
}
