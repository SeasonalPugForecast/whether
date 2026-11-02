//! A Pirate Weather key through `fetch` and `view_model` as the app uses them: the
//! key goes in a header, the month's budget is counted, and the credit shows. The
//! fake host answers from recorded fixtures; nothing touches the network.

use std::sync::{Arc, Mutex};

use whether_core::model::ProviderId;
use whether_core::time::UnixTime;
use whether_ffi::{
    Cached, FetchHost, HostError, HostRequest, HostResponse, LocationEntry, Place, Settings,
    default_settings, default_sources, fetch, view_model,
};
use whether_providers::{Budget, UsageLedger};

const PIRATE: &str =
    include_str!("../../whether-providers/tests/fixtures/pirate_weather/forecast_10001.json");
const PIRATE_401: &str =
    include_str!("../../whether-providers/tests/fixtures/pirate_weather/error_401.json");
const OPEN_METEO: &str =
    include_str!("../../whether-providers/tests/fixtures/open_meteo/forecast_full_10001.json");
const AIR: &str =
    include_str!("../../whether-providers/tests/fixtures/open_meteo/air_quality_10001.json");
const NWS_NONE: &str = include_str!("../../whether-providers/tests/fixtures/nws/alerts_10001.json");
/// The `currently.time` of the recorded Pirate Weather forecast (2026-11-02).
const RECORDED_AT: i64 = 1_793_637_720;
const KEY: &str = "PIRATEKEY123";

/// Answers Pirate Weather with `pirate` (a status and body), Open-Meteo and NWS from
/// fixtures; records the requests.
struct Fake {
    pirate: (u16, &'static str),
    requests: Mutex<Vec<HostRequest>>,
    saved: Mutex<Vec<String>>,
}

impl Fake {
    fn new(pirate: (u16, &'static str)) -> Arc<Self> {
        Arc::new(Self {
            pirate,
            requests: Mutex::new(Vec::new()),
            saved: Mutex::new(Vec::new()),
        })
    }

    fn pirate_requests(&self) -> Vec<HostRequest> {
        let all = self.requests.lock().unwrap();
        all.iter()
            .filter(|r| r.url.contains("pirateweather"))
            .cloned()
            .collect()
    }
}

#[async_trait::async_trait]
impl FetchHost for Fake {
    async fn send(&self, request: HostRequest) -> Result<HostResponse, HostError> {
        self.requests.lock().unwrap().push(request.clone());
        let (status, body) = if request.url.contains("pirateweather") {
            self.pirate
        } else if request.url.contains("air-quality") {
            (200, AIR)
        } else if request.url.contains("api.weather.gov") {
            (200, NWS_NONE)
        } else {
            (200, OPEN_METEO)
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

/// What the app sends with a Pirate Weather key saved: the sources list, the key.
fn with_key() -> Settings {
    let mut s = Settings {
        locale: Some("en_US".into()),
        sources: default_sources(),
        ..default_settings()
    };
    s.keys.insert("pirate-weather".into(), KEY.into());
    s
}

fn run(
    settings: Settings,
    cached: Cached,
    host: &Arc<Fake>,
) -> Result<whether_ffi::FetchOutcome, whether_ffi::WhetherError> {
    pollster::block_on(fetch(
        new_york(),
        settings,
        cached,
        RECORDED_AT,
        Arc::clone(host) as Arc<dyn FetchHost>,
    ))
}

#[test]
fn a_key_makes_pirate_weather_serve_with_the_key_in_a_header() {
    let host = Fake::new((200, PIRATE));
    let out = run(with_key(), Cached::default(), &host).unwrap();
    assert_eq!(out.provider, "pirate-weather");
    let asked = host.pirate_requests();
    assert_eq!(asked.len(), 1, "one call serves everything");
    assert!(!asked[0].url.contains(KEY), "never in the URL");
    assert!(
        asked[0]
            .headers
            .iter()
            .any(|h| h.name == "apikey" && h.value == KEY),
        "{:?}",
        asked[0]
    );
    assert!(
        !format!("{:?}", asked[0]).contains(KEY),
        "Debug masks the header"
    );
    assert!(!format!("{out:?}").contains(KEY), "never in outcomes");
}

#[test]
fn the_month_is_counted_and_the_credit_shows() {
    let host = Fake::new((200, PIRATE));
    let out = run(with_key(), Cached::default(), &host).unwrap();
    // The ledger was saved before the metered call: one call in the month.
    let saved = host.saved.lock().unwrap().clone();
    assert_eq!(saved.len(), 1);
    let ledger: UsageLedger = serde_json::from_str(&saved[0]).unwrap();
    assert_eq!(
        ledger.used(ProviderId::PirateWeather, UnixTime(RECORDED_AT)),
        1
    );
    assert!(saved[0].contains(r#""period":"month""#), "{}", saved[0]);
    let text = whether_ffi::month_usage_text(
        Some(saved[0].clone()),
        "pirate-weather".into(),
        9000,
        RECORDED_AT,
    );
    assert_eq!(text, "1 of 9000 calls used this month");
    // The footer's credit comes from the snapshot's providers.
    let view: serde_json::Value = serde_json::from_str(
        &view_model(
            vec![LocationEntry {
                place: new_york(),
                snapshot_json: Some(out.snapshot_json),
                last_success: Some(RECORDED_AT),
                error: None,
                refreshing: false,
                history_json: None,
            }],
            0,
            with_key(),
            RECORDED_AT,
        )
        .unwrap(),
    )
    .unwrap();
    let attribution = view["attribution"].as_str().unwrap();
    assert!(attribution.contains("Pirate Weather"), "{attribution}");
}

#[test]
fn a_rejected_key_falls_back_and_a_full_month_skips_the_call() {
    // 401: the next source serves, and the failure names Pirate Weather.
    let host = Fake::new((401, PIRATE_401));
    let out = run(with_key(), Cached::default(), &host).unwrap();
    assert_eq!(out.provider, "open-meteo");
    let failed: Vec<_> = out.attempts.iter().filter(|a| a.error.is_some()).collect();
    assert_eq!(failed[0].provider, "pirate-weather");
    assert!(!format!("{out:?}").contains(KEY));

    // The month's budget used up: nothing is sent to Pirate Weather.
    let mut ledger = UsageLedger::new();
    ledger
        .try_spend(
            ProviderId::PirateWeather,
            Budget::monthly(50),
            50,
            UnixTime(RECORDED_AT),
        )
        .unwrap();
    let mut settings = with_key();
    settings.pirate_weather_monthly_calls = 50;
    let host = Fake::new((200, PIRATE));
    let cached = Cached {
        usage_json: Some(serde_json::to_string(&ledger).unwrap()),
        ..Cached::default()
    };
    let out = run(settings, cached, &host).unwrap();
    assert_eq!(out.provider, "open-meteo");
    assert_eq!(host.pirate_requests().len(), 0, "the budget is the limit");
}

#[test]
fn no_key_means_no_pirate_weather_request() {
    let host = Fake::new((200, PIRATE));
    let mut settings = with_key();
    settings.keys.clear();
    let out = run(settings, Cached::default(), &host).unwrap();
    assert_eq!(out.provider, "open-meteo");
    assert_eq!(host.pirate_requests().len(), 0);
    assert!(host.saved.lock().unwrap().is_empty(), "nothing metered");
}
