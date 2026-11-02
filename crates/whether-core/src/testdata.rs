//! Deterministic sample snapshots for tests in this and other crates.
//!
//! The location is the test location, ZIP 10001 (New York, New York),
//! on 2026-10-05 in EDT (UTC−4). Sunrise and sunset come from Open-Meteo.

// Indices here are small loop counters.
#![allow(clippy::cast_possible_wrap)]

use alloc::format;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;

use crate::condition::{Condition, Intensity};
use crate::model::{
    AirQuality, Alert, Current, Daily, Hourly, Minutely, ProviderId, ResolvedLocation,
    WeatherSnapshot,
};
use crate::presentation::{PresentationConfig, WidgetKind};
use crate::scales::Level;
use crate::time::{LocalDate, UnixTime, UtcOffset};
use crate::units::{Celsius, Degrees, Hpa, Meters, Mm, Mps, Percent};

/// The view of `snapshot` as one location called `name`, just fetched at `now`,
/// with the default presentation and strings.
pub fn view_of(name: &str, snapshot: &WeatherSnapshot, now: UnixTime) -> crate::view::ViewModel {
    view_with(name, snapshot, now, &PresentationConfig::default())
}

/// Like [`view_of`], with `presentation`.
pub fn view_with(
    name: &str,
    snapshot: &WeatherSnapshot,
    now: UnixTime,
    presentation: &PresentationConfig,
) -> crate::view::ViewModel {
    use crate::view::{LocationView, StatusInfo, ViewInput, build_view};
    let status = StatusInfo {
        last_success: Some(now),
        error: None,
        stale: false,
        refreshing: false,
    };
    let locations = [LocationView {
        name,
        snapshot: Some(snapshot),
        status: &status,
        history: None,
    }];
    build_view(&ViewInput {
        locations: &locations,
        active: 0,
        presentation,
        strings: &crate::strings::Strings::default(),
        now,
    })
}

/// The sample view: New York at [`SAMPLE_NOW`].
pub fn sample_view() -> crate::view::ViewModel {
    view_of("New York", &sample_snapshot(), SAMPLE_NOW)
}

/// The default presentation with the alerts widget turned on.
pub fn presentation_with_alerts() -> PresentationConfig {
    let mut p = PresentationConfig::default();
    p.widgets.push(WidgetKind::Alerts);
    p
}

/// The sample with alerts, viewed with the alerts widget on.
pub fn sample_view_with_alerts() -> crate::view::ViewModel {
    view_with(
        "New York",
        &sample_snapshot_with_alerts(),
        SAMPLE_NOW,
        &presentation_with_alerts(),
    )
}

/// 2026-10-05 11:00 EDT.
pub const SAMPLE_NOW: UnixTime = UnixTime(1_791_212_400);
/// 2026-10-05 00:00 EDT.
pub const SAMPLE_MIDNIGHT: UnixTime = UnixTime(1_791_172_800);
/// Sunrise on 2026-10-05 (07:03 EDT).
pub const SAMPLE_SUNRISE: UnixTime = UnixTime(1_791_198_192);
/// Sunset on 2026-10-05 (18:38 EDT).
pub const SAMPLE_SUNSET: UnixTime = UnixTime(1_791_239_912);
/// EDT.
pub const SAMPLE_OFFSET: UtcOffset = UtcOffset(-14_400);

const HOUR: i64 = 3_600;
const DAY: i64 = 86_400;

/// Hour-of-day temperatures (°C) for the first sample day.
const DIURNAL: [f32; 24] = [
    9.8, 9.4, 9.0, 8.7, 8.5, 8.4, 8.6, 9.3, 10.6, 12.2, 13.8, 15.3, 16.5, 17.4, 17.9, 18.0, 17.6,
    16.8, 15.6, 14.3, 13.1, 12.1, 11.3, 10.6,
];
/// Hour-of-day precipitation probability (%) for the first sample day: rain moves in
/// during the late afternoon.
const DAY1_POP: [f32; 24] = [
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 5.0, 5.0, 10.0, 10.0, 15.0, 20.0, 35.0, 55.0,
    70.0, 80.0, 65.0, 45.0, 30.0, 15.0, 5.0,
];

