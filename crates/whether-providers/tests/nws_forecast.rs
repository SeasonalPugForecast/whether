//! The NWS forecast: the point lookup, its follow-ups, the gridpoint
//! layers expanded to hours, and Open-Meteo filling what the NWS lacks. Values
//! were checked by hand against the recorded files (ZIP 10001, 2026-11-02).

use whether_core::condition::{Condition, Intensity};
use whether_core::model::{ProviderId, WeatherSnapshot};
use whether_core::requirements::DataRequirements;
use whether_core::time::{LocalDate, UnixTime, UtcOffset};
use whether_providers::nws::Nws;
use whether_providers::open_meteo::OpenMeteo;
use whether_providers::{
    Attempt, Coordinates, Endpoint, FetchContext, FetchError, FetchSession, HttpResponse, Provider,
    ProviderError,
};

const LOC: Coordinates = Coordinates {
    latitude: 40.7143,
    longitude: -74.005_97,
};
const POINT: &str = include_str!("fixtures/nws/points_10001.json");
const GRID: &str = include_str!("fixtures/nws/gridpoint_OKX_33_42.json");
const PERIODS: &str = include_str!("fixtures/nws/forecast_OKX_33_42.json");
const ALERTS: &str = include_str!("fixtures/nws/alerts_10001.json");
const STATIONS: &str = include_str!("fixtures/nws/stations_OKX_33_42.json");
/// Central Park at 14:51 UTC: 15 °C and clear, with no wind reported.
const OBSERVATION: &str = include_str!("fixtures/nws/observation_KNYC.json");
/// The observation's time, 2026-11-02 14:51 UTC.
const OBSERVED_AT: UnixTime = UnixTime(1_793_631_060);
/// Ten minutes after the observation.
const OBSERVED_NOW: UnixTime = UnixTime(OBSERVED_AT.0 + 600);
const OPEN_METEO: &str = include_str!("fixtures/open_meteo/forecast_full_10001_with_nws.json");
/// The recording, 2026-11-02 16:30 UTC (11:30 AM in New York): the gridpoint
/// was updated at 11:19:51 UTC, and the observation is 99 minutes old, too old
/// to be "now".
const NOW: UnixTime = UnixTime(1_793_637_000);
/// The hour `NOW` falls in, 16:00 UTC (11 AM).
const NOW_HOUR: UnixTime = UnixTime(1_793_635_200);
/// The gridpoint's `updateTime`, 2026-11-02 11:19:51 UTC.
const GRID_UPDATED: i64 = 1_793_618_391;
const EST: UtcOffset = UtcOffset(-5 * 3600);

fn ctx(now: UnixTime) -> FetchContext {
    FetchContext {
        now,
        user_agent: "whether-test".into(),
        utc_offset_hint: Some(EST),
        if_modified_since: None,
        language: "en".into(),
    }
}

type Outcome = Result<(WeatherSnapshot, Vec<Attempt>), FetchError>;

/// The NWS forecast first, Open-Meteo as fallback and supplement, at `now`.
fn fetch(now: UnixTime, grid_status: u16) -> (Outcome, Vec<String>) {
    fetch_with(
        &Nws::forecaster("https://api.weather.gov"),
        now,
        grid_status,
        OBSERVATION,
    )
}

/// Like [`fetch`], with `nws` and the station's `observation`.
fn fetch_with(
    nws: &Nws,
    now: UnixTime,
    grid_status: u16,
    observation: &str,
) -> (Outcome, Vec<String>) {
    let om = OpenMeteo::default();
    let chain: [&dyn Provider; 2] = [nws, &om];
    let supplements: [&dyn Provider; 1] = [&om];
    let mut session = FetchSession::new(
        &chain,
        &supplements,
        LOC,
        DataRequirements::full(),
        ctx(now),
    );
    let mut urls = Vec::new();
    while let Some(request) = session.next_request() {
        urls.push(request.url.clone());
        let (status, body) = match request.endpoint {
            Endpoint::Point => (200, POINT),
            Endpoint::Forecast if request.url.contains("/gridpoints/") => (grid_status, GRID),
            Endpoint::Daily => (200, PERIODS),
            Endpoint::Stations => (200, STATIONS),
            Endpoint::Observation => (200, observation),
            Endpoint::Alert => (200, ALERTS),
            Endpoint::AirQuality => (404, ""),
            _ => (200, OPEN_METEO),
        };
        session.on_response(&HttpResponse::new(status, body.as_bytes()));
    }
    (session.finish(now), urls)
}

