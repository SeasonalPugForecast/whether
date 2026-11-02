//! Tests of the view-model builder against the ZIP 10001 sample day.

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt::Write;

use proptest::prelude::*;

use super::*;
use crate::model::ProviderId;
use crate::presentation::TimeFormat;
use crate::testdata::{
    SAMPLE_NOW, sample_snapshot, sample_snapshot_owm_free, sample_snapshot_sparse,
};
use crate::units::UnitPrefs;

fn all_widgets() -> PresentationConfig {
    PresentationConfig {
        widgets: vec![
            WidgetKind::Current,
            WidgetKind::Summary,
            WidgetKind::Nowcast,
            WidgetKind::Hourly,
            WidgetKind::Daily,
            WidgetKind::Details,
            WidgetKind::Sun,
            WidgetKind::Moon,
            WidgetKind::AirQuality,
            WidgetKind::Alerts,
        ],
        hourly_hours: 6,
        daily_days: 3,
        ..PresentationConfig::default()
    }
}

fn ok_status() -> StatusInfo {
    StatusInfo {
        last_success: Some(crate::time::UnixTime(SAMPLE_NOW.0 - 300)),
        ..Default::default()
    }
}

fn view(snap: Option<&WeatherSnapshot>, p: &PresentationConfig, status: &StatusInfo) -> ViewModel {
    let strings = Strings::default();
    let locations = [LocationView {
        name: "New York",
        snapshot: snap,
        status,
        history: None,
    }];
    build_view(&ViewInput {
        locations: &locations,
        active: 0,
        presentation: p,
        strings: &strings,
        now: SAMPLE_NOW,
    })
}

/// A compact rendering of the parts that change between variants.
fn brief(vm: &ViewModel) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "status: {} | {} | stale={} error={:?}",
        vm.status.updated, vm.status.provider, vm.status.stale, vm.status.error
    );
    let _ = writeln!(
        out,
        "theme: {:?} day={} widgets: {:?}",
        vm.theme, vm.is_day, vm.widgets
    );
    if let Some(c) = &vm.current {
        let _ = writeln!(
            out,
            "current: {} {} | {} | H {} L {}",
            c.temperature.text, c.condition_label, c.feels_like_label, c.high.text, c.low.text
        );
    }
    if let Some(s) = &vm.summary {
        let _ = writeln!(out, "summary: {s}");
    }
    if let Some(h) = &vm.hourly {
        let cols: Vec<String> = h
            .hours
            .iter()
            .take(4)
            .map(|h| {
                format!(
                    "{} {} {:?}%",
                    h.label, h.temperature.text, h.precip_probability
                )
            })
            .collect();
        let _ = writeln!(
            out,
            "hourly: {} | range {:.1}..{:.1}",
            cols.join(", "),
            h.temp_min,
            h.temp_max
        );
    }
    if let Some(d) = &vm.daily {
        let rows: Vec<String> = d
            .days
            .iter()
            .map(|d| {
                format!(
                    "{} {} {}/{}",
                    d.label, d.date_short, d.high.text, d.low.text
                )
            })
            .collect();
        let _ = writeln!(out, "daily: {}", rows.join(", "));
    }
    for d in &vm.details {
        let _ = writeln!(out, "detail: {} = {} ({:?})", d.title, d.value, d.caption);
    }
    if let Some(s) = &vm.sun {
        let _ = writeln!(
            out,
            "sun: {} - {} ({}) progress={:?}",
            s.sunrise,
            s.sunset,
            s.daylight,
            s.progress.map(|p| (p * 100.0) as u8)
        );
    }
    let _ = writeln!(out, "attribution: {}", vm.attribution);
    out
}

#[test]
fn full_view_all_widgets() {
    let snap = sample_snapshot();
    insta::assert_debug_snapshot!(view(Some(&snap), &all_widgets(), &ok_status()));
}

#[test]
fn default_metric() {
    let snap = sample_snapshot();
    insta::assert_snapshot!(brief(&view(
        Some(&snap),
        &PresentationConfig::default(),
        &ok_status()
    )));
}

#[test]
fn imperial() {
    let snap = sample_snapshot();
    let p = PresentationConfig {
        units: UnitPrefs::IMPERIAL,
        ..Default::default()
    };
    insta::assert_snapshot!(brief(&view(Some(&snap), &p, &ok_status())));
}

#[test]
fn twelve_hour_clock() {
    let snap = sample_snapshot();
    let p = PresentationConfig {
        time_format: TimeFormat::H12,
        ..Default::default()
    };
    insta::assert_snapshot!(brief(&view(Some(&snap), &p, &ok_status())));
}

#[test]
fn decimals_and_unit_letters() {
    let snap = sample_snapshot();
    let p = PresentationConfig {
        temperature_decimals: 1,
        show_unit_letter: true,
        ..Default::default()
    };
    insta::assert_snapshot!(brief(&view(Some(&snap), &p, &ok_status())));
}

#[test]
fn sparse_snapshot() {
    let snap = sample_snapshot_sparse();
    insta::assert_snapshot!(brief(&view(
        Some(&snap),
        &PresentationConfig::default(),
        &ok_status()
    )));
}

#[test]
fn no_snapshot_with_error() {
    let status = StatusInfo {
        error: Some("OpenWeather: unauthorized".into()),
        ..Default::default()
    };
    let vm = view(None, &PresentationConfig::default(), &status);
    assert!(vm.widgets.is_empty() && vm.current.is_none() && vm.attribution.is_empty());
    insta::assert_snapshot!(brief(&vm));
}

#[test]
fn polar_night_has_no_sun_times() {
    let mut snap = sample_snapshot();
    for d in &mut snap.daily {
        d.sunrise = None;
        d.sunset = None;
        d.daylight_s = None;
    }
    let vm = view(Some(&snap), &PresentationConfig::default(), &ok_status());
    let sun = vm.sun.unwrap();
    assert_eq!(
        (
            sun.sunrise.as_str(),
            sun.sunset.as_str(),
            sun.daylight.as_str()
        ),
        ("—", "—", "—")
    );
    assert_eq!(sun.progress, None);
    assert_eq!((sun.rise.as_ref(), sun.set.as_ref()), (None, None));
}

#[test]
fn sunrise_sunset_and_moon_times_carry_labels_and_instants() {
    let snap = sample_snapshot();
    let vm = view(Some(&snap), &all_widgets(), &ok_status());
    let today = &snap.daily[0];
    let sun = vm.sun.as_ref().unwrap();
    let (rise, set) = (sun.rise.as_ref().unwrap(), sun.set.as_ref().unwrap());
    assert_eq!(
        (rise.label.as_str(), rise.time.as_str()),
        ("Sunrise", "07:03")
    );
    assert_eq!((set.label.as_str(), set.time.as_str()), ("Sunset", "18:38"));
    assert_eq!((Some(rise.at), Some(set.at)), (today.sunrise, today.sunset));
    assert_eq!(rise.time, sun.sunrise, "the same text as the plain field");
    let noon = sun.noon.as_ref().unwrap();
    assert_eq!(noon.label, "Solar noon");
    assert_eq!(Some(noon.time.clone()), sun.solar_noon);
    assert!(
        (rise.at.0..set.at.0).contains(&noon.at.0),
        "between the two"
    );
    let moon = vm.moon.as_ref().unwrap();
    let (rise, set) = (moon.rise.as_ref().unwrap(), moon.set.as_ref().unwrap());
    assert_eq!(
        (rise.label.as_str(), set.label.as_str()),
        ("Moonrise", "Moonset")
    );
    assert_eq!(
        (Some(rise.at), Some(set.at)),
        (today.moonrise, today.moonset)
    );
    assert_eq!(Some(rise.time.clone()), moon.moonrise);
    assert_eq!(Some(set.time.clone()), moon.moonset);
}

