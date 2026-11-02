//! The history widget and the normals and records cards: observed
//! past days against normal, a date's climate, the month so far, and today
//! against yesterday. Formatting stays here, in core.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::history::{DayClimate, History, HistoryKind, PastDay, Record, period_text};
use crate::strings::{Text, weekday_text};
use crate::time::{LocalDate, civil_from_days, weekday_from_days};
use crate::units::{Celsius, Meters, Mm};
use crate::view::format::{Formatter, number};
use crate::view::{ClimateVm, HistoryVm, MonthToDateVm, PastDayVm, PlanDayVm, PlanVm, Quantity};

/// The date `days` after `date` (negative: before).
fn shifted(date: LocalDate, days: i64) -> LocalDate {
    let (year, month, day) = civil_from_days(date.days_since_epoch() + days);
    LocalDate { year, month, day }
}

/// A temperature difference in display units: "+5°", "-3°", "0°".
fn delta(f: &Formatter<'_>, a: Option<Celsius>, b: Option<Celsius>) -> Quantity {
    let (Some(a), Some(b)) = (f.temperature(a).value, f.temperature(b).value) else {
        return f.temperature(None);
    };
    let decimals = f.presentation.temperature_decimals;
    let scale = libm::powf(10.0, f32::from(decimals));
    let d = libm::roundf((a - b) * scale) / scale;
    let unit = f.temperature(None).unit;
    let sign = if d > 0.0 {
        "+"
    } else if d < 0.0 {
        "-"
    } else {
        ""
    };
    Quantity {
        value: Some(d),
        text: format!("{sign}{}{}", number(d.abs(), decimals), degree(unit)),
        unit,
    }
}

/// The sign after a temperature: "°", or " K".
fn degree(unit: &str) -> &'static str {
    if unit == "K" { " K" } else { "°" }
}

/// "94° in 1941".
fn record(f: &Formatter<'_>, r: Option<Record>) -> Option<String> {
    let r = r?;
    Some(f.strings.fmt(
        Text::RecordIn,
        &[
            ("value", &f.temperature(Some(Celsius(r.value))).text),
            ("year", &r.year.to_string()),
        ],
    ))
}

/// A date's climate as shown: normals, records and the chance of precipitation.
pub(super) fn climate(f: &Formatter<'_>, c: &DayClimate) -> ClimateVm {
    let (high, low) = (f.temperature(c.normal_max), f.temperature(c.normal_min));
    ClimateVm {
        normal_text: f.strings.fmt(
            Text::NormalHighLow,
            &[("high", &high.text), ("low", &low.text)],
        ),
        normal_high: high,
        normal_low: low,
        record_high: record(f, c.record_max),
        record_low: record(f, c.record_min),
        precip_chance: c.precip_chance.map(|p| {
            f.strings.fmt(
                Text::PrecipChance,
                &[("n", &number(p.0.clamp(0.0, 100.0), 0))],
            )
        }),
    }
}

/// The normals card's value: "69° / 55°".
pub(super) fn normals_value(f: &Formatter<'_>, c: &DayClimate) -> Option<String> {
    (c.normal_max.is_some() || c.normal_min.is_some()).then(|| {
        format!(
            "{} / {}",
            f.temperature(c.normal_max).text,
            f.temperature(c.normal_min).text
        )
    })
}

/// The records card's value and caption: "94° · 35°" and "1941 · 1881".
pub(super) fn records_value(f: &Formatter<'_>, c: &DayClimate) -> Option<(String, String)> {
    if c.record_max.is_none() && c.record_min.is_none() {
        return None;
    }
    let value = |r: Option<Record>| f.temperature(r.map(|r| Celsius(r.value))).text;
    let year = |r: Option<Record>| {
        r.map_or_else(|| f.presentation.missing.clone(), |r| r.year.to_string())
    };
    Some((
        format!("{} · {}", value(c.record_max), value(c.record_min)),
        format!("{} · {}", year(c.record_max), year(c.record_min)),
    ))
}