/// The sample location.
pub fn sample_location() -> ResolvedLocation {
    ResolvedLocation {
        name: "New York".to_string(),
        region: Some("New York, US".to_string()),
        latitude: 40.7143,
        longitude: -74.005_97,
        elevation_m: Some(71.0),
        utc_offset: SAMPLE_OFFSET,
        tz_name: Some("America/New_York".to_string()),
        tz_abbrev: Some("EDT".to_string()),
        station: None,
    }
}

fn condition_for(pop: f32, hour_of_day: usize) -> Condition {
    match pop {
        p if p >= 75.0 => Condition::Rain(Intensity::Heavy),
        p if p >= 60.0 => Condition::Rain(Intensity::Moderate),
        p if p >= 45.0 => Condition::RainShowers(Intensity::Light),
        p if p >= 25.0 => Condition::Overcast,
        p if p >= 10.0 => Condition::Cloudy,
        _ if (9..=16).contains(&hour_of_day) => Condition::PartlyCloudy,
        _ => Condition::Clear,
    }
}

fn is_day(t: i64) -> bool {
    let sod = (t + i64::from(SAMPLE_OFFSET.0)).rem_euclid(DAY);
    // Sunrise 07:03, sunset 18:38 local.
    (7 * HOUR + 3 * 60..18 * HOUR + 38 * 60).contains(&sod)
}

fn hourly_point(i: usize) -> Hourly {
    let hour = i % 24;
    let day = i / 24;
    let t = SAMPLE_MIDNIGHT.0 + i as i64 * HOUR;
    let pop = if day == 0 {
        DAY1_POP[hour]
    } else {
        DAY1_POP[hour] / 2.0
    };
    let uv = if (8..=17).contains(&hour) {
        // A bell curve peaking at 4.2 around 13:00.
        let x = (hour as f32 - 13.0) / 3.0;
        4.2 * libm::expf(-x * x)
    } else {
        0.0
    };
    Hourly {
        time: UnixTime(t),
        is_day: is_day(t),
        condition: condition_for(pop, hour),
        temperature: Some(Celsius(DIURNAL[hour] + day as f32)),
        apparent: Some(Celsius(DIURNAL[hour] + day as f32 - 1.2)),
        precip_probability: Some(Percent(pop)),
        precipitation: Some(Mm(if pop >= 60.0 { pop / 40.0 } else { 0.0 })),
        wind_speed: Some(Mps(2.5 + hour as f32 * 0.15)),
        wind_gust: Some(Mps(5.0 + hour as f32 * 0.3)),
        wind_dir: Some(Degrees(200.0 + hour as f32 * 2.0)),
        humidity: Some(Percent(55.0 + pop / 3.0)),
        pressure_msl: Some(Hpa(1_021.0 - i as f32 * 0.18)),
        uv_index: Some(uv),
        visibility: Some(Meters(if pop >= 60.0 { 8_000.0 } else { 24_140.0 })),
        cloud_cover: Some(Percent(pop.max(20.0))),
    }
}