#[test]
fn a_moon_that_does_not_rise_today_has_no_rise_event() {
    let mut snap = sample_snapshot();
    snap.daily[0].moonrise = None;
    let vm = view(Some(&snap), &all_widgets(), &ok_status());
    let moon = vm.moon.unwrap();
    assert!(moon.rise.is_none() && moon.moonrise.is_none());
    assert!(moon.set.is_some());
}

#[test]
fn stale_and_refreshing_status() {
    let snap = sample_snapshot();
    let status = StatusInfo {
        last_success: Some(crate::time::UnixTime(SAMPLE_NOW.0 - 7_300)),
        stale: true,
        refreshing: true,
        error: None,
    };
    let vm = view(Some(&snap), &PresentationConfig::default(), &status);
    assert_eq!(vm.status.updated, "Updated 2 h ago");
    assert!(vm.status.stale && vm.status.refreshing);
    let fetching = view(
        None,
        &PresentationConfig::default(),
        &StatusInfo {
            refreshing: true,
            ..Default::default()
        },
    );
    assert_eq!(fetching.status.updated, "Fetching weather…");
}

#[test]
fn key_values_for_the_sample_day() {
    let snap = sample_snapshot();
    let vm = view(Some(&snap), &all_widgets(), &ok_status());
    assert_eq!(vm.location.local_time, "Mon 11:00");
    assert_eq!(vm.theme, crate::theme::SkyTheme::ClearDay);
    let hourly = vm.hourly.as_ref().unwrap();
    assert_eq!(hourly.hours[0].label, "Now");
    assert_eq!(hourly.hours[1].label, "12:00");
    let daily = vm.daily.as_ref().unwrap();
    let labels: Vec<&str> = daily.days.iter().map(|d| d.label.as_str()).collect();
    assert_eq!(labels, ["Today", "Tomorrow", "Wed"]);
    assert_eq!(daily.days[0].date_short, "Oct 5");
    assert_eq!(
        vm.summary.as_deref(),
        Some("Partly cloudy. High 18°, low 8°. Rain starting in ~45 min.")
    );
    assert_eq!(
        vm.nowcast.as_ref().unwrap().text,
        "Rain starting in ~45 min."
    );
    assert_eq!(vm.air_quality.as_ref().unwrap().index, 42);
    assert_eq!(vm.attribution, "Weather data by Open-Meteo.com");
    assert_eq!(vm.widgets.len(), 9, "alerts has no data and is dropped");
    let sun = vm.sun.as_ref().unwrap();
    assert_eq!(
        (sun.sunrise.as_str(), sun.sunset.as_str()),
        ("07:03", "18:38")
    );
}

#[test]
fn supplemented_snapshots_credit_every_source() {
    let mut snap = sample_snapshot_owm_free();
    snap.fill_gaps_from(&sample_snapshot());
    let vm = view(Some(&snap), &PresentationConfig::default(), &ok_status());
    assert_eq!(
        vm.attribution,
        "Weather data provided by OpenWeather · Open-Meteo.com"
    );
    assert_eq!(vm.status.provider, "OpenWeather");
    assert_eq!(snap.supplemented_by, [ProviderId::OpenMeteo]);
}

#[test]
fn the_status_names_the_station_that_observed() {
    let mut snap = sample_snapshot();
    let p = PresentationConfig {
        time_format: TimeFormat::H12,
        ..Default::default()
    };
    assert_eq!(
        view(Some(&snap), &p, &ok_status()).status.observed,
        None,
        "a forecast"
    );
    snap.location.station = Some(crate::model::Station {
        id: "KNYC".into(),
        name: "Central Park".into(),
        distance_m: Some(14_000),
    });
    // 12:55 UTC is 8:55 AM in New York (EDT).
    let current = snap.current.as_mut().unwrap();
    current.time = UnixTime(1_792_241_700);
    assert_eq!(snap.location.utc_offset.0, -4 * 3600);
    assert_eq!(
        view(Some(&snap), &p, &ok_status())
            .status
            .observed
            .as_deref(),
        Some("Observed at Central Park, 8:55 AM")
    );
}

#[test]
fn the_status_names_the_models_in_order() {
    let mut snap = sample_snapshot();
    snap.models = vec![
        "ncep_hrrr_conus".into(),
        "ncep_nbm_conus".into(),
        "knmi_seamless".into(),
    ];
    let vm = view(Some(&snap), &PresentationConfig::default(), &ok_status());
    assert_eq!(
        vm.status.provider,
        "Open-Meteo · HRRR → NBM → knmi_seamless"
    );
    // The license credit stays the provider's.
    assert_eq!(vm.attribution, "Weather data by Open-Meteo.com");
}

#[test]
fn air_quality_scale_preference_and_fallback() {
    let snap = sample_snapshot();
    let eu = PresentationConfig {
        aqi_scale: crate::presentation::AqiScalePref::European,
        ..Default::default()
    };
    let vm = view(Some(&snap), &eu, &ok_status());
    assert_eq!(vm.air_quality.unwrap().scale, AqiScale::European);
    let owm = sample_snapshot_owm_free();
    let vm = view(Some(&owm), &PresentationConfig::default(), &ok_status());
    let aq = vm.air_quality.unwrap();
    assert_eq!(
        (aq.scale, aq.index, aq.category_label.as_str()),
        (AqiScale::Owm, 2, "Fair")
    );
}

#[test]
fn out_of_range_active_index_is_clamped() {
    let snap = sample_snapshot();
    let strings = Strings::default();
    let status = ok_status();
    let p = PresentationConfig::default();
    let locations = [LocationView {
        name: "A",
        snapshot: Some(&snap),
        status: &status,
        history: None,
    }];
    let vm = build_view(&ViewInput {
        locations: &locations,
        active: 7,
        presentation: &p,
        strings: &strings,
        now: SAMPLE_NOW,
    });
    assert_eq!(vm.active, 0);
    let vm = build_view(&ViewInput {
        locations: &[],
        active: 0,
        presentation: &p,
        strings: &strings,
        now: SAMPLE_NOW,
    });
    assert!(vm.tabs.is_empty() && vm.widgets.is_empty());
}

