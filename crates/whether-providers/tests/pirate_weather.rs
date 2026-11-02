//! Pirate Weather against a recording (`cargo xtask fixtures --provider
//! pirate-weather`) and a hand-made storm. The recording is a dry, partly cloudy
//! afternoon with no alerts and rain later in the week;
//! `forecast_storm.synthetic.json` is built from the documented response
//! shape (docs.pirateweather.net, Data Blocks and Alerts) to cover rain, a nowcast,
//! thunder, snow and alerts.

use whether_core::condition::{Condition, Intensity};
use whether_core::model::{ProviderId, WeatherSnapshot};
use whether_core::requirements::DataRequirements;
use whether_core::scales::Level;
use whether_core::time::{LocalDate, UnixTime, UtcOffset};
use whether_providers::pirate_weather::{
    DEFAULT_MONTHLY_CALL_LIMIT, PirateWeather, PirateWeatherSettings,
};
use whether_providers::{
    Attempt, Coordinates, Endpoint, FetchContext, FetchError, FetchSession, HttpResponse, Period,
    Provider, ProviderError, QuotaError, UsageLedger, redact_url,
};

const LOC: Coordinates = Coordinates {
    latitude: 40.7143,
    longitude: -74.005_97,
};
const FORECAST: &str = include_str!("fixtures/pirate_weather/forecast_10001.json");
const ERROR_401: &str = include_str!("fixtures/pirate_weather/error_401.json");
/// `currently.time` of the recording (2026-11-02 16:42 UTC).
const RECORDED_AT: UnixTime = UnixTime(1_793_637_720);
/// `currently.time` of the storm: 22:00 local on 2026-10-25.
const STORM: &str = include_str!("fixtures/pirate_weather/forecast_storm.synthetic.json");
const STORM_AT: UnixTime = UnixTime(1_792_980_000);
const EST: UtcOffset = UtcOffset(-5 * 3600);

fn provider() -> PirateWeather {
    PirateWeather::new(PirateWeatherSettings::new("TESTKEY123"))
}

fn ctx(now: UnixTime) -> FetchContext {
    FetchContext {
        now,
        user_agent: "whether-test".into(),
        utc_offset_hint: None,
        if_modified_since: None,
        language: "en".into(),
    }
}