fn daily_point(i: usize) -> Daily {
    const CONDITIONS: [Condition; 7] = [
        Condition::Rain(Intensity::Heavy),
        Condition::RainShowers(Intensity::Light),
        Condition::PartlyCloudy,
        Condition::Clear,
        Condition::Cloudy,
        Condition::Overcast,
        Condition::Drizzle(Intensity::Light),
    ];
    const HIGHS: [f32; 7] = [18.0, 16.4, 19.2, 21.5, 20.1, 17.3, 15.8];
    const LOWS: [f32; 7] = [8.4, 10.2, 9.1, 8.0, 11.4, 12.0, 10.5];
    const POPS: [f32; 7] = [80.0, 45.0, 10.0, 0.0, 15.0, 25.0, 55.0];
    let d = i as i64;
    let (year, month, day) = crate::time::civil_from_days(20_731 + d);
    let sunrise = SAMPLE_SUNRISE.0 + d * DAY + d * 61;
    let sunset = SAMPLE_SUNSET.0 + d * DAY - d * 98;
    Daily {
        date: LocalDate { year, month, day },
        condition: CONDITIONS[i],
        temp_max: Some(Celsius(HIGHS[i])),
        temp_min: Some(Celsius(LOWS[i])),
        apparent_max: Some(Celsius(HIGHS[i] + 1.0)),
        apparent_min: Some(Celsius(LOWS[i] - 2.0)),
        precip_sum: Some(Mm(if POPS[i] >= 50.0 { POPS[i] / 10.0 } else { 0.0 })),
        precip_probability_max: Some(Percent(POPS[i])),
        sunrise: Some(UnixTime(sunrise)),
        sunset: Some(UnixTime(sunset)),
        daylight_s: u32::try_from(sunset - sunrise).ok(),
        uv_index_max: Some(4.2 - i as f32 * 0.2),
        wind_speed_max: Some(Mps(6.1 + i as f32 * 0.4)),
        wind_gust_max: Some(Mps(11.0 + i as f32 * 0.6)),
        wind_dir_dominant: Some(Degrees(220.0 + i as f32 * 15.0)),
        moon_phase: Some(libm::fmodf(0.817 + i as f32 * 0.034, 1.0)),
        moonrise: Some(UnixTime(1_791_151_751 + d * (DAY + 3_000))),
        moonset: Some(UnixTime(1_791_211_658 + d * (DAY + 3_000))),
    }
}

/// A complete, deterministic snapshot: current conditions, 48 hourly steps, 7 days,
/// a 2-hour nowcast with rain from +45 min, and air quality.
pub fn sample_snapshot() -> WeatherSnapshot {
    let minutely = (0..8)
        .map(|i| Minutely {
            time: UnixTime(SAMPLE_NOW.0 + i * 900),
            precipitation: Some(Mm(if i >= 3 { 0.3 } else { 0.0 })),
        })
        .collect();
    WeatherSnapshot {
        location: sample_location(),
        provider: ProviderId::OpenMeteo,
        supplemented_by: Vec::new(),
        fetched_at: UnixTime(SAMPLE_NOW.0 - 300),
        current: Some(Current {
            time: SAMPLE_NOW,
            is_day: true,
            condition: Condition::PartlyCloudy,
            temperature: Some(Celsius(15.3)),
            apparent: Some(Celsius(14.1)),
            dew_point: None,
            humidity: Some(Percent(62.0)),
            pressure_msl: Some(Hpa(1_019.8)),
            cloud_cover: Some(Percent(45.0)),
            wind_speed: Some(Mps(3.6)),
            wind_gust: Some(Mps(7.2)),
            wind_dir: Some(Degrees(225.0)),
            visibility: Some(Meters(24_140.0)),
            uv_index: Some(3.4),
            precipitation: Some(Mm(0.0)),
        }),
        hourly: (0..48).map(hourly_point).collect(),
        daily: (0..7).map(daily_point).collect(),
        minutely_15: minutely,
        minutely_1: Vec::new(),
        air_quality: Some(AirQuality {
            time: SAMPLE_NOW,
            us_aqi: Some(42),
            european_aqi: Some(28),
            owm_index: None,
            pm2_5: Some(8.4),
            pm10: Some(14.2),
            o3: Some(61.0),
            no2: Some(12.5),
            so2: Some(2.1),
            co: Some(210.0),
        }),
        alerts: Vec::new(),
        offset_changes: Vec::new(),
        models: Vec::new(),
    }
}

/// Two active alerts, the more severe first: a flood warning since 09:00 until
/// Tuesday 06:00 and a wind advisory from 14:00 until midnight. The shape follows a
/// One Call 4.0 alert detail after parsing; the texts are made up.
pub fn sample_alerts() -> Vec<Alert> {
    let alert = |id: &str, title: &str, severity, start: i64, end: i64, text: &str| Alert {
        id: Some(id.to_string()),
        title: title.to_string(),
        description: text.to_string(),
        severity,
        start: UnixTime(SAMPLE_NOW.0 + start * HOUR),
        end: UnixTime(SAMPLE_NOW.0 + end * HOUR),
        source: "NWS Upton NY".to_string(),
        checked_at: Some(UnixTime(SAMPLE_NOW.0 - 300)),
        url: Some(format!("https://api.weather.gov/alerts/{id}")),
    };
    vec![
        alert(
            "sample-flood-warning",
            "Flood Warning",
            Level::VeryPoor,
            -2,
            19,
            "The East Branch Bronx River near New York is above flood stage and \
             is expected to crest this evening. Roads along the creek are flooded; do not \
             drive through flooded roadways.",
        ),
        alert(
            "sample-wind-advisory",
            "Wind Advisory",
            Level::Moderate,
            3,
            13,
            "West winds 20 to 30 mph with gusts up to 50 mph. Gusty winds will blow \
             around unsecured objects and may bring down tree limbs.",
        ),
    ]
}

