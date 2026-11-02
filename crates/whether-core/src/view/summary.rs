//! The one-sentence outlook.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::condition::Condition;
use crate::derive::{current_hour_index, next_precip, nowcast_start, nowcast_steps, nowcast_stop};
use crate::model::{Daily, Hourly, WeatherSnapshot};
use crate::strings::Text;
use crate::time::UnixTime;
use crate::view::format::Formatter;

/// Seconds in an hour.
const HOUR: i64 = 3_600;
/// How far ahead the summary's nowcast sentence looks.
const SUMMARY_NOWCAST_HORIZON: i64 = 2 * 3_600;
/// Smallest precipitation (mm an hour) that counts as falling.
pub(crate) const NOWCAST_MIN_RATE: f32 = 0.4;
/// Beyond this many minutes away, a start or stop is said as a clock time.
const NOWCAST_MINUTES_MAX: i64 = 90;

/// The word for precipitation at `t`: from the forecast step's condition when it
/// precipitates, else from the current temperature (snow at or below 0 °C).
fn precip_kind(snap: &WeatherSnapshot, t: UnixTime) -> Text {
    let condition = condition_at(snap, t);
    if condition.is_precipitation() {
        return if condition.is_frozen() {
            Text::PrecipSnow
        } else {
            Text::PrecipRain
        };
    }
    match snap.current.as_ref().and_then(|c| c.temperature) {
        Some(t) if t.0 <= 0.0 => Text::PrecipSnow,
        Some(_) => Text::PrecipRain,
        None => Text::PrecipGeneric,
    }
}

/// The condition at `t`: the forecast step's when it is precipitation, otherwise
/// the current condition.
fn condition_at(snap: &WeatherSnapshot, t: UnixTime) -> Condition {
    let current = snap
        .current
        .as_ref()
        .map(|c| c.condition)
        .unwrap_or_default();
    current_hour_index(&snap.hourly, t)
        .map(|i| snap.hourly[i].condition)
        .filter(|c| c.is_precipitation())
        .unwrap_or(current)
}

/// "Rain starting in ~45 min." (or "around 3:15 PM" when further off) when the
/// nowcast shows precipitation within `horizon` seconds; once it has started,
/// "Rain stopping in ~20 min." when it ends within them, else "Rain now.";
/// otherwise `None`.
pub(crate) fn nowcast_sentence(
    f: &Formatter<'_>,
    snap: &WeatherSnapshot,
    now: UnixTime,
    horizon: i64,
) -> Option<String> {
    let m = nowcast_steps(snap);
    let start =
        nowcast_start(&m, now, NOWCAST_MIN_RATE).filter(|start| start.0 - now.0 <= horizon)?;
    let s = f.strings;
    let kind = s.get(precip_kind(snap, start));
    // "in ~n min" up close, a clock time further off.
    let when = |t: UnixTime, minutes: Text, at: Text| {
        let secs = t.0 - now.0;
        if secs <= NOWCAST_MINUTES_MAX * 60 {
            let n = ((secs + 150) / 300 * 5).max(5);
            s.fmt(minutes, &[("kind", kind), ("n", &n.to_string())])
        } else {
            let time = f.clock(t, snap.offset_at(t));
            s.fmt(at, &[("kind", kind), ("time", &time)])
        }
    };
    Some(if start.0 <= now.0 {
        match nowcast_stop(&m, now, NOWCAST_MIN_RATE).filter(|stop| stop.0 - now.0 <= horizon) {
            Some(stop) => when(stop, Text::NowcastStopping, Text::NowcastStoppingAt),
            None => s.fmt(Text::NowcastNow, &[("kind", kind)]),
        }
    } else {
        when(start, Text::NowcastStarting, Text::NowcastStartingAt)
    })
}

