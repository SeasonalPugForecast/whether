//! ACIS history against responses recorded for ZIP 10001 on 2026-11-02
//! (EST): the station choice, observed days, normals, the chance of
//! precipitation, and records from a threaded city record.

use whether_core::history::History;
use whether_core::model::ProviderId;
use whether_core::requirements::HistoryRequirements;
use whether_core::time::{LocalDate, UnixTime, UtcOffset};
use whether_providers::acis::{Acis, AcisSettings};
use whether_providers::{
    Attempt, Coordinates, Endpoint, FetchError, HistoryContext, HistoryProvider, HistorySession,
    HttpResponse, Method, ProviderError,
};

const LOC: Coordinates = Coordinates {
    latitude: 40.7143,
    longitude: -74.005_97,
};

macro_rules! fixture {
    ($name:literal) => {
        include_str!(concat!("fixtures/acis/", $name, ".json"))
    };
}

/// The recorded answer to an ACIS query, by its station and kind.
fn answer(body: &str, missing_thread: bool) -> (u16, &'static str) {
    let q: serde_json::Value = serde_json::from_str(body).unwrap();
    if q.get("bbox").is_some() {
        return (200, fixture!("stnmeta_10001"));
    }
    let kind = if body.contains("normal") {
        "stndata"
    } else if body.contains("cnt_ge") {
        "chance"
    } else {
        "records"
    };
    let text = match (kind, q["sid"].as_str().unwrap()) {
        ("stndata", "NYC") => fixture!("stndata_NYC"),
        ("chance", "NYC") => fixture!("chance_NYC"),
        ("records", "NYC") => fixture!("records_NYC"),
        ("records", "NYCthr") => fixture!("records_NYCthr"),
        ("stndata", "LGA") => fixture!("stndata_LGA"),
        ("chance", "LGA") => fixture!("chance_LGA"),
        ("records", "LGA") => fixture!("records_LGA"),
        ("records", "LGAthr") if !missing_thread => fixture!("records_LGAthr"),
        _ => r#"{"error":"Unknown sId"}"#,
    };
    (200, text)
}

/// 2 November 2026, 11:30 EST (UTC-5), when the fixtures were recorded.
fn ctx() -> HistoryContext {
    HistoryContext {
        now: UnixTime(1_793_637_000),
        today: date(11, 2),
        utc_offset: UtcOffset(-5 * 3600),
        user_agent: "whether-test".into(),
        normals: None,
    }
}

fn date(month: u8, day: u8) -> LocalDate {
    LocalDate {
        year: 2026,
        month,
        day,
    }
}

fn req() -> HistoryRequirements {
    HistoryRequirements {
        past_days: 7,
        climate_days: 7,
        records: true,
        month_to_date: true,
    }
}

type Outcome = Result<(History, Vec<Attempt>), FetchError>;

fn fetch(acis: &Acis, missing_thread: bool) -> (Outcome, Vec<String>) {
    let chain: [&dyn HistoryProvider; 1] = [acis];
    let mut session = HistorySession::new(&chain, &[], LOC, req(), ctx());
    let mut asked = Vec::new();
    while let Some(request) = session.next_request() {
        assert_eq!(request.method, Method::Post);
        let body = request.body.clone().unwrap();
        let q: serde_json::Value = serde_json::from_str(&body).unwrap();
        asked.push(format!(
            "{} {}",
            request.url.rsplit('/').next().unwrap(),
            q["sid"].as_str().unwrap_or("-")
        ));
        let (status, text) = answer(&body, missing_thread);
        session.on_response(&HttpResponse::new(status, text.as_bytes()));
    }
    (session.finish(ctx().now), asked)
}

fn pinned_to_laguardia() -> Acis {
    Acis::new(AcisSettings {
        station: Some("LGA".into()),
        ..AcisSettings::default()
    })
}

fn f(c: f32) -> f32 {
    c * 9.0 / 5.0 + 32.0
}