fn past_day(f: &Formatter<'_>, h: &History, date: LocalDate, today: LocalDate) -> PastDayVm {
    let s = f.strings;
    let day = h.day(date);
    let normal = h.climate_on(date);
    let high = day.and_then(|d| d.temp_max);
    let normal_high = normal.and_then(|c| c.normal_max);
    let precipitation = match day {
        Some(d) if d.trace => Quantity {
            value: Some(0.0),
            text: s.get(Text::Trace).to_string(),
            unit: f.precipitation(None).unit,
        },
        _ => f.precipitation(day.and_then(|d| d.precip_sum)),
    };
    PastDayVm {
        label: if shifted(today, -1) == date {
            s.get(Text::Yesterday).to_string()
        } else {
            s.get(weekday_text(weekday_from_days(date.days_since_epoch())))
                .to_string()
        },
        date_short: f.date(date),
        high: f.temperature(high),
        low: f.temperature(day.and_then(|d| d.temp_min)),
        precipitation,
        departure: delta(f, high, normal_high),
        normal_high: f.temperature(normal_high),
        normal_low: f.temperature(normal.and_then(|c| c.normal_min)),
        low_departure: delta(
            f,
            day.and_then(|d| d.temp_min),
            normal.and_then(|c| c.normal_min),
        ),
        normal_precip: f.precipitation(normal.and_then(|c| c.normal_precip)),
        snow: day
            .and_then(|d| d.snow_sum)
            .filter(|s| s.0 > 0.0)
            .map(|s| f.precipitation(Some(s))),
        record_high: record(f, normal.and_then(|c| c.record_max)),
        record_low: record(f, normal.and_then(|c| c.record_min)),
        summary: day.map(|d| past_summary(f, d, normal)),
    }
}

/// A past day in a sentence or two: its high against normal, its rain or
/// snow, and a record it set ("A record high for the date.").
fn past_summary(f: &Formatter<'_>, d: &PastDay, normal: Option<&DayClimate>) -> String {
    let s = f.strings;
    let mut parts: Vec<String> = Vec::new();
    let departure = delta(f, d.temp_max, normal.and_then(|c| c.normal_max));
    if let Some(v) = departure.value {
        let size = format!(
            "{}{}",
            number(v.abs(), f.presentation.temperature_decimals),
            degree(departure.unit)
        );
        parts.push(if v.abs() < 2.0 {
            s.get(Text::PastNormal).to_string()
        } else if v > 0.0 {
            s.fmt(Text::PastWarmer, &[("n", &size)])
        } else {
            s.fmt(Text::PastCooler, &[("n", &size)])
        });
    }
    let snow = d.snow_sum.filter(|s| s.0 > 0.0);
    match (snow, d.precip_sum) {
        (Some(snow), _) => parts.push(s.fmt(
            Text::PastSnow,
            &[("amount", &f.precipitation(Some(snow)).text)],
        )),
        _ if d.trace => parts.push(s.get(Text::PastTrace).to_string()),
        (None, Some(p)) if p.0 > 0.0 => parts.push(s.fmt(
            Text::PastRain,
            &[("amount", &f.precipitation(Some(p)).text)],
        )),
        (None, Some(_)) => parts.push(s.get(Text::PastDry).to_string()),
        (None, None) => {}
    }
    // A record set that day carries its year.
    let set = |r: Option<Record>| r.is_some_and(|r| r.year == d.date.year);
    if set(normal.and_then(|c| c.record_max)) {
        parts.push(s.get(Text::PastRecordHigh).to_string());
    }
    if set(normal.and_then(|c| c.record_min)) {
        parts.push(s.get(Text::PastRecordLow).to_string());
    }
    if set(normal.and_then(|c| c.record_precip)) {
        parts.push(s.get(Text::PastRecordWet).to_string());
    }
    parts.join(" ")
}

/// Precipitation from the 1st to yesterday against the normal of those days;
/// `None` on the 1st or without measurements.
fn month_to_date(f: &Formatter<'_>, h: &History, today: LocalDate) -> Option<MonthToDateVm> {
    let dates: Vec<LocalDate> = (1..today.day)
        .map(|day| LocalDate { day, ..today })
        .collect();
    let amounts: Vec<f32> = dates
        .iter()
        .filter_map(|d| h.day(*d))
        .filter_map(|d| d.precip_sum.map(|p| p.0).or(d.trace.then_some(0.0)))
        .collect();
    if amounts.is_empty() {
        return None;
    }
    let total = f.precipitation(Some(Mm(amounts.iter().sum())));
    let normals: Option<Vec<f32>> = dates
        .iter()
        .map(|d| h.climate_on(*d).and_then(|c| c.normal_precip).map(|p| p.0))
        .collect();
    let normal = f.precipitation(normals.map(|n| Mm(n.iter().sum())));
    let text = if normal.value.is_some() {
        f.strings.fmt(
            Text::MonthToDate,
            &[("amount", &total.text), ("normal", &normal.text)],
        )
    } else {
        f.strings
            .fmt(Text::MonthToDateOnly, &[("amount", &total.text)])
    };
    Some(MonthToDateVm {
        precipitation: total,
        normal,
        text,
    })
}

