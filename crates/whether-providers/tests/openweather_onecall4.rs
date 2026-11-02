//! OpenWeather One Call 4.0: planning, parsing, and the daily call budget.

use std::sync::LazyLock;

use whether_core::model::{Alert, ProviderId, WeatherSnapshot};
use whether_core::requirements::{DailyFields, DataRequirements, HourlyFields};
use whether_core::time::{UnixTime, UtcOffset};
use whether_providers::open_meteo::OpenMeteo;
use whether_providers::openweather::{
    DEFAULT_DAILY_CALL_LIMIT, FREE_DAILY_CALLS, OpenWeather, OpenWeatherSettings, onecall4,
};
use whether_providers::usage::paced_refresh_secs;
use whether_providers::{
    Attempt, Budget, Coordinates, Endpoint, FetchContext, FetchError, FetchSession, HttpResponse,
    Period, Provider, ProviderError, QuotaError, UsageLedger, redact_url,
};

const LOC: Coordinates = Coordinates {
    latitude: 40.7143,
    longitude: -74.005_97,
};
/// 2026-10-05 12:30 EDT, when the synthetic files start.
const NOW: UnixTime = UnixTime(1_791_217_800);

macro_rules! fixture {
    ($name:literal) => {
        include_str!(concat!("fixtures/openweather/", $name))
    };
}
/// One Call 4.0 response bodies and the time they describe.
struct OneCallFiles {
    now: UnixTime,
    current: &'static str,
    hourly: [&'static str; 3],
    daily: &'static str,
    nowcast: &'static str,
}

/// Hand-made files; tests that assert hand-picked values use them.
const SYNTHETIC: OneCallFiles = OneCallFiles {
    now: NOW,
    current: fixture!("onecall4_current_10001.synthetic.json"),
    hourly: [
        fixture!("onecall4_1h_p0_10001.synthetic.json"),
        fixture!("onecall4_1h_p1_10001.synthetic.json"),
        fixture!("onecall4_1h_p2_10001.synthetic.json"),
    ],
    daily: fixture!("onecall4_1day_10001.synthetic.json"),
    nowcast: fixture!("onecall4_15min_10001.synthetic.json"),
};

/// Responses recorded on 2026-06-28 with `cargo xtask fixtures --provider
/// openweather-onecall4`, at the time of their `current` block.
const RECORDED: OneCallFiles = OneCallFiles {
    now: UnixTime(1_791_474_885),
    current: fixture!("onecall4_current_10001.json"),
    hourly: [
        fixture!("onecall4_1h_p0_10001.json"),
        fixture!("onecall4_1h_p1_10001.json"),
        fixture!("onecall4_1h_p2_10001.json"),
    ],
    daily: fixture!("onecall4_1day_10001.json"),
    nowcast: fixture!("onecall4_15min_10001.json"),
};
/// Hand-made from the alert fields of the One Call 4.0 reference: `current` naming
/// one alert by id, and that alert's `/alert/{id}` details.
const ALERT_ID: &str = "8B46C632-DCA7-44D7-8BDF-02445621BAFF";
const WITH_ALERT: OneCallFiles = OneCallFiles {
    current: fixture!("onecall4_current_alerts_10001.synthetic.json"),
    ..SYNTHETIC
};
const ALERT: &str = fixture!("onecall4_alert_10001.synthetic.json");
const AIR: &str = fixture!("air_pollution_10001.json");
const ERR_401: &str = fixture!("error_401_onecall4_subscription.json");
const OM_FORECAST: &str = include_str!("fixtures/open_meteo/forecast_full_10001.json");
const OM_AIR: &str = include_str!("fixtures/open_meteo/air_quality_10001.json");

fn ctx(now: UnixTime) -> FetchContext {
    FetchContext {
        now,
        user_agent: "whether-test".into(),
        utc_offset_hint: None,
        if_modified_since: None,
        language: "en".into(),
    }
}

fn onecall(limit: u32) -> OpenWeather {
    OpenWeather::new(OpenWeatherSettings::onecall4("TESTKEY", limit))
}

/// The page index of an hourly request, from its `start` parameter.
fn page_of(url: &str, now: UnixTime) -> usize {
    let start: i64 = url
        .split("start=")
        .nth(1)
        .and_then(|rest| rest.split('&').next())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    usize::try_from((start - onecall4::hour_start(now).0) / (20 * 3600)).unwrap_or(0)
}

/// Answers a request from the synthetic fixtures.
fn answer(url: &str) -> (u16, &'static str) {
    answer_from(&SYNTHETIC, url)
}

/// Answers a request from `files`, with the shared air quality and Open-Meteo files.
fn answer_from(files: &OneCallFiles, url: &str) -> (u16, &'static str) {
    let body = if url.contains("/onecall/alert/") {
        ALERT
    } else if url.contains("/onecall/current") {
        files.current
    } else if url.contains("/timeline/1h") {
        files.hourly[page_of(url, files.now).min(2)]
    } else if url.contains("/timeline/1day") {
        files.daily
    } else if url.contains("/timeline/15min") {
        files.nowcast
    } else if url.contains("air_pollution") {
        AIR
    } else if url.contains("air-quality") {
        OM_AIR
    } else if url.contains("open-meteo") {
        om_forecast_for(files)
    } else {
        panic!("unexpected request {}", redact_url(url))
    };
    (200, body)
}

/// The Open-Meteo recording moved to the day of `files`. It is from November (EST)
/// and the One Call files from October (EDT), and a supplement only counts when the
/// UTC offsets and the times agree.
fn om_forecast_for(files: &OneCallFiles) -> &'static str {
    static SYNTHETIC_DAY: LazyLock<String> = LazyLock::new(|| moved_to(OM_FORECAST, NOW));
    static RECORDED_DAY: LazyLock<String> = LazyLock::new(|| moved_to(OM_FORECAST, RECORDED.now));
    if files.now == RECORDED.now {
        &RECORDED_DAY
    } else {
        &SYNTHETIC_DAY
    }
}

