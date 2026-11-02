//! OpenWeather free API: planning and parsing against fixtures for ZIP 10001.

use proptest::prelude::*;
use whether_core::builder::SnapshotBuilder;
use whether_core::model::ProviderId;
use whether_core::requirements::DataRequirements;
use whether_core::time::{LocalDate, UnixTime};
use whether_providers::openweather::{OpenWeather, OpenWeatherSettings, OwmApi, forecast_steps};
use whether_providers::{
    Coordinates, Endpoint, FetchContext, FetchError, FetchSession, HttpResponse, Provider,
    ProviderError, redact_url,
};

const LOC: Coordinates = Coordinates {
    latitude: 40.7143,
    longitude: -74.005_97,
};
const WEATHER: &str = include_str!("fixtures/openweather/weather_10001.synthetic.json");
const FORECAST: &str = include_str!("fixtures/openweather/forecast_10001_cnt40.synthetic.json");
const AIR: &str = include_str!("fixtures/openweather/air_pollution_10001.synthetic.json");
const REC_WEATHER: &str = include_str!("fixtures/openweather/weather_10001.json");
const REC_FORECAST: &str = include_str!("fixtures/openweather/forecast_10001_cnt40.json");
const REC_FORECAST_8: &str = include_str!("fixtures/openweather/forecast_10001_cnt8.json");
const REC_AIR: &str = include_str!("fixtures/openweather/air_pollution_10001.json");
/// When the recorded files were fetched (their `dt`).
const RECORDED_AT: UnixTime = UnixTime(1_791_217_783);
const ERR_KEY: &str = include_str!("fixtures/openweather/error_401_invalid_key.json");
/// Hand-made after OpenWeather's documented 401 body (`{"cod":401,"message":"…"}`).
const KEY_REJECTED: &str = include_str!("fixtures/openweather/key_rejected.synthetic.json");
const ERR_ONECALL: &str = include_str!("fixtures/openweather/error_401_onecall_subscription.json");

fn ctx() -> FetchContext {
    FetchContext {
        now: UnixTime(1_791_212_400),
        user_agent: "whether-test".into(),
        utc_offset_hint: None,
        if_modified_since: None,
        language: "en".into(),
    }
}

fn provider() -> OpenWeather {
    OpenWeather::new(OpenWeatherSettings::free("TESTKEY"))
}

/// Fixture bodies for the current, forecast and air-quality endpoints.
struct Bodies<'a> {
    weather: &'a str,
    forecast: &'a str,
    air: &'a str,
}

const SYNTHETIC: Bodies<'static> = Bodies {
    weather: WEATHER,
    forecast: FORECAST,
    air: AIR,
};
const RECORDED: Bodies<'static> = Bodies {
    weather: REC_WEATHER,
    forecast: REC_FORECAST,
    air: REC_AIR,
};

/// Answers each planned request from `bodies` (or `override_body`) at time `now`.
fn fetch_from(
    bodies: &Bodies<'_>,
    override_body: Option<(u16, &str)>,
    req: DataRequirements,
    now: UnixTime,
) -> Result<whether_core::model::WeatherSnapshot, FetchError> {
    let p = provider();
    let chain: [&dyn Provider; 1] = [&p];
    let context = FetchContext { now, ..ctx() };
    let mut session = FetchSession::new(&chain, &[], LOC, req, context);
    while let Some(req) = session.next_request() {
        let (status, body) = override_body.unwrap_or(match req.endpoint {
            Endpoint::Current => (200, bodies.weather),
            Endpoint::Forecast => (200, bodies.forecast),
            _ => (200, bodies.air),
        });
        session.on_response(&HttpResponse::new(status, body.as_bytes()));
    }
    session
        .finish(UnixTime(now.0 + 60))
        .map(|(snapshot, _)| snapshot)
}

/// Answers each planned request from the synthetic fixtures (or `override_body`).
fn fetch(
    override_body: Option<(u16, &str)>,
) -> Result<whether_core::model::WeatherSnapshot, FetchError> {
    fetch_from(
        &SYNTHETIC,
        override_body,
        DataRequirements::full(),
        ctx().now,
    )
}

#[test]
fn planned_urls() {
    let requests = provider()
        .plan(&LOC, &DataRequirements::full(), &ctx())
        .unwrap();
    let urls: Vec<String> = requests.iter().map(|r| redact_url(&r.url)).collect();
    insta::assert_debug_snapshot!(urls);
    assert!(requests.iter().all(|r| r.url.contains("appid=TESTKEY")));
    assert!(!requests[2].required, "air quality is optional");
}

#[test]
fn no_air_quality_request_when_not_needed() {
    let req = DataRequirements {
        air_quality: false,
        ..DataRequirements::full()
    };
    let requests = provider().plan(&LOC, &req, &ctx()).unwrap();
    assert_eq!(requests.len(), 2);
}

#[test]
fn step_counts() {
    let req = |hours, days| DataRequirements {
        hourly_hours: hours,
        daily_days: days,
        ..Default::default()
    };
    assert_eq!(forecast_steps(&req(24, 5)), 40);
    assert_eq!(forecast_steps(&req(12, 0)), 4);
    assert_eq!(forecast_steps(&req(12, 1)), 8);
    assert_eq!(forecast_steps(&req(0, 0)), 1);
    assert_eq!(forecast_steps(&req(48, 16)), 40);
}

#[test]
fn normalized_snapshot() {
    let snapshot = fetch(None).unwrap();
    assert_eq!(snapshot.provider, ProviderId::OpenWeather);
    insta::assert_debug_snapshot!(snapshot);
}