/// "4° warmer than yesterday", from today's forecast high and yesterday's
/// observed one.
fn yesterday_delta(
    f: &Formatter<'_>,
    h: &History,
    today: LocalDate,
    today_high: Option<Celsius>,
) -> Option<String> {
    let yesterday = h.day(shifted(today, -1))?.temp_max?;
    let d = delta(f, today_high, Some(yesterday));
    let value = d.value?;
    let size = format!(
        "{}{}",
        number(value.abs(), f.presentation.temperature_decimals),
        degree(d.unit)
    );
    Some(if value > 0.0 {
        f.strings.fmt(Text::WarmerThanYesterday, &[("n", &size)])
    } else if value < 0.0 {
        f.strings.fmt(Text::CoolerThanYesterday, &[("n", &size)])
    } else {
        f.strings.get(Text::SameAsYesterday).to_string()
    })
}

/// Where the history comes from: "NY City Central Park, 7.8 km · records:
/// New York-Central Park Area · Normals 1991–2020".
fn credit(f: &Formatter<'_>, h: &History) -> String {
    let s = f.strings;
    let mut parts: Vec<String> = source_credit(f, h).into_iter().collect();
    if let Some(records) = &h.records_station
        && h.station.as_ref().is_none_or(|s| s.id != records.id)
    {
        parts.push(s.fmt(Text::RecordsFrom, &[("station", &records.name)]));
    }
    if let Some(period) = h.period {
        parts.push(s.fmt(Text::NormalsPeriod, &[("years", &period_text(period))]));
    }
    parts.join(" · ")
}

/// The history widget for `today`, with today's forecast high.
pub(super) fn history(
    f: &Formatter<'_>,
    h: &History,
    today: LocalDate,
    today_high: Option<Celsius>,
) -> HistoryVm {
    let n = i64::from(f.presentation.history_days.min(31));
    HistoryVm {
        days: (1..=n)
            .rev()
            .map(|back| past_day(f, h, shifted(today, -back), today))
            .collect(),
        today: h.climate_on(today).map(|c| climate(f, c)),
        month_to_date: month_to_date(f, h, today),
        yesterday_delta: yesterday_delta(f, h, today, today_high),
        credit: credit(f, h),
    }
}

/// The data attribution of history shown alone: its source's, and
/// the sources that filled gaps.
pub(super) fn attribution(h: &History) -> String {
    let mut out = String::from(h.source.attribution());
    for p in &h.supplemented_by {
        if *p != h.source {
            out.push_str(" · ");
            out.push_str(p.short_credit());
        }
    }
    out
}

/// The source half of a credit: the station and its distance, or the
/// reanalysis.
fn source_credit(f: &Formatter<'_>, h: &History) -> Option<String> {
    match (h.kind, &h.station) {
        (HistoryKind::Reanalysis, _) => Some(f.strings.get(Text::Reanalysis).to_string()),
        (HistoryKind::Station, Some(station)) => Some(match station.distance_m {
            #[allow(clippy::cast_precision_loss)] // a few hundred km
            Some(m) => format!(
                "{}, {}",
                station.name,
                f.distance(Some(Meters(m as f32))).text
            ),
            None => station.name.clone(),
        }),
        (HistoryKind::Station, None) => None,
    }
}

/// A date's typical weather from its climate: the share of wet years picks
/// sun (under 15 %), some cloud (under 30 %), showers (under 50 %, storms when
/// the average high is 29 °C or more) or rain; at or below freezing on
/// average, their snowy forms.
fn typical_icon(c: Option<&DayClimate>) -> crate::icon::IconId {
    use crate::icon::IconId;
    let Some(chance) = c.and_then(|c| c.precip_chance).map(|p| p.0) else {
        return IconId::NotAvailable;
    };
    let high = c.and_then(|c| c.normal_max).map_or(15.0, |t| t.0);
    let low = c.and_then(|c| c.normal_min).map_or(5.0, |t| t.0);
    let frozen = low <= 0.0 && high <= 3.0;
    match chance {
        n if n < 15.0 => IconId::ClearDay,
        n if n < 30.0 => IconId::PartlyCloudyDay,
        n if n < 50.0 && frozen => IconId::SnowShowersDay,
        n if n < 50.0 && high >= 29.0 => IconId::ThunderstormsDay,
        n if n < 50.0 => IconId::ShowersDay,
        _ if frozen => IconId::Snow,
        _ => IconId::Rain,
    }
}

