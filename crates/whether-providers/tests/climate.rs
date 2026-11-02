//! Climate queries (the vacation planner) for ZIP 10001: 12–19 July
//! over 1995–2024, from ACIS and the Open-Meteo archive, recorded 2026-11-02.

use whether_core::history::{ClimateQuery, History, HistoryKind, YearRange};
use whether_core::model::ProviderId;
use whether_core::time::{LocalDate, UnixTime, UtcOffset};
use whether_providers::acis::{Acis, AcisSettings};
use whether_providers::open_meteo::history::OpenMeteoHistory;
use whether_providers::{
    Attempt, Coordinates, Endpoint, FetchError, HistoryContext, HistoryProvider, HistorySession,
    HttpResponse,
};

const LOC: Coordinates = Coordinates {
    latitude: 40.7143,
    longitude: -74.005_97,
};
const STATIONS: &str = include_str!("fixtures/acis/stnmeta_10001.json");
/// Central Park (7.8 km), the nearest station with all of 1995–2024: World
/// Trade Center (0.8 km) ended in June 2024.
const ACIS: &str = include_str!("fixtures/acis/climate_NYC_0712_0719.json");
const ARCHIVE: &str = include_str!("fixtures/open_meteo/climate_10001_0712_0719.json");

const YEARS: YearRange = YearRange {
    first: 1995,
    last: 2024,
};

fn on(year: i32, month: u8, day: u8) -> LocalDate {
    LocalDate { year, month, day }
}

fn july() -> ClimateQuery {
    ClimateQuery::new(on(2027, 7, 12), on(2027, 7, 19), YEARS).unwrap()
}

fn ctx() -> HistoryContext {
    HistoryContext {
        now: UnixTime(1_793_637_000),
        today: on(2026, 11, 2),
        utc_offset: UtcOffset(-5 * 3600),
        user_agent: "whether-test".into(),
        normals: None,
    }
}

fn f(c: f32) -> f32 {
    c * 9.0 / 5.0 + 32.0
}

type Outcome = Result<(History, Vec<Attempt>), FetchError>;

/// Runs a climate session, answering by URL: the station list, the ACIS
/// summary (or a 503 when `acis_down`) and the archive.
fn ask(
    chain: &[&dyn HistoryProvider],
    query: ClimateQuery,
    acis_down: bool,
) -> (Outcome, Vec<String>) {
    let mut session = HistorySession::climate(chain, LOC, query, ctx());
    let mut asked = Vec::new();
    while let Some(r) = session.next_request() {
        asked.push(r.url.clone());
        let (status, body) = if r.url.ends_with("StnMeta") {
            (200, STATIONS)
        } else if r.url.ends_with("StnData") {
            if acis_down { (503, "") } else { (200, ACIS) }
        } else {
            (200, ARCHIVE)
        };
        session.on_response(&HttpResponse::new(status, body.as_bytes()));
    }
    (session.finish(ctx().now), asked)
}

#[test]
fn acis_answers_from_the_nearest_station_with_those_years() {
    let acis = Acis::default();
    let (outcome, asked) = ask(&[&acis], july(), false);
    assert_eq!(asked.len(), 2, "station list, then one summary");
    let (h, _) = outcome.unwrap();
    assert_eq!((h.source, h.kind), (ProviderId::Acis, HistoryKind::Station));
    assert_eq!(h.period, Some(YEARS));
    let station = h.station.as_ref().unwrap();
    assert_eq!(
        (station.id.as_str(), station.name.as_str()),
        ("NYC", "NY City Central Park")
    );
    assert!((7_000..9_000).contains(&station.distance_m.unwrap()));
    assert_eq!(h.climate.len(), 8);
    assert!(h.days.is_empty(), "no observed days");
    // 12 July: 84.3/68.9 °F on average, wet in 10 of the 30 years, 94 °F in
    // 2010 at the most, 62 °F in 2002 at the least.
    let c = h.climate_on(on(2027, 7, 12)).unwrap();
    assert!((f(c.normal_max.unwrap().0) - 84.3).abs() < 0.01);
    assert!((f(c.normal_min.unwrap().0) - 68.9).abs() < 0.01);
    assert!((c.precip_chance.unwrap().0 - 100.0 * 10.0 / 30.0).abs() < 0.01);
    let (high, low) = (c.record_max.unwrap(), c.record_min.unwrap());
    assert_eq!((libm::roundf(f(high.value)), high.year), (94.0, 2010));
    assert_eq!((libm::roundf(f(low.value)), low.year), (62.0, 2002));
    // The wettest day of the week: 3.13 in on 17 July 1995.
    let wettest = h
        .climate_on(on(2027, 7, 17))
        .unwrap()
        .record_precip
        .unwrap();
    assert_eq!(wettest.year, 1995);
    assert!((wettest.value - 3.13 * 25.4).abs() < 0.01);
}

