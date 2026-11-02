//! `ViewModel` → Slint structs. Plain data (no Slint models), so it can be built off
//! the UI thread; `apply` (desktop) turns it into Slint models on the UI thread.
//! Shared with the MCU build (`no_std` + alloc).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use slint::SharedString;
use whether_core::icon::UiIcon;
use whether_core::presentation::DetailKind;
use whether_core::view::ViewModel;

use crate::{
    AirUi, AlertUi, CurrentUi, DayUi, DetailUi, HistoryDayUi, HistoryUi, HourUi, MarkUi,
    PollutantUi, PrecipUi, SpanUi, StatusUi, SunUi, TabUi, TickUi,
};

/// Everything a window shows, as Slint structs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiModel {
    /// Location tabs.
    pub tabs: Vec<TabUi>,
    /// The shown tab.
    pub active: usize,
    /// Fetch status.
    pub status: StatusUi,
    /// Current conditions (empty `temp` when there is no data).
    pub current: CurrentUi,
    /// Whether there is data to show.
    pub has_data: bool,
    /// Hourly forecast from the current hour.
    pub hours: Vec<HourUi>,
    /// Daily forecast.
    pub days: Vec<DayUi>,
    /// Detail cards.
    pub details: Vec<DetailUi>,
    /// Air quality.
    pub air: AirUi,
    /// Pollutants.
    pub pollutants: Vec<PollutantUi>,
    /// Sun and moon.
    pub sun: SunUi,
    /// Active alerts, as the view orders them (empty unless the alerts widget is on).
    pub alerts: Vec<AlertUi>,
    /// `SkyTheme` index (the order of `whether_core::theme::SkyTheme`).
    pub sky: i32,
    /// The required data credit.
    pub attribution: SharedString,
    /// Wind speeds as fractions of the period's maximum (for the chart).
    pub wind_fracs: Vec<f32>,
    /// Radar frame times, oldest first (empty without radar); the pixels travel
    /// separately.
    pub radar_labels: Vec<SharedString>,
    /// The radar credit.
    pub radar_attribution: SharedString,
    /// The precipitation table's hours (empty without the widget).
    pub precip: Vec<PrecipUi>,
    /// "Total 0.12 in · highest chance 70%" parts: the total and the chance.
    pub precip_total: SharedString,
    pub precip_max_chance: SharedString,
    /// "No precipitation in the next 12 hours." when every hour is dry.
    pub precip_dry: SharedString,
    /// Every day's stretches of weather and every third hour, tagged with the
    /// day (empty without `daily_hours`).
    pub day_spans: Vec<SpanUi>,
    pub day_marks: Vec<MarkUi>,
    /// The next hours' sentence (empty without the nowcast widget), whether it is
    /// wet, each step's intensity (0..1), the axis and the scale's names.
    pub nowcast_text: SharedString,
    pub nowcast_wet: bool,
    pub nowcast_bars: Vec<f32>,
    pub nowcast_ticks: Vec<TickUi>,
    pub nowcast_levels: Vec<SharedString>,
    /// The history card's lines (`present` false without history).
    pub history: HistoryUi,
    /// The history card's days, oldest first.
    pub history_days: Vec<HistoryDayUi>,
}

/// Every day's stretches (as fractions of its hours) and every third hour, each
/// tagged with its day.
fn day_hours(vm: &ViewModel) -> (Vec<SpanUi>, Vec<MarkUi>) {
    let (mut spans, mut marks) = (Vec::new(), Vec::new());
    let days = vm.daily.as_ref().map_or(&[][..], |d| &d.days[..]);
    for (i, day) in days.iter().enumerate() {
        let i = i32::try_from(i).unwrap_or(i32::MAX);
        #[allow(clippy::cast_precision_loss)] // at most 24 hours
        let count = day.hours.len().max(1) as f32;
        #[allow(clippy::cast_precision_loss)]
        spans.extend(day.timeline.iter().map(|span| SpanUi {
            day: i,
            start: span.start as f32 / count,
            len: span.len as f32 / count,
            group: span.group as i32,
            label: s(&span.label),
        }));
        #[allow(clippy::cast_precision_loss)]
        marks.extend(
            day.hours
                .iter()
                .enumerate()
                .step_by(3)
                .map(|(k, h)| MarkUi {
                    day: i,
                    pos: k as f32 / count,
                    label: s(&h.label),
                    icon: index(h.icon.index()),
                    temp: s(&h.temperature.text),
                }),
        );
    }
    (spans, marks)
}