fn thin(mut snap: WeatherSnapshot, mask: u64) -> WeatherSnapshot {
    let bit = |n: u32| mask & (1 << n) != 0;
    if bit(0) {
        snap.current = None;
    }
    if let Some(c) = snap.current.as_mut() {
        if bit(1) {
            c.temperature = None;
        }
        if bit(2) {
            c.humidity = None;
        }
        if bit(3) {
            c.wind_dir = None;
        }
        if bit(4) {
            c.uv_index = None;
        }
        if bit(5) {
            c.pressure_msl = None;
        }
    }
    if bit(6) {
        snap.hourly.clear();
    }
    if bit(7) {
        snap.daily.clear();
    }
    if bit(8) {
        snap.minutely_15.clear();
    }
    if bit(9) {
        snap.air_quality = None;
    }
    for h in &mut snap.hourly {
        if bit(10) {
            h.temperature = None;
        }
        if bit(11) {
            h.precip_probability = None;
        }
    }
    for d in &mut snap.daily {
        if bit(12) {
            d.temp_max = None;
        }
        if bit(13) {
            d.temp_min = None;
        }
        if bit(14) {
            d.moon_phase = None;
        }
    }
    snap
}

proptest! {
    #[test]
    fn never_panics(mask in any::<u64>(), shift in -200_000i64..200_000, hours in 0u8..60, days in 0u8..20) {
        let snap = thin(sample_snapshot(), mask);
        let p = PresentationConfig { hourly_hours: hours, daily_days: days, ..all_widgets() };
        let strings = Strings::default();
        let status = ok_status();
        let locations = [LocationView { name: "x", snapshot: Some(&snap), status: &status, history: None }];
        let now = crate::time::UnixTime(SAMPLE_NOW.0 + shift);
        let vm = build_view(&ViewInput { locations: &locations, active: 0, presentation: &p, strings: &strings, now });
        prop_assert!(vm.widgets.len() <= p.widgets.len());
    }
}

#[test]
fn timeline_spans_the_next_24_hours() {
    use crate::view::TimelineGroup::{Clear, Cloudy, HeavyRain, PartlyCloudy, Rain};
    let p = PresentationConfig {
        widgets: vec![WidgetKind::Hourly],
        hourly_hours: 48,
        ..PresentationConfig::default()
    };
    let vm = view(Some(&sample_snapshot()), &p, &ok_status());
    let hourly = vm.hourly.unwrap();
    let spans: Vec<_> = hourly
        .timeline
        .iter()
        .map(|s| (s.start, s.len, s.group))
        .collect();
    // From 11:00: clouds, the evening's rain with a heavy hour at 18:00, clouds,
    // a clear night, and a partly cloudy morning.
    assert_eq!(
        spans,
        [
            (0, 5, Cloudy),
            (5, 2, Rain),
            (7, 1, HeavyRain),
            (8, 2, Rain),
            (10, 2, Cloudy),
            (12, 10, Clear),
            (22, 2, PartlyCloudy),
        ]
    );
    assert_eq!(hourly.timeline[1].label, "Rain", "the worst of the stretch");
    assert_eq!(hourly.timeline[2].label, "Heavy rain");
    assert_eq!(hourly.hours[0].uv_text.as_deref(), Some("3"));
    assert_eq!(hourly.hours[0].humidity_text.as_deref(), Some("58%"));
}

/// Each hour carries its day and date, so a chart crossing midnight can say
/// which day the hour under the finger is.
#[test]
fn hours_carry_their_day_and_date() {
    let p = PresentationConfig {
        widgets: vec![WidgetKind::Hourly],
        hourly_hours: 48,
        date_format: crate::presentation::DateFormat::Compact,
        ..PresentationConfig::default()
    };
    let vm = view(Some(&sample_snapshot()), &p, &ok_status());
    let day = |i: usize| {
        let h = &vm.hourly.as_ref().unwrap().hours[i];
        (h.label.as_str(), h.day.as_str(), h.date_short.as_str())
    };
    // From 11:00 on Sunday 5 October 2025.
    assert_eq!(day(0), ("Now", "Today", "OCT5"));
    assert_eq!(day(12), ("23:00", "Today", "OCT5"));
    assert_eq!(day(13), ("00:00", "Tomorrow", "OCT6"));
    assert_eq!(day(36), ("23:00", "Tomorrow", "OCT6"));
}

#[test]
fn precipitation_table_hour_by_hour() {
    let p = PresentationConfig {
        widgets: vec![WidgetKind::Precipitation],
        precip_hours: 12,
        units: UnitPrefs::METRIC,
        time_format: TimeFormat::H24,
        ..PresentationConfig::default()
    };
    let snap = sample_snapshot();
    let vm = view(Some(&snap), &p, &ok_status());
    assert_eq!(vm.widgets, [WidgetKind::Precipitation]);
    let t = vm.precipitation.as_ref().unwrap();
    assert_eq!(t.hours.len(), 12);
    assert_eq!(t.hours[0].label, "Now");
    let wet: Vec<&str> = t
        .hours
        .iter()
        .filter(|h| h.intensity > 0.0)
        .map(|h| h.label.as_str())
        .collect();
    assert_eq!(
        wet,
        ["17:00", "18:00", "19:00"],
        "the sample's evening rain"
    );
    assert!(t.hours.iter().all(|h| (0.0..=1.0).contains(&h.intensity)));
    assert_eq!(t.max_chance.as_deref(), Some("80%"));
    assert!(t.total.text.ends_with("mm"), "{}", t.total.text);
    assert_eq!(t.dry, None);

    // The first hours of the day are dry: six hours from midnight.
    let early = PresentationConfig {
        precip_hours: 6,
        ..p
    };
    let strings = Strings::default();
    let status = ok_status();
    let locations = [LocationView {
        name: "New York",
        snapshot: Some(&snap),
        status: &status,
        history: None,
    }];
    let vm = build_view(&ViewInput {
        locations: &locations,
        active: 0,
        presentation: &early,
        strings: &strings,
        now: snap.hourly[0].time,
    });
    let t = vm.precipitation.unwrap();
    assert_eq!(
        t.dry.as_deref(),
        Some("No precipitation in the next 6 hours.")
    );
    assert!(t.hours.iter().all(|h| h.intensity == 0.0));
}

#[test]
fn alerts_show_until_they_end() {
    let mut snap = sample_snapshot();
    let alert = |title: &str, start: i64, end: i64| crate::model::Alert {
        id: Some(format!("id-{title}")),
        title: title.into(),
        description: "Details.".into(),
        severity: crate::scales::Level::VeryPoor,
        start: crate::time::UnixTime(start),
        end: crate::time::UnixTime(end),
        source: "NWS Upton NY".into(),
        checked_at: None,
        url: None,
    };
    snap.alerts = vec![
        alert("Frost Advisory", SAMPLE_NOW.0 - 7200, SAMPLE_NOW.0),
        alert("Wind Advisory", SAMPLE_NOW.0 - 3600, SAMPLE_NOW.0 + 7200),
    ];
    let vm = view(Some(&snap), &all_widgets(), &ok_status());
    assert_eq!(vm.alerts.len(), 1, "the frost advisory has ended");
    let a = &vm.alerts[0];
    assert_eq!(
        (a.title.as_str(), a.body.as_str(), a.level),
        ("Wind Advisory", "Details.", crate::scales::Level::VeryPoor)
    );
    assert_eq!(a.period, "Mon 10:00 – Mon 13:00");
    assert_eq!(
        a.id.as_deref(),
        Some("id-Wind Advisory"),
        "the provider's id"
    );
    assert_eq!(a.end.0, SAMPLE_NOW.0 + 7200);
    assert!(vm.widgets.contains(&WidgetKind::Alerts));
}