/// Moves a recorded Open-Meteo forecast by whole days to the local date of `now`,
/// with the One Call files' offset (EDT).
fn moved_to(body: &str, now: UnixTime) -> String {
    fn shift(v: &mut serde_json::Value, by: i64) {
        if let Some(t) = v.as_i64() {
            *v = (t - by).into();
        } else if let Some(items) = v.as_array_mut() {
            for x in items {
                shift(x, by);
            }
        }
    }
    const EDT: UtcOffset = UtcOffset(-14_400);
    let mut v: serde_json::Value = serde_json::from_str(body).unwrap();
    let recorded_at = UnixTime(v["current"]["time"].as_i64().unwrap());
    let recorded = UtcOffset(i32::try_from(v["utc_offset_seconds"].as_i64().unwrap()).unwrap());
    let days = recorded_at.local_date(recorded).days_since_epoch()
        - now.local_date(EDT).days_since_epoch();
    let by = days * 86_400;
    for (block, keys) in [
        ("current", &["time"][..]),
        ("hourly", &["time"]),
        ("minutely_15", &["time"]),
        (
            "daily",
            &["time", "sunrise", "sunset", "moonrise", "moonset"],
        ),
    ] {
        for key in keys {
            shift(&mut v[block][*key], by);
        }
    }
    v["utc_offset_seconds"] = EDT.0.into();
    v["timezone_abbreviation"] = "GMT-4".into();
    v.to_string()
}

struct Run {
    outcome: Result<(WeatherSnapshot, Vec<Attempt>), FetchError>,
    urls: Vec<String>,
    ledger: Option<UsageLedger>,
    /// One Call 4.0 requests sent.
    onecall_calls: usize,
}