/// [`sample_snapshot`] with [`sample_alerts`].
pub fn sample_snapshot_with_alerts() -> WeatherSnapshot {
    WeatherSnapshot {
        alerts: sample_alerts(),
        ..sample_snapshot()
    }
}

/// Only current conditions and three hourly steps.
pub fn sample_snapshot_sparse() -> WeatherSnapshot {
    let full = sample_snapshot();
    WeatherSnapshot {
        hourly: full.hourly[11..14].to_vec(),
        daily: Vec::new(),
        minutely_15: Vec::new(),
        minutely_1: Vec::new(),
        air_quality: None,
        ..full
    }
}

/// What OpenWeather's free API provides: 3-hour steps for 5 days, daily aggregates
/// without UV or moon data, no nowcast, and only the 1–5 air-quality index.
pub fn sample_snapshot_owm_free() -> WeatherSnapshot {
    let full = sample_snapshot();
    let hourly = (0..40)
        .map(|i| {
            let mut h = hourly_point((i * 3) % 48);
            h.time = UnixTime(SAMPLE_MIDNIGHT.0 + i as i64 * 3 * HOUR);
            h.is_day = is_day(h.time.0);
            h.uv_index = None;
            h
        })
        .collect();
    let daily = full.daily[..5]
        .iter()
        .map(|d| Daily {
            // The free API has no daily data; these days stand in for its own.
            apparent_max: None,
            apparent_min: None,
            uv_index_max: None,
            moon_phase: None,
            moonrise: None,
            moonset: None,
            daylight_s: None,
            ..d.clone()
        })
        .collect();
    let current = full.current.clone().map(|c| Current {
        uv_index: None,
        ..c
    });
    WeatherSnapshot {
        provider: ProviderId::OpenWeather,
        current,
        hourly,
        daily,
        minutely_15: vec![],
        minutely_1: vec![],
        air_quality: Some(AirQuality {
            us_aqi: None,
            european_aqi: None,
            owm_index: Some(2),
            ..full.air_quality.clone().unwrap_or_default()
        }),
        ..full
    }
}

/// An observed day: (month, day), high °F, low °F, precipitation in (`None` for
/// a trace).
type ObservedDay = ((u8, u8), f32, f32, Option<f32>);

/// °F as °C.
fn fahrenheit(f: f32) -> Celsius {
    Celsius((f - 32.0) * 5.0 / 9.0)
}