/// the alert's link reaches the view, and an Apple alert (which has no
/// start or end of its own) says when it was checked instead of a period.
#[test]
fn alerts_carry_their_link_and_apple_alerts_say_when() {
    let mut snap = sample_snapshot();
    let alert = |id: &str, url: Option<&str>| crate::model::Alert {
        id: Some(id.into()),
        title: "Flood Warning".into(),
        description: "Details.".into(),
        severity: crate::scales::Level::VeryPoor,
        start: crate::time::UnixTime(SAMPLE_NOW.0 - 3600),
        end: crate::time::UnixTime(SAMPLE_NOW.0 + 3600),
        source: "Met Office".into(),
        checked_at: None,
        url: url.map(Into::into),
    };
    snap.alerts = vec![
        alert("nws-1", Some("https://example.invalid/a")),
        alert("apple:Met Office|Flood Warning|Kent", None),
    ];
    let vm = view(Some(&snap), &all_widgets(), &ok_status());
    assert_eq!(
        vm.alerts[0].url.as_deref(),
        Some("https://example.invalid/a")
    );
    assert_eq!(vm.alerts[0].source.as_deref(), Some("Met Office"));
    assert_eq!(vm.alerts[0].period, "Mon 10:00 – Mon 12:00");
    assert_eq!(vm.alerts[1].url, None);
    assert_eq!(vm.alerts[1].period, "As of Mon 10:00");
}

/// the German pack reaches labels, conditions, weekdays and summaries.
#[test]
fn the_view_speaks_german() {
    let snap = sample_snapshot();
    let status = ok_status();
    let strings = Strings::localized("de", &alloc::collections::BTreeMap::new()).0;
    let locations = [LocationView {
        name: "New York",
        snapshot: Some(&snap),
        status: &status,
        history: None,
    }];
    let vm = build_view(&ViewInput {
        locations: &locations,
        active: 0,
        presentation: &PresentationConfig::default(),
        strings: &strings,
        now: SAMPLE_NOW,
    });
    let hourly = vm.hourly.as_ref().unwrap();
    assert_eq!(hourly.hours[hourly.now_index].label, "Jetzt");
    let days: Vec<&str> = vm
        .daily
        .as_ref()
        .unwrap()
        .days
        .iter()
        .map(|d| d.label.as_str())
        .collect();
    assert_eq!(&days[..3], ["Heute", "Morgen", "Mi"]);
    let current = vm.current.as_ref().unwrap();
    assert_eq!(current.condition_label, "Teilweise bewölkt");
    assert!(current.feels_like_label.starts_with("Gefühlt "));
    let summary = vm.summary.as_deref().unwrap();
    assert!(
        summary.contains("Höchstwert") && summary.contains("Tiefstwert"),
        "{summary}"
    );
    assert_eq!(vm.status.updated, "Vor 5 Min. aktualisiert");
}

/// after an offset change (DST ends) times are labelled in the new offset;
/// before it, in the old one.
#[test]
fn labels_follow_offset_changes() {
    use crate::model::OffsetChange;
    use crate::time::UtcOffset;
    let mut snap = sample_snapshot();
    // From 14:00 EDT on, standard time (UTC−5): pretend the clocks go back then.
    let change = UnixTime(SAMPLE_NOW.0 + 3 * 3600);
    snap.offset_changes = vec![OffsetChange {
        at: change,
        offset: UtcOffset(-18_000),
    }];
    assert_eq!(snap.offset_at(SAMPLE_NOW), snap.location.utc_offset);
    assert_eq!(snap.offset_at(change), UtcOffset(-18_000));
    let vm = view(Some(&snap), &all_widgets(), &ok_status());
    let hourly = vm.hourly.as_ref().unwrap();
    let labels: Vec<&str> = hourly.hours[hourly.now_index..hourly.now_index + 5]
        .iter()
        .map(|h| h.label.as_str())
        .collect();
    // 13:00 EDT is followed by 14:00 EDT = 13:00 EST.
    assert_eq!(labels, ["Now", "12:00", "13:00", "13:00", "14:00"]);
    let sun = vm.sun.as_ref().unwrap();
    assert_eq!(
        sun.sunset, "17:38",
        "after the change: 18:38 EDT is 17:38 EST"
    );
    assert_eq!(sun.sunrise, "07:03", "before it");
}

/// The sample with 15-minute steps from now for `hours`, `mm` per step.
fn with_nowcast(hours: i64, mm: impl Fn(i64) -> f32) -> WeatherSnapshot {
    let mut snap = sample_snapshot();
    snap.minutely_15 = (0..hours * 4)
        .map(|i| crate::model::Minutely {
            time: crate::time::UnixTime(SAMPLE_NOW.0 + i * 900),
            precipitation: Some(crate::units::Mm(mm(i))),
        })
        .collect();
    snap
}

fn nowcast_for(snap: &WeatherSnapshot, hours: u8) -> NowcastVm {
    let p = PresentationConfig {
        nowcast_hours: hours,
        ..all_widgets()
    };
    view(Some(snap), &p, &ok_status()).nowcast.unwrap()
}

#[test]
fn nowcast_covers_the_chosen_hours() {
    let dry = with_nowcast(8, |_| 0.0);
    let one = nowcast_for(&dry, 1);
    assert_eq!(one.text, "No precipitation in the next hour.");
    assert_eq!((one.series_mm.len(), one.hours, one.wet), (4, 1, false));
    let ticks: Vec<&str> = one.ticks.iter().map(|t| t.label.as_str()).collect();
    assert_eq!(ticks, ["Now", "30 min", "1 h"]);
    let eight = nowcast_for(&dry, 8);
    assert_eq!(eight.text, "No precipitation in the next 8 hours.");
    let ticks: Vec<(usize, &str)> = eight
        .ticks
        .iter()
        .map(|t| (t.step, t.label.as_str()))
        .collect();
    assert_eq!(
        ticks,
        [
            (0, "Now"),
            (8, "2 h"),
            (16, "4 h"),
            (24, "6 h"),
            (32, "8 h")
        ]
    );
    assert_eq!(eight.levels, ["Light", "Moderate", "Heavy"]);

    // Rain from three hours on: a clock time, and only within the hours shown.
    let later = with_nowcast(8, |i| if i >= 12 { 1.0 } else { 0.0 });
    assert_eq!(nowcast_for(&later, 4).text, "Rain starting around 14:00.");
    assert_eq!(
        nowcast_for(&later, 2).text,
        "No precipitation in the next 2 hours."
    );
    let wet = nowcast_for(&later, 8);
    assert!(wet.wet);
    // 1 mm a quarter hour is 4 mm an hour: moderate, in the middle third.
    assert!((wet.intensity[12] - (1.0 + 1.5 / 5.1) / 3.0).abs() < 1e-4);

    // Raining now, stopping in half an hour.
    let stopping = with_nowcast(8, |i| if i < 2 { 0.5 } else { 0.0 });
    assert_eq!(nowcast_for(&stopping, 2).text, "Rain stopping in ~30 min.");
}