#[test]
fn the_nearest_long_running_station_that_reported_yesterday() {
    let (outcome, asked) = fetch(&Acis::default(), false);
    // The Wall Street heliport (1.5 km) has temperatures only since 2021 and
    // last reported on 31 October, and World Trade Center (0.8 km, since 1871)
    // ended in 2024: Central Park, 7.8 km.
    assert_eq!(
        asked,
        [
            "StnMeta -",
            "StnData NYC",
            "StnData NYC",
            "StnData NYC",
            "StnData NYCthr"
        ]
    );
    let (h, _) = outcome.unwrap();
    assert_eq!(h.source, ProviderId::Acis);
    let station = h.station.as_ref().unwrap();
    assert_eq!(
        (station.id.as_str(), station.name.as_str()),
        ("NYC", "NY City Central Park")
    );
    assert!((7_000..9_000).contains(&station.distance_m.unwrap()));
    let records = h.records_station.as_ref().unwrap();
    assert_eq!(
        (records.id.as_str(), records.name.as_str()),
        ("NYCthr", "New York-central Park Area")
    );
    let today = h.climate_on(date(11, 2)).unwrap();
    let high = today.record_max.unwrap();
    assert_eq!((libm::roundf(f(high.value)), high.year), (83.0, 1950));
    // Yesterday, as reported: 59/42 °F and dry (0.00 in).
    let yesterday = h.day(date(11, 1)).unwrap();
    assert!((f(yesterday.temp_max.unwrap().0) - 59.0).abs() < 0.01);
    assert!((f(yesterday.temp_min.unwrap().0) - 42.0).abs() < 0.01);
    assert_eq!(yesterday.precip_sum.unwrap().0, 0.0);
    assert!(!yesterday.trace);
}

#[test]
fn a_configured_station_skips_the_search() {
    let (outcome, asked) = fetch(&pinned_to_laguardia(), false);
    assert_eq!(asked[0], "StnData LGA", "no StnMeta");
    let (h, _) = outcome.unwrap();
    assert_eq!(h.station.as_ref().unwrap().name, "Laguardia Airport");
    let today = h.climate_on(date(11, 2)).unwrap();
    // The verified numbers for 2 November: normals 60/47 °F and 0.11 in,
    // records 83 °F (1950) and 29 °F (1965) from La Guardia's threaded record,
    // and precipitation on 8 of the 30 days in 1991–2020.
    assert!((f(today.normal_max.unwrap().0) - 60.0).abs() < 0.01);
    assert!((f(today.normal_min.unwrap().0) - 47.0).abs() < 0.01);
    assert!((today.normal_precip.unwrap().0 - 0.11 * 25.4).abs() < 0.001);
    let (high, low) = (today.record_max.unwrap(), today.record_min.unwrap());
    assert_eq!((libm::roundf(f(high.value)), high.year), (83.0, 1950));
    assert_eq!((libm::roundf(f(low.value)), low.year), (29.0, 1965));
    assert!((today.precip_chance.unwrap().0 - 100.0 * 8.0 / 30.0).abs() < 0.01);
    assert_eq!(h.records_station.as_ref().unwrap().id, "LGAthr");
    // The past days have their records too, for comparing each with.
    let past = h.climate_on(date(10, 28)).unwrap();
    assert!(past.record_max.is_some() && past.record_min.is_some());
    assert!(past.record_max.unwrap().year < 2026);
    assert_eq!(h.period.unwrap().first, 1991);
    // The past week (26 October to yesterday) reaches further back than the
    // month to date (1 November). Every day of it was dry: 0.00 in, no trace.
    assert_eq!(h.days.first().unwrap().date, date(10, 26));
    assert_eq!(h.days.last().unwrap().date, date(11, 1));
    assert_eq!(h.days.len(), 7);
    assert!(
        h.days
            .iter()
            .all(|d| !d.trace && d.precip_sum.unwrap().0 == 0.0)
    );
    // Normals for the week ahead, and for the past days' departures.
    assert!(h.climate_on(date(11, 8)).is_some());
    assert!(h.climate_on(date(10, 26)).unwrap().normal_max.is_some());
}