/// A history for ZIP 10001 on [`SAMPLE_NOW`] (2026-10-05), from the ACIS numbers
/// verified on 2026-11-02 for 5 October: Central Park's normals 69/55 °F and
/// 0.15 in, New York's threaded records 94 °F (1941) and 35 °F (1881), rain on 7
/// of the 30 days in 1991–2020, and its reports for the week before. The seven
/// days before today cross into September; 30 September is left out (no report),
/// 2 and 3 October had only a trace.
///
/// # Panics
///
/// Never: the location is set.
pub fn history_10001() -> crate::history::History {
    use crate::history::{HistoryBuilder, Record, YearRange};
    use crate::model::Station;

    let mut b = HistoryBuilder::new(ProviderId::Acis);
    b.set_location(sample_location());
    b.set_station(Station {
        id: "NYC".to_string(),
        name: "NY City Central Park".to_string(),
        distance_m: Some(7_800),
    });
    b.set_records_station(Station {
        id: "NYCthr".to_string(),
        name: "New York-Central Park Area".to_string(),
        distance_m: None,
    });
    b.set_period(YearRange::NORMALS_1991_2020);
    let days: [ObservedDay; 6] = [
        ((9, 28), 62.0, 57.0, Some(0.45)),
        ((9, 29), 70.0, 59.0, Some(0.0)),
        ((10, 1), 76.0, 65.0, Some(0.0)),
        ((10, 2), 83.0, 68.0, None),
        ((10, 3), 70.0, 59.0, None),
        ((10, 4), 62.0, 56.0, Some(0.15)),
    ];
    for ((month, day), high, low, precip) in days {
        let d = b.day_mut(LocalDate {
            year: 2026,
            month,
            day,
        });
        d.temp_max = Some(fahrenheit(high));
        d.temp_min = Some(fahrenheit(low));
        d.precip_sum = Some(Mm(precip.unwrap_or(0.0) * 25.4));
        d.trace = precip.is_none();
    }
    // Normals from 28 September to the week ahead; records and the chance of
    // precipitation for today.
    for back in -7..=7_i64 {
        let (year, month, day) = crate::time::civil_from_days(
            SAMPLE_NOW.local_date(SAMPLE_OFFSET).days_since_epoch() + back,
        );
        let c = b.climate_mut(LocalDate { year, month, day });
        c.normal_max = Some(fahrenheit(69.0));
        c.normal_min = Some(fahrenheit(55.0));
        c.normal_precip = Some(Mm(0.15 * 25.4));
    }
    let today = b.climate_mut(SAMPLE_NOW.local_date(SAMPLE_OFFSET));
    today.precip_chance = Some(Percent(23.0));
    today.record_max = Some(Record {
        value: fahrenheit(94.0).0,
        year: 1941,
    });
    today.record_min = Some(Record {
        value: fahrenheit(35.0).0,
        year: 1881,
    });
    // INVARIANT: the location is set above.
    b.finish(SAMPLE_NOW).expect("a location is set")
}

/// The trip the planner tests ask about: 12–19 July 2027 over 1995–2024.
///
/// # Panics
///
/// Never: the dates and years are valid.
pub fn july_query() -> crate::history::ClimateQuery {
    let on = |day| LocalDate {
        year: 2027,
        month: 7,
        day,
    };
    // INVARIANT: eight real dates, years in order.
    crate::history::ClimateQuery::new(
        on(12),
        on(19),
        crate::history::YearRange {
            first: 1995,
            last: 2024,
        },
    )
    .expect("a valid query")
}

/// 12–19 July at Central Park over 1995–2024: day, mean high and low (°F),
/// mean precipitation (in), the share of years wet, the hottest high and the
/// coldest low with their years.
type ClimateRow = (u8, f32, f32, f32, f32, (f32, i32), (f32, i32));
#[rustfmt::skip]
const JULY_10001: [ClimateRow; 8] = [
    (12, 84.3, 68.9, 0.192, 10.0 / 30.0, (94.0, 2010), (62.0, 2002)),
    (13, 83.2, 69.3, 0.197, 11.0 / 30.0, (92.0, 1997), (61.0, 2009)),
    (14, 84.1, 69.8, 0.152, 11.0 / 30.0, (96.0, 1995), (61.0, 2009)),
    (15, 86.2, 70.7, 0.150, 10.0 / 30.0, (102.0, 1995), (63.0, 2009)),
    (16, 86.7, 71.1, 0.085, 6.0 / 30.0, (97.0, 1999), (64.0, 2015)),
    (17, 88.2, 71.8, 0.353, 10.0 / 30.0, (98.0, 1999), (66.0, 2003)),
    (18, 87.9, 71.7, 0.282, 16.0 / 30.0, (100.0, 2012), (64.0, 2014)),
    (19, 85.9, 71.2, 0.047, 8.0 / 30.0, (97.0, 1999), (61.0, 2000)),
];