/// The sample with every hour clear but `wet` local hours of today light drizzle.
fn with_drizzle(wet: core::ops::Range<u8>) -> WeatherSnapshot {
    let mut snap = sample_snapshot();
    let today = SAMPLE_NOW.local_date(snap.location.utc_offset);
    let offset = snap.location.utc_offset;
    for h in &mut snap.hourly {
        let local = h.time.to_local(offset);
        h.condition = if local.date == today && wet.contains(&local.hour) {
            crate::condition::Condition::Drizzle(crate::condition::Intensity::Light)
        } else {
            crate::condition::Condition::Clear
        };
    }
    snap
}

#[test]
fn days_say_when_precipitation_falls() {
    let p = PresentationConfig {
        daily_hours: true,
        ..all_widgets()
    };
    let text = |wet| {
        let vm = view(Some(&with_drizzle(wet)), &p, &ok_status());
        vm.daily.unwrap().days[0].precip_text.clone()
    };
    assert_eq!(
        text(13..15).as_deref(),
        Some("Light drizzle in the early afternoon.")
    );
    assert_eq!(
        text(13..21).as_deref(),
        Some("Light drizzle from 13:00 to 21:00.")
    );
    assert_eq!(text(0..0), None, "dry");

    let vm = view(Some(&with_drizzle(13..15)), &p, &ok_status());
    let today = &vm.daily.as_ref().unwrap().days[0];
    assert!(today.hours.iter().any(|h| h.label == "Now"));
    assert!(
        today
            .timeline
            .iter()
            .any(|s| s.group == TimelineGroup::Drizzle)
    );
    let plain = view(Some(&with_drizzle(13..15)), &all_widgets(), &ok_status());
    assert!(
        plain.daily.unwrap().days[0].hours.is_empty(),
        "only with daily_hours"
    );
}

/// A night shower must stay visible: the day keeps its daylight icon,
/// but its text says when it falls and opening the day shows the hour.
#[test]
fn rain_between_3_and_4_am_shows_in_the_opened_day() {
    let mut snap = sample_snapshot();
    let offset = snap.location.utc_offset;
    let today = SAMPLE_NOW.local_date(offset);
    let tomorrow = snap
        .daily
        .iter()
        .map(|d| d.date)
        .find(|d| *d > today)
        .unwrap();
    for h in &mut snap.hourly {
        let local = h.time.to_local(offset);
        let wet = local.date == tomorrow && local.hour == 3;
        h.condition = if wet {
            crate::condition::Condition::Drizzle(crate::condition::Intensity::Light)
        } else {
            crate::condition::Condition::Clear
        };
        h.precipitation = Some(crate::units::Mm(if wet { 0.3 } else { 0.0 }));
        h.precip_probability = Some(crate::units::Percent(if wet { 60.0 } else { 0.0 }));
    }
    let p = PresentationConfig {
        daily_hours: true,
        ..all_widgets()
    };
    let vm = view(Some(&snap), &p, &ok_status());
    let day = &vm.daily.as_ref().unwrap().days[1];
    assert_eq!(day.condition_label, "Clear", "a dry daylight icon");
    let text = day.precip_text.as_deref().unwrap_or_default();
    assert!(text.contains("drizzle"), "{text:?}");
    let three = day
        .hours
        .iter()
        .find(|h| h.label == "03:00")
        .expect("the opened day shows 3 AM");
    assert_eq!(three.precip_probability, Some(60));
    assert!(three.precipitation.value.is_some_and(|mm| mm > 0.0));
    assert!(
        day.timeline
            .iter()
            .any(|s| s.group == TimelineGroup::Drizzle),
        "and a drizzle span in its timeline"
    );
}

#[test]
fn an_hour_of_minutes_draws_the_one_hour_graph() {
    // Dry quarter hours, but the minute nowcast has rain from minute 12.
    let mut snap = with_nowcast(8, |_| 0.0);
    snap.minutely_1 = (0..60)
        .map(|i| crate::model::Minutely {
            time: crate::time::UnixTime(SAMPLE_NOW.0 + i * 60),
            precipitation: Some(crate::units::Mm(if i >= 12 { 0.05 } else { 0.0 })),
        })
        .collect();
    let one = nowcast_for(&snap, 1);
    assert_eq!((one.step_minutes, one.series_mm.len()), (1, 60));
    assert_eq!(one.text, "Rain starting in ~10 min.");
    assert!(one.wet);
    let ticks: Vec<(usize, &str)> = one
        .ticks
        .iter()
        .map(|t| (t.step, t.label.as_str()))
        .collect();
    assert_eq!(ticks, [(0, "Now"), (30, "30 min"), (60, "1 h")]);
    // Longer graphs keep the quarter hours, and their sentence the minutes.
    let two = nowcast_for(&snap, 2);
    assert_eq!((two.step_minutes, two.series_mm.len()), (15, 8));
    assert_eq!(two.text, "Rain starting in ~10 min.");
}

#[test]
fn feels_like_can_lead_every_temperature() {
    let snap = sample_snapshot();
    let actual = view(Some(&snap), &all_widgets(), &ok_status());
    let p = PresentationConfig {
        temperature_basis: crate::presentation::TemperatureBasis::FeelsLike,
        ..all_widgets()
    };
    let feels = view(Some(&snap), &p, &ok_status());
    let (a, f) = (actual.current.unwrap(), feels.current.unwrap());
    assert_eq!(f.temperature.text, a.feels_like.text, "the big number");
    assert_eq!(f.feels_like_label, format!("Actual {}", a.temperature.text));
    assert!(a.feels_like_label.starts_with("Feels like"));
    let (ad, fd) = (
        &actual.daily.as_ref().unwrap().days[0],
        &feels.daily.as_ref().unwrap().days[0],
    );
    assert_ne!(
        fd.high.text, ad.high.text,
        "the sample feels a degree warmer"
    );
    assert_eq!(f.high.text, fd.high.text, "today's high matches its row");
    let hour = &feels.hourly.as_ref().unwrap().hours[1];
    assert_eq!(hour.temperature.text, hour.feels_like.text);
    // The air temperature stays beside the feels-like one, and the view says
    // which leads.
    let actual_hour = &actual.hourly.as_ref().unwrap().hours[1];
    assert_eq!(hour.actual.text, actual_hour.temperature.text);
    assert_eq!(
        (fd.actual_high.text.as_str(), fd.actual_low.text.as_str()),
        (ad.high.text.as_str(), ad.low.text.as_str())
    );
    assert!(feels.feels_like_leads && !actual.feels_like_leads);
    assert_eq!(feels.tabs[0].temp, f.temperature.text);
    assert!(
        feels
            .summary
            .unwrap()
            .contains(&format!("Feels-like high {}", fd.high.text)),
        "the summary says its highs and lows are feels-like"
    );
    assert!(
        actual
            .summary
            .unwrap()
            .contains(&format!("High {}", ad.high.text))
    );
}