#[test]
fn the_point_lookup_names_the_grid_and_alerts_come_last() {
    let (outcome, urls) = fetch(NOW, 200);
    assert_eq!(
        &urls[..6],
        [
            "https://api.weather.gov/points/40.7143,-74.006",
            "https://api.weather.gov/gridpoints/OKX/33,42",
            "https://api.weather.gov/gridpoints/OKX/33,42/forecast",
            "https://api.weather.gov/gridpoints/OKX/33,42/stations?limit=3",
            "https://api.weather.gov/stations/KNYC/observations/latest",
            "https://api.weather.gov/alerts/active?point=40.7143,-74.006",
        ]
    );
    let (snap, _) = outcome.unwrap();
    assert_eq!(snap.provider, ProviderId::Nws);
    assert_eq!(snap.location.name, "New York");
    assert_eq!(snap.location.tz_name.as_deref(), Some("America/New_York"));
}

#[test]
fn hours_follow_the_gridpoint_layers() {
    let (snap, _) = fetch(NOW, 200).0.unwrap();
    // From 05:00 UTC on the 2nd to 01:00 UTC on the 10th, hour by hour (gap
    // filling adds Open-Meteo's hours before them).
    let (first, end) = (1_793_595_600, 1_794_272_400);
    let nws_hours = snap
        .hourly
        .iter()
        .filter(|h| (first..end).contains(&h.time.0));
    assert_eq!(nws_hours.count(), 188);
    let hour = |t: i64| snap.hourly.iter().find(|h| h.time.0 == t).unwrap();
    let eleven_am = hour(NOW_HOUR.0);
    assert!((eleven_am.temperature.unwrap().0 - 15.556).abs() < 0.01);
    // 12.964 km/h.
    assert!((eleven_am.wind_speed.unwrap().0 - 3.601).abs() < 0.01);
    assert_eq!(
        eleven_am.condition,
        Condition::PartlyCloudy,
        "46 % sky cover"
    );
    assert!(eleven_am.is_day);
    let monday_9pm = hour(1_793_671_200);
    assert!((monday_9pm.temperature.unwrap().0 - 16.111).abs() < 0.01);
    assert_eq!(monday_9pm.condition, Condition::Clear, "6 % sky cover");
    assert!(!monday_9pm.is_day);
    let wednesday_2am = hour(1_793_775_600);
    assert!((wednesday_2am.temperature.unwrap().0 - 14.444).abs() < 0.01);
    assert!(!wednesday_2am.is_day);
    // A chance of light rain at 47 % is no rain: the sky (78 %) decides.
    let saturday_1am = hour(1_794_031_200);
    assert_eq!(saturday_1am.condition, Condition::Cloudy);
    assert_ne!(saturday_1am.condition, Condition::Rain(Intensity::Light));
}

#[test]
fn days_take_the_forecasters_highs_lows_and_icons() {
    let (snap, _) = fetch(NOW, 200).0.unwrap();
    let day = |d: u8| {
        snap.daily
            .iter()
            .find(|x| {
                x.date
                    == LocalDate {
                        year: 2026,
                        month: 11,
                        day: d,
                    }
            })
            .unwrap()
    };
    assert!((day(2).temp_max.unwrap().0 - 18.333).abs() < 0.01);
    // The forecasters' low, below every hour's 10.556 °C.
    assert!((day(2).temp_min.unwrap().0 - 10.0).abs() < 0.01);
    // The low from 7 PM Monday to 9 AM Tuesday is Tuesday's.
    assert!((day(3).temp_min.unwrap().0 - 11.111).abs() < 0.01);
    // "sct" today, "rain,20/rain,50" on Thursday.
    assert_eq!(day(2).condition, Condition::PartlyCloudy);
    assert_eq!(day(5).condition, Condition::Rain(Intensity::Moderate));
    assert!(day(2).sunrise.is_some() && day(2).daylight_s.is_some());
    let current = snap.current.as_ref().unwrap();
    assert_eq!(current.time, NOW_HOUR, "the forecast hour");
    assert!(current.dew_point.is_some());
}

#[test]
fn open_meteo_fills_what_the_nws_lacks() {
    let (snap, _) = fetch(NOW, 200).0.unwrap();
    assert_eq!(snap.supplemented_by, [ProviderId::OpenMeteo]);
    let eleven_am = snap.hourly.iter().find(|h| h.time == NOW_HOUR).unwrap();
    assert_eq!(eleven_am.uv_index, Some(2.75), "UV from Open-Meteo");
    assert!(!snap.minutely_15.is_empty(), "and the nowcast");
}

