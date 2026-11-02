//! NWS alerts: planning, parsing the recorded responses, and supplementing a
//! forecast through the fetch session.

use whether_core::model::{ProviderId, WeatherSnapshot};
use whether_core::requirements::DataRequirements;
use whether_core::scales::Level;
use whether_core::time::{UnixTime, parse_rfc3339};
use whether_providers::nws::Nws;
use whether_providers::open_meteo::{OpenMeteo, OpenMeteoSettings};
use whether_providers::regions::covers_us;
use whether_providers::{
    Coordinates, Endpoint, FetchContext, FetchSession, HttpResponse, Provider, ProviderError,
};

const LOC: Coordinates = Coordinates {
    latitude: 40.7143,
    longitude: -74.005_97,
};
const FORECAST: &str = include_str!("fixtures/open_meteo/forecast_full_10001.json");
const AIR: &str = include_str!("fixtures/open_meteo/air_quality_10001.json");
/// No alerts: the empty collection recorded for New York on 2026-11-02.
const NONE: &str = include_str!("fixtures/nws/alerts_10001.json");
/// Florida's active alerts on 2026-07-28: a coastal flood statement and four
/// flood warnings, one without an end.
const FLORIDA: &str = include_str!("fixtures/nws/alerts_flood.json");
/// The Open-Meteo fixtures' time: 2026-11-02 16:30 UTC.
const RECORDED_AT: UnixTime = UnixTime(1_793_637_000);
/// When Florida's alerts were recorded: 2026-10-13 12:15:49 UTC.
const FLORIDA_AT: UnixTime = UnixTime(1_791_893_749);

fn ctx(now: UnixTime) -> FetchContext {
    FetchContext {
        now,
        user_agent: "whether-test".into(),
        utc_offset_hint: None,
        if_modified_since: None,
        language: "en".into(),
    }
}

fn alerts_only() -> DataRequirements {
    DataRequirements {
        alerts: true,
        ..DataRequirements::default()
    }
}

#[test]
fn plans_one_unmetered_request_inside_the_us() {
    let nws = Nws::default();
    let plan = nws.plan(&LOC, &alerts_only(), &ctx(RECORDED_AT)).unwrap();
    assert_eq!(plan.len(), 1);
    let r = &plan[0];
    assert!(
        r.url
            .starts_with("https://api.weather.gov/alerts/active?point=40.7143"),
        "{}",
        r.url
    );
    assert!(!r.metered && r.required);
    assert!(
        r.headers
            .iter()
            .any(|(k, v)| *k == "User-Agent" && v.starts_with("whether-test"))
    );
    assert!(
        r.headers
            .iter()
            .any(|(k, v)| *k == "Accept" && v == "application/geo+json")
    );
    let none = nws.plan(&LOC, &DataRequirements::default(), &ctx(RECORDED_AT));
    assert_eq!(none.unwrap().len(), 0, "nothing when alerts are not shown");
    let london = Coordinates {
        latitude: 51.5074,
        longitude: -0.1278,
    };
    assert!(matches!(
        nws.plan(&london, &alerts_only(), &ctx(RECORDED_AT)),
        Err(ProviderError::OutOfArea(_))
    ));
    for (lat, lon) in [
        (61.22, -149.9),
        (21.31, -157.86),
        (18.47, -66.11),
        (24.56, -81.78),
    ] {
        assert!(
            covers_us(&Coordinates {
                latitude: lat,
                longitude: lon
            }),
            "{lat}, {lon}"
        );
    }
}

/// Open-Meteo's forecast with NWS's `alerts` as the supplement, at `now`.
fn fetch(alerts: &str, now: UnixTime) -> WeatherSnapshot {
    let (om, nws) = (OpenMeteo::default(), Nws::default());
    let chain: [&dyn Provider; 1] = [&om];
    let supplements: [&dyn Provider; 1] = [&nws];
    let req = DataRequirements::full();
    let mut session = FetchSession::new(&chain, &supplements, LOC, req, ctx(now));
    while let Some(request) = session.next_request() {
        let body = match request.endpoint {
            Endpoint::AirQuality => AIR,
            Endpoint::Alert => alerts,
            _ => FORECAST,
        };
        session.on_response(&HttpResponse::new(200, body.as_bytes()));
    }
    session.finish(now).unwrap().0
}

