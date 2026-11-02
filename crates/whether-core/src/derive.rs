//! Values derived from a snapshot: dew point, the current step, pressure trend and
//! upcoming precipitation.

use alloc::vec::Vec;

use crate::condition::Condition;
use crate::model::{Daily, Hourly, Minutely, WeatherSnapshot};
use crate::time::{UnixTime, UtcOffset};
use crate::units::{Celsius, Percent};

const HOUR: i64 = 3_600;

/// Direction of the pressure change over the next three hours.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "snake_case")
)]
#[allow(missing_docs)]
pub enum Trend {
    Rising,
    Falling,
    Steady,
}

/// Dew point from temperature and relative humidity (Magnus formula, b = 17.62,
/// c = 243.12 °C). `None` when humidity is not positive.
pub fn dew_point(t: Celsius, rh: Percent) -> Option<Celsius> {
    const B: f32 = 17.62;
    const C: f32 = 243.12;
    if rh.0 <= 0.0 {
        return None;
    }
    let gamma = libm::logf(rh.0.min(100.0) / 100.0) + B * t.0 / (C + t.0);
    Some(Celsius(C * gamma / (B - gamma)))
}

/// Fills values that can be derived from others: a missing dew point, and a
/// day's "feels like" extremes from its hours.
pub fn fill_derived(snapshot: &mut WeatherSnapshot) {
    if let Some(c) = snapshot.current.as_mut()
        && c.dew_point.is_none()
        && let (Some(t), Some(rh)) = (c.temperature, c.humidity)
    {
        c.dew_point = dew_point(t, rh);
    }
    let days: Vec<(usize, Option<(Celsius, Celsius)>)> = snapshot
        .daily
        .iter()
        .enumerate()
        .filter(|(_, d)| d.apparent_max.is_none() && d.apparent_min.is_none())
        .map(|(i, d)| (i, apparent_extremes(snapshot, d)))
        .collect();
    for (i, extremes) in days {
        if let Some((max, min)) = extremes {
            snapshot.daily[i].apparent_max = Some(max);
            snapshot.daily[i].apparent_min = Some(min);
        }
    }
}

/// The highest and lowest "feels like" temperatures of `day`'s hours, when they
/// cover at least 18 hours of it (one or 3-hour steps).
fn apparent_extremes(snapshot: &WeatherSnapshot, day: &Daily) -> Option<(Celsius, Celsius)> {
    let mut covered = 0_i64;
    let (mut max, mut min) = (f32::NEG_INFINITY, f32::INFINITY);
    for (i, h) in snapshot.hourly.iter().enumerate() {
        let Some(a) = h.apparent else { continue };
        if h.time.local_date(snapshot.offset_at(h.time)) != day.date {
            continue;
        }
        let step = snapshot
            .hourly
            .get(i + 1)
            .map_or(HOUR, |next| next.time.0 - h.time.0)
            .clamp(HOUR, 3 * HOUR);
        covered += step;
        max = max.max(a.0);
        min = min.min(a.0);
    }
    (covered >= 18 * HOUR).then_some((Celsius(max), Celsius(min)))
}

/// The index of the step containing `now`. The last step is assumed to last as
/// long as the one before it (one hour if it stands alone).
pub fn current_hour_index(hourly: &[Hourly], now: UnixTime) -> Option<usize> {
    let first = hourly.first()?;
    if now < first.time {
        return None;
    }
    let idx = hourly.partition_point(|h| h.time <= now) - 1;
    if idx + 1 < hourly.len() {
        return Some(idx);
    }
    let step = match hourly.len() {
        1 => HOUR,
        n => hourly[n - 1].time.0 - hourly[n - 2].time.0,
    };
    (now.0 < hourly[idx].time.0 + step).then_some(idx)
}

/// The index of today's entry, by the location's local date.
pub fn today_index(daily: &[Daily], now: UnixTime, offset: UtcOffset) -> Option<usize> {
    let today = now.local_date(offset);
    daily.iter().position(|d| d.date == today)
}

