//! `fetch` and `view_model` as the app uses them, with a fake host answering from
//! recorded fixtures.

use std::sync::{Arc, Mutex};

use whether_ffi::{
    Cached, FetchHost, HostError, HostRequest, HostResponse, LocationEntry, Place, Settings, Units,
    WhetherError, default_settings, fetch, view_model,
};

const FORECAST: &str =
    include_str!("../../whether-providers/tests/fixtures/open_meteo/forecast_full_10001.json");
const AIR: &str =
    include_str!("../../whether-providers/tests/fixtures/open_meteo/air_quality_10001.json");
const NWS_NONE: &str = include_str!("../../whether-providers/tests/fixtures/nws/alerts_10001.json");
const OWM_401: &str =
    include_str!("../../whether-providers/tests/fixtures/openweather/error_401_invalid_key.json");
/// The `current.time` of the recorded Open-Meteo files.
const RECORDED_AT: i64 = 1_793_637_000;

/// Answers Open-Meteo and NWS from fixtures and OpenWeather with 401; records what
/// it saw.
struct Fake {
    urls: Mutex<Vec<String>>,
    saved: Mutex<Vec<String>>,
    save_works: bool,
}

impl Fake {
    fn new(save_works: bool) -> Arc<Self> {
        Arc::new(Self {
            urls: Mutex::new(Vec::new()),
            saved: Mutex::new(Vec::new()),
            save_works,
        })
    }

