//! A minute-by-minute nowcast the app gets elsewhere (Apple's WeatherKit on the
//! iPhone), added to a snapshot for the one-hour graph and the
//! "starting in ~10 min" sentence.

use whether_core::model::{Minutely, WeatherSnapshot};
use whether_core::time::UnixTime;
use whether_core::units::Mm;

use crate::{WhetherError, from_json};

/// One minute of precipitation.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct MinuteStep {
    /// The minute's start, Unix seconds.
    pub time: i64,
    /// How hard it falls, in millimetres an hour.
    pub mm_per_hour: f32,
}

/// `snapshot_json` with `minutes` as its one-minute nowcast (replacing any
/// earlier one); no minutes clear it.
#[uniffi::export]
#[allow(clippy::needless_pass_by_value)] // UniFFI passes arguments by value
pub fn with_minutes(
    snapshot_json: String,
    minutes: Vec<MinuteStep>,
) -> Result<String, WhetherError> {
    let mut snapshot: WeatherSnapshot = from_json("snapshot", &snapshot_json)?;
    let mut steps: Vec<Minutely> = minutes
        .iter()
        .filter(|m| m.mm_per_hour.is_finite())
        .map(|m| Minutely {
            time: UnixTime(m.time),
            precipitation: Some(Mm(m.mm_per_hour.max(0.0) / 60.0)),
        })
        .collect();
    steps.sort_by_key(|m| m.time);
    steps.dedup_by_key(|m| m.time);
    snapshot.minutely_1 = steps;
    serde_json::to_string(&snapshot).map_err(|e| WhetherError::InvalidJson {
        what: "snapshot".into(),
        message: e.to_string(),
    })
}

/// How far ahead rain makes Apple's minute-by-minute nowcast worth fetching.
const WANTED_WINDOW_S: i64 = 2 * 3_600;
/// The hourly chance of precipitation (percent) that counts as "maybe rain".
const WANTED_PROBABILITY: f32 = 20.0;

/// Whether the snapshot hints at precipitation in the next two hours, so
/// Apple's minute nowcast (a metered WeatherKit call) is worth fetching: any
/// 15-minute nowcast step or hourly step overlapping `[now, now + 2 h)` with
/// precipitation above zero, or an hourly chance of 20 % or more.
#[uniffi::export]
#[allow(clippy::needless_pass_by_value)] // UniFFI passes arguments by value
pub fn minutes_wanted(snapshot_json: String, now: i64) -> Result<bool, WhetherError> {
    let snapshot: WeatherSnapshot = from_json("snapshot", &snapshot_json)?;
    let end = now.saturating_add(WANTED_WINDOW_S);
    // A step covers [time, time + length): it counts when it overlaps the window.
    let overlaps =
        |time: UnixTime, length: i64| time.0 < end && time.0.saturating_add(length) > now;
    let nowcast = snapshot
        .minutely_15
        .iter()
        .filter(|m| overlaps(m.time, 900))
        .any(|m| m.precipitation.is_some_and(|p| p.0 > 0.0));
    let hourly = snapshot
        .hourly
        .iter()
        .filter(|h| overlaps(h.time, 3_600))
        .any(|h| {
            h.precipitation.is_some_and(|p| p.0 > 0.0)
                || h.precip_probability
                    .is_some_and(|p| p.0 >= WANTED_PROBABILITY)
        });
    Ok(nowcast || hourly)
}

#[cfg(test)]
mod tests {
    use super::*;
    use whether_core::model::Hourly;
    use whether_core::units::Percent;

    const NOW: i64 = 1_000_000;

    /// A snapshot with the given steps only, as JSON.
    fn snapshot(hourly: Vec<Hourly>, nowcast: Vec<Minutely>) -> String {
        let mut snap = whether_core::testdata::sample_snapshot();
        snap.hourly = hourly;
        snap.minutely_15 = nowcast;
        serde_json::to_string(&snap).unwrap()
    }

    /// An hourly step `hours` after `NOW`.
    fn hour(hours: i64, mm: f32, pop: f32) -> Hourly {
        Hourly {
            time: UnixTime(NOW + hours * 3_600),
            precipitation: Some(Mm(mm)),
            precip_probability: Some(Percent(pop)),
            ..Hourly::default()
        }
    }

