//! A day in a TV forecaster's words: "Cold in the morning, then sunny and
//! warm in the afternoon. Breezy." From the day's hours: the sky in the morning
//! (7–12) and afternoon (12–18), how the temperature feels then, and wind and
//! mugginess; the day's rain or snow comes from `day_precip_sentence`.

use alloc::string::String;
use alloc::vec::Vec;

use crate::condition::Condition;
use crate::model::{Daily, Hourly, WeatherSnapshot};
use crate::strings::Text;
use crate::units::Mps;
use crate::view::format::Formatter;

/// Gusts at least this strong (m/s, about 38 mph or 61 km/h) are windy.
const WINDY_GUST: f32 = 17.0;
/// Gusts (m/s, about 25 mph) or a steady wind (about 18 mph) this strong are
/// breezy.
const BREEZY_GUST: f32 = 11.0;
const BREEZY_WIND: f32 = 8.0;
/// The small hours this much warmer (°C) than the afternoon: temperatures fall
/// through the day (a cold front).
const FALLING: f32 = 6.0;
/// A dew point (°C) this high feels humid.
const HUMID_DEW_POINT: f32 = 20.0;

/// How cloudy an hour is, 0 (clear) to 2 (overcast, or falling weather).
fn cloudiness(c: Condition) -> f32 {
    match c {
        Condition::Clear => 0.0,
        Condition::MainlyClear => 0.5,
        Condition::PartlyCloudy
        | Condition::Haze
        | Condition::Smoke
        | Condition::Dust
        | Condition::Unknown => 1.0,
        Condition::Cloudy => 1.6,
        _ => 2.0,
    }
}

/// The sky over `hours`: "sunny" to "cloudy", or "foggy" when fog holds half
/// of them.
fn sky(hours: &[&Hourly]) -> Option<Text> {
    if hours.is_empty() {
        return None;
    }
    #[allow(clippy::cast_precision_loss)] // at most 24 hours
    let n = hours.len() as f32;
    #[allow(clippy::cast_precision_loss)]
    let fog = hours
        .iter()
        .filter(|h| h.condition == Condition::Fog)
        .count() as f32;
    if fog / n >= 0.5 {
        return Some(Text::SkyFoggy);
    }
    Some(sky_word(
        hours.iter().map(|h| cloudiness(h.condition)).sum::<f32>() / n,
    ))
}

/// The sky word for a mean cloudiness (0–2).
fn sky_word(mean: f32) -> Text {
    match mean {
        m if m < 0.35 => Text::SkySunny,
        m if m < 0.8 => Text::SkyMostlySunny,
        m if m < 1.3 => Text::SkyPartlyCloudy,
        m if m < 1.75 => Text::SkyMostlyCloudy,
        _ => Text::SkyCloudy,
    }
}

/// How sunny a sky word is, for telling a real change from a small one.
fn sky_rank(t: Text) -> i32 {
    match t {
        Text::SkySunny => 0,
        Text::SkyMostlySunny => 1,
        Text::SkyPartlyCloudy => 2,
        Text::SkyMostlyCloudy => 3,
        Text::SkyFoggy => 10,
        _ => 4,
    }
}

/// How a temperature (°C) feels, coldest first.
const BANDS: [(f32, Text); 8] = [
    (-12.0, Text::TempBitter),
    (0.0, Text::TempFreezing),
    (8.0, Text::TempCold),
    (15.0, Text::TempCool),
    (22.0, Text::TempMild),
    (28.0, Text::TempWarm),
    (33.0, Text::TempHot),
    (f32::INFINITY, Text::TempVeryHot),
];

fn band(t: f32) -> usize {
    BANDS
        .iter()
        .position(|(below, _)| t < *below)
        .unwrap_or(BANDS.len() - 1)
}

/// The first letter in capitals.
fn capitalized(s: &str) -> String {
    let mut chars = s.chars();
    chars.next().map_or_else(String::new, |first| {
        let mut out: String = first.to_uppercase().collect();
        out.push_str(chars.as_str());
        out
    })
}

/// "Windy, with gusts to 45 mph." or "Breezy." after `out`, from the day's
/// strongest gust and wind (m/s).
fn push_wind(f: &Formatter<'_>, out: &mut String, gust: Option<f32>, wind: Option<f32>) {
    let s = f.strings;
    if let Some(g) = gust.filter(|g| *g >= WINDY_GUST) {
        out.push(' ');
        out.push_str(&s.fmt(Text::DayWindy, &[("gust", &f.wind(Some(Mps(g))).text)]));
    } else if gust.is_some_and(|g| g >= BREEZY_GUST) || wind.is_some_and(|w| w >= BREEZY_WIND) {
        out.push(' ');
        out.push_str(s.get(Text::DayBreezy));
    }
}