/// One fetch with Pirate Weather alone and a fresh ledger.
fn fetch(
    p: &PirateWeather,
    now: UnixTime,
    reply: (u16, &'static str),
    ledger: UsageLedger,
) -> (
    Result<(WeatherSnapshot, Vec<Attempt>), FetchError>,
    UsageLedger,
) {
    let chain: [&dyn Provider; 1] = [p];
    let mut session =
        FetchSession::new(&chain, &[], LOC, DataRequirements::full(), ctx(now)).with_usage(ledger);
    while let Some(request) = session.next_request() {
        assert_eq!(request.endpoint, Endpoint::Forecast);
        session.on_response(&HttpResponse::new(reply.0, reply.1.as_bytes()));
    }
    let ledger = session.usage().cloned().unwrap();
    (session.finish(UnixTime(now.0 + 30)), ledger)
}

fn snapshot(body: &'static str, now: UnixTime) -> WeatherSnapshot {
    let (outcome, _) = fetch(&provider(), now, (200, body), UsageLedger::new());
    let (snap, attempts) = outcome.unwrap();
    assert!(attempts.iter().all(|a| a.error.is_none()), "{attempts:?}");
    snap
}

#[test]
fn the_request_carries_the_key_in_a_header_only() {
    let requests = provider()
        .plan(&LOC, &DataRequirements::full(), &ctx(RECORDED_AT))
        .unwrap();
    assert_eq!(requests.len(), 1);
    let r = &requests[0];
    assert_eq!(
        r.url,
        "https://api.pirateweather.net/forecast/key/40.7143,-74.006\
         ?units=si&version=2&exclude=summary"
    );
    assert!(r.headers.contains(&("apikey", "TESTKEY123".to_string())));
    assert!(
        r.headers
            .contains(&("User-Agent", "whether-test".to_string()))
    );
    assert!(r.required && r.metered && r.endpoint == Endpoint::Forecast);
    let logged = format!("{r:?} {}", redact_url(&r.url));
    assert!(!logged.contains("TESTKEY123"), "{logged}");
    assert!(!r.url.contains("TESTKEY123"));
}

#[test]
fn the_request_asks_for_only_what_is_needed() {
    let mut p = PirateWeatherSettings::new("K");
    p.exclude_models = vec!["gfs".into(), " hrrr ".into(), "x&y=1".into(), String::new()];
    p.ai_models = true;
    let p = PirateWeather::new(p);
    let url = |req: &DataRequirements| p.plan(&LOC, req, &ctx(RECORDED_AT)).unwrap()[0].url.clone();
    let query = |u: String| u.split_once('?').unwrap().1.to_string();
    // Nothing but the current conditions.
    assert_eq!(
        query(url(&DataRequirements::default())),
        "units=si&version=2&exclude=summary,minutely,hourly,daily,alerts,gfs,hrrr&include=aimodels"
    );
    // A week of hours needs the extended hourly block.
    let week = DataRequirements {
        hourly_hours: 168,
        daily_days: 7,
        minutely_hours: 1,
        alerts: true,
        ..DataRequirements::default()
    };
    assert_eq!(
        query(url(&week)),
        "units=si&version=2&exclude=summary,gfs,hrrr&extend=hourly&include=aimodels"
    );
}

#[test]
fn a_missing_key_is_a_configuration_error() {
    let p = PirateWeather::new(PirateWeatherSettings::new("  "));
    assert!(matches!(
        p.plan(&LOC, &DataRequirements::full(), &ctx(RECORDED_AT)),
        Err(ProviderError::Config(_))
    ));
}

#[test]
fn the_budget_is_monthly_and_one_call_a_fetch() {
    let p = provider();
    assert_eq!(p.id(), ProviderId::PirateWeather);
    let budget = p.budget().unwrap();
    assert_eq!(
        (budget.period, budget.limit),
        (Period::Month, DEFAULT_MONTHLY_CALL_LIMIT)
    );
    assert_eq!(p.metered_calls_per_fetch(&DataRequirements::full()), 1);
    assert_eq!(p.metered_cost_per_fetch(&DataRequirements::full()), 1);
    // 9,000 a month paced over 31 days is about one fetch every 5 minutes.
    let secs = budget.paced_refresh_secs(1, 1);
    assert!((290..=300).contains(&secs), "{secs}");
    assert_eq!(p.min_refresh_secs(), 900);
}

#[test]
fn the_recorded_forecast_parses() {
    let snap = snapshot(FORECAST, RECORDED_AT);
    assert_eq!(snap.provider, ProviderId::PirateWeather);
    let l = &snap.location;
    assert_eq!(l.utc_offset, EST);
    assert_eq!(l.tz_name.as_deref(), Some("America/New_York"));
    assert_eq!(l.elevation_m, Some(19.0));
    assert_eq!((l.latitude, l.longitude), (40.7143, -74.006));

    let c = snap.current.as_ref().unwrap();
    assert_eq!(c.time, RECORDED_AT);
    assert_eq!(c.condition, Condition::PartlyCloudy);
    assert!(c.is_day, "between sunrise and sunset");
    assert_eq!(c.temperature.map(|t| t.0), Some(16.6));
    assert_eq!(c.apparent.map(|t| t.0), Some(14.31));
    assert_eq!(c.dew_point.map(|t| t.0), Some(4.61));
    assert_eq!(c.humidity.map(|h| h.0.round()), Some(45.0));
    assert_eq!(c.pressure_msl.map(|p| p.0), Some(1028.27));
    assert_eq!(c.cloud_cover.map(|p| p.0.round()), Some(54.0));
    assert_eq!(c.wind_speed.map(|w| w.0), Some(5.06));
    assert_eq!(c.wind_gust.map(|w| w.0), Some(7.68));
    assert_eq!(c.wind_dir.map(|d| d.0), Some(203.0));
    assert_eq!(c.visibility.map(|v| v.0.round()), Some(16_090.0));
    assert_eq!(c.uv_index, Some(2.97));
    assert_eq!(c.precipitation.map(|p| p.0), Some(0.0));

    assert_eq!(snap.hourly.len(), 48);
    assert!(
        snap.hourly
            .windows(2)
            .all(|w| w[1].time.0 - w[0].time.0 == 3600)
    );
    assert_eq!(snap.hourly[0].time, UnixTime(1_793_635_200));
    assert!(snap.hourly[0].is_day);
    // After the day's sunset, before the next sunrise.
    let night = snap
        .hourly
        .iter()
        .find(|h| h.time.0 == 1_793_664_000)
        .unwrap();
    assert!(!night.is_day);
    assert!(
        snap.hourly
            .iter()
            .all(|h| h.temperature.is_some() && h.precip_probability.is_some())
    );

    // Four 15-minute steps for the next hour, dry.
    assert_eq!(snap.minutely_15.len(), 4);
    assert!(
        snap.minutely_15
            .windows(2)
            .all(|w| w[1].time.0 - w[0].time.0 == 900)
    );
    assert!(
        snap.minutely_15
            .iter()
            .all(|m| m.precipitation == Some(whether_core::units::Mm(0.0)))
    );

    // Today and seven more days, by local date.
    assert_eq!(snap.daily.len(), 8);
    let today = &snap.daily[0];
    assert_eq!(
        today.date,
        LocalDate {
            year: 2026,
            month: 11,
            day: 2
        }
    );
    assert_eq!(
        snap.daily[1].date,
        LocalDate {
            year: 2026,
            month: 11,
            day: 3
        }
    );
    // A clear-day icon with 23% cloud.
    assert_eq!(today.condition, Condition::MainlyClear);
    assert_eq!(today.temp_max.map(|t| t.0), Some(17.85));
    assert_eq!(today.temp_min.map(|t| t.0), Some(11.65));
    assert_eq!(today.apparent_max.map(|t| t.0), Some(15.62));
    assert_eq!(today.sunrise, Some(UnixTime(1_793_618_868)));
    assert_eq!(today.sunset, Some(UnixTime(1_793_656_244)));
    assert_eq!(today.daylight_s, Some(37_376));
    assert_eq!(today.moon_phase, Some(0.77));
    assert_eq!(today.uv_index_max, Some(3.22));
    assert_eq!(today.wind_gust_max.map(|w| w.0), Some(5.03));
    assert_eq!(snap.alerts.len(), 0);
    insta::assert_debug_snapshot!(
        "pirate_weather_days",
        snap.daily
            .iter()
            .map(|d| (
                d.date,
                d.condition,
                d.temp_max.map(|t| t.0),
                d.temp_min.map(|t| t.0),
                d.precip_probability_max.map(|p| p.0.round()),
                d.precip_sum.map(|p| (p.0 * 10.0).round() / 10.0),
            ))
            .collect::<Vec<_>>()
    );
}

#[test]
fn rain_thunder_snow_a_nowcast_and_alerts() {
    let snap = snapshot(STORM, STORM_AT);
    let c = snap.current.as_ref().unwrap();
    assert_eq!(c.condition, Condition::Rain(Intensity::Moderate));
    assert!(
        !c.is_day,
        "22:00 local, and no complete sun times to say otherwise"
    );
    assert_eq!(c.precipitation.map(|p| p.0), Some(3.2));
    assert_eq!(c.humidity.map(|h| h.0.round()), Some(93.0));
    assert_eq!(c.visibility.map(|v| v.0.round()), Some(6_400.0));

    // Rates of 3, 6, 0 and 1.2 mm/h for a quarter hour each.
    let mm: Vec<f32> = snap
        .minutely_15
        .iter()
        .map(|m| m.precipitation.unwrap().0)
        .collect();
    for (got, want) in mm.iter().zip([0.75, 1.5, 0.0, 0.3]) {
        assert!((got - want).abs() < 1e-5, "{mm:?}");
    }
    assert_eq!(snap.minutely_15[0].time, STORM_AT);

    let conditions: Vec<Condition> = snap.hourly.iter().map(|h| h.condition).collect();
    assert_eq!(
        conditions,
        [
            Condition::Rain(Intensity::Moderate),
            Condition::Thunderstorm(Intensity::Heavy),
            Condition::Snow(Intensity::Light),
        ]
    );
    assert_eq!(
        snap.hourly[2].precip_probability.map(|p| p.0.round()),
        Some(30.0)
    );
    assert_eq!(snap.hourly[1].precipitation.map(|p| p.0), Some(9.0));

    let d = &snap.daily[0];
    assert_eq!(
        d.date,
        LocalDate {
            year: 2026,
            month: 10,
            day: 25
        }
    );
    // The day's strongest rate sets the strength of the icon's rain.
    assert_eq!(d.condition, Condition::Rain(Intensity::Heavy));
    assert_eq!(d.precip_sum.map(|p| p.0), Some(36.0));
    assert_eq!(d.precip_probability_max.map(|p| p.0), Some(100.0));
    assert_eq!(d.daylight_s, Some(39_000));
    // A day without sun times and with a clear icon.
    assert_eq!(snap.daily[1].condition, Condition::Clear);
    assert_eq!(snap.daily[1].sunrise, None);

    // The expired advisory and the untitled one are dropped.
    assert_eq!(snap.alerts.len(), 2);
    let flood = &snap.alerts[0];
    assert_eq!(flood.title, "Flood Watch");
    assert_eq!(
        flood.description,
        "Flooding of low-lying roads is possible."
    );
    assert_eq!(flood.severity, Level::Moderate);
    assert_eq!(
        flood.url.as_deref(),
        Some("https://example.invalid/alert/1")
    );
    assert_eq!(
        (flood.start.0, flood.end.0),
        (STORM_AT.0 - 7200, STORM_AT.0 + 43_200)
    );
    let tornado = &snap.alerts[1];
    assert_eq!(
        (tornado.title.as_str(), tornado.severity),
        ("Tornado Warning", Level::Extreme)
    );
    assert_eq!(
        tornado.end.0,
        STORM_AT.0 + 24 * 3600,
        "no expiry: until a fetch drops it"
    );
    assert_eq!(tornado.url, None, "only web links are kept");
}

#[test]
fn only_the_blocks_present_are_set() {
    let body = r#"{"latitude":40.0,"longitude":-75.0,"offset":5.5,
        "currently":{"time":1792951140,"icon":"wind","cloudCover":0.5,"temperature":20}}"#;
    let snap = snapshot(Box::leak(body.to_string().into_boxed_str()), RECORDED_AT);
    assert_eq!(snap.location.utc_offset, UtcOffset(19_800));
    assert_eq!(snap.location.tz_name, None);
    assert_eq!(snap.current.unwrap().condition, Condition::PartlyCloudy);
    assert!(snap.hourly.is_empty() && snap.daily.is_empty() && snap.minutely_15.is_empty());
}