/// The nowcast's axis labels, at fractions of its length.
fn nowcast_ticks(vm: &ViewModel) -> Vec<TickUi> {
    vm.nowcast.as_ref().map_or_else(Vec::new, |n| {
        #[allow(clippy::cast_precision_loss)] // at most 60 steps
        let steps = n.intensity.len().max(1) as f32;
        #[allow(clippy::cast_precision_loss)]
        n.ticks
            .iter()
            .map(|t| TickUi {
                pos: t.step as f32 / steps,
                label: s(&t.label),
            })
            .collect()
    })
}

fn s(text: &str) -> SharedString {
    SharedString::from(text)
}

fn percent(p: Option<u8>) -> SharedString {
    p.map_or_else(SharedString::new, |p| {
        SharedString::from(format!("{p}%").as_str())
    })
}

/// The interface icon for a detail card.
fn detail_icon(kind: DetailKind) -> UiIcon {
    match kind {
        DetailKind::Wind | DetailKind::Gusts => UiIcon::Wind,
        DetailKind::Humidity => UiIcon::Humidity,
        DetailKind::DewPoint
        | DetailKind::FeelsLike
        | DetailKind::Normals
        | DetailKind::Records => UiIcon::Thermometer,
        DetailKind::Pressure => UiIcon::Barometer,
        DetailKind::Uv => UiIcon::UvIndex,
        DetailKind::Visibility => UiIcon::Visibility,
        DetailKind::CloudCover => UiIcon::Cloud,
        DetailKind::Precipitation => UiIcon::Raindrop,
    }
}

fn index<T: Copy + Into<usize>>(v: T) -> i32 {
    i32::try_from(v.into()).unwrap_or(-1)
}

/// `(v - lo) / (hi - lo)` clamped to 0..1 (0.5 for an empty range).
fn span(lo: f32, hi: f32, v: f32) -> f32 {
    if hi > lo {
        ((v - lo) / (hi - lo)).clamp(0.0, 1.0)
    } else {
        0.5
    }
}

/// Hours from the current one, and wind speeds as fractions of their maximum.
fn hours(vm: &ViewModel) -> (Vec<HourUi>, Vec<f32>) {
    vm.hourly.as_ref().map_or_else(
        || (Vec::new(), Vec::new()),
        |h| {
            let shown: Vec<_> = h.hours.iter().skip(h.now_index).collect();
            let max_wind = shown
                .iter()
                .filter_map(|x| x.wind.speed.value)
                .fold(0.0_f32, f32::max);
            let hours = shown
                .iter()
                .map(|x| HourUi {
                    label: s(&x.label),
                    icon: index(x.icon.index()),
                    temp: s(&x.temperature.text),
                    temp_frac: x
                        .temperature
                        .value
                        .map_or(0.0, |t| span(h.temp_min, h.temp_max, t)),
                    pop: percent(x.precip_probability),
                    pop_frac: f32::from(x.precip_probability.unwrap_or(0)) / 100.0,
                    wind: s(&x.wind.speed.text),
                    wind_deg: x.wind.direction_deg.unwrap_or(-1.0),
                    is_day: x.is_day,
                })
                .collect();
            let fracs = shown
                .iter()
                .map(|x| x.wind.speed.value.map_or(0.0, |v| span(0.0, max_wind, v)))
                .collect();
            (hours, fracs)
        },
    )
}

