//! Open-Meteo history: past days recorded for ZIP 10001 on 2026-11-02,
//! normals from a generated 30-year archive, and the fallback from ACIS.

use whether_core::history::{HistoryKind, Normals, YearRange};
use whether_core::model::ProviderId;
use whether_core::requirements::HistoryRequirements;
use whether_core::time::{LocalDate, UnixTime, UtcOffset, civil_from_days, days_from_civil};
use whether_core::view::{LocationView, StatusInfo, ViewInput, build_view};
use whether_providers::acis::Acis;
use whether_providers::open_meteo::history::{OpenMeteoHistory, OpenMeteoHistorySettings};
use whether_providers::{
    Coordinates, Endpoint, HistoryContext, HistoryProvider, HistorySession, HttpResponse,
};

const LOC: Coordinates = Coordinates {
    latitude: 40.7143,
    longitude: -74.005_97,
};
/// 26 October to 1 November 2026, all dry; the 1st: 15.3/5.0 °C, 0 mm.
const PAST: &str = include_str!("fixtures/open_meteo/history_past_10001.json");
const EST: UtcOffset = UtcOffset(-5 * 3600);

fn date(month: u8, day: u8) -> LocalDate {
    LocalDate {
        year: 2026,
        month,
        day,
    }
}

/// 2 November 2026, 11:30 EST, when the past days were recorded.
fn ctx(normals: Option<Normals>) -> HistoryContext {
    HistoryContext {
        now: UnixTime(1_793_637_000),
        today: date(11, 2),
        utc_offset: EST,
        user_agent: "whether-test".into(),
        normals,
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

/// An archive answer for 1991–2020, generated: the high is 20 °C plus a
/// hundredth of the day of the year, the low 10 °C, and every third day has
/// 1 mm (the shape of `/v1/archive` with `timeformat=unixtime`).
fn archive() -> String {
    let (first, last) = (days_from_civil(1991, 1, 1), days_from_civil(2020, 12, 31));
    let (mut time, mut max, mut min, mut rain) = (vec![], vec![], vec![], vec![]);
    for d in first..=last {
        let (year, _, _) = civil_from_days(d);
        let doy = d - days_from_civil(year, 1, 1);
        time.push(d * 86_400 + 18_000);
        max.push(20.0 + doy as f64 / 100.0);
        min.push(10.0);
        rain.push(if d % 3 == 0 { 1.0 } else { 0.0 });
    }
    serde_json::json!({
        "latitude": 40.7, "longitude": -74.0, "utc_offset_seconds": -18_000,
        "daily": {
            "time": time, "temperature_2m_max": max,
            "temperature_2m_min": min, "precipitation_sum": rain,
        }
    })
    .to_string()
}

#[test]
fn acis_failing_falls_back_to_open_meteo_with_era5_normals() {
    let (acis, om) = (Acis::default(), OpenMeteoHistory::default());
    let chain: [&dyn HistoryProvider; 2] = [&acis, &om];
    let mut session = HistorySession::new(&chain, &[], LOC, req(), ctx(None));
    let archive = archive();
    let mut asked = Vec::new();
    while let Some(r) = session.next_request() {
        asked.push(r.endpoint);
        let (status, body) = match r.endpoint {
            Endpoint::Stations => (503, ""),
            Endpoint::Daily => (200, PAST),
            _ => (200, archive.as_str()),
        };
        session.on_response(&HttpResponse::new(status, body.as_bytes()));
    }
    assert_eq!(
        asked,
        [Endpoint::Stations, Endpoint::Daily, Endpoint::Climate]
    );
    let (h, _) = session.finish(ctx(None).now).unwrap();
    assert_eq!(
        (h.source, h.kind),
        (ProviderId::OpenMeteo, HistoryKind::Reanalysis)
    );
    // The past week (26 October on) reaches further back than the month to
    // date (1 November).
    assert_eq!(h.days.len(), 7);
    assert_eq!(h.days.first().unwrap().date, date(10, 26));
    let yesterday = h.day(date(11, 1)).unwrap();
    assert_eq!(yesterday.temp_max.map(|c| c.0), Some(15.3));
    assert_eq!(yesterday.temp_min.map(|c| c.0), Some(5.0));
    assert_eq!(yesterday.precip_sum.map(|p| p.0), Some(0.0));
    // The week ahead and the past days take the computed normals.
    let today = h.climate_on(date(11, 2)).unwrap();
    assert!(
        (today.normal_max.unwrap().0 - (20.0 + 3.05)).abs() < 0.02,
        "{today:?}"
    );
    assert!((today.precip_chance.unwrap().0 - 100.0 / 3.0).abs() < 1.0);
    assert!(h.climate_on(date(11, 8)).is_some() && h.climate_on(date(10, 26)).is_some());
    assert_eq!(today.record_max, None, "no records from a grid");
    assert_eq!(h.period, Some(YearRange::NORMALS_1991_2020));
    // The normals come back for the caller to keep.
    assert_eq!(h.normals.as_ref().unwrap().days.len(), 366);

    // The view credits the reanalysis.
    let snap = whether_core::testdata::sample_snapshot();
    let status = StatusInfo::default();
    let presentation = whether_core::presentation::PresentationConfig {
        widgets: vec![whether_core::presentation::WidgetKind::History],
        ..Default::default()
    };
    let strings = whether_core::strings::Strings::default();
    let locations = [LocationView {
        name: "New York",
        snapshot: Some(&snap),
        status: &status,
        history: Some(&h),
    }];
    let vm = build_view(&ViewInput {
        locations: &locations,
        active: 0,
        presentation: &presentation,
        strings: &strings,
        now: ctx(None).now,
    });
    let credit = vm.history.unwrap().credit;
    assert!(credit.starts_with("ERA5 reanalysis"), "{credit}");
    assert!(credit.ends_with("Normals 1991–2020"), "{credit}");
}

#[test]
fn kept_normals_spare_the_archive() {
    let om = OpenMeteoHistory::default();
    let mut normals_once = None;
    {
        let chain: [&dyn HistoryProvider; 1] = [&om];
        let mut s = HistorySession::new(&chain, &[], LOC, req(), ctx(None));
        let archive = archive();
        while let Some(r) = s.next_request() {
            let body = if r.endpoint == Endpoint::Daily {
                PAST
            } else {
                archive.as_str()
            };
            s.on_response(&HttpResponse::new(200, body.as_bytes()));
        }
        normals_once = s.finish(UnixTime(0)).unwrap().0.normals.or(normals_once);
    }
    let supplement = OpenMeteoHistory::new(OpenMeteoHistorySettings {
        normals: false,
        ..OpenMeteoHistorySettings::default()
    });
    assert_eq!(
        supplement.plan(&LOC, &req(), &ctx(None)).unwrap().len(),
        1,
        "a supplement never computes normals"
    );
    let plan = om.plan(&LOC, &req(), &ctx(normals_once.clone())).unwrap();
    assert_eq!(
        plan.iter().map(|r| r.endpoint).collect::<Vec<_>>(),
        [Endpoint::Daily],
        "no archive request with normals kept"
    );
    // With only the week ahead to show and normals kept, nothing is asked, and
    // the kept normals still fill it.
    let ahead = HistoryRequirements {
        past_days: 0,
        climate_days: 7,
        records: false,
        month_to_date: false,
    };
    let chain: [&dyn HistoryProvider; 1] = [&om];
    let mut s = HistorySession::new(&chain, &[], LOC, ahead, ctx(normals_once));
    assert!(s.next_request().is_none());
    let (h, _) = s.finish(UnixTime(0)).unwrap();
    assert!(h.climate_on(date(11, 5)).unwrap().normal_max.is_some());
    assert_eq!(h.normals, None, "kept normals are not handed back");
}