#[test]
fn alerts_supplement_any_forecast() {
    let s = fetch(FLORIDA, FLORIDA_AT);
    assert_eq!(s.provider, ProviderId::OpenMeteo);
    assert_eq!(s.supplemented_by, [ProviderId::Nws]);
    let ids: Vec<&str> = s.alerts.iter().filter_map(|a| a.id.as_deref()).collect();
    assert_eq!(
        ids,
        [
            "KKEY.CF.S.0009",
            "KMOB.FL.W.0048",
            "KMLB.FL.W.0003",
            "KTBW.FL.W.0009",
            "KTBW.FL.W.0014"
        ],
        "updates keep their event's id"
    );
    let statement = &s.alerts[0];
    assert_eq!(statement.title, "Coastal Flood Statement");
    assert_eq!(statement.severity, Level::Fair);
    assert_eq!(statement.source, "NWS Key West FL");
    assert_eq!(
        Some(statement.start),
        parse_rfc3339("2026-10-13T04:00:00-04:00")
    );
    assert_eq!(
        Some(statement.end),
        parse_rfc3339("2026-10-14T05:00:00-04:00"),
        "ends, not expires"
    );
    let warning = &s.alerts[1];
    assert_eq!(
        (warning.title.as_str(), warning.severity),
        ("Flood Warning", Level::VeryPoor)
    );
    assert!(
        warning.description.contains("\n\n"),
        "the instructions follow the description"
    );
    assert_eq!(
        Some(s.alerts[2].end),
        parse_rfc3339("2026-10-13T12:00:00-04:00"),
        "without an end, the message's expiry"
    );

    // Later, the ended alerts are gone.
    let later = fetch(FLORIDA, parse_rfc3339("2026-10-14T06:00:00-04:00").unwrap());
    let ids: Vec<&str> = later
        .alerts
        .iter()
        .filter_map(|a| a.id.as_deref())
        .collect();
    assert_eq!(ids, ["KTBW.FL.W.0009", "KTBW.FL.W.0014"]);
}

/// An alert links to the NWS's page for the point (a page to read), never to
/// its `GeoJSON` on `api.weather.gov` nor to the NWS home page (`web` in the
/// recorded file).
#[test]
fn alerts_link_to_the_forecast_page_for_the_point() {
    let s = fetch(FLORIDA, FLORIDA_AT);
    assert_eq!(s.alerts.len(), 5);
    for alert in &s.alerts {
        let url = alert.url.as_deref().unwrap();
        assert!(
            url.starts_with("https://forecast.weather.gov/MapClick.php?lat=")
                && url.contains("&lon="),
            "{url}"
        );
        assert!(!url.contains("api.weather.gov"), "{url}");
    }
    // The requested point, 40.7143, -74.00597, to two places.
    let url = s.alerts[0].url.as_deref().unwrap();
    assert_eq!(
        url,
        "https://forecast.weather.gov/MapClick.php?lat=40.71&lon=-74.01"
    );
}

#[test]
fn no_alerts_add_nothing_and_no_credit() {
    let s = fetch(NONE, RECORDED_AT);
    assert_eq!(s.alerts.len(), 0);
    assert_eq!(s.supplemented_by, [], "no credit without alerts");
}

/// The NOAA blend forecasts and Open-Meteo's best match fills what it lacks: a
/// supplement of the same provider runs when its model differs.
#[test]
fn another_model_of_the_primary_supplements_it() {
    let blend = OpenMeteo::new(OpenMeteoSettings {
        models: vec!["ncep_nbm_conus".into()],
        ..OpenMeteoSettings::default()
    });
    let (best, again) = (
        OpenMeteo::default(),
        OpenMeteo::new(OpenMeteoSettings {
            models: vec!["ncep_nbm_conus".into()],
            ..OpenMeteoSettings::default()
        }),
    );
    let chain: [&dyn Provider; 1] = [&blend];
    let supplements: [&dyn Provider; 2] = [&best, &again];
    let mut session = FetchSession::new(
        &chain,
        &supplements,
        LOC,
        DataRequirements::full(),
        ctx(RECORDED_AT),
    );
    let mut forecasts = Vec::new();
    while let Some(request) = session.next_request() {
        let body = if request.endpoint == Endpoint::AirQuality {
            AIR
        } else {
            forecasts.push(request.url.contains("models=ncep_nbm_conus"));
            FORECAST
        };
        session.on_response(&HttpResponse::new(200, body.as_bytes()));
    }
    assert_eq!(
        forecasts,
        [true, false],
        "the blend, then the best match; not the blend twice"
    );
}