/// A day beyond the hourly forecast, from its daily values: "Partly cloudy
/// and mild. Rain showers possible. Breezy."
pub(crate) fn daily_outlook(f: &Formatter<'_>, day: &Daily) -> String {
    let s = f.strings;
    let c = day.condition;
    let sky = match c {
        Condition::Clear => Text::SkySunny,
        Condition::MainlyClear => Text::SkyMostlySunny,
        Condition::PartlyCloudy => Text::SkyPartlyCloudy,
        Condition::Cloudy => Text::SkyMostlyCloudy,
        Condition::Fog => Text::SkyFoggy,
        other => sky_word(cloudiness(other)),
    };
    let main = match f.lead(day.temp_max, day.apparent_max) {
        Some(t) => s.fmt(
            Text::DayAnd,
            &[("a", s.get(sky)), ("b", s.get(BANDS[band(t.0)].1))],
        ),
        None => String::from(s.get(sky)),
    };
    let mut out = capitalized(&main);
    out.push('.');
    if c.is_precipitation() {
        let likely = day.precip_probability_max.is_none_or(|p| p.0 >= 60.0);
        out.push(' ');
        out.push_str(&s.fmt(
            if likely {
                Text::DayLikely
            } else {
                Text::DayPossible
            },
            &[("kind", s.get(c.label()))],
        ));
    }
    push_wind(
        f,
        &mut out,
        day.wind_gust_max.map(|g| g.0),
        day.wind_speed_max.map(|w| w.0),
    );
    out
}

/// The day described: sky and temperature (morning then afternoon when they
/// differ), then `precip` (when the rain or snow falls), wind and mugginess.
/// `None` without daytime hours (the evening of today).
pub(crate) fn day_outlook(
    f: &Formatter<'_>,
    snap: &WeatherSnapshot,
    hours: &[&Hourly],
    precip: Option<&str>,
) -> Option<String> {
    let s = f.strings;
    let hour_of = |h: &Hourly| h.time.to_local(snap.offset_at(h.time)).hour;
    let window = |from: u8, to: u8| -> Vec<&Hourly> {
        hours
            .iter()
            .copied()
            .filter(|h| (from..to).contains(&hour_of(h)))
            .collect()
    };
    let (morning, afternoon) = (window(7, 12), window(12, 18));
    let daytime: Vec<&Hourly> = morning.iter().chain(&afternoon).copied().collect();
    let lead = |h: &&Hourly| f.lead(h.temperature, h.apparent).map(|c| c.0);
    // The morning as it starts (its first three hours), the afternoon at its
    // warmest.
    let am_temp = {
        let t: Vec<f32> = morning.iter().take(3).filter_map(lead).collect();
        #[allow(clippy::cast_precision_loss)] // at most 3
        (!t.is_empty()).then(|| t.iter().sum::<f32>() / t.len() as f32)
    };
    let pm_temp = afternoon.iter().filter_map(lead).reduce(f32::max);
    let word = |t: Text| s.get(t);
    let and = |a: &str, b: &str| s.fmt(Text::DayAnd, &[("a", a), ("b", b)]);

    let (am_sky, pm_sky, day_sky) = (sky(&morning), sky(&afternoon), sky(&daytime));
    let main = if let (Some(am_sky), Some(pm_sky), Some(am_t), Some(pm_t)) =
        (am_sky, pm_sky, am_temp.map(band), pm_temp.map(band))
    {
        let sky_shift = (sky_rank(am_sky) - sky_rank(pm_sky)).abs() >= 2;
        let temp_shift = am_t.abs_diff(pm_t) >= 2;
        let pm = and(word(pm_sky), word(BANDS[pm_t].1));
        if sky_shift || temp_shift {
            let am = match (sky_shift, temp_shift) {
                (true, true) => and(word(am_sky), word(BANDS[am_t].1)),
                (true, false) => String::from(word(am_sky)),
                _ => String::from(word(BANDS[am_t].1)),
            };
            s.fmt(Text::DayMorningThen, &[("am", &am), ("pm", &pm)])
        } else {
            and(word(day_sky.unwrap_or(pm_sky)), word(BANDS[pm_t].1))
        }
    } else {
        // Only part of the day ahead (today after noon), or no temperatures.
        let sky = day_sky?;
        match pm_temp.or(am_temp) {
            Some(t) => and(word(sky), word(BANDS[band(t)].1)),
            None => String::from(word(sky)),
        }
    };
    let mut out = capitalized(&main);
    out.push('.');
    let night: Vec<f32> = window(0, 7).iter().filter_map(lead).collect();
    #[allow(clippy::cast_precision_loss)] // at most 7
    let night = (!night.is_empty()).then(|| night.iter().sum::<f32>() / night.len() as f32);
    if let (Some(night), Some(pm)) = (night, pm_temp)
        && night - pm >= FALLING
    {
        out.push(' ');
        out.push_str(s.get(Text::DayFalling));
    }
    if let Some(precip) = precip {
        out.push(' ');
        out.push_str(precip);
    }
    let gust = hours
        .iter()
        .filter_map(|h| h.wind_gust.map(|g| g.0))
        .reduce(f32::max);
    let wind = hours
        .iter()
        .filter_map(|h| h.wind_speed.map(|w| w.0))
        .reduce(f32::max);
    push_wind(f, &mut out, gust, wind);
    let humid = afternoon.iter().any(|h| {
        h.temperature
            .zip(h.humidity)
            .and_then(|(t, rh)| crate::derive::dew_point(t, rh))
            .is_some_and(|d| d.0 >= HUMID_DEW_POINT)
    });
    if humid {
        out.push(' ');
        out.push_str(s.get(Text::DayHumid));
    }
    Some(out)
}
