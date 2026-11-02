//! Open-Meteo forecast and air quality: planning and parsing against recorded fixtures,
//! and supplementing OpenWeather's free API.

use proptest::prelude::*;
use whether_core::model::{ProviderId, WeatherSnapshot};
use whether_core::requirements::{CurrentFields, DailyFields, DataRequirements, HourlyFields};
use whether_core::testdata;
use whether_core::time::{UnixTime, UtcOffset};
use whether_providers::open_meteo::{OpenMeteo, OpenMeteoSettings};
use whether_providers::{
    Attempt, Coordinates, Endpoint, FetchContext, FetchError, FetchSession, HttpResponse, Provider,
    ProviderError, redact_url,
};

const LOC: Coordinates = Coordinates {
    latitude: 40.7143,
    longitude: -74.005_97,
};
const FULL: &str = include_str!("fixtures/open_meteo/forecast_full_10001.json");
const ESP32: &str = include_str!("fixtures/open_meteo/forecast_esp32_10001.json");
const AIR: &str = include_str!("fixtures/open_meteo/air_quality_10001.json");
const NULLS: &str = include_str!("fixtures/open_meteo/forecast_nulls.synthetic.json");
const MISMATCH: &str = include_str!("fixtures/open_meteo/forecast_length_mismatch.synthetic.json");
const ERR_400: &str = include_str!("fixtures/open_meteo/error_400_latitude.json");
/// The `current.time` of the recorded 10001 files.
const RECORDED_AT: UnixTime = UnixTime(1_793_637_000);

fn ctx() -> FetchContext {
    FetchContext {
        now: RECORDED_AT,
        user_agent: "whether-test".into(),
        utc_offset_hint: None,
        if_modified_since: None,
        language: "en".into(),
    }
}

/// Fetches with Open-Meteo alone, answering the forecast with `forecast` and air
/// quality with `AIR`.
fn fetch(
    forecast: (u16, &str),
    req: DataRequirements,
) -> Result<(WeatherSnapshot, Vec<Attempt>), FetchError> {
    let p = OpenMeteo::default();
    let chain: [&dyn Provider; 1] = [&p];
    let mut session = FetchSession::new(&chain, &[], LOC, req, ctx());
    while let Some(request) = session.next_request() {
        let (status, body) = match request.endpoint {
            Endpoint::AirQuality => (200, AIR),
            _ => forecast,
        };
        session.on_response(&HttpResponse::new(status, body.as_bytes()));
    }
    session.finish(UnixTime(RECORDED_AT.0 + 60))
}

#[test]
fn planned_urls() {
    let p = OpenMeteo::default();
    let full = p.plan(&LOC, &DataRequirements::full(), &ctx()).unwrap();
    assert_eq!(full.len(), 2);
    assert!(
        full[0].required && !full[1].required,
        "air quality is optional"
    );
    let small = DataRequirements {
        current: CurrentFields::TEMPERATURE | CurrentFields::IS_DAY,
        hourly: HourlyFields::TEMPERATURE,
        hourly_hours: 12,
        daily: DailyFields::TEMP_MAX,
        daily_days: 5,
        ..Default::default()
    };
    let small = p.plan(&LOC, &small, &ctx()).unwrap();
    assert_eq!(small.len(), 1, "no air-quality request when not needed");
    let urls: Vec<String> = full.iter().chain(&small).map(|r| r.url.clone()).collect();
    insta::assert_debug_snapshot!(urls);
}

#[test]
fn key_and_models_are_sent_and_redacted() {
    let p = OpenMeteo::new(OpenMeteoSettings {
        base_url: "https://customer-api.open-meteo.com/v1".into(),
        api_key: Some("TESTKEY".into()),
        models: vec!["gfs_seamless".into()],
        ..Default::default()
    });
    let requests = p.plan(&LOC, &DataRequirements::full(), &ctx()).unwrap();
    let url = &requests[0].url;
    assert!(url.starts_with("https://customer-api.open-meteo.com/v1/forecast?"));
    assert!(url.contains("models=gfs_seamless") && url.contains("apikey=TESTKEY"));
    assert!(!redact_url(url).contains("TESTKEY"));
    assert!(!format!("{p:?}").contains("TESTKEY"));
}