/// A normal's place on a range bar from `lo` to `hi` (display units), or -1.
fn normal_at(q: &whether_core::view::Quantity, lo: f32, hi: f32) -> f32 {
    q.value.map_or(-1.0, |v| span(lo, hi, v))
}

fn days(vm: &ViewModel) -> Vec<DayUi> {
    vm.daily.as_ref().map_or_else(Vec::new, |d| {
        let (lo, hi) = (d.period_min, d.period_max);
        d.days
            .iter()
            .map(|day| DayUi {
                normal_lo: normal_at(&day.normal_low, lo, hi),
                normal_hi: normal_at(&day.normal_high, lo, hi),
                label: s(&day.label),
                icon: index(day.icon.index()),
                condition: s(&day.condition_label),
                pop: percent(day.precip_probability),
                low: s(&day.low.text),
                high: s(&day.high.text),
                high_level: day.temp_level_high as i32,
                range_lo: day.range.0,
                range_hi: day.range.1,
                precip_text: day.precip_text.as_deref().map_or_else(SharedString::new, s),
                has_hours: !day.hours.is_empty(),
            })
            .collect()
    })
}

/// The history card: its lines, and its days on one scale across their
/// highs, lows and normals.
fn history(vm: &ViewModel) -> (HistoryUi, Vec<HistoryDayUi>) {
    let Some(h) = &vm.history else {
        return (HistoryUi::default(), Vec::new());
    };
    let today = h.today.as_ref();
    let ui = HistoryUi {
        present: true,
        normal: today.map_or_else(SharedString::new, |c| s(&c.normal_text)),
        records: today
            .and_then(|c| match (&c.record_high, &c.record_low) {
                (Some(high), Some(low)) => Some(format!("{high} · {low}")),
                (Some(one), None) | (None, Some(one)) => Some(one.clone()),
                (None, None) => None,
            })
            .map_or_else(SharedString::new, |r| s(&r)),
        chance: today
            .and_then(|c| c.precip_chance.as_deref())
            .map_or_else(SharedString::new, s),
        delta: h
            .yesterday_delta
            .as_deref()
            .map_or_else(SharedString::new, s),
        month: h
            .month_to_date
            .as_ref()
            .map_or_else(SharedString::new, |m| s(&m.text)),
        credit: s(&h.credit),
    };
    let values = h.days.iter().flat_map(|d| {
        [&d.high, &d.low, &d.normal_high, &d.normal_low]
            .into_iter()
            .filter_map(|q| q.value)
    });
    let (lo, hi) = values.fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), v| {
        (lo.min(v), hi.max(v))
    });
    let days = h
        .days
        .iter()
        .map(|d| HistoryDayUi {
            label: s(&d.label),
            date: s(&d.date_short),
            high: s(&d.high.text),
            low: s(&d.low.text),
            departure: s(&d.departure.text),
            warmer: d.departure.value.map_or(0, |v| {
                if v > 0.0 {
                    1
                } else if v < 0.0 {
                    -1
                } else {
                    0
                }
            }),
            precip: s(&d.precipitation.text),
            // A day without both ends draws no bar (-1).
            range_lo: d
                .low
                .value
                .filter(|_| d.high.value.is_some())
                .map_or(-1.0, |v| span(lo, hi, v)),
            range_hi: d
                .high
                .value
                .filter(|_| d.low.value.is_some())
                .map_or(-1.0, |v| span(lo, hi, v)),
            normal_lo: normal_at(&d.normal_low, lo, hi),
            normal_hi: normal_at(&d.normal_high, lo, hi),
        })
        .collect();
    (ui, days)
}