#[test]
fn bad_bodies_are_malformed_not_panics() {
    for body in [
        "",
        "{",
        "[]",
        r#"{"latitude":1,"longitude":2}"#,
        r#"{"currently":{"time":1}}"#,
    ] {
        let (outcome, _) = fetch(&provider(), RECORDED_AT, (200, body), UsageLedger::new());
        let Err(FetchError::AllProvidersFailed(attempts)) = outcome else {
            panic!("{body:?} parsed");
        };
        assert!(
            matches!(attempts[0].error, Some(ProviderError::Malformed(_))),
            "{body:?}: {attempts:?}"
        );
    }
}

#[test]
fn a_rejected_key_gets_guidance() {
    let (outcome, ledger) = fetch(
        &provider(),
        RECORDED_AT,
        (401, ERROR_401),
        UsageLedger::new(),
    );
    let Err(FetchError::AllProvidersFailed(attempts)) = outcome else {
        panic!("a rejected key succeeded");
    };
    let Some(ProviderError::Unauthorized(hint)) = &attempts[0].error else {
        panic!("{attempts:?}")
    };
    assert!(
        hint.contains("API key") && hint.contains("Forecast API"),
        "{hint}"
    );
    assert_eq!(
        ledger.used(ProviderId::PirateWeather, RECORDED_AT),
        1,
        "the call still counts"
    );
}