fn view_with_history(
    snap: &WeatherSnapshot,
    history: Option<&crate::history::History>,
    p: &PresentationConfig,
) -> ViewModel {
    let strings = Strings::default();
    let status = ok_status();
    let locations = [LocationView {
        name: "New York",
        snapshot: Some(snap),
        status: &status,
        history,
    }];
    build_view(&ViewInput {
        locations: &locations,
        active: 0,
        presentation: p,
        strings: &strings,
        now: SAMPLE_NOW,
    })
}

fn with_history_widgets(units: UnitPrefs) -> PresentationConfig {
    PresentationConfig {
        units,
        widgets: vec![
            WidgetKind::Current,
            WidgetKind::Daily,
            WidgetKind::Details,
            WidgetKind::History,
        ],
        details: vec![DetailKind::Normals, DetailKind::Records],
        ..Default::default()
    }
}

#[test]
fn history_in_imperial() {
    let snap = sample_snapshot();
    let history = crate::testdata::history_10001();
    let vm = view_with_history(
        &snap,
        Some(&history),
        &with_history_widgets(UnitPrefs::IMPERIAL),
    );
    assert!(vm.widgets.contains(&WidgetKind::History));
    let h = vm.history.as_ref().unwrap();
    let today = h.today.as_ref().unwrap();
    assert_eq!(today.normal_text, "Normal 69° / 55°");
    assert_eq!(today.record_high.as_deref(), Some("94° in 1941"));
    assert_eq!(today.record_low.as_deref(), Some("35° in 1881"));
    assert_eq!(
        today.precip_chance.as_deref(),
        Some("Rain or snow on this date in 23% of years")
    );
    // Seven days, oldest first: 28 September to yesterday.
    let labels: Vec<(&str, &str)> = h
        .days
        .iter()
        .map(|d| (d.label.as_str(), d.date_short.as_str()))
        .collect();
    assert_eq!(labels.len(), 7);
    assert_eq!(labels[0].1, "Sep 28");
    assert_eq!(labels[6], ("Yesterday", "Oct 4"));
    let yesterday = &h.days[6];
    // 62 °F against the normal 69 °F.
    assert_eq!(yesterday.high.text, "62°");
    assert_eq!(yesterday.departure.text, "-7°");
    assert_eq!(yesterday.precipitation.text, "0.15 in");
    // 30 September has no report; 2 and 3 October only a trace.
    assert_eq!(h.days[2].high.text, "—");
    assert_eq!(h.days[2].departure.text, "—");
    assert_eq!(h.days[4].precipitation.text, "Trace");
    assert_eq!(h.days[5].precipitation.text, "Trace");
    // 3 October: 70 °F against 69 °F.
    assert_eq!(h.days[5].departure.text, "+1°");
    // Tapped, each day says more.
    assert_eq!(
        h.days[6].summary.as_deref(),
        Some("7° cooler than normal. 0.15 in of rain.")
    );
    // 56 °F against the normal low 55 °F.
    assert_eq!(h.days[6].low_departure.text, "+1°");
    assert_eq!(h.days[6].normal_precip.text, "0.15 in");
    // 2 October: 83 °F against 69 °F.
    assert_eq!(
        h.days[4].summary.as_deref(),
        Some("14° warmer than normal. A trace of rain.")
    );
    assert_eq!(h.days[2].summary, None, "no report");
    // Since the 1st only: 0 + trace + trace + 0.15, against 4 × 0.15.
    let mtd = h.month_to_date.as_ref().unwrap();
    assert_eq!(mtd.text, "0.15 in this month, normal 0.60 in");
    assert_eq!(
        h.credit,
        "NY City Central Park, 4.8 mi · records: New York-Central Park Area · Normals 1991–2020"
    );
    // Today's forecast high against yesterday's 62 °F.
    let high = snap.daily[0].temp_max.unwrap().0 * 9.0 / 5.0 + 32.0;
    let d = libm::roundf(high - 62.0);
    let expected = match d {
        d if d > 0.0 => format!("{d}° warmer than yesterday"),
        d if d < 0.0 => format!("{}° cooler than yesterday", -d),
        _ => "As warm as yesterday".to_string(),
    };
    assert_eq!(h.yesterday_delta.as_deref(), Some(expected.as_str()));
    // The daily list marks normals; the cards show today's.
    let daily = vm.daily.as_ref().unwrap();
    assert_eq!(daily.days[0].normal_high.text, "69°");
    assert_eq!(daily.days[0].normal_low.text, "55°");
    let cards: Vec<(&str, &str, Option<&str>)> = vm
        .details
        .iter()
        .map(|c| (c.title.as_str(), c.value.as_str(), c.caption.as_deref()))
        .collect();
    assert_eq!(
        cards,
        [
            ("Normal", "69° / 55°", None),
            ("Records", "94° · 35°", Some("1941 · 1881"))
        ]
    );
    assert!(
        vm.attribution.ends_with(" · NOAA RCC (ACIS)"),
        "{}",
        vm.attribution
    );
}

#[test]
fn history_in_metric() {
    let snap = sample_snapshot();
    let history = crate::testdata::history_10001();
    let vm = view_with_history(
        &snap,
        Some(&history),
        &with_history_widgets(UnitPrefs::METRIC),
    );
    let h = vm.history.as_ref().unwrap();
    // 69 °F and 55 °F are 20.6 °C and 12.8 °C.
    assert_eq!(h.today.as_ref().unwrap().normal_text, "Normal 21° / 13°");
    // 16.7 °C against 20.6 °C.
    assert_eq!(h.days[6].departure.text, "-4°");
    // 0.15 in.
    assert_eq!(h.days[6].precipitation.text, "3.8 mm");
    assert_eq!(
        h.month_to_date.as_ref().unwrap().text,
        "3.8 mm this month, normal 15.2 mm"
    );
    assert!(h.credit.starts_with("NY City Central Park, 7.8 km"));
}

#[test]
fn history_without_yesterday_or_data() {
    let snap = sample_snapshot();
    let mut history = crate::testdata::history_10001();
    let p = with_history_widgets(UnitPrefs::IMPERIAL);
    history.days.retain(|d| d.date.day != 4);
    let vm = view_with_history(&snap, Some(&history), &p);
    let h = vm.history.as_ref().unwrap();
    assert_eq!(h.yesterday_delta, None, "yesterday has no report");
    assert_eq!(h.days[6].high.text, "—");
    // No history: no widget, no normals, no cards, no credit.
    let none = view_with_history(&snap, None, &p);
    assert!(!none.widgets.contains(&WidgetKind::History));
    assert_eq!(none.history, None);
    assert_eq!(none.daily.unwrap().days[0].normal_high.text, "—");
    assert_eq!(none.details, Vec::new());
    assert!(!none.attribution.contains("ACIS"));
}