fn details(vm: &ViewModel) -> Vec<DetailUi> {
    vm.details
        .iter()
        .map(|d| DetailUi {
            kind: d.kind as i32,
            title: s(&d.title),
            value: s(&d.value),
            caption: d.caption.as_deref().map_or_else(SharedString::new, s),
            level: d.level.map_or(-1, |l| l as i32),
            gauge: d.gauge.unwrap_or(-1.0),
            direction: d.direction_deg.unwrap_or(-1.0),
            icon: index(detail_icon(d.kind).index()),
        })
        .collect()
}

fn air(vm: &ViewModel) -> (AirUi, Vec<PollutantUi>) {
    vm.air_quality.as_ref().map_or_else(
        || (AirUi::default(), Vec::new()),
        |a| {
            (
                AirUi {
                    present: true,
                    index: SharedString::from(format!("{}", a.index).as_str()),
                    category: s(&a.category_label),
                    level: a.level as i32,
                    gauge: a.gauge,
                },
                a.pollutants
                    .iter()
                    .map(|(name, value)| PollutantUi {
                        name: s(name),
                        value: s(value),
                    })
                    .collect(),
            )
        },
    )
}

fn sun(vm: &ViewModel) -> SunUi {
    let mut sun = vm.sun.as_ref().map_or_else(SunUi::default, |x| SunUi {
        present: true,
        sunrise: s(&x.sunrise),
        sunset: s(&x.sunset),
        daylight: s(&x.daylight),
        progress: x.progress.unwrap_or(0.0),
        is_day: x.is_day,
        moon_phase: SharedString::new(),
        moon_icon: -1,
    });
    if let Some(moon) = &vm.moon {
        sun.moon_phase = s(&moon.phase_label);
        sun.moon_icon = index(moon.icon.index());
    }
    sun
}

fn alerts(vm: &ViewModel) -> Vec<AlertUi> {
    vm.alerts
        .iter()
        .map(|a| AlertUi {
            title: s(&a.title),
            body: s(&a.body),
            period: s(&a.period),
            level: a.level as i32,
        })
        .collect()
}

fn current(vm: &ViewModel) -> CurrentUi {
    vm.current
        .as_ref()
        .map_or_else(CurrentUi::default, |c| CurrentUi {
            temp: s(&c.temperature.text),
            temp_level: c.temp_level as i32,
            condition: s(&c.condition_label),
            icon: index(c.icon.index()),
            feels: s(&c.feels_like_label),
            high: s(&c.high.text),
            low: s(&c.low.text),
            summary: vm.summary.as_deref().map_or_else(SharedString::new, s),
        })
}