    fn urls(&self) -> Vec<String> {
        self.urls.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl FetchHost for Fake {
    async fn send(&self, request: HostRequest) -> Result<HostResponse, HostError> {
        self.urls.lock().unwrap().push(request.url.clone());
        let (status, body) = if request.url.contains("openweathermap") {
            (401, OWM_401)
        } else if request.url.contains("air-quality") {
            (200, AIR)
        } else if request.url.contains("api.weather.gov") {
            (200, NWS_NONE)
        } else {
            (200, FORECAST)
        };
        Ok(HostResponse {
            status,
            body: body.as_bytes().to_vec(),
            date: None,
            expires: None,
            last_modified: None,
        })
    }

    fn save_usage(&self, ledger_json: String) -> Result<(), HostError> {
        if !self.save_works {
            return Err(HostError::Failed {
                message: "disk full".into(),
            });
        }
        self.saved.lock().unwrap().push(ledger_json);
        Ok(())
    }
}

fn new_york() -> Place {
    Place {
        name: "New York".into(),
        latitude: 40.7143,
        longitude: -74.005_97,
        units: None,
        utc_offset_seconds: Some(-5 * 3600),
        offset_changes: Vec::new(),
    }
}

fn us_settings() -> Settings {
    Settings {
        locale: Some("en_US".into()),
        ..default_settings()
    }
}

fn run(settings: Settings, host: &Arc<Fake>) -> Result<whether_ffi::FetchOutcome, WhetherError> {
    pollster::block_on(fetch(
        new_york(),
        settings,
        Cached::default(),
        RECORDED_AT,
        Arc::clone(host) as Arc<dyn FetchHost>,
    ))
}

#[test]
fn open_meteo_without_a_key() {
    let host = Fake::new(true);
    let out = run(us_settings(), &host).unwrap();
    assert_eq!(out.provider, "open-meteo");
    assert!(out.attempts.iter().all(|a| a.error.is_none()));
    let urls = host.urls();
    assert_eq!(urls.len(), 3, "forecast, air quality, NWS alerts: {urls:?}");
    assert!(urls[2].starts_with("https://api.weather.gov/alerts/active?point="));
    assert!(
        urls[0].contains("latitude=40.71&"),
        "rounded to 2 decimals: {}",
        urls[0]
    );
    let json: serde_json::Value = serde_json::from_str(&out.snapshot_json).unwrap();
    assert_eq!(json["provider"], "open-meteo");
}

#[test]
fn a_rejected_key_falls_back_to_open_meteo() {
    let host = Fake::new(true);
    let settings = Settings {
        openweather_key: Some("BADKEY".into()),
        ..us_settings()
    };
    let out = run(settings, &host).unwrap();
    assert_eq!(out.provider, "open-meteo");
    let failed: Vec<_> = out.attempts.iter().filter(|a| a.error.is_some()).collect();
    assert_eq!(failed[0].provider, "openweather");
    assert!(
        !format!("{out:?}").contains("BADKEY"),
        "the key never appears in outcomes"
    );
}

#[test]
fn metered_requests_wait_for_the_ledger() {
    let one_call = Settings {
        openweather_key: Some("KEY".into()),
        one_call: true,
        ..us_settings()
    };
    let refusing = Fake::new(false);
    let out = run(one_call.clone(), &refusing).unwrap();
    assert!(
        refusing
            .urls()
            .iter()
            .all(|u| !u.contains("openweathermap")),
        "nothing metered is sent unless the ledger is saved"
    );
    assert_eq!(out.provider, "open-meteo");

    let saving = Fake::new(true);
    run(one_call, &saving).unwrap();
    let saved = saving.saved.lock().unwrap().clone();
    assert_ne!(saved, Vec::<String>::new(), "the ledger was saved");
    assert!(
        saving.urls().iter().any(|u| u.contains("openweathermap")),
        "sent after saving"
    );
    let ledger: serde_json::Value = serde_json::from_str(&saved[0]).unwrap();
    assert!(ledger.is_object(), "{ledger}");
}

#[test]
fn bad_cached_json_is_reported() {
    let host = Fake::new(true);
    let err = pollster::block_on(fetch(
        new_york(),
        us_settings(),
        Cached {
            snapshot_json: Some("{".into()),
            ..Cached::default()
        },
        RECORDED_AT,
        host as Arc<dyn FetchHost>,
    ))
    .unwrap_err();
    assert!(matches!(err, WhetherError::InvalidJson { .. }), "{err:?}");
}

#[test]
fn the_view_model_is_formatted_per_place() {
    let host = Fake::new(true);
    let out = run(us_settings(), &host).unwrap();
    let entry = |place: Place| LocationEntry {
        place,
        snapshot_json: Some(out.snapshot_json.clone()),
        last_success: Some(RECORDED_AT),
        error: None,
        refreshing: false,
        history_json: None,
    };
    let abroad = Place {
        name: "Abroad".into(),
        units: Some(Units::Metric),
        ..new_york()
    };
    let locations = vec![entry(new_york()), entry(abroad)];
    let home: serde_json::Value = serde_json::from_str(
        &view_model(locations.clone(), 0, us_settings(), RECORDED_AT + 60).unwrap(),
    )
    .unwrap();
    let away: serde_json::Value =
        serde_json::from_str(&view_model(locations, 1, us_settings(), RECORDED_AT + 60).unwrap())
            .unwrap();
    assert_eq!(home["current"]["temperature"]["unit"], "°F");
    assert_eq!(away["current"]["temperature"]["unit"], "°C");
    assert_eq!(home["tabs"].as_array().unwrap().len(), 2);
    assert!(home["attribution"].as_str().unwrap().contains("Open-Meteo"));
    assert!(home["hourly"]["hours"].as_array().unwrap().len() >= 24);
    assert_eq!(home["status"]["stale"], false);
    let late: serde_json::Value = serde_json::from_str(
        &view_model(
            vec![LocationEntry {
                place: new_york(),
                snapshot_json: Some(out.snapshot_json.clone()),
                last_success: Some(RECORDED_AT),
                error: None,
                refreshing: false,
                history_json: None,
            }],
            0,
            us_settings(),
            RECORDED_AT + 3 * 3600,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        late["status"]["stale"], true,
        "older than twice the refresh"
    );
    let empty = view_model(Vec::new(), 0, us_settings(), RECORDED_AT).unwrap();
    assert!(empty.contains("\"tabs\":[]"));
}

#[test]
fn unreadable_history_leaves_the_forecast() {
    let host = Fake::new(true);
    let out = run(us_settings(), &host).unwrap();
    let json = view_model(
        vec![LocationEntry {
            place: new_york(),
            snapshot_json: Some(out.snapshot_json),
            last_success: Some(RECORDED_AT),
            error: None,
            refreshing: false,
            history_json: Some("{\"not\": \"history\"}".into()),
        }],
        0,
        us_settings(),
        RECORDED_AT + 60,
    )
    .unwrap();
    let vm: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(vm["current"].is_object());
    assert!(vm["history"].is_null());
}