/// A climate query's answer as shown (the vacation planner): each of the
/// query's dates, labelled with the trip's weekday, on one temperature scale.
pub fn plan_vm(
    presentation: &crate::presentation::PresentationConfig,
    strings: &crate::strings::Strings,
    history: &History,
    query: &crate::history::ClimateQuery,
) -> PlanVm {
    let f = Formatter::new(presentation, strings);
    let dates = query.dates();
    let climates: Vec<Option<&DayClimate>> = dates.iter().map(|d| history.climate_on(*d)).collect();
    let short = |d: LocalDate| f.date(d);
    // One scale for every date: the coldest low to the hottest high.
    let temps: Vec<f32> = climates
        .iter()
        .flatten()
        .flat_map(|c| {
            [
                c.normal_min,
                c.normal_max,
                c.record_min.map(|r| Celsius(r.value)),
                c.record_max.map(|r| Celsius(r.value)),
            ]
        })
        .flatten()
        .map(|c| c.0)
        .collect();
    let (lo, hi) = temps
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), t| {
            (a.min(*t), b.max(*t))
        });
    let at = |c: Option<Celsius>| match c {
        Some(c) if hi > lo => (c.0 - lo) / (hi - lo),
        Some(_) => 0.5,
        None => -1.0,
    };
    let days = dates
        .iter()
        .zip(&climates)
        .map(|(date, c)| PlanDayVm {
            label: strings
                .get(weekday_text(weekday_from_days(date.days_since_epoch())))
                .to_string(),
            date_short: short(*date),
            icon: typical_icon(*c),
            high: f.temperature(c.and_then(|c| c.normal_max)),
            low: f.temperature(c.and_then(|c| c.normal_min)),
            precipitation: f.precipitation(c.and_then(|c| c.normal_precip)),
            precip_chance: f.percent(c.and_then(|c| c.precip_chance)),
            hottest: record(&f, c.and_then(|c| c.record_max)),
            coldest: record(&f, c.and_then(|c| c.record_min)),
            highest: f.temperature(c.and_then(|c| c.record_max).map(|r| Celsius(r.value))),
            lowest: f.temperature(c.and_then(|c| c.record_min).map(|r| Celsius(r.value))),
            range: (
                at(c.and_then(|c| c.normal_min)),
                at(c.and_then(|c| c.normal_max)),
            ),
            extremes: (
                at(c.and_then(|c| c.record_min).map(|r| Celsius(r.value))),
                at(c.and_then(|c| c.record_max).map(|r| Celsius(r.value))),
            ),
        })
        .collect();
    let mean = |pick: fn(&DayClimate) -> Option<f32>| {
        let v: Vec<f32> = climates.iter().flatten().filter_map(|c| pick(c)).collect();
        #[allow(clippy::cast_precision_loss)] // at most 31
        (!v.is_empty()).then(|| v.iter().sum::<f32>() / v.len() as f32)
    };
    let (high, low) = (
        mean(|c| c.normal_max.map(|t| t.0)),
        mean(|c| c.normal_min.map(|t| t.0)),
    );
    let average = high.zip(low).map(|(high, low)| {
        strings.fmt(
            Text::PlanAverage,
            &[
                ("high", &f.temperature(Some(Celsius(high))).text),
                ("low", &f.temperature(Some(Celsius(low))).text),
            ],
        )
    });
    let wet_days = mean(|c| c.precip_chance.map(|p| p.0))
        .map(|n| strings.fmt(Text::PlanWetDays, &[("n", &number(n.clamp(0.0, 100.0), 0))]));
    let years = history.period.unwrap_or(query.years);
    let span = match (dates.first(), dates.last()) {
        (Some(a), Some(b)) if a != b => format!("{} – {}", short(*a), short(*b)),
        (Some(a), _) => short(*a),
        _ => String::new(),
    };
    let credit = source_credit(&f, history)
        .into_iter()
        .chain([period_text(years)])
        .collect::<Vec<_>>()
        .join(" · ");
    PlanVm {
        attribution: attribution(history),
        title: strings.fmt(
            Text::PlanTitle,
            &[("dates", &span), ("years", &period_text(years))],
        ),
        days,
        average,
        wet_days,
        credit,
    }
}