#[test]
fn full_10001() {
    let (s, attempts) = fetch((200, FULL), DataRequirements::full()).unwrap();
    assert!(attempts.iter().all(|a| a.error.is_none()));
    assert_eq!(s.provider, ProviderId::OpenMeteo);
    assert_eq!(s.location.utc_offset.0, -18_000);
    assert_eq!(s.location.tz_name.as_deref(), Some("America/New_York"));
    let current = s.current.as_ref().unwrap();
    assert_eq!(current.time, RECORDED_AT);
    assert!(current.temperature.is_some() && current.uv_index.is_some());
    assert_eq!(
        (s.hourly.len(), s.daily.len(), s.minutely_15.len()),
        (72, 7, 32)
    );
    assert_eq!(
        s.daily[0].date,
        RECORDED_AT.local_date(s.location.utc_offset)
    );
    assert!(
        s.daily
            .iter()
            .all(|d| d.moon_phase.is_some() && d.sunrise.is_some())
    );
    let aq = s.air_quality.as_ref().unwrap();
    assert!(aq.us_aqi.is_some() && aq.european_aqi.is_some() && aq.owm_index.is_none());
    // The first record of each section is enough to pin the field mapping.
    insta::assert_debug_snapshot!((
        &s.location,
        current,
        &s.hourly[0],
        &s.daily[0],
        &s.minutely_15[0],
        aq
    ));
}

#[test]
fn esp32_profile_payload_is_small() {
    assert!(ESP32.len() < 3_000, "{} bytes", ESP32.len());
    let req = DataRequirements {
        current: CurrentFields::all(),
        hourly: HourlyFields::TEMPERATURE,
        hourly_hours: 12,
        daily: DailyFields::TEMP_MAX | DailyFields::TEMP_MIN,
        daily_days: 5,
        ..Default::default()
    };
    let (s, _) = fetch((200, ESP32), req).unwrap();
    assert_eq!((s.hourly.len(), s.daily.len()), (12, 5));
    assert!(s.air_quality.is_none() && s.minutely_15.is_empty());
}

#[test]
fn daily_dates_are_local_everywhere() {
    let places = [
        (
            include_str!("fixtures/open_meteo/forecast_full_tromso.json"),
            7_200,
        ),
        (
            include_str!("fixtures/open_meteo/forecast_full_sydney.json"),
            39_600,
        ),
        (
            include_str!("fixtures/open_meteo/forecast_full_honolulu.json"),
            -36_000,
        ),
        (
            include_str!("fixtures/open_meteo/forecast_full_reykjavik.json"),
            0,
        ),
    ];
    for (body, offset) in places {
        let (s, _) = fetch((200, body), DataRequirements::full()).unwrap();
        assert_eq!(s.location.utc_offset.0, offset);
        let current = s.current.as_ref().unwrap();
        // The first daily row is the local date of the current observation.
        assert_eq!(
            s.daily[0].date,
            current.time.local_date(s.location.utc_offset)
        );
        let days: Vec<i64> = s.daily.iter().map(|d| d.date.days_since_epoch()).collect();
        assert!(days.windows(2).all(|w| w[1] == w[0] + 1), "{days:?}");
    }
}

#[test]
fn nulls_become_missing_values() {
    let (s, _) = fetch((200, NULLS), DataRequirements::full()).unwrap();
    let current = s.current.as_ref().unwrap();
    assert!(current.temperature.is_none() && current.wind_speed.is_none());
    assert!(current.is_day, "a missing is_day defaults to day");
    assert!(
        s.hourly
            .iter()
            .all(|h| h.temperature.is_none() && h.uv_index.is_none())
    );
    assert!(s.daily[0].temp_max.is_none() && s.daily[0].sunrise.is_none());
    assert!(s.minutely_15.iter().all(|m| m.precipitation.is_none()));
    assert!(s.location.elevation_m.is_none());
}

#[test]
fn short_columns_are_malformed() {
    let attempts = match fetch((200, MISMATCH), DataRequirements::full()) {
        Err(FetchError::AllProvidersFailed(attempts)) => attempts,
        other => panic!("{other:?}"),
    };
    assert!(matches!(
        &attempts[0].error,
        Some(ProviderError::Malformed(m)) if m.contains("hourly.temperature_2m")
    ));
}