/// Converts a view model. Deterministic: the same input gives the same output.
pub fn to_ui(vm: &ViewModel) -> UiModel {
    let (hours, wind_fracs) = hours(vm);
    let (air, pollutants) = air(vm);
    let current = current(vm);
    let days = days(vm);
    let details = details(vm);
    let sun = sun(vm);
    UiModel {
        tabs: vm
            .tabs
            .iter()
            .map(|t| TabUi {
                name: s(&t.name),
                temp: s(&t.temp),
                icon: index(t.icon.index()),
            })
            .collect(),
        active: vm.active,
        status: StatusUi {
            updated: s(&vm.status.updated),
            provider: s(&vm.status.provider),
            stale: vm.status.stale,
            refreshing: vm.status.refreshing,
            error: vm.status.error.as_deref().map_or_else(SharedString::new, s),
            observed: vm
                .status
                .observed
                .as_deref()
                .map_or_else(SharedString::new, s),
        },
        has_data: vm.current.is_some(),
        current,
        hours,
        days,
        details,
        air,
        pollutants,
        sun,
        alerts: alerts(vm),
        sky: vm.theme as i32,
        attribution: s(&vm.attribution),
        wind_fracs,
        radar_labels: vm
            .radar
            .as_ref()
            .map_or_else(Vec::new, |r| r.frames.iter().map(|f| s(f)).collect()),
        radar_attribution: vm
            .radar
            .as_ref()
            .map_or_else(SharedString::new, |r| s(&r.attribution)),
        precip: vm.precipitation.as_ref().map_or_else(Vec::new, |p| {
            p.hours
                .iter()
                .map(|h| PrecipUi {
                    label: s(&h.label),
                    chance: h.chance.as_deref().map_or_else(SharedString::new, s),
                    amount: s(&h.amount.text),
                    intensity: h.intensity,
                    frozen: h.frozen,
                })
                .collect()
        }),
        precip_total: vm
            .precipitation
            .as_ref()
            .map_or_else(SharedString::new, |p| s(&p.total.text)),
        precip_max_chance: vm
            .precipitation
            .as_ref()
            .and_then(|p| p.max_chance.as_deref())
            .map_or_else(SharedString::new, s),
        precip_dry: vm
            .precipitation
            .as_ref()
            .and_then(|p| p.dry.as_deref())
            .map_or_else(SharedString::new, s),
        day_spans: day_hours(vm).0,
        day_marks: day_hours(vm).1,
        nowcast_text: vm
            .nowcast
            .as_ref()
            .map_or_else(SharedString::new, |n| s(&n.text)),
        nowcast_wet: vm.nowcast.as_ref().is_some_and(|n| n.wet),
        nowcast_bars: vm
            .nowcast
            .as_ref()
            .map_or_else(Vec::new, |n| n.intensity.clone()),
        nowcast_ticks: nowcast_ticks(vm),
        history: history(vm).0,
        history_days: history(vm).1,
        nowcast_levels: vm
            .nowcast
            .as_ref()
            .map_or_else(Vec::new, |n| n.levels.iter().map(|l| s(l)).collect()),
    }
}