#[test]
fn history_speaks_german() {
    let snap = sample_snapshot();
    let history = crate::testdata::history_10001();
    let (strings, _) = Strings::localized("de", &alloc::collections::BTreeMap::default());
    let status = ok_status();
    let locations = [LocationView {
        name: "New York",
        snapshot: Some(&snap),
        status: &status,
        history: Some(&history),
    }];
    let vm = build_view(&ViewInput {
        locations: &locations,
        active: 0,
        presentation: &with_history_widgets(UnitPrefs::METRIC),
        strings: &strings,
        now: SAMPLE_NOW,
    });
    let h = vm.history.unwrap();
    assert_eq!(h.days[6].label, "Gestern");
    assert_eq!(h.days[4].precipitation.text, "Spuren");
    assert_eq!(h.today.unwrap().record_high.as_deref(), Some("34° (1941)"));
}

/// `whether history`, or a forecast that failed: the history still shows, on
/// the place's own day, without a forecast high to compare with yesterday.
#[test]
fn history_without_a_forecast() {
    let history = crate::testdata::history_10001();
    let status = ok_status();
    let locations = [LocationView {
        name: "New York",
        snapshot: None,
        status: &status,
        history: Some(&history),
    }];
    let p = with_history_widgets(UnitPrefs::IMPERIAL);
    let vm = build_view(&ViewInput {
        locations: &locations,
        active: 0,
        presentation: &p,
        strings: &Strings::default(),
        now: SAMPLE_NOW,
    });
    assert_eq!(vm.widgets, [WidgetKind::History]);
    assert_eq!(
        vm.attribution,
        "History from NOAA's Regional Climate Centers (ACIS)"
    );
    let h = vm.history.unwrap();
    assert_eq!(h.today.unwrap().normal_text, "Normal 69° / 55°");
    assert_eq!(h.days[6].label, "Yesterday");
    assert_eq!(h.yesterday_delta, None);
    // Not asked for, not shown.
    let off = PresentationConfig {
        widgets: vec![WidgetKind::Current],
        ..p
    };
    let vm = build_view(&ViewInput {
        locations: &locations,
        active: 0,
        presentation: &off,
        strings: &Strings::default(),
        now: SAMPLE_NOW,
    });
    assert_eq!(vm.history, None);
}

/// The vacation planner's view: 12–19 July 2027 at 10001, as in 1995–2024.
#[test]
fn a_trip_as_it_was_in_those_years() {
    let h = crate::testdata::climate_10001();
    let q = crate::testdata::july_query();
    let p = with_history_widgets(UnitPrefs::IMPERIAL);
    let vm = super::plan_vm(&p, &Strings::default(), &h, &q);
    assert_eq!(vm.title, "Jul 12 – Jul 19, as in 1995–2024");
    assert_eq!(vm.days.len(), 8);
    let first = &vm.days[0];
    assert_eq!(
        (first.label.as_str(), first.date_short.as_str()),
        ("Mon", "Jul 12")
    );
    assert_eq!(
        (first.high.text.as_str(), first.low.text.as_str()),
        ("84°", "69°")
    );
    // Wet in 10 of the 30 years.
    assert_eq!(first.precip_chance.text, "33%");
    assert_eq!(first.precipitation.text, "0.19 in");
    assert_eq!(first.hottest.as_deref(), Some("94° in 2010"));
    assert_eq!(first.coldest.as_deref(), Some("62° in 2002"));
    assert_eq!(
        (first.highest.value, first.lowest.value),
        (Some(94.0), Some(62.0))
    );
    // Wet in a third of the years, 84.3 °F (29.1 °C) on average: summer
    // storms; wet in a fifth (16 July): some cloud.
    assert_eq!(first.icon, crate::icon::IconId::ThunderstormsDay);
    assert_eq!(vm.days[4].icon, crate::icon::IconId::PartlyCloudyDay);
    // One scale, 61 °F (the coldest low on 13, 14 and 19 July) to 102 °F
    // (15 July's hottest): 12 July's 68.9 °F and 84.3 °F sit at 7.9/41 and
    // 23.3/41.
    assert!((first.range.0 - 0.193).abs() < 0.01 && (first.range.1 - 0.568).abs() < 0.01);
    assert!((vm.days[2].extremes.0).abs() < 1e-6 && (vm.days[3].extremes.1 - 1.0).abs() < 1e-6);
    // Highs average 85.8 °F, lows 70.6 °F; wet in 82 of the 8 × 30 date-years.
    assert_eq!(vm.average.as_deref(), Some("On average 86° / 71°"));
    assert_eq!(vm.wet_days.as_deref(), Some("Rain or snow on 34% of days"));
    assert_eq!(vm.credit, "NY City Central Park, 4.8 mi · 1995–2024");
    assert_eq!(
        vm.attribution,
        "History from NOAA's Regional Climate Centers (ACIS)"
    );
    // Metric and German.
    let (de, _) = Strings::localized("de", &alloc::collections::BTreeMap::default());
    let vm = super::plan_vm(&with_history_widgets(UnitPrefs::METRIC), &de, &h, &q);
    assert_eq!(vm.title, "Juli 12 – Juli 19, wie in 1995–2024");
    // 84.3 °F is 29.1 °C.
    assert_eq!(vm.days[0].high.text, "29°");
    assert_eq!(vm.days[0].label, "Mo");
}

#[test]
fn a_trip_without_answers() {
    let mut h = crate::testdata::climate_10001();
    h.climate.clear();
    h.station = None;
    let q = crate::testdata::july_query();
    let vm = super::plan_vm(
        &with_history_widgets(UnitPrefs::METRIC),
        &Strings::default(),
        &h,
        &q,
    );
    assert_eq!(vm.days.len(), 8, "the dates still show");
    assert_eq!(vm.days[0].high.text, "—");
    assert_eq!(vm.days[0].range, (-1.0, -1.0));
    assert_eq!(vm.days[0].icon, crate::icon::IconId::NotAvailable);
    assert_eq!((vm.average, vm.wet_days), (None, None));
    assert_eq!(vm.credit, "1995–2024");
}