/// Pressure trend from the current step to three hours later: a change of at least
/// +1 hPa is rising, at most −1 hPa falling.
pub fn pressure_trend(hourly: &[Hourly], now: UnixTime) -> Option<Trend> {
    let i = current_hour_index(hourly, now)?;
    let start = hourly[i].pressure_msl?;
    let target = hourly[i].time.0 + 3 * HOUR;
    let later = hourly[i..]
        .iter()
        .find(|h| h.time.0 >= target)?
        .pressure_msl?;
    let delta = later.0 - start.0;
    Some(if delta >= 1.0 {
        Trend::Rising
    } else if delta <= -1.0 {
        Trend::Falling
    } else {
        Trend::Steady
    })
}

/// The first step within 12 hours, starting at the current one, whose precipitation
/// probability is at least `threshold_pct`.
pub fn next_precip(hourly: &[Hourly], now: UnixTime, threshold_pct: u8) -> Option<(UnixTime, u8)> {
    let start =
        current_hour_index(hourly, now).unwrap_or_else(|| hourly.partition_point(|h| h.time < now));
    hourly[start..]
        .iter()
        .take_while(|h| h.time.0 < now.0 + 12 * HOUR)
        .find_map(|h| {
            let pct = h.precip_probability?.0;
            (pct >= f32::from(threshold_pct))
                .then(|| (h.time, libm::roundf(pct).clamp(0.0, 100.0) as u8))
        })
}

/// A day's condition over its daylight hours: the hours whose middle
/// falls between sunrise and sunset, each weighted by its step (1 hour, or 3
/// for 3-hourly forecasts). Precipitation wins when it covers at least two
/// hours and a quarter of the daylight; otherwise the most common dry condition
/// does. Ties go to the more severe. Forecasts start at the current hour, so
/// today counts only the daylight still to come. `None` without sunrise and
/// sunset, after today's sunset, or when the hourly data covers less than three
/// quarters of the daylight.
pub fn daylight_condition(day: &Daily, hourly: &[Hourly]) -> Option<Condition> {
    let set = day.sunset?.0;
    let rise = day.sunrise?.0.max(hourly.first()?.time.0);
    if set - rise < HOUR {
        return None;
    }
    let mut weights: Vec<(Condition, i64)> = Vec::new();
    for (i, h) in hourly.iter().enumerate() {
        let step = hourly
            .get(i + 1)
            .map(|next| next.time.0 - h.time.0)
            .or_else(|| i.checked_sub(1).map(|p| h.time.0 - hourly[p].time.0))
            .unwrap_or(HOUR)
            .clamp(HOUR / 4, 6 * HOUR);
        // The part of [time, time + step) that is daylight.
        let covered = (h.time.0 + step).min(set) - h.time.0.max(rise);
        if covered <= 0 || h.condition == Condition::Unknown {
            continue;
        }
        match weights.iter_mut().find(|(c, _)| *c == h.condition) {
            Some((_, w)) => *w += covered,
            None => weights.push((h.condition, covered)),
        }
    }
    let total: i64 = weights.iter().map(|(_, w)| w).sum();
    if total * 4 < (set - rise) * 3 {
        return None;
    }
    let wet: i64 = weights
        .iter()
        .filter(|(c, _)| c.is_precipitation())
        .map(|(_, w)| w)
        .sum();
    let rainy = wet >= 2 * HOUR && wet * 4 >= total;
    weights
        .into_iter()
        .filter(|(c, _)| c.is_precipitation() == rainy)
        .max_by_key(|(c, w)| (*w, c.severity_rank()))
        .map(|(c, _)| c)
}

/// One step of the nowcast: when it starts, how long it lasts, how hard it falls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NowcastStep {
    /// Its start.
    pub time: UnixTime,
    /// Its length in seconds (60 or 900).
    pub secs: i64,
    /// Precipitation in millimetres an hour.
    pub mm_per_hour: f32,
}