/// "Partly cloudy. High 18°, low 8°. Rain starting in ~45 min."
pub(crate) fn summary(
    f: &Formatter<'_>,
    snap: &WeatherSnapshot,
    today: Option<&Daily>,
    now: UnixTime,
) -> Option<String> {
    let current = snap.current.as_ref()?;
    let s = f.strings;
    let mut out = String::from(s.get(current.condition.label()));
    out.push('.');

    if let Some(day) = today
        && day.temp_max.is_some()
        && day.temp_min.is_some()
    {
        let (hi, lo) = (
            f.temperature(f.lead(day.temp_max, day.apparent_max)).text,
            f.temperature(f.lead(day.temp_min, day.apparent_min)).text,
        );
        // Said to be feels-like when they are.
        let feels = f.presentation.temperature_basis
            == crate::presentation::TemperatureBasis::FeelsLike
            && day.apparent_max.is_some()
            && day.apparent_min.is_some();
        let text = if feels {
            Text::SummaryHighLowFeels
        } else {
            Text::SummaryHighLow
        };
        out.push(' ');
        out.push_str(&s.fmt(text, &[("hi", &hi), ("lo", &lo)]));
    }

    out.push(' ');
    if let Some(sentence) = nowcast_sentence(f, snap, now, SUMMARY_NOWCAST_HORIZON) {
        out.push_str(&sentence);
    } else if let Some((at, pct)) = next_precip(&snap.hourly, now, f.presentation.precip_threshold)
    {
        let kind = s.get(precip_kind(snap, at));
        let time = f.clock(at, snap.offset_at(at));
        out.push_str(&s.fmt(
            Text::RainLikelyAt,
            &[("kind", kind), ("time", &time), ("pct", &pct.to_string())],
        ));
    } else {
        out.push_str(s.get(Text::NoPrecipToday));
    }
    Some(out)
}

/// Where `hour` (local, 0–23) falls in the day.
fn part_of_day(hour: u8) -> Text {
    match hour {
        0..=4 => Text::PartOvernight,
        5..=7 => Text::PartEarlyMorning,
        8..=9 => Text::PartMorning,
        10..=11 => Text::PartLateMorning,
        12 => Text::PartMidday,
        13..=14 => Text::PartEarlyAfternoon,
        15..=16 => Text::PartAfternoon,
        17 => Text::PartLateAfternoon,
        18..=20 => Text::PartEvening,
        _ => Text::PartLateEvening,
    }
}

/// When a day's precipitation falls, from its hours: "Light drizzle in the
/// early afternoon.", "Rain from 2 PM to 9 PM." or "Rain throughout the day."
/// The longest stretch counts (wet hours at most an hour apart join up), named
/// after its most severe condition. `None` for a dry day.
pub(crate) fn day_precip_sentence(
    f: &Formatter<'_>,
    snap: &WeatherSnapshot,
    hours: &[&Hourly],
) -> Option<String> {
    // (start, end, worst) of each wet stretch, in seconds.
    let mut runs: Vec<(i64, i64, Condition)> = Vec::new();
    for (i, h) in hours.iter().enumerate() {
        if !h.condition.is_precipitation() {
            continue;
        }
        let step = hours
            .get(i + 1)
            .map_or(HOUR, |next| next.time.0 - h.time.0)
            .clamp(HOUR / 4, 6 * HOUR);
        let (start, end) = (h.time.0, h.time.0 + step);
        match runs.last_mut() {
            Some((_, run_end, worst)) if start - *run_end <= HOUR => {
                *run_end = end;
                if h.condition.severity_rank() > worst.severity_rank() {
                    *worst = h.condition;
                }
            }
            _ => runs.push((start, end, h.condition)),
        }
    }
    let (start, end, worst) = runs
        .into_iter()
        .max_by_key(|(start, end, c)| (end - start, c.severity_rank()))?;
    let s = f.strings;
    let kind = s.get(worst.label());
    let local = |t: i64| UnixTime(t).to_local(snap.offset_at(UnixTime(t)));
    let length = end - start;
    Some(if length >= 12 * HOUR {
        s.fmt(Text::DayAllDay, &[("kind", kind)])
    } else if length <= 4 * HOUR {
        let when = s.get(part_of_day(local(start + length / 2).hour));
        s.fmt(Text::DayWhen, &[("kind", kind), ("when", when)])
    } else {
        let label = |t: i64| f.hour_label(UnixTime(t), snap.offset_at(UnixTime(t)));
        s.fmt(
            Text::DayFromTo,
            &[
                ("kind", kind),
                ("start", &label(start)),
                ("end", &label(end)),
            ],
        )
    })
}
