//! MET Norway against recorded fixtures (`cargo xtask fixtures --provider met-norway`).

use whether_core::condition::Condition;
use whether_core::model::{ProviderId, WeatherSnapshot};
use whether_core::requirements::DataRequirements;
use whether_core::time::{LocalDate, UnixTime, UtcOffset};
use whether_providers::met_norway::{MetNorway, MetNorwaySettings};
use whether_providers::{
    Attempt, Coordinates, Endpoint, FetchContext, FetchError, FetchSession, HttpResponse, Provider,
    ProviderError,
};

const LOC: Coordinates = Coordinates {
    latitude: 40.7143,
    longitude: -74.005_97,
};
const TROMSO: Coordinates = Coordinates {
    latitude: 69.6492,
    longitude: 18.9553,
};
const COMPLETE: &str = include_str!("fixtures/met_norway/complete_10001.json");
const TROMSO_COMPLETE: &str = include_str!("fixtures/met_norway/complete_tromso.json");
const SUN: &str = include_str!("fixtures/met_norway/sun_10001.json");
/// The first time step of the recordings (2026-10-06 19:00 UTC).
const RECORDED_AT: UnixTime = UnixTime(1_791_313_200);
const EDT: UtcOffset = UtcOffset(-4 * 3600);

fn provider() -> MetNorway {
    MetNorway::new(MetNorwaySettings::new("tests@example.org"))
}

fn ctx(offset: UtcOffset) -> FetchContext {
    FetchContext {
        now: RECORDED_AT,
        user_agent: "whether-test".into(),
        utc_offset_hint: Some(offset),
        if_modified_since: None,
        language: "en".into(),
    }
}

/// Fetches with MET Norway alone, answering each endpoint from `answer`.
fn fetch(
    loc: Coordinates,
    offset: UtcOffset,
    answer: impl Fn(Endpoint) -> (u16, &'static str),
) -> Result<(WeatherSnapshot, Vec<Attempt>), FetchError> {
    let p = provider();
    let chain: [&dyn Provider; 1] = [&p];
    let mut session = FetchSession::new(&chain, &[], loc, DataRequirements::full(), ctx(offset));
    while let Some(request) = session.next_request() {
        let (status, body) = answer(request.endpoint);
        session.on_response(&HttpResponse::new(status, body.as_bytes()));
    }
    session.finish(UnixTime(RECORDED_AT.0 + 60))
}

#[test]
fn forecast_and_sunrise() {
    let (snap, attempts) = fetch(LOC, EDT, |e| match e {
        Endpoint::Sun => (200, SUN),
        _ => (200, COMPLETE),
    })
    .unwrap();
    assert!(attempts.iter().all(|a| a.error.is_none()), "{attempts:?}");
    assert_eq!(snap.provider, ProviderId::MetNorway);
    assert_eq!(snap.location.utc_offset, EDT);
    assert_eq!(snap.location.elevation_m, Some(10.0));
    let c = snap.current.as_ref().unwrap();
    assert_eq!(c.time, RECORDED_AT);
    assert_eq!(c.temperature.map(|t| t.0), Some(15.5));
    assert_eq!(c.condition, Condition::Clear);
    assert!(c.is_day);
    // Hourly steps are the 1-hour periods (65 in this recording), in order.
    assert_eq!(snap.hourly.len(), 65);
    assert!(
        snap.hourly
            .windows(2)
            .all(|w| w[1].time.0 - w[0].time.0 == 3600)
    );
    // Days in local time; today has the recorded sunrise and sunset.
    let today = &snap.daily[0];
    assert_eq!(
        today.date,
        LocalDate {
            year: 2026,
            month: 10,
            day: 6
        }
    );
    let local = |t: UnixTime| {
        let l = t.to_local(EDT);
        (l.hour, l.minute)
    };
    assert_eq!(today.sunrise.map(local), Some((7, 4)));
    assert_eq!(today.sunset.map(local), Some((18, 36)));
    assert!(snap.daily.len() >= 9, "{}", snap.daily.len());
    assert!(snap.daily.iter().all(|d| {
        d.temp_max
            .zip(d.temp_min)
            .is_some_and(|(hi, lo)| hi.0 >= lo.0)
    }));
    insta::assert_debug_snapshot!(
        "met_norway_days",
        snap.daily
            .iter()
            .map(|d| (
                d.date,
                d.condition,
                d.temp_max.map(|t| t.0),
                d.temp_min.map(|t| t.0),
                d.precip_sum.map(|p| (p.0 * 10.0).round() / 10.0),
            ))
            .collect::<Vec<_>>()
    );
}

#[test]
fn tromso_in_its_own_time_zone() {
    let (snap, _) = fetch(TROMSO, UtcOffset(2 * 3600), |_| (200, TROMSO_COMPLETE)).unwrap();
    assert_eq!(snap.location.utc_offset, UtcOffset(2 * 3600));
    // 19:00 UTC is 21:00 in Tromsø: the first day is still the 6th.
    assert_eq!(snap.daily[0].date.day, 6);
    assert!(snap.current.is_some() && !snap.hourly.is_empty());
}

#[test]
fn not_modified_reaches_the_caller() {
    let result = fetch(LOC, EDT, |e| match e {
        Endpoint::Forecast => (304, ""),
        _ => (200, SUN),
    });
    assert_eq!(result.unwrap_err(), FetchError::NotModified);
}

#[test]
fn a_rejection_names_the_contact_setting() {
    let p = provider();
    let explained = p.explain(
        ProviderError::Unauthorized("Forbidden".into()),
        &HttpResponse::new(403, b""),
    );
    assert!(
        explained
            .to_string()
            .contains("[providers.met-norway] contact"),
        "{explained}"
    );
    assert!(p.honors_expires());
}