#[test]
fn without_a_threaded_record_the_station_keeps_its_own() {
    let (outcome, _) = fetch(&pinned_to_laguardia(), true);
    let (h, attempts) = outcome.unwrap();
    // The airport's own record; La Guardia's threaded record starts with the
    // airport (1939), so it holds the same 83 °F (1950) here.
    let high = h.climate_on(date(11, 2)).unwrap().record_max.unwrap();
    assert_eq!((libm::roundf(f(high.value)), high.year), (83.0, 1950));
    assert_eq!(h.records_station, None);
    assert!(attempts.iter().any(|a| matches!(
        &a.error,
        Some(ProviderError::Rejected(m)) if m == "Unknown sId"
    )));
}

#[test]
fn errors_and_places_abroad_fail() {
    let acis = Acis::default();
    let chain: [&dyn HistoryProvider; 1] = [&acis];
    let mut s = HistorySession::new(&chain, &[], LOC, req(), ctx());
    let _ = s.next_request();
    s.on_response(&HttpResponse::new(200, br#"{"error":"bad bbox"}"#));
    assert!(s.next_request().is_none());
    assert!(matches!(
        s.finish(UnixTime(0)),
        Err(FetchError::AllProvidersFailed(_))
    ));
    let oslo = Coordinates {
        latitude: 59.91,
        longitude: 10.75,
    };
    assert!(matches!(
        acis.plan(&oslo, &req(), &ctx()),
        Err(ProviderError::OutOfArea(_))
    ));
    let none = acis
        .follow_up(
            Endpoint::Stations,
            &HttpResponse::new(200, br#"{"meta":[]}"#),
            &LOC,
            &req(),
            &ctx(),
        )
        .unwrap_err();
    assert!(matches!(none, ProviderError::Malformed(_)));
}

#[test]
fn nearby_stations_best_first() {
    let acis = Acis::default();
    let request = acis.stations_request(&LOC, &ctx()).unwrap();
    assert_eq!(request.endpoint, Endpoint::Stations);
    let resp = HttpResponse::new(200, fixture!("stnmeta_10001").as_bytes());
    let list = Acis::nearby_stations(&resp, &LOC, &ctx()).unwrap();
    // The station history uses comes first.
    assert_eq!(list[0].id, "NYC");
    assert!(list[0].usable && list[0].long_record && list[0].reported_yesterday);
    let find = |name: &str| list.iter().find(|s| s.name == name).unwrap();
    let laguardia = find("Laguardia Airport");
    assert_eq!(laguardia.id, "LGA");
    assert!(laguardia.usable && laguardia.long_record);
    assert!((12_000..14_000).contains(&laguardia.distance_m));
    // Harrison, New Jersey (13 km), only since 1996.
    let harrison = find("Harrison");
    assert!(harrison.usable && !harrison.long_record, "since 1996");
    assert_eq!(
        harrison.first,
        Some(LocalDate {
            year: 1996,
            month: 1,
            day: 1
        })
    );
    // The Wall Street heliport (1.5 km), the nearest station still reporting:
    // since 2021, and its last day was 31 October.
    let heliport = list.iter().find(|s| s.id == "JRB").unwrap();
    assert!(heliport.distance_m < 2_000);
    assert!(heliport.usable && !heliport.long_record && !heliport.reported_yesterday);
    // World Trade Center (0.8 km) shares Central Park's id but ended in 2024.
    let world_trade_center = find("World Trade Center");
    assert_eq!(world_trade_center.id, "NYC");
    assert!(!world_trade_center.usable, "ended in 2024");
    // Usable stations before the rest; precipitation-only gauges are left out.
    let first_unusable = list.iter().position(|s| !s.usable).unwrap();
    assert!(list[first_unusable..].iter().all(|s| !s.usable));
    assert!(
        list.iter()
            .all(|s| s.distance_m <= 60_000 && s.first.is_some())
    );
    // Abroad, and errors.
    let oslo = Coordinates {
        latitude: 59.91,
        longitude: 10.75,
    };
    assert!(acis.stations_request(&oslo, &ctx()).is_err());
    let error = HttpResponse::new(200, br#"{"error":"bad bbox"}"#);
    assert!(matches!(
        Acis::nearby_stations(&error, &LOC, &ctx()),
        Err(ProviderError::Rejected(_))
    ));
}