#[test]
fn recorded_10001() {
    let s = fetch_from(&RECORDED, None, DataRequirements::full(), RECORDED_AT).unwrap();
    assert_eq!(s.location.name, "New York");
    assert_eq!(s.location.utc_offset.0, -14_400);
    let current = s.current.as_ref().unwrap();
    assert_eq!(current.time, RECORDED_AT);
    assert!(current.temperature.is_some() && current.wind_speed.is_some());
    assert_eq!(s.hourly.len(), 40, "40 three-hour steps");
    assert!(
        s.hourly
            .windows(2)
            .all(|w| w[1].time.0 - w[0].time.0 == 10_800)
    );
    assert!((5..=6).contains(&s.daily.len()));
    let today = s.daily[0].date;
    assert_eq!(today, RECORDED_AT.local_date(s.location.utc_offset));
    assert!(s.daily[0].sunrise.is_some() && s.daily[0].sunset.is_some());
    let aq = s.air_quality.as_ref().unwrap();
    assert!(aq.owm_index.is_some() && aq.pm2_5.is_some());
}

#[test]
fn recorded_10001_short_forecast() {
    let req = DataRequirements {
        hourly_hours: 24,
        daily_days: 1,
        ..DataRequirements::full()
    };
    let bodies = Bodies {
        forecast: REC_FORECAST_8,
        ..RECORDED
    };
    let s = fetch_from(&bodies, None, req, RECORDED_AT).unwrap();
    assert_eq!(s.hourly.len(), 8);
}

#[test]
fn daily_aggregation_respects_the_local_date() {
    let s = fetch(None).unwrap();
    // The first step is 14:00 EDT on Oct 5; 23:00 EDT (03:00 UTC on Oct 6) is still Oct 5.
    assert_eq!(
        s.daily[0].date,
        LocalDate {
            year: 2026,
            month: 10,
            day: 5
        }
    );
    let late = s
        .hourly
        .iter()
        .find(|h| h.time == UnixTime(1_791_255_600))
        .unwrap();
    assert_eq!(late.time.local_date(s.location.utc_offset).day, 5);
    assert_eq!(s.daily[0].sunrise, Some(UnixTime(1_791_198_192)));
    assert!(s.daily[1].sunrise.is_none(), "only today has sun times");
    assert_eq!(s.daily.len(), 6, "5 days of steps span 6 local dates");
    let max_pop = s.daily[0].precip_probability_max.unwrap().0;
    assert!((max_pop - 80.0).abs() < 1e-3, "{max_pop}");
}

#[test]
fn invalid_key_hint() {
    let Err(FetchError::AllProvidersFailed(attempts)) = fetch(Some((401, ERR_KEY))) else {
        panic!("expected failure");
    };
    let Some(ProviderError::Unauthorized(hint)) = &attempts[0].error else {
        panic!("{attempts:?}")
    };
    assert!(hint.contains("few hours to activate"), "{hint}");
}

#[test]
fn one_call_subscription_hint() {
    let p = provider();
    let error = p.explain(
        ProviderError::Unauthorized(
            serde_json::from_str::<serde_json::Value>(ERR_ONECALL).unwrap()["message"]
                .as_str()
                .unwrap()
                .into(),
        ),
        &HttpResponse::new(401, ERR_ONECALL.as_bytes()),
    );
    let ProviderError::Unauthorized(hint) = error else {
        panic!()
    };
    assert!(hint.contains("api = \"free\""), "{hint}");
}

#[test]
fn missing_key_and_one_call_are_config_errors() {
    let empty = OpenWeather::new(OpenWeatherSettings::free("  "));
    assert!(matches!(
        empty.plan(&LOC, &DataRequirements::full(), &ctx()),
        Err(ProviderError::Config(_))
    ));
    let one_call = OpenWeather::new(OpenWeatherSettings {
        api: OwmApi::OneCall3,
        ..OpenWeatherSettings::free("k")
    });
    assert!(matches!(
        one_call.plan(&LOC, &DataRequirements::full(), &ctx()),
        Err(ProviderError::Config(_))
    ));
}

#[test]
fn settings_debug_hides_the_key() {
    let debug = format!("{:?}", OpenWeatherSettings::free("SECRET42"));
    assert!(!debug.contains("SECRET42"));
}

#[test]
fn key_check_reads_the_answer() {
    use whether_providers::openweather::{KeyStatus, key_check_request, parse_key_check};
    let request = key_check_request("SECRET42");
    assert_eq!(request.endpoint, Endpoint::Current);
    assert!(!request.metered, "the check is never a One Call call");
    assert!(!redact_url(&request.url).contains("SECRET42"));
    assert_eq!(
        parse_key_check(200, REC_WEATHER.as_bytes()),
        KeyStatus::Accepted
    );
    assert_eq!(
        parse_key_check(401, KEY_REJECTED.as_bytes()),
        KeyStatus::Rejected
    );
    assert_eq!(
        parse_key_check(401, ERR_KEY.as_bytes()),
        KeyStatus::Rejected
    );
}

proptest! {
    #[test]
    fn arbitrary_bytes_never_panic(body in proptest::collection::vec(any::<u8>(), 0..512)) {
        let p = provider();
        for endpoint in [Endpoint::Current, Endpoint::Forecast, Endpoint::AirQuality] {
            let mut out = SnapshotBuilder::new(ProviderId::OpenWeather);
            let _ = p.parse(endpoint, &HttpResponse::new(200, &body), &ctx(), &mut out);
        }
    }
}