#[test]
fn the_monthly_limit_stops_calls_and_a_429_says_so() {
    let mut settings = PirateWeatherSettings::new("K");
    settings.monthly_call_limit = 2;
    let p = PirateWeather::new(settings);
    let mut ledger = UsageLedger::new();
    for _ in 0..2 {
        let (outcome, l) = fetch(&p, RECORDED_AT, (200, FORECAST), ledger);
        outcome.unwrap();
        ledger = l;
    }
    assert_eq!(ledger.used(ProviderId::PirateWeather, RECORDED_AT), 2);
    let exhausted = Some(ProviderError::Quota(QuotaError::Exhausted {
        limit: 2,
        period: Period::Month,
    }));
    // The third call is never sent.
    let chain: [&dyn Provider; 1] = [&p];
    let mut session =
        FetchSession::new(&chain, &[], LOC, DataRequirements::full(), ctx(RECORDED_AT))
            .with_usage(ledger);
    assert!(session.next_request().is_none());
    let Err(FetchError::AllProvidersFailed(attempts)) = session.finish(RECORDED_AT) else {
        panic!("fetched beyond the budget");
    };
    assert_eq!(attempts[0].error, exhausted);
    // The next month starts a new count.
    let next_month = UnixTime(RECORDED_AT.0 + 7 * 86_400);
    // …and the server's own 429 (another device used the key) reads the same.
    let (outcome, _) = fetch(
        &p,
        next_month,
        (429, r#"{"message":"limit"}"#),
        UsageLedger::new(),
    );
    let Err(FetchError::AllProvidersFailed(attempts)) = outcome else {
        panic!("429 succeeded");
    };
    assert_eq!(attempts[0].error, exhausted);
}

#[test]
fn settings_debug_hides_the_key() {
    let text = format!("{:?}", PirateWeatherSettings::new("SECRETKEY"));
    assert!(
        !text.contains("SECRETKEY") && text.contains("monthly_call_limit: 9000"),
        "{text}"
    );
    assert!(!format!("{:?}", provider()).contains("TESTKEY123"));
}