    /// A 15-minute step `minutes` after `NOW`.
    fn quarter(minutes: i64, mm: f32) -> Minutely {
        Minutely {
            time: UnixTime(NOW + minutes * 60),
            precipitation: Some(Mm(mm)),
        }
    }

    fn wanted(hourly: Vec<Hourly>, nowcast: Vec<Minutely>) -> bool {
        minutes_wanted(snapshot(hourly, nowcast), NOW).unwrap()
    }

    #[test]
    fn a_dry_two_hours_do_not_want_minutes() {
        let hourly = vec![hour(0, 0.0, 5.0), hour(1, 0.0, 10.0), hour(2, 0.0, 15.0)];
        let nowcast = vec![quarter(0, 0.0), quarter(45, 0.0), quarter(105, 0.0)];
        assert!(!wanted(hourly, nowcast));
    }

    #[test]
    fn rain_in_ninety_minutes_wants_minutes() {
        assert!(wanted(Vec::new(), vec![quarter(0, 0.0), quarter(90, 0.2)]));
        // Hourly only: the step starting in an hour is inside the window.
        assert!(wanted(
            vec![hour(0, 0.0, 0.0), hour(1, 0.4, 0.0)],
            Vec::new()
        ));
        // A chance of 20 % is enough, 19 % is not.
        assert!(wanted(vec![hour(1, 0.0, 20.0)], Vec::new()));
        assert!(!wanted(vec![hour(1, 0.0, 19.0)], Vec::new()));
    }

    #[test]
    fn rain_in_three_hours_does_not() {
        let hourly = vec![hour(0, 0.0, 0.0), hour(1, 0.0, 0.0), hour(3, 2.0, 90.0)];
        let nowcast = vec![quarter(0, 0.0), quarter(180, 1.0)];
        assert!(!wanted(hourly, nowcast));
    }

    #[test]
    fn the_current_hour_counts_but_a_past_one_does_not() {
        // The step began 30 minutes ago and is still running.
        let mut running = hour(0, 0.0, 60.0);
        running.time = UnixTime(NOW - 1_800);
        assert!(wanted(vec![running], Vec::new()));
        // This one ended an hour ago.
        let mut past = hour(0, 3.0, 90.0);
        past.time = UnixTime(NOW - 3_600);
        assert!(!wanted(vec![past], Vec::new()));
    }

    #[test]
    fn no_hourly_data_and_unknown_values_do_not_want_minutes() {
        assert!(!wanted(Vec::new(), Vec::new()));
        let blank = Hourly {
            time: UnixTime(NOW),
            ..Hourly::default()
        };
        assert!(!wanted(vec![blank], Vec::new()));
        assert!(matches!(
            minutes_wanted("{".into(), NOW),
            Err(WhetherError::InvalidJson { .. })
        ));
    }

    #[test]
    fn minutes_become_the_snapshots_minute_nowcast() {
        let snap = serde_json::to_string(&whether_core::testdata::sample_snapshot()).unwrap();
        let minutes = [
            MinuteStep {
                time: 120,
                mm_per_hour: 3.0,
            },
            MinuteStep {
                time: 60,
                mm_per_hour: f32::NAN,
            },
            MinuteStep {
                time: 60,
                mm_per_hour: 6.0,
            },
        ];
        let json = with_minutes(snap.clone(), minutes.to_vec()).unwrap();
        let back: WeatherSnapshot = serde_json::from_str(&json).unwrap();
        let got: Vec<(i64, f32)> = back
            .minutely_1
            .iter()
            .map(|m| (m.time.0, m.precipitation.unwrap().0))
            .collect();
        assert_eq!(got, [(60, 0.1), (120, 0.05)]);
        let cleared = with_minutes(json, Vec::new()).unwrap();
        assert!(
            !cleared.contains("\"minutely_1\""),
            "an empty nowcast is left out"
        );
        assert!(matches!(
            with_minutes("{".into(), Vec::new()),
            Err(WhetherError::InvalidJson { .. })
        ));
    }
}