/// The finest nowcast there is: the one-minute steps (an app's minute nowcast)
/// while they last, then the 15-minute steps after them.
pub fn nowcast_steps(snap: &WeatherSnapshot) -> Vec<NowcastStep> {
    let steps = |m: &[Minutely], secs: i64| -> Vec<NowcastStep> {
        m.iter()
            .map(|m| NowcastStep {
                time: m.time,
                secs,
                #[allow(clippy::cast_precision_loss)] // 60 or 900
                mm_per_hour: m.precipitation.map_or(0.0, |p| p.0 * 3_600.0 / secs as f32),
            })
            .collect()
    };
    let mut out = steps(&snap.minutely_1, 60);
    let end = out.last().map_or(i64::MIN, |s| s.time.0 + s.secs);
    out.extend(
        steps(&snap.minutely_15, 900)
            .into_iter()
            .filter(|s| s.time.0 >= end),
    );
    out
}

/// The start of the first step, from the one containing `now`, with at least
/// `min_rate` (mm an hour).
pub fn nowcast_start(steps: &[NowcastStep], now: UnixTime, min_rate: f32) -> Option<UnixTime> {
    steps
        .iter()
        .filter(|s| s.time.0 + s.secs > now.0)
        .find(|s| s.mm_per_hour >= min_rate)
        .map(|s| s.time)
}