/// The window title, `"Whether — New York"`.
pub fn title(m: &UiModel) -> String {
    m.tabs.get(m.active).map_or_else(
        || String::from("Whether"),
        |t| format!("Whether — {}", t.name),
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use whether_core::presentation::PresentationConfig;
    use whether_core::strings::Strings;
    use whether_core::testdata;
    use whether_core::view::{LocationView, StatusInfo, ViewInput, build_view};

    pub(crate) fn sample_vm() -> ViewModel {
        let snapshot = testdata::sample_snapshot();
        let status = StatusInfo {
            last_success: Some(testdata::SAMPLE_NOW),
            error: None,
            stale: false,
            refreshing: false,
        };
        let presentation = PresentationConfig::default();
        build_view(&ViewInput {
            locations: &[LocationView {
                name: "New York",
                snapshot: Some(&snapshot),
                status: &status,
                history: None,
            }],
            active: 0,
            presentation: &presentation,
            strings: &Strings::default(),
            now: testdata::SAMPLE_NOW,
        })
    }

    #[test]
    fn conversion_is_deterministic_and_complete() {
        let vm = sample_vm();
        let a = to_ui(&vm);
        assert_eq!(a, to_ui(&vm));
        assert!(a.has_data && !a.hours.is_empty() && !a.days.is_empty() && !a.details.is_empty());
        assert_eq!(a.hours.len(), a.wind_fracs.len());
        assert!(
            a.hours
                .iter()
                .all(|h| (0.0..=1.0).contains(&h.temp_frac) && (0.0..=1.0).contains(&h.pop_frac))
        );
        assert!(a.air.present && a.sun.present);
        assert_eq!(title(&a), "Whether — New York");
    }

    /// Entries of the `name` image array in an icons file.
    fn array_len(file: &str, name: &str) -> usize {
        let start = file.find(&alloc::format!("<[image]> {name}: [")).unwrap();
        let end = start + file[start..].find("];").unwrap();
        file[start..end].matches("@image-url(").count()
    }

    #[test]
    fn icon_arrays_match_the_enums() {
        for file in [
            include_str!("../ui/icons-svg/icons.slint"),
            include_str!("../ui/icons-png/icons.slint"),
        ] {
            assert_eq!(
                array_len(file, "condition"),
                whether_core::icon::IconId::ALL.len()
            );
            assert_eq!(array_len(file, "ui"), UiIcon::ALL.len());
        }
    }

    #[test]
    fn alerts_keep_their_text_and_level() {
        let m = to_ui(&testdata::sample_view_with_alerts());
        let shown: Vec<_> = m
            .alerts
            .iter()
            .map(|a| (a.title.as_str(), a.period.as_str(), a.level))
            .collect();
        assert_eq!(
            shown,
            [
                ("Flood Warning", "Mon 09:00 – Tue 06:00", 4),
                ("Wind Advisory", "Mon 14:00 – Tue 00:00", 2),
            ]
        );
        assert!(m.alerts[0].body.starts_with("The East Branch"));
        assert_eq!(to_ui(&sample_vm()).alerts, []);
    }

    /// History adds normals to the days without changing them: a drizzle between
    /// 3 and 4 AM tomorrow still shows when the day is opened.
    #[test]
    fn history_keeps_night_rain_in_the_opened_day() {
        use whether_core::condition::{Condition, Intensity};
        use whether_core::units::{Mm, Percent};
        use whether_core::view::TimelineGroup;

        let mut snapshot = testdata::sample_snapshot();
        let offset = snapshot.location.utc_offset;
        let tomorrow = snapshot.daily[1].date;
        for h in &mut snapshot.hourly {
            let local = h.time.to_local(offset);
            let wet = local.date == tomorrow && local.hour == 3;
            h.condition = if wet {
                Condition::Drizzle(Intensity::Light)
            } else {
                Condition::Clear
            };
            h.precipitation = Some(Mm(if wet { 0.3 } else { 0.0 }));
            h.precip_probability = Some(Percent(if wet { 60.0 } else { 0.0 }));
        }
        let history = testdata::history_10001();
        let status = StatusInfo::default();
        let mut presentation = PresentationConfig {
            daily_hours: true,
            ..PresentationConfig::default()
        };
        presentation
            .widgets
            .push(whether_core::presentation::WidgetKind::History);
        let m = to_ui(&build_view(&ViewInput {
            locations: &[LocationView {
                name: "New York",
                snapshot: Some(&snapshot),
                status: &status,
                history: Some(&history),
            }],
            active: 0,
            presentation: &presentation,
            strings: &Strings::default(),
            now: testdata::SAMPLE_NOW,
        }));
        let day = &m.days[1];
        assert!(day.has_hours);
        assert!(day.precip_text.contains("drizzle"), "{:?}", day.precip_text);
        assert!(
            m.day_spans.iter().any(|s| s.day == 1
                && s.group == TimelineGroup::Drizzle as i32
                && (s.start - 3.0 / 24.0).abs() < 0.01),
            "a drizzle span at 3 AM"
        );
        assert!(m.day_marks.iter().any(|k| k.day == 1 && k.label == "03:00"));
        // The normals sit on the days' scale; a day without one has none.
        assert!(
            m.days
                .iter()
                .all(|d| d.normal_hi < 0.0 || (0.0..=1.0).contains(&d.normal_hi))
        );
        assert!(m.days.iter().any(|d| d.normal_hi >= 0.0));
        assert!(m.history.present);
        let missing = m.history_days.iter().find(|d| d.date == "Sep 30").unwrap();
        assert!(missing.range_lo < 0.0, "a missing day draws no bar");
    }

    #[test]
    fn no_data_is_empty() {
        let vm = build_view(&ViewInput {
            locations: &[],
            active: 0,
            presentation: &PresentationConfig::default(),
            strings: &Strings::default(),
            now: testdata::SAMPLE_NOW,
        });
        let m = to_ui(&vm);
        assert!(!m.has_data && m.hours.is_empty() && !m.air.present);
    }
}