#[test]
fn rejected_latitude_keeps_the_reason() {
    let attempts = match fetch((400, ERR_400), DataRequirements::full()) {
        Err(FetchError::AllProvidersFailed(attempts)) => attempts,
        other => panic!("{other:?}"),
    };
    assert_eq!(
        attempts[0].error,
        Some(ProviderError::Rejected(
            "Latitude must be in range of -90 to 90°. Given: 952.52.".into()
        ))
    );
}

/// Moves a recorded forecast by whole days to the local date of `now`, with
/// `offset`. The recording is from November (EST) and the OpenWeather sample it
/// supplements from October (EDT), and a supplement only counts when the UTC
/// offsets and the times agree.
fn moved_to(body: &str, now: UnixTime, offset: UtcOffset) -> String {
    fn shift(v: &mut serde_json::Value, by: i64) {
        if let Some(t) = v.as_i64() {
            *v = (t - by).into();
        } else if let Some(items) = v.as_array_mut() {
            for x in items {
                shift(x, by);
            }
        }
    }
    let mut v: serde_json::Value = serde_json::from_str(body).unwrap();
    let recorded_at = UnixTime(v["current"]["time"].as_i64().unwrap());
    let recorded = UtcOffset(i32::try_from(v["utc_offset_seconds"].as_i64().unwrap()).unwrap());
    let days = recorded_at.local_date(recorded).days_since_epoch()
        - now.local_date(offset).days_since_epoch();
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
    v["utc_offset_seconds"] = offset.0.into();
    v["timezone_abbreviation"] = format!("GMT{}", offset.0 / 3600).into();
    v.to_string()
}

#[test]
fn supplements_openweather_free() {
    let mut primary = testdata::sample_snapshot_owm_free();
    assert!(primary.minutely_15.is_empty() && primary.daily.len() == 5);
    let full = moved_to(FULL, testdata::SAMPLE_NOW, testdata::SAMPLE_OFFSET);
    let (om, _) = fetch((200, &full), DataRequirements::full()).unwrap();
    let report = primary.fill_gaps_from(&om);
    assert!(!report.is_empty());
    assert_eq!(
        primary.provider,
        ProviderId::OpenWeather,
        "the primary stays primary"
    );
    assert_eq!(primary.daily.len(), 7, "days 6 and 7 come from Open-Meteo");
    assert_eq!(
        primary.minutely_15.len(),
        32,
        "the nowcast comes from Open-Meteo"
    );
    assert!(primary.daily[0].uv_index_max.is_some() && primary.daily[0].moon_phase.is_some());
    let aq = primary.air_quality.as_ref().unwrap();
    assert!(aq.owm_index.is_some(), "OpenWeather's index is kept");
    assert!(aq.us_aqi.is_some() && aq.european_aqi.is_some());
}

proptest! {
    #[test]
    fn arbitrary_bytes_never_panic(body in proptest::collection::vec(any::<u8>(), 0..512)) {
        let p = OpenMeteo::default();
        let chain: [&dyn Provider; 1] = [&p];
        let mut session = FetchSession::new(&chain, &[], LOC, DataRequirements::full(), ctx());
        while session.next_request().is_some() {
            session.on_response(&HttpResponse::new(200, &body));
        }
        let _ = session.finish(RECORDED_AT);
    }

    #[test]
    fn arbitrary_model_responses_never_panic(
        hrrr in proptest::collection::vec(proptest::option::of(-50.0f32..50.0), 0..6),
        nbm in proptest::collection::vec(proptest::option::of(-50.0f32..50.0), 0..6),
        steps in 0i64..6,
    ) {
        let p = with_models(&["ncep_hrrr_conus", "ncep_nbm_conus"]);
        let body = serde_json::json!({
            "latitude": 40.7, "longitude": -74.0, "utc_offset_seconds": -18000,
            "hourly": {
                "time": (0..steps).map(|i| i * 3600).collect::<Vec<_>>(),
                "temperature_2m_ncep_hrrr_conus": hrrr,
                "temperature_2m_ncep_nbm_conus": nbm,
            },
            "daily": {
                "time": (0..steps).map(|i| i * 86400).collect::<Vec<_>>(),
                "temperature_2m_max_ncep_nbm_conus": [1.0],
            }
        })
        .to_string();
        let mut out = whether_core::builder::SnapshotBuilder::new(ProviderId::OpenMeteo);
        let _ = p.parse(Endpoint::Forecast, &HttpResponse::new(200, body.as_bytes()), &ctx(), &mut out);
    }
}