/// A winter trip: snow where it is cold enough, rain where it is not, sun where
/// it is mostly dry.
#[test]
fn a_trips_typical_weather() {
    use crate::icon::IconId;
    let mut h = crate::testdata::climate_10001();
    let q = crate::testdata::july_query();
    let set = |h: &mut crate::history::History, i: usize, chance: f32, high: f32, low: f32| {
        let c = &mut h.climate[i];
        c.precip_chance = Some(crate::units::Percent(chance));
        c.normal_max = Some(crate::units::Celsius(high));
        c.normal_min = Some(crate::units::Celsius(low));
    };
    set(&mut h, 0, 60.0, 1.0, -6.0);
    set(&mut h, 1, 40.0, 0.0, -8.0);
    set(&mut h, 2, 60.0, 12.0, 4.0);
    set(&mut h, 3, 40.0, 12.0, 4.0);
    set(&mut h, 4, 10.0, 12.0, 4.0);
    let vm = super::plan_vm(
        &with_history_widgets(UnitPrefs::METRIC),
        &Strings::default(),
        &h,
        &q,
    );
    let icons: Vec<IconId> = vm.days[..5].iter().map(|d| d.icon).collect();
    assert_eq!(
        icons,
        [
            IconId::Snow,
            IconId::SnowShowersDay,
            IconId::Rain,
            IconId::ShowersDay,
            IconId::ClearDay
        ]
    );
}

/// The sample with tomorrow's hours set by `hour` (local hour → condition,
/// temperature °C, gust m/s, humidity %).
fn tomorrow_as(hour: impl Fn(u8) -> (Condition, f32, f32, f32)) -> WeatherSnapshot {
    let mut snap = sample_snapshot();
    let offset = snap.location.utc_offset;
    let tomorrow = snap.daily[1].date;
    for h in &mut snap.hourly {
        let local = h.time.to_local(offset);
        if local.date != tomorrow {
            continue;
        }
        let (condition, t, gust, rh) = hour(local.hour);
        h.condition = condition;
        h.temperature = Some(crate::units::Celsius(t));
        h.apparent = None;
        h.wind_gust = Some(crate::units::Mps(gust));
        h.wind_speed = Some(crate::units::Mps(gust / 2.0));
        h.humidity = Some(crate::units::Percent(rh));
        h.precipitation = Some(crate::units::Mm(0.0));
    }
    snap
}

fn tomorrow_outlook(snap: &WeatherSnapshot, units: UnitPrefs) -> String {
    let p = PresentationConfig {
        units,
        ..all_widgets()
    };
    let vm = view(Some(snap), &p, &ok_status());
    vm.daily.unwrap().days[1].outlook.clone().unwrap()
}

#[test]
fn days_are_described_like_a_forecaster_would() {
    // A cold start, then a warm sunny afternoon.
    let snap = tomorrow_as(|h| (Condition::Clear, if h < 12 { 2.0 } else { 24.0 }, 4.0, 40.0));
    assert_eq!(
        tomorrow_outlook(&snap, UnitPrefs::IMPERIAL),
        "Cold in the morning, then sunny and warm in the afternoon."
    );
    // Mostly clear with high winds.
    let snap = tomorrow_as(|h| {
        let c = if h % 2 == 0 {
            Condition::PartlyCloudy
        } else {
            Condition::Clear
        };
        (c, 18.0, 20.0, 40.0)
    });
    assert_eq!(
        tomorrow_outlook(&snap, UnitPrefs::IMPERIAL),
        "Mostly sunny and mild. Windy, with gusts to 45 mph."
    );
    // Clouds clearing; muggy and hot with an afternoon storm.
    let snap = tomorrow_as(|h| match h {
        0..12 => (Condition::Overcast, 26.0, 5.0, 85.0),
        15 | 16 => (
            Condition::Thunderstorm(crate::condition::Intensity::Moderate),
            31.0,
            9.0,
            70.0,
        ),
        _ => (Condition::Clear, 31.0, 5.0, 70.0),
    });
    let text = tomorrow_outlook(&snap, UnitPrefs::IMPERIAL);
    assert!(
        text.starts_with(
            "Cloudy in the morning, then mostly sunny and hot in the afternoon. Thunderstorm"
        ),
        "{text}"
    );
    assert!(text.ends_with("Humid."), "{text}");
    // A cold front: mild at midnight, cold all afternoon.
    let snap = tomorrow_as(|h| {
        (
            Condition::Overcast,
            if h < 7 { 14.0 } else { 5.0 },
            4.0,
            60.0,
        )
    });
    assert_eq!(
        tomorrow_outlook(&snap, UnitPrefs::IMPERIAL),
        "Cloudy and cold. Temperatures falling through the day."
    );
    // Fog that burns off, in German.
    let snap = tomorrow_as(|h| {
        let c = if h < 11 {
            Condition::Fog
        } else {
            Condition::Clear
        };
        (c, 12.0, 3.0, 60.0)
    });
    let (de, _) = Strings::localized("de", &alloc::collections::BTreeMap::default());
    let p = PresentationConfig {
        units: UnitPrefs::METRIC,
        ..all_widgets()
    };
    let locations = [LocationView {
        name: "New York",
        snapshot: Some(&snap),
        status: &ok_status(),
        history: None,
    }];
    let vm = build_view(&ViewInput {
        locations: &locations,
        active: 0,
        presentation: &p,
        strings: &de,
        now: SAMPLE_NOW,
    });
    assert_eq!(
        vm.daily.unwrap().days[1].outlook.as_deref(),
        Some("Morgens neblig, nachmittags sonnig und kühl.")
    );
}

#[test]
fn days_beyond_the_hours_are_described_from_their_values() {
    let mut snap = sample_snapshot();
    let mut day = snap.daily[2].clone();
    day.condition = Condition::RainShowers(crate::condition::Intensity::Moderate);
    day.precip_probability_max = Some(crate::units::Percent(40.0));
    day.temp_max = Some(crate::units::Celsius(9.0));
    day.apparent_max = None;
    day.wind_gust_max = Some(crate::units::Mps(5.0));
    day.wind_speed_max = Some(crate::units::Mps(3.0));
    snap.daily[2] = day;
    let p = PresentationConfig {
        units: UnitPrefs::IMPERIAL,
        ..all_widgets()
    };
    let vm = view(Some(&snap), &p, &ok_status());
    assert_eq!(
        vm.daily.unwrap().days[2].outlook.as_deref(),
        Some("Cloudy and cool. Showers possible.")
    );
}

/// A day that set a record says so: 2 October's 83 °F, made the date's record.
#[test]
fn a_past_day_that_set_a_record() {
    let snap = sample_snapshot();
    let mut history = crate::testdata::history_10001();
    let oct_2 = crate::time::LocalDate {
        year: 2026,
        month: 10,
        day: 2,
    };
    let c = history
        .climate
        .iter_mut()
        .find(|c| c.date == oct_2)
        .unwrap();
    c.record_max = Some(crate::history::Record {
        value: (83.0 - 32.0) * 5.0 / 9.0,
        year: 2026,
    });
    let vm = view_with_history(
        &snap,
        Some(&history),
        &with_history_widgets(UnitPrefs::IMPERIAL),
    );
    let day = &vm.history.unwrap().days[4];
    assert_eq!(day.date_short, "Oct 2");
    assert_eq!(
        day.summary.as_deref(),
        Some("14° warmer than normal. A trace of rain. A record high for the date.")
    );
    assert_eq!(day.record_high.as_deref(), Some("83° in 2026"));
}