#[test]
fn acis_asks_one_summary_over_the_years() {
    let pinned = Acis::new(AcisSettings {
        station: Some("LGA".into()),
        ..AcisSettings::default()
    });
    let requests = pinned.plan_climate(&LOC, &july(), &ctx()).unwrap();
    assert_eq!(requests.len(), 1, "a configured station needs no list");
    let body: serde_json::Value =
        serde_json::from_str(requests[0].body.as_deref().unwrap()).unwrap();
    assert_eq!(body["sid"], "LGA");
    assert_eq!(
        (body["sdate"].as_str(), body["edate"].as_str()),
        (Some("1995-01-01"), Some("2024-12-31"))
    );
    let elems = body["elems"].as_array().unwrap();
    assert_eq!(elems.len(), 10);
    assert_eq!(
        elems[0]["groupby"],
        serde_json::json!(["year", "07-12", "07-19"])
    );
    // Across the new year, one window still.
    let winter = ClimateQuery::new(on(2026, 12, 30), on(2027, 1, 2), YEARS).unwrap();
    let body: serde_json::Value = serde_json::from_str(
        pinned.plan_climate(&LOC, &winter, &ctx()).unwrap()[0]
            .body
            .as_deref()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        body["elems"][0]["groupby"],
        serde_json::json!(["year", "12-30", "01-02"])
    );
}

#[test]
fn the_archive_is_reduced_per_date() {
    let om = OpenMeteoHistory::default();
    let (outcome, asked) = ask(&[&om], july(), false);
    assert_eq!(asked.len(), 1);
    assert!(asked[0].starts_with("https://archive-api.open-meteo.com/v1/archive?"));
    assert!(
        asked[0].contains("start_date=1995-07-12&end_date=2024-07-19"),
        "{}",
        asked[0]
    );
    let (h, _) = outcome.unwrap();
    assert_eq!(
        (h.source, h.kind),
        (ProviderId::OpenMeteo, HistoryKind::Reanalysis)
    );
    assert_eq!(h.period, Some(YEARS));
    assert_eq!(h.climate.len(), 8);
    for c in &h.climate {
        let (high, low) = (c.normal_max.unwrap().0, c.normal_min.unwrap().0);
        assert!(
            (26.0..34.0).contains(&high) && (14.0..24.0).contains(&low),
            "{c:?}"
        );
        assert!((0.0..=100.0).contains(&c.precip_chance.unwrap().0));
        let hottest = c.record_max.unwrap();
        assert!(hottest.value >= high && (1995..=2024).contains(&hottest.year));
    }
    // Across the new year: the first year's December to the year after the last.
    let winter = ClimateQuery::new(on(2026, 12, 30), on(2027, 1, 2), YEARS).unwrap();
    let url = om.plan_climate(&LOC, &winter, &ctx()).unwrap()[0]
        .url
        .clone();
    assert!(
        url.contains("start_date=1995-12-30&end_date=2025-01-02"),
        "{url}"
    );
    // 29 February in a common year is the 28th.
    let leap = ClimateQuery::new(on(2028, 2, 29), on(2028, 3, 1), YEARS).unwrap();
    let url = om.plan_climate(&LOC, &leap, &ctx()).unwrap()[0].url.clone();
    assert!(
        url.contains("start_date=1995-02-28&end_date=2024-03-01"),
        "{url}"
    );
}

#[test]
fn acis_failing_falls_back_to_the_archive() {
    let (acis, om) = (Acis::default(), OpenMeteoHistory::default());
    let (outcome, asked) = ask(&[&acis, &om], july(), true);
    assert_eq!(asked.len(), 3);
    let (h, attempts) = outcome.unwrap();
    assert_eq!(h.source, ProviderId::OpenMeteo);
    assert!(
        attempts
            .iter()
            .any(|a| a.provider == ProviderId::Acis && a.error.is_some())
    );
    // Abroad, ACIS steps aside at once.
    let oslo = Coordinates {
        latitude: 59.91,
        longitude: 10.75,
    };
    assert!(acis.plan_climate(&oslo, &july(), &ctx()).is_err());
}

#[test]
fn sources_without_climate_queries_are_skipped() {
    struct Plain;
    impl HistoryProvider for Plain {
        fn id(&self) -> ProviderId {
            ProviderId::OpenWeather
        }
        fn attribution(&self) -> whether_providers::Attribution {
            OpenMeteoHistory::default().attribution()
        }
        fn plan(
            &self,
            _: &Coordinates,
            _: &whether_core::requirements::HistoryRequirements,
            _: &HistoryContext,
        ) -> Result<Vec<whether_providers::HttpRequest>, whether_providers::ProviderError> {
            Ok(Vec::new())
        }
        fn parse(
            &self,
            _: Endpoint,
            _: &HttpResponse<'_>,
            _: &HistoryContext,
            _: &mut whether_core::history::HistoryBuilder,
        ) -> Result<(), whether_providers::ProviderError> {
            Ok(())
        }
    }
    let om = OpenMeteoHistory::default();
    let (outcome, _) = ask(&[&Plain, &om], july(), false);
    let (h, attempts) = outcome.unwrap();
    assert_eq!(h.source, ProviderId::OpenMeteo);
    assert!(matches!(
        &attempts[0].error,
        Some(whether_providers::ProviderError::Config(_))
    ));
}
