//! Daily values from forecast steps (OpenWeather's 3-hour steps, MET Norway's
//! 1- and 6-hour periods), grouped by local date.

use alloc::vec::Vec;

use whether_core::model::{Daily, Hourly};
use whether_core::time::{UnixTime, UtcOffset};
use whether_core::units::{Celsius, Mm, Percent};

/// One forecast step with the extra fields the daily aggregation needs.
pub(crate) struct Step {
    pub(crate) hourly: Hourly,
    pub(crate) temp_min: Option<f32>,
    pub(crate) temp_max: Option<f32>,
}

/// One `Daily` per local date covered by `steps`. The condition is the most
/// significant among daytime steps (or all steps when none is daytime).
/// `sun` (today's sunrise and sunset) is applied to its own date.
pub(crate) fn daily_from_steps(
    steps: &[Step],
    offset: UtcOffset,
    sun: Option<(UnixTime, UnixTime)>,
) -> Vec<Daily> {
    let mut days: Vec<Daily> = Vec::new();
    let mut groups: Vec<Vec<&Step>> = Vec::new();
    for step in steps {
        let date = step.hourly.time.local_date(offset);
        match days.last() {
            Some(d) if d.date == date => {}
            _ => {
                days.push(Daily {
                    date,
                    ..Daily::default()
                });
                groups.push(Vec::new());
            }
        }
        if let Some(group) = groups.last_mut() {
            group.push(step);
        }
    }
    for (day, group) in days.iter_mut().zip(&groups) {
        let fold_max = |f: &dyn Fn(&Step) -> Option<f32>| {
            group
                .iter()
                .filter_map(|s| f(s))
                .fold(None, |m: Option<f32>, v| Some(m.map_or(v, |m| m.max(v))))
        };
        let fold_min = |f: &dyn Fn(&Step) -> Option<f32>| {
            group
                .iter()
                .filter_map(|s| f(s))
                .fold(None, |m: Option<f32>, v| Some(m.map_or(v, |m| m.min(v))))
        };
        day.temp_max = fold_max(&|s| s.temp_max.or(s.hourly.temperature.map(|c| c.0))).map(Celsius);
        day.temp_min = fold_min(&|s| s.temp_min.or(s.hourly.temperature.map(|c| c.0))).map(Celsius);
        day.precip_probability_max =
            fold_max(&|s| s.hourly.precip_probability.map(|p| p.0)).map(Percent);
        day.wind_speed_max =
            fold_max(&|s| s.hourly.wind_speed.map(|w| w.0)).map(whether_core::units::Mps);
        day.wind_gust_max =
            fold_max(&|s| s.hourly.wind_gust.map(|w| w.0)).map(whether_core::units::Mps);
        let amounts: Vec<f32> = group
            .iter()
            .filter_map(|s| s.hourly.precipitation.map(|p| p.0))
            .collect();
        day.precip_sum = (!amounts.is_empty()).then(|| Mm(amounts.iter().sum()));
        let daytime: Vec<&&Step> = group.iter().filter(|s| s.hourly.is_day).collect();
        let pool: Vec<&&Step> = if daytime.is_empty() {
            group.iter().collect()
        } else {
            daytime
        };
        if let Some(dominant) = pool
            .iter()
            .max_by_key(|s| s.hourly.condition.severity_rank())
        {
            day.condition = dominant.hourly.condition;
        }
        day.wind_dir_dominant = group
            .iter()
            .filter(|s| s.hourly.wind_dir.is_some())
            .max_by(|a, b| {
                let speed = |s: &Step| s.hourly.wind_speed.map_or(0.0, |w| w.0);
                speed(a).total_cmp(&speed(b))
            })
            .and_then(|s| s.hourly.wind_dir);
        if let Some((sunrise, sunset)) = sun
            && sunrise.local_date(offset) == day.date
        {
            day.sunrise = Some(sunrise);
            day.sunset = Some(sunset);
            day.daylight_s = u32::try_from(sunset.0 - sunrise.0).ok();
        }
    }
    days
}