/// The answer to [`july_query`] at ZIP 10001 as ACIS gave it on 2026-11-02
/// (`climate_NYC_0712_0719.json`): Central Park's averages, chance of
/// rain or snow, hottest high and coldest low per date.
///
/// # Panics
///
/// Never: the location is set.
pub fn climate_10001() -> crate::history::History {
    use crate::history::{HistoryBuilder, Record};
    use crate::model::Station;

    let query = july_query();
    let mut b = HistoryBuilder::new(ProviderId::Acis);
    b.set_location(sample_location());
    b.set_station(Station {
        id: "NYC".to_string(),
        name: "NY City Central Park".to_string(),
        distance_m: Some(7_800),
    });
    b.set_period(query.years);
    for (day, high, low, precip, chance, hottest, coldest) in JULY_10001 {
        let c = b.climate_mut(LocalDate {
            year: 2027,
            month: 7,
            day,
        });
        c.normal_max = Some(fahrenheit(high));
        c.normal_min = Some(fahrenheit(low));
        c.normal_precip = Some(Mm(precip * 25.4));
        c.precip_chance = Some(Percent(100.0 * chance));
        c.record_max = Some(Record {
            value: fahrenheit(hottest.0).0,
            year: hottest.1,
        });
        c.record_min = Some(Record {
            value: fahrenheit(coldest.0).0,
            year: coldest.1,
        });
    }
    // INVARIANT: the location is set above.
    b.finish(SAMPLE_NOW).expect("a location is set")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::SnapshotBuilder;
    use crate::derive::{current_hour_index, today_index};

    fn rebuilt(s: &WeatherSnapshot) -> WeatherSnapshot {
        let mut b = SnapshotBuilder::new(s.provider);
        b.set_location(s.location.clone());
        if let Some(c) = s.current.clone() {
            b.set_current(c);
        }
        b.set_hourly(s.hourly.clone());
        b.set_daily(s.daily.clone());
        b.set_minutely(s.minutely_15.clone());
        if let Some(a) = s.air_quality.clone() {
            b.set_air_quality(a);
        }
        for a in &s.alerts {
            b.push_alert(a.clone());
        }
        b.finish(s.fetched_at).unwrap()
    }

    #[test]
    fn samples_are_already_valid_snapshots() {
        for s in [
            sample_snapshot(),
            sample_snapshot_sparse(),
            sample_snapshot_owm_free(),
            sample_snapshot_with_alerts(),
        ] {
            assert_eq!(rebuilt(&s), s);
        }
    }

    #[test]
    fn sample_shape() {
        let s = sample_snapshot();
        assert_eq!(s.hourly.len(), 48);
        assert_eq!(s.daily.len(), 7);
        assert_eq!(
            s.daily[0].date,
            LocalDate {
                year: 2026,
                month: 10,
                day: 5
            }
        );
        assert_eq!(current_hour_index(&s.hourly, SAMPLE_NOW), Some(11));
        assert_eq!(today_index(&s.daily, SAMPLE_NOW, SAMPLE_OFFSET), Some(0));
        assert_eq!(s.daily[0].sunrise, Some(SAMPLE_SUNRISE));
        assert!(s.hourly[11].is_day && !s.hourly[7].is_day && s.hourly[18].is_day);
    }

    #[test]
    fn alerts_sample_shows_both_alerts() {
        let vm = sample_view_with_alerts();
        let shown: Vec<_> = vm
            .alerts
            .iter()
            .map(|a| (a.title.as_str(), a.level, a.period.as_str()))
            .collect();
        assert_eq!(
            shown,
            [
                ("Flood Warning", Level::VeryPoor, "Mon 09:00 – Tue 06:00"),
                ("Wind Advisory", Level::Moderate, "Mon 14:00 – Tue 00:00"),
            ]
        );
        assert!(vm.widgets.contains(&WidgetKind::Alerts));
        assert_eq!(sample_view().alerts, []);
    }

    #[test]
    fn owm_free_sample_lacks_what_the_free_api_lacks() {
        let s = sample_snapshot_owm_free();
        assert_eq!(s.hourly.len(), 40);
        assert_eq!(s.hourly[1].time.0 - s.hourly[0].time.0, 3 * HOUR);
        assert_eq!(s.daily.len(), 5);
        assert!(
            s.daily
                .iter()
                .all(|d| d.uv_index_max.is_none() && d.moon_phase.is_none())
        );
        assert_eq!(s.minutely_15, Vec::<Minutely>::new());
        let aq = s.air_quality.unwrap();
        assert_eq!((aq.owm_index, aq.us_aqi), (Some(2), None));
    }
}