/// One fetch with OpenWeather (One Call 4.0) first and Open-Meteo as fallback and
/// supplement.
fn fetch(
    owm: &OpenWeather,
    req: DataRequirements,
    ledger: Option<UsageLedger>,
    now: UnixTime,
    reply: impl Fn(&str) -> (u16, &'static str),
) -> Run {
    fetch_knowing(owm, req, ledger, now, Vec::new(), reply)
}

/// Like [`fetch`], with alerts from an earlier fetch.
fn fetch_knowing(
    owm: &OpenWeather,
    req: DataRequirements,
    ledger: Option<UsageLedger>,
    now: UnixTime,
    known: Vec<Alert>,
    reply: impl Fn(&str) -> (u16, &'static str),
) -> Run {
    let om = OpenMeteo::default();
    let chain: [&dyn Provider; 2] = [owm, &om];
    let supplements: [&dyn Provider; 1] = [&om];
    let mut session =
        FetchSession::new(&chain, &supplements, LOC, req, ctx(now)).with_known_alerts(known);
    if let Some(ledger) = ledger {
        session = session.with_usage(ledger);
    }
    let mut urls = Vec::new();
    while let Some(request) = session.next_request() {
        let url = request.url.clone();
        let (status, body) = reply(&url);
        urls.push(url);
        session.on_response(&HttpResponse::new(status, body.as_bytes()));
    }
    let ledger = session.usage().cloned();
    let onecall_calls = urls.iter().filter(|u| u.contains("/data/4.0/")).count();
    Run {
        outcome: session.finish(UnixTime(now.0 + 1)),
        urls,
        ledger,
        onecall_calls,
    }
}

#[test]
fn planned_requests() {
    let requests = onecall(900)
        .plan(&LOC, &DataRequirements::full(), &ctx(NOW))
        .unwrap();
    let summary: Vec<(String, Endpoint, bool, bool)> = requests
        .iter()
        .map(|r| (redact_url(&r.url), r.endpoint, r.required, r.metered))
        .collect();
    insta::assert_debug_snapshot!(summary);
    assert!(requests.iter().all(|r| r.url.contains("appid=TESTKEY")));
    let metered = requests.iter().filter(|r| r.metered).count();
    assert_eq!(
        metered as u32 + 1,
        onecall(900).metered_calls_per_fetch(&DataRequirements::full()),
        "pacing reserves one call for alert details"
    );
}

#[test]
fn only_needed_endpoints_are_planned() {
    let req = DataRequirements {
        hourly: HourlyFields::TEMPERATURE,
        hourly_hours: 12,
        daily: DailyFields::TEMP_MAX,
        daily_days: 5,
        ..Default::default()
    };
    let owm = onecall(900);
    let requests = owm.plan(&LOC, &req, &ctx(NOW)).unwrap();
    let endpoints: Vec<Endpoint> = requests.iter().map(|r| r.endpoint).collect();
    assert_eq!(
        endpoints,
        vec![Endpoint::Current, Endpoint::Hourly, Endpoint::Daily]
    );
    assert_eq!(owm.metered_calls_per_fetch(&req), 3);
}

#[test]
fn limits_are_capped_at_the_free_calls() {
    assert_eq!(onecall(900).budget(), Some(Budget::daily(900)));
    assert_eq!(
        onecall(5_000).budget(),
        Some(Budget::daily(FREE_DAILY_CALLS))
    );
    assert_eq!(
        OpenWeather::new(OpenWeatherSettings::free("K")).budget(),
        None
    );
    assert_eq!(
        OpenWeatherSettings::free("K").daily_call_limit,
        DEFAULT_DAILY_CALL_LIMIT
    );
}

#[test]
fn full_fetch_within_budget() {
    let run = fetch(
        &onecall(900),
        DataRequirements::full(),
        Some(UsageLedger::new()),
        NOW,
        answer,
    );
    let (s, attempts) = run.outcome.unwrap();
    assert!(attempts.iter().all(|a| a.error.is_none()), "{attempts:?}");
    assert_eq!(s.provider, ProviderId::OpenWeather);
    assert_eq!(s.location.utc_offset.0, -14_400);
    let current = s.current.as_ref().unwrap();
    assert_eq!(current.time, NOW);
    assert!(current.uv_index.is_some() && current.is_day);
    // Three 20-hour pages, with Open-Meteo's past day before them.
    let first = s
        .hourly
        .iter()
        .position(|h| h.time == onecall4::hour_start(NOW))
        .unwrap();
    assert_eq!((first, s.hourly.len()), (24, 24 + 60));
    assert!(
        s.hourly
            .windows(2)
            .all(|w| w[1].time.0 - w[0].time.0 == 3600)
    );
    assert_eq!(s.minutely_15.len(), 32, "eight hours");
    assert!(
        (s.minutely_15[3].precipitation.unwrap().0 - 0.3).abs() < 1e-6,
        "1.2 mm/h for 15 min"
    );
    assert_eq!(s.daily.len(), 10);
    assert_eq!(s.daily[0].date, NOW.local_date(s.location.utc_offset));
    assert!(s.daily[0].moon_phase.is_some() && s.daily[0].uv_index_max.is_some());
    assert!(
        s.daily[0].precip_sum.is_some(),
        "Open-Meteo supplements daily precipitation"
    );
    assert_eq!(s.air_quality.as_ref().unwrap().owm_index, Some(2));
    assert_eq!(run.onecall_calls, 6);
    assert_eq!(run.ledger.unwrap().used(ProviderId::OpenWeather, NOW), 6);
    insta::assert_debug_snapshot!((
        &s.current,
        &s.hourly[first],
        &s.daily[0],
        &s.minutely_15[..2]
    ));
}

/// The recorded responses parse into a complete snapshot. The `1h` timeline ends
/// 48 hours ahead, so the third page holds only 8 hours.
#[test]
fn recorded_responses_parse() {
    let now = RECORDED.now;
    let run = fetch(
        &onecall(900),
        DataRequirements::full(),
        Some(UsageLedger::new()),
        now,
        |url| answer_from(&RECORDED, url),
    );
    let (s, attempts) = run.outcome.unwrap();
    assert!(attempts.iter().all(|a| a.error.is_none()), "{attempts:?}");
    assert_eq!(s.provider, ProviderId::OpenWeather);
    assert_eq!(s.location.utc_offset.0, -14_400);
    let current = s.current.as_ref().unwrap();
    assert_eq!(current.time, now);
    assert!(current.is_day && current.uv_index.is_some());
    // One Call's 48 hours, with Open-Meteo's past hours before them.
    let first = s
        .hourly
        .iter()
        .position(|h| h.time == onecall4::hour_start(now))
        .unwrap();
    assert!(s.hourly.len() >= first + 48, "{} hours", s.hourly.len());
    assert!(
        s.hourly[first..first + 48]
            .windows(2)
            .all(|w| w[1].time.0 - w[0].time.0 == 3600)
    );
    let plausible =
        |c: Option<whether_core::units::Celsius>| c.is_some_and(|c| (-40.0..50.0).contains(&c.0));
    assert!(plausible(current.temperature));
    assert!(s.hourly.iter().all(|h| plausible(h.temperature)));
    assert_eq!(s.minutely_15.len(), 32, "the first eight hours");
    assert!(
        s.minutely_15
            .windows(2)
            .all(|w| w[1].time.0 - w[0].time.0 == 900)
    );
    assert_eq!(s.daily.len(), 10);
    assert_eq!(s.daily[0].date, now.local_date(s.location.utc_offset));
    assert!(
        s.daily
            .iter()
            .all(|d| plausible(d.temp_max) && plausible(d.temp_min))
    );
    assert_eq!(run.onecall_calls, 6);
    insta::assert_debug_snapshot!((
        &s.current,
        &s.hourly[first],
        &s.daily[0],
        &s.minutely_15[..2]
    ));
}

#[test]
fn no_ledger_means_no_one_call_requests() {
    let run = fetch(&onecall(900), DataRequirements::full(), None, NOW, answer);
    assert_eq!(run.onecall_calls, 0);
    let (s, attempts) = run.outcome.unwrap();
    assert_eq!(
        s.provider,
        ProviderId::OpenMeteo,
        "Open-Meteo serves instead"
    );
    assert_eq!(
        attempts[0].error,
        Some(ProviderError::Quota(QuotaError::NoLedger))
    );
}

#[test]
fn a_tight_budget_keeps_the_most_important_calls() {
    let owm = onecall(10);
    let mut ledger = UsageLedger::new();
    for _ in 0..8 {
        ledger
            .try_spend(ProviderId::OpenWeather, Budget::daily(10), 1, NOW)
            .unwrap();
    }
    let run = fetch(&owm, DataRequirements::full(), Some(ledger), NOW, answer);
    let sent: Vec<&str> = run
        .urls
        .iter()
        .filter(|u| u.contains("/data/4.0/"))
        .map(|u| {
            if u.contains("/current") {
                "current"
            } else {
                "1h"
            }
        })
        .collect();
    assert_eq!(
        sent,
        vec!["current", "1h"],
        "current and the first hourly page"
    );
    assert_eq!(run.ledger.unwrap().used(ProviderId::OpenWeather, NOW), 10);
    let (s, _) = run.outcome.unwrap();
    assert_eq!(s.provider, ProviderId::OpenWeather);
    assert_eq!(s.daily.len(), 7, "days come from the Open-Meteo supplement");
    assert!(s.air_quality.is_some(), "air pollution is not metered");
}

#[test]
fn an_exhausted_budget_falls_back_to_open_meteo() {
    let owm = onecall(3);
    let mut ledger = Some(UsageLedger::new());
    let mut total = 0;
    for _ in 0..4 {
        let run = fetch(&owm, DataRequirements::full(), ledger, NOW, answer);
        total += run.onecall_calls;
        assert!(run.outcome.is_ok());
        ledger = run.ledger;
    }
    assert_eq!(total, 3, "never more than the limit");
}

#[test]
fn a_whole_day_at_the_paced_interval_stays_within_the_limit() {
    let owm = onecall(DEFAULT_DAILY_CALL_LIMIT);
    let req = DataRequirements::full();
    let interval = paced_refresh_secs(
        owm.metered_calls_per_fetch(&req),
        1,
        DEFAULT_DAILY_CALL_LIMIT,
    );
    let midnight = NOW.0 - NOW.0.rem_euclid(86_400);
    let mut ledger = Some(UsageLedger::new());
    let (mut sent, mut refused) = (0, 0);
    let mut t = midnight;
    while t < midnight + 86_400 {
        let run = fetch(&owm, req, ledger, UnixTime(t), answer);
        sent += run.onecall_calls;
        let (_, attempts) = run.outcome.unwrap();
        refused += attempts
            .iter()
            .filter(|a| {
                matches!(
                    a.error,
                    Some(ProviderError::Quota(QuotaError::Exhausted { .. }))
                )
            })
            .count();
        ledger = run.ledger;
        t += i64::from(interval);
    }
    assert!(sent <= DEFAULT_DAILY_CALL_LIMIT as usize, "{sent}");
    assert_eq!(refused, 0, "pacing never runs into the limit");
    assert!(sent > 750, "the budget is used, not wasted: {sent}");
}

/// The worst case for the alert reserve: every fetch names an alert it has to look
/// up. The day still fits the budget without a refused call.
#[test]
fn a_new_alert_every_fetch_stays_within_the_limit() {
    let owm = onecall(DEFAULT_DAILY_CALL_LIMIT);
    let req = DataRequirements::full();
    let interval = paced_refresh_secs(
        owm.metered_calls_per_fetch(&req),
        1,
        DEFAULT_DAILY_CALL_LIMIT,
    );
    let midnight = NOW.0 - NOW.0.rem_euclid(86_400);
    let mut ledger = Some(UsageLedger::new());
    let (mut sent, mut refused) = (0, 0);
    let mut t = midnight;
    while t < midnight + 86_400 {
        let run = fetch(&owm, req, ledger, UnixTime(t), |url| {
            answer_from(&WITH_ALERT, url)
        });
        sent += run.onecall_calls;
        let (_, attempts) = run.outcome.unwrap();
        refused += attempts
            .iter()
            .filter(|a| {
                matches!(
                    a.error,
                    Some(ProviderError::Quota(QuotaError::Exhausted { .. }))
                )
            })
            .count();
        ledger = run.ledger;
        t += i64::from(interval);
    }
    assert!(sent <= DEFAULT_DAILY_CALL_LIMIT as usize, "{sent}");
    assert_eq!(refused, 0, "the reserve covers one alert per fetch");
    assert!(sent > 850, "the budget is used, not wasted: {sent}");
}

#[test]
fn alert_details_follow_the_current_conditions() {
    let run = fetch(
        &onecall(900),
        DataRequirements::full(),
        Some(UsageLedger::new()),
        NOW,
        |url| answer_from(&WITH_ALERT, url),
    );
    assert_eq!(
        redact_url(&run.urls[1]),
        format!("https://api.openweathermap.org/data/4.0/onecall/alert/{ALERT_ID}?appid=***")
    );
    assert_eq!(run.onecall_calls, 7);
    let (s, attempts) = run.outcome.unwrap();
    assert!(attempts.iter().all(|a| a.error.is_none()), "{attempts:?}");
    assert_eq!(s.alerts.len(), 1);
    insta::assert_debug_snapshot!(s.alerts[0]);
}

#[test]
fn a_known_alert_costs_no_call() {
    let first = fetch(
        &onecall(900),
        DataRequirements::full(),
        Some(UsageLedger::new()),
        NOW,
        |url| answer_from(&WITH_ALERT, url),
    );
    let known = first.outcome.unwrap().0.alerts;
    let later = UnixTime(NOW.0 + 900);
    let run = fetch_knowing(
        &onecall(900),
        DataRequirements::full(),
        Some(UsageLedger::new()),
        later,
        known.clone(),
        |url| answer_from(&WITH_ALERT, url),
    );
    assert!(run.urls.iter().all(|u| !u.contains("/alert/")));
    assert_eq!(run.onecall_calls, 6);
    assert_eq!(run.outcome.unwrap().0.alerts, known);
}

#[test]
fn alerts_cost_nothing_when_not_shown() {
    let req = DataRequirements {
        alerts: false,
        ..DataRequirements::full()
    };
    let run = fetch(&onecall(900), req, Some(UsageLedger::new()), NOW, |url| {
        answer_from(&WITH_ALERT, url)
    });
    assert_eq!(run.onecall_calls, 6);
    assert_eq!(run.outcome.unwrap().0.alerts, vec![]);
}

#[test]
fn no_calls_while_the_daily_counter_resets() {
    let just_after_midnight = UnixTime(NOW.0 - NOW.0.rem_euclid(86_400) + 120);
    let run = fetch(
        &onecall(900),
        DataRequirements::full(),
        Some(UsageLedger::new()),
        just_after_midnight,
        answer,
    );
    assert_eq!(run.onecall_calls, 0);
    let (s, attempts) = run.outcome.unwrap();
    assert_eq!(s.provider, ProviderId::OpenMeteo);
    assert_eq!(
        attempts[0].error,
        Some(ProviderError::Quota(QuotaError::ResetWindow))
    );
}

#[test]
fn missing_subscription_hint() {
    let reply = |url: &str| {
        if url.contains("/data/4.0/") {
            (401, ERR_401)
        } else {
            answer(url)
        }
    };
    let run = fetch(
        &onecall(900),
        DataRequirements::full(),
        Some(UsageLedger::new()),
        NOW,
        reply,
    );
    let (s, attempts) = run.outcome.unwrap();
    assert_eq!(s.provider, ProviderId::OpenMeteo);
    let Some(ProviderError::Unauthorized(hint)) = &attempts[0].error else {
        panic!("{attempts:?}")
    };
    assert!(
        hint.contains("One Call by Call") && hint.contains("api = \"free\""),
        "{hint}"
    );
    assert_eq!(run.onecall_calls, 1, "the failed call still counts");
}

#[test]
fn server_side_limit_is_reported_as_quota() {
    let reply = |url: &str| {
        if url.contains("/data/4.0/") {
            (429, r#"{"cod":429,"message":"limit"}"#)
        } else {
            answer(url)
        }
    };
    let run = fetch(
        &onecall(900),
        DataRequirements::full(),
        Some(UsageLedger::new()),
        NOW,
        reply,
    );
    let (_, attempts) = run.outcome.unwrap();
    assert_eq!(
        attempts[0].error,
        Some(ProviderError::Quota(QuotaError::Exhausted {
            limit: 900,
            period: Period::Day
        }))
    );
}

#[test]
fn settings_debug_hides_the_key() {
    let text = format!("{:?}", OpenWeatherSettings::onecall4("SECRETKEY", 900));
    assert!(
        !text.contains("SECRETKEY") && text.contains("daily_call_limit: 900"),
        "{text}"
    );
}