#[test]
fn a_stale_or_failing_grid_falls_back_to_open_meteo() {
    let thirteen_hours_on = UnixTime(GRID_UPDATED + 13 * 3_600);
    let (snap, attempts) = fetch(thirteen_hours_on, 200).0.unwrap();
    assert_eq!(snap.provider, ProviderId::OpenMeteo);
    assert!(attempts.iter().any(|a| matches!(
        &a.error,
        Some(ProviderError::Malformed(m)) if m.contains("last updated")
    )));
    let (snap, _) = fetch(NOW, 503).0.unwrap();
    assert_eq!(snap.provider, ProviderId::OpenMeteo);
}

#[test]
fn as_a_supplement_it_fetches_alerts_only() {
    let nws = Nws::default();
    let plan = nws
        .plan(&LOC, &DataRequirements::full(), &ctx(NOW))
        .unwrap();
    assert_eq!(plan.len(), 1);
    assert_eq!(plan[0].endpoint, Endpoint::Alert);
    let forecaster = Nws::forecaster("https://api.weather.gov");
    let plan = forecaster
        .plan(&LOC, &DataRequirements::full(), &ctx(NOW))
        .unwrap();
    assert_eq!(
        plan.iter()
            .map(|r| (r.endpoint, r.required))
            .collect::<Vec<_>>(),
        [(Endpoint::Point, true), (Endpoint::Alert, false)]
    );
}

#[test]
fn a_fresh_observation_is_the_current_conditions() {
    let (snap, _) = fetch(OBSERVED_NOW, 200).0.unwrap();
    let current = snap.current.as_ref().unwrap();
    assert_eq!(current.time, OBSERVED_AT, "14:51 UTC");
    assert_eq!(current.temperature.map(|c| c.0), Some(15.0));
    // No wind measured: the forecast hour's 11.112 km/h stays, and with neither
    // a wind chill nor a heat index, it feels like the temperature.
    assert!((current.wind_speed.unwrap().0 - 3.087).abs() < 0.01);
    assert_eq!(current.apparent.map(|c| c.0), Some(15.0));
    let station = snap.location.station.as_ref().unwrap();
    assert_eq!(
        (station.id.as_str(), station.name.as_str()),
        ("KNYC", "New York City, Central Park")
    );
    // The observation gives the station as 40.77, -73.98 (rounded): 6.6 km.
    assert!((6_400..6_800).contains(&station.distance_m.unwrap()));
    // The station's "skc" icon replaces the forecast hour's partly cloudy sky.
    let forecast_hour = snap
        .hourly
        .iter()
        .find(|h| h.time == UnixTime(1_793_631_600))
        .unwrap();
    assert_eq!(forecast_hour.condition, Condition::PartlyCloudy);
    assert_eq!(current.condition, Condition::Clear);
    assert!(current.is_day);
}

#[test]
fn an_old_observation_is_ignored() {
    let two_hours_on = UnixTime(OBSERVED_AT.0 + 2 * 3_600);
    let (snap, _) = fetch(two_hours_on, 200).0.unwrap();
    assert_eq!(snap.location.station, None);
    let current = snap.current.as_ref().unwrap();
    assert_eq!(current.time, NOW_HOUR, "the forecast hour");
}

#[test]
fn a_temperature_failed_by_quality_control_is_ignored() {
    let failed = OBSERVATION.replacen(r#""qualityControl": "V""#, r#""qualityControl": "X""#, 1);
    assert_ne!(failed, OBSERVATION, "the temperature comes first");
    let nws = Nws::forecaster("https://api.weather.gov");
    let (snap, _) = fetch_with(&nws, OBSERVED_NOW, 200, &failed).0.unwrap();
    assert_eq!(snap.location.station, None);
    assert_ne!(snap.current.unwrap().temperature.map(|c| c.0), Some(15.0));
}

#[test]
fn forecast_current_conditions_ask_no_station() {
    let nws = Nws::forecaster("https://api.weather.gov").observing(false);
    let (outcome, urls) = fetch_with(&nws, OBSERVED_NOW, 200, OBSERVATION);
    assert!(!urls.iter().any(|u| u.contains("stations")), "{urls:?}");
    assert_eq!(outcome.unwrap().0.location.station, None);
}