/// When precipitation that is falling at `now` stops: the first step after it
/// below `min_rate`, if the nowcast reaches that far.
pub fn nowcast_stop(steps: &[NowcastStep], now: UnixTime, min_rate: f32) -> Option<UnixTime> {
    steps
        .iter()
        .filter(|s| s.time.0 > now.0)
        .find(|s| s.mm_per_hour < min_rate)
        .map(|s| s.time)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::LocalDate;
    use crate::units::{Hpa, Mm};
    use alloc::vec::Vec;

    fn hours(n: i64, step: i64) -> Vec<Hourly> {
        (0..n)
            .map(|i| Hourly {
                time: UnixTime(1_000 + i * step),
                ..Hourly::default()
            })
            .collect()
    }

    /// A day from 07:00 to 18:00 (hours counted from midnight at 0), with `wet`
    /// hours raining and the rest `dry`, at `step`-second steps over 24 hours.
    fn day(wet: &[i64], dry: Condition, step: i64) -> (Daily, Vec<Hourly>) {
        let day = Daily {
            sunrise: Some(UnixTime(7 * HOUR)),
            sunset: Some(UnixTime(18 * HOUR)),
            condition: Condition::Rain(crate::condition::Intensity::Moderate),
            ..Daily::default()
        };
        let hourly = (0..24 * HOUR / step)
            .map(|i| {
                let t = i * step;
                let hour = t / HOUR;
                Hourly {
                    time: UnixTime(t),
                    condition: if wet.contains(&hour) {
                        Condition::Rain(crate::condition::Intensity::Moderate)
                    } else {
                        dry
                    },
                    ..Hourly::default()
                }
            })
            .collect();
        (day, hourly)
    }

    #[test]
    fn day_icons_follow_daylight() {
        use crate::condition::Intensity::Moderate;
        // An example: rain from midnight to 2 AM, then sun.
        let (d, h) = day(&[0, 1], Condition::Clear, HOUR);
        assert_eq!(daylight_condition(&d, &h), Some(Condition::Clear));
        // An hour of rain in the afternoon is not the day's weather either.
        let (d, h) = day(&[14], Condition::PartlyCloudy, HOUR);
        assert_eq!(daylight_condition(&d, &h), Some(Condition::PartlyCloudy));
        // Three wet afternoon hours are (two hours, and a quarter of eleven).
        let (d, h) = day(&[13, 14, 15], Condition::Clear, HOUR);
        assert_eq!(daylight_condition(&d, &h), Some(Condition::Rain(Moderate)));
        // Three-hourly forecasts count each step as three hours.
        let (d, h) = day(&[12, 13, 14], Condition::Cloudy, 3 * HOUR);
        assert_eq!(daylight_condition(&d, &h), Some(Condition::Rain(Moderate)));
    }

    #[test]
    fn day_icons_need_sun_times_and_enough_hours() {
        let (mut d, h) = day(&[], Condition::Clear, HOUR);
        assert_eq!(daylight_condition(&d, &h[..10]), None, "only until 10:00");
        // Today, from 10:00: the rest of the daylight counts, rain before it not.
        let (d2, mut h2) = day(&[8, 9], Condition::Clear, HOUR);
        h2.drain(..10);
        assert_eq!(daylight_condition(&d2, &h2), Some(Condition::Clear));
        assert_eq!(daylight_condition(&d2, &h2[9..]), None, "after sunset");
        d.sunset = None;
        assert_eq!(daylight_condition(&d, &h), None);
        // The most common dry condition wins: seven overcast hours, four clear.
        let (d, mut h) = day(&[], Condition::Clear, HOUR);
        for hour in &mut h[7..13] {
            hour.condition = Condition::Overcast;
        }
        h[13].condition = Condition::Overcast;
        assert_eq!(daylight_condition(&d, &h), Some(Condition::Overcast));
    }

    #[test]
    fn dew_point_reference() {
        let d = dew_point(Celsius(20.0), Percent(50.0)).unwrap();
        assert!((d.0 - 9.3).abs() <= 0.1, "{}", d.0);
        assert_eq!(dew_point(Celsius(20.0), Percent(0.0)), None);
    }

    #[test]
    fn current_hour_boundaries() {
        let h = hours(3, HOUR);
        assert_eq!(current_hour_index(&h, UnixTime(999)), None);
        assert_eq!(current_hour_index(&h, UnixTime(1_000)), Some(0));
        assert_eq!(current_hour_index(&h, UnixTime(1_000 + HOUR - 1)), Some(0));
        assert_eq!(current_hour_index(&h, UnixTime(1_000 + HOUR)), Some(1));
        assert_eq!(
            current_hour_index(&h, UnixTime(1_000 + 3 * HOUR - 1)),
            Some(2)
        );
        assert_eq!(current_hour_index(&h, UnixTime(1_000 + 3 * HOUR)), None);
        assert_eq!(current_hour_index(&[], UnixTime(0)), None);
        let three_hourly = hours(2, 3 * HOUR);
        assert_eq!(
            current_hour_index(&three_hourly, UnixTime(1_000 + 5 * HOUR)),
            Some(1)
        );
    }

    #[test]
    fn today_uses_the_local_date() {
        let d = |day| Daily {
            date: LocalDate {
                year: 2026,
                month: 10,
                day,
            },
            ..Daily::default()
        };
        let daily = [d(4), d(5), d(6)];
        // 2026-10-05T02:00Z is still Oct 4 in UTC−4.
        assert_eq!(
            today_index(&daily, UnixTime(1_791_165_600), UtcOffset(-14_400)),
            Some(0)
        );
        assert_eq!(
            today_index(&daily, UnixTime(1_791_165_600), UtcOffset(0)),
            Some(1)
        );
        assert_eq!(
            today_index(&daily[..1], UnixTime(1_791_212_400), UtcOffset(0)),
            None
        );
    }

    #[test]
    fn pressure_trends() {
        let mut h = hours(5, HOUR);
        for (i, p) in [1_010.0, 1_010.5, 1_011.0, 1_011.3, 1_011.2]
            .into_iter()
            .enumerate()
        {
            h[i].pressure_msl = Some(Hpa(p));
        }
        assert_eq!(pressure_trend(&h, UnixTime(1_000)), Some(Trend::Rising));
        assert_eq!(
            pressure_trend(&h, UnixTime(1_000 + HOUR)),
            Some(Trend::Steady)
        );
        assert_eq!(
            pressure_trend(&h, UnixTime(1_000 + 2 * HOUR)),
            None,
            "no point 3 h later"
        );
        h[3].pressure_msl = Some(Hpa(1_008.0));
        assert_eq!(pressure_trend(&h, UnixTime(1_000)), Some(Trend::Falling));
    }

    #[test]
    fn next_precip_within_twelve_hours() {
        let mut h = hours(20, HOUR);
        h[3].precip_probability = Some(Percent(40.0));
        h[5].precip_probability = Some(Percent(70.4));
        assert_eq!(next_precip(&h, UnixTime(1_000), 50), Some((h[5].time, 70)));
        assert_eq!(next_precip(&h, UnixTime(1_000), 30), Some((h[3].time, 40)));
        assert_eq!(next_precip(&h, UnixTime(1_000 + 6 * HOUR), 30), None);
        h[15].precip_probability = Some(Percent(90.0));
        assert_eq!(next_precip(&h, UnixTime(1_000), 80), None, "beyond 12 h");
        assert_eq!(
            next_precip(&h, UnixTime(0), 80),
            None,
            "before the first point: from start"
        );
    }

    /// Steps of `secs` from 0, `rate` mm an hour where `wet` says.
    fn steps(n: i64, secs: i64, wet: impl Fn(i64) -> bool) -> Vec<NowcastStep> {
        (0..n)
            .map(|i| NowcastStep {
                time: UnixTime(i * secs),
                secs,
                mm_per_hour: if wet(i) { 1.6 } else { 0.0 },
            })
            .collect()
    }

    #[test]
    fn nowcast_starts_at_the_first_wet_step() {
        let m = steps(8, 900, |i| i >= 3);
        assert_eq!(nowcast_start(&m, UnixTime(0), 0.4), Some(UnixTime(2_700)));
        assert_eq!(
            nowcast_start(&m, UnixTime(3_000), 0.4),
            Some(UnixTime(2_700))
        );
        assert_eq!(nowcast_start(&m, UnixTime(10_000), 0.4), None);
        assert_eq!(nowcast_start(&m, UnixTime(0), 4.0), None);
        // A minute that ended is not now.
        let minutes = steps(60, 60, |i| i == 0);
        assert_eq!(nowcast_start(&minutes, UnixTime(90), 0.4), None);
    }

    #[test]
    fn nowcast_stops_at_the_first_dry_step_after_now() {
        let m = steps(8, 900, |i| i < 3);
        assert_eq!(nowcast_stop(&m, UnixTime(100), 0.4), Some(UnixTime(2_700)));
        let wet = steps(8, 900, |_| true);
        assert_eq!(
            nowcast_stop(&wet, UnixTime(0), 0.4),
            None,
            "not within the steps"
        );
    }

    #[test]
    fn feels_like_extremes_come_from_the_hours() {
        let mut snap = crate::testdata::sample_snapshot();
        let day = snap.daily[1].date;
        snap.daily[1].apparent_max = None;
        snap.daily[1].apparent_min = None;
        fill_derived(&mut snap);
        let hours: Vec<f32> = snap
            .hourly
            .iter()
            .filter(|h| h.time.local_date(snap.offset_at(h.time)) == day)
            .filter_map(|h| h.apparent.map(|a| a.0))
            .collect();
        assert!(hours.len() >= 18, "the sample covers tomorrow");
        let max = hours.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let min = hours.iter().copied().fold(f32::INFINITY, f32::min);
        assert_eq!(snap.daily[1].apparent_max, Some(Celsius(max)));
        assert_eq!(snap.daily[1].apparent_min, Some(Celsius(min)));
        // Days the hours do not cover keep none.
        let last = snap.daily.len() - 1;
        snap.daily[last].apparent_max = None;
        snap.daily[last].apparent_min = None;
        fill_derived(&mut snap);
        assert_eq!(snap.daily[last].apparent_max, None);
    }

    #[test]
    fn minutes_come_first_then_quarter_hours() {
        let mut snap = crate::testdata::sample_snapshot();
        snap.minutely_1 = (0..60)
            .map(|i| Minutely {
                time: UnixTime(i * 60),
                precipitation: Some(Mm(0.05)),
            })
            .collect();
        snap.minutely_15 = (0..8)
            .map(|i| Minutely {
                time: UnixTime(i * 900),
                precipitation: Some(Mm(0.1)),
            })
            .collect();
        let s = nowcast_steps(&snap);
        assert_eq!(s.len(), 60 + 4, "the 15-minute steps after the hour");
        assert!((s[0].mm_per_hour - 3.0).abs() < 1e-4, "0.05 mm a minute");
        assert_eq!((s[60].time, s[60].secs), (UnixTime(3_600), 900));
        assert!((s[60].mm_per_hour - 0.4).abs() < 1e-4);
    }
}