const MODELS: &str = include_str!("fixtures/open_meteo/forecast_models_hrrr_nbm_ecmwf.json");

fn with_models(ids: &[&str]) -> OpenMeteo {
    OpenMeteo::new(OpenMeteoSettings {
        models: ids.iter().map(ToString::to_string).collect(),
        ..OpenMeteoSettings::default()
    })
}

fn parse_forecast(p: &OpenMeteo, body: &str) -> WeatherSnapshot {
    let mut out = whether_core::builder::SnapshotBuilder::new(ProviderId::OpenMeteo);
    p.parse(
        Endpoint::Forecast,
        &HttpResponse::new(200, body.as_bytes()),
        &ctx(),
        &mut out,
    )
    .unwrap();
    out.finish(RECORDED_AT).unwrap()
}

/// Recorded 2026-11-02 for 10001 with HRRR, the NOAA blend and ECMWF.
#[test]
fn model_lists_take_each_value_from_the_first_model_that_has_it() {
    let p = with_models(&["ncep_hrrr_conus", "ncep_nbm_conus", "ecmwf_ifs025"]);
    let url = &p.plan(&LOC, &DataRequirements::full(), &ctx()).unwrap()[0].url;
    assert!(
        url.ends_with("&models=ncep_hrrr_conus,ncep_nbm_conus,ecmwf_ifs025"),
        "{url}"
    );
    let snap = parse_forecast(&p, MODELS);
    assert_eq!(
        snap.models,
        ["ncep_hrrr_conus", "ncep_nbm_conus", "ecmwf_ifs025"]
    );
    assert_eq!(snap.hourly.len(), 72);
    let hour = |t: i64| snap.hourly.iter().find(|h| h.time.0 == t).unwrap();
    // HRRR while it lasts (69 hours), then the blend.
    assert_eq!(hour(1_793_563_200).temperature.map(|c| c.0), Some(14.9));
    let late = hour(1_793_797_200);
    assert_eq!(late.temperature.map(|c| c.0), Some(14.6), "NBM after HRRR");
    // The blend has no pressure, so ECMWF supplies it.
    assert_eq!(late.pressure_msl.map(|p| p.0), Some(1023.0));
    // Days 1–2 from HRRR, day 3 whole from the blend.
    let highs: Vec<_> = snap.daily.iter().map(|d| d.temp_max.map(|c| c.0)).collect();
    assert_eq!(&highs[..3], [Some(18.7), Some(19.9), Some(19.4)]);
    assert_eq!(snap.daily[2].temp_min.map(|c| c.0), Some(13.8));
    assert_eq!(snap.daily.len(), 7);
    assert_eq!(snap.current.as_ref().map(|c| c.time.0), Some(1_793_637_000));
    assert_eq!(snap.minutely_15.len(), 32, "8 hours of 15-minute steps");
}

#[test]
fn models_outside_their_region_are_not_requested() {
    let oslo = Coordinates {
        latitude: 59.91,
        longitude: 10.75,
    };
    let p = with_models(&["ncep_hrrr_conus", "ncep_nbm_conus", "ecmwf_ifs025"]);
    let url = &p.plan(&oslo, &DataRequirements::full(), &ctx()).unwrap()[0].url;
    assert!(url.contains("models=ecmwf_ifs025"), "{url}");
    assert!(!url.contains("ncep"), "{url}");
    let blend_only = with_models(&["ncep_nbm_conus"]);
    let url = &blend_only
        .plan(&oslo, &DataRequirements::full(), &ctx())
        .unwrap()[0]
        .url;
    assert!(!url.contains("models="), "{url}");
    // An unsuffixed answer (one model sent) is credited to the model the plan chose.
    let snap = parse_forecast(&blend_only, FULL);
    assert_eq!(
        snap.models,
        ["ncep_nbm_conus"],
        "10001 is in the blend's area"
    );
    assert_eq!(
        parse_forecast(&OpenMeteo::default(), FULL).models,
        Vec::<String>::new()
    );
}

#[test]
fn the_variant_is_the_whole_list() {
    let p = with_models(&["ncep_hrrr_conus", "gfs_seamless"]);
    assert_eq!(p.variant(), Some("ncep_hrrr_conus,gfs_seamless"));
    assert_eq!(OpenMeteo::default().variant(), None);
}
