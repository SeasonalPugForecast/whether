//! The raw gridpoint forecast (`/gridpoints/OKX/33,42`): the forecasters' layers
//! as ISO 8601 intervals (`2026-10-17T04:00:00+00:00/PT8H`) in WMO units,
//! expanded to hours.

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use serde::Deserialize;
use whether_core::builder::SnapshotBuilder;
use whether_core::condition::{Condition, Intensity};
use whether_core::model::{Current, Daily, Hourly};
use whether_core::sun::{SunDay, sun_day};
use whether_core::time::{LocalDate, UnixTime, UtcOffset, parse_rfc3339};
use whether_core::units::{Celsius, Degrees, Hpa, Meters, Mm, Mps, Percent, finite};

use crate::aggregate::{Step, daily_from_steps};
use crate::error::ProviderError;
use crate::http::FetchContext;

const HOUR: i64 = 3_600;
/// The longest forecast taken, in hours (the NWS forecasts 7 days).
const MAX_HOURS: i64 = 10 * 24;
/// A forecast last updated longer ago than this is stale.
pub(super) const STALE_AFTER_S: i64 = 12 * HOUR;

#[derive(Deserialize)]
struct Gridpoint {
    properties: Layers,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Layers {
    update_time: Option<String>,
    #[serde(default)]
    temperature: Layer,
    #[serde(default)]
    apparent_temperature: Layer,
    #[serde(default)]
    dewpoint: Layer,
    #[serde(default)]
    relative_humidity: Layer,
    #[serde(default)]
    sky_cover: Layer,
    #[serde(default)]
    wind_speed: Layer,
    #[serde(default)]
    wind_gust: Layer,
    #[serde(default)]
    wind_direction: Layer,
    #[serde(default)]
    probability_of_precipitation: Layer,
    #[serde(default)]
    quantitative_precipitation: Layer,
    #[serde(default)]
    visibility: Layer,
    #[serde(default)]
    pressure: Layer,
    #[serde(default)]
    max_temperature: Layer,
    #[serde(default)]
    min_temperature: Layer,
    #[serde(default)]
    weather: WeatherLayer,
}

#[derive(Default, Deserialize)]
struct Layer {
    uom: Option<String>,
    #[serde(default)]
    values: Vec<Entry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    valid_time: String,
    value: Option<f64>,
}

#[derive(Default, Deserialize)]
struct WeatherLayer {
    #[serde(default)]
    values: Vec<WeatherEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WeatherEntry {
    valid_time: String,
    #[serde(default)]
    value: Option<Vec<Phenomenon>>,
}

/// One weather phenomenon of a period: `chance` of `light` `rain_showers`.
#[derive(Deserialize)]
struct Phenomenon {
    coverage: Option<String>,
    weather: Option<String>,
    intensity: Option<String>,
    #[serde(default)]
    attributes: Vec<String>,
}

/// An ISO 8601 duration of days, hours, minutes and seconds (`P7DT12H`,
/// `PT1H`), in seconds; years, months and weeks are not used by the API.
fn duration_secs(d: &str) -> Option<i64> {
    let rest = d.strip_prefix('P')?;
    let (date, time) = rest.split_once('T').unwrap_or((rest, ""));
    let mut total = 0_i64;
    for (part, units) in [
        (date, &[('D', 86_400)][..]),
        (time, &[('H', HOUR), ('M', 60), ('S', 1)][..]),
    ] {
        let mut number = String::new();
        for c in part.chars() {
            if c.is_ascii_digit() {
                number.push(c);
                continue;
            }
            let (_, scale) = units.iter().find(|(u, _)| *u == c)?;
            total = total.checked_add(number.parse::<i64>().ok()?.checked_mul(*scale)?)?;
            number.clear();
        }
        if !number.is_empty() {
            return None;
        }
    }
    Some(total)
}

/// `start/duration` as `(start, end)` Unix seconds.
fn interval(s: &str) -> Option<(i64, i64)> {
    let (start, duration) = s.split_once('/')?;
    let start = parse_rfc3339(start)?.0;
    let secs = duration_secs(duration)?;
    (secs > 0).then_some((start, start + secs))
}

/// A value in `uom` (`wmoUnit:km_h-1`) in canonical units.
pub(super) fn canonical(uom: Option<&str>, v: f64) -> Option<f32> {
    let unit = uom.unwrap_or("").trim_start_matches("wmoUnit:");
    let v = match unit {
        "degC" | "percent" | "degree_(angle)" | "mm" | "m" | "hPa" | "m_s-1" => v,
        "degF" => (v - 32.0) * 5.0 / 9.0,
        "km_h-1" => v / 3.6,
        "km" => v * 1_000.0,
        "Pa" => v / 100.0,
        _ => return None,
    };
    #[allow(clippy::cast_possible_truncation)] // weather values are small
    finite(v as f32)
}

/// The hours the forecast covers: from its first temperature's hour on.
struct Hours {
    start: i64,
    count: usize,
}

impl Hours {
    fn of(layer: &Layer) -> Option<Self> {
        let spans: Vec<(i64, i64)> = layer
            .values
            .iter()
            .filter_map(|e| interval(&e.valid_time))
            .collect();
        let start = spans.iter().map(|(s, _)| *s).min()?.div_euclid(HOUR) * HOUR;
        let end = spans
            .iter()
            .map(|(_, e)| *e)
            .max()?
            .min(start + MAX_HOURS * HOUR);
        let count = usize::try_from((end - start + HOUR - 1) / HOUR).ok()?;
        Some(Self { start, count })
    }

    fn time(&self, i: usize) -> i64 {
        self.start + i64::try_from(i).unwrap_or(i64::MAX / HOUR) * HOUR
    }

    /// The indices of the hours that start within `[from, to)`.
    fn within(&self, from: i64, to: i64) -> impl Iterator<Item = usize> + '_ {
        (0..self.count).filter(move |&i| (from..to).contains(&self.time(i)))
    }

    /// `layer` hour by hour. An amount (precipitation) is spread evenly over its
    /// interval's hours; any other value holds for each of them.
    fn spread(&self, layer: &Layer, amount: bool) -> Vec<Option<f32>> {
        let mut out = vec![None; self.count];
        for e in &layer.values {
            let (Some((from, to)), Some(v)) = (interval(&e.valid_time), e.value) else {
                continue;
            };
            let Some(v) = canonical(layer.uom.as_deref(), v) else {
                continue;
            };
            #[allow(clippy::cast_precision_loss)] // a few hundred hours
            let v = if amount {
                v / ((to - from) as f32 / HOUR as f32).max(1.0)
            } else {
                v
            };
            for i in self.within(from, to) {
                out[i] = Some(v);
            }
        }
        out
    }

    /// Each hour's weather: what is likely or certain, and what is only a
    /// chance (see [`hour_condition`]).
    fn weather(&self, layer: &WeatherLayer) -> Vec<Weather> {
        let mut out = vec![Weather::default(); self.count];
        for e in &layer.values {
            let Some((from, to)) = interval(&e.valid_time) else {
                continue;
            };
            let list = e.value.as_deref().unwrap_or_default();
            let weather = Weather {
                firm: weather_condition(list, Coverage::Firm),
                chance: weather_condition(list, Coverage::Chance),
            };
            for i in self.within(from, to) {
                out[i] = weather;
            }
        }
        out
    }
}

/// The condition of a phenomenon, whatever its coverage.
fn phenomenon(p: &Phenomenon) -> Option<Condition> {
    use Condition as C;
    let i = match p.intensity.as_deref() {
        Some("very_light" | "light") => Intensity::Light,
        Some("heavy") => Intensity::Heavy,
        _ => Intensity::Moderate,
    };
    let hail = p.attributes.iter().any(|a| a.contains("hail"));
    Some(match p.weather.as_deref()? {
        "rain" => C::Rain(i),
        "rain_showers" => C::RainShowers(i),
        "drizzle" => C::Drizzle(i),
        "thunderstorms" if hail => C::ThunderstormHail(i),
        "thunderstorms" => C::Thunderstorm(i),
        "hail" => C::ThunderstormHail(i),
        "snow" | "blowing_snow" => C::Snow(i),
        "snow_showers" => C::SnowShowers(i),
        "flurries" => C::SnowShowers(Intensity::Light),
        "sleet" => C::Sleet(i),
        "freezing_rain" => C::FreezingRain(i),
        "freezing_drizzle" => C::FreezingDrizzle(i),
        "ice_crystals" => C::SnowGrains,
        "fog" | "freezing_fog" | "ice_fog" => C::Fog,
        "haze" => C::Haze,
        "smoke" => C::Smoke,
        "dust" | "blowing_dust" | "blowing_sand" => C::Dust,
        _ => return None,
    })
}

/// How sure a phenomenon is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Coverage {
    /// "likely", "definite", "numerous", "areas of" fog, … (and unstated).
    Firm,
    /// "chance" or "scattered": 30–50 %.
    Chance,
}

fn coverage(p: &Phenomenon) -> Option<Coverage> {
    match p.coverage.as_deref() {
        // 10–20 %: never the hour's weather.
        Some("slight_chance" | "isolated") => None,
        Some("chance" | "scattered") => Some(Coverage::Chance),
        _ => Some(Coverage::Firm),
    }
}

/// The most severe phenomenon of `coverage` in a period; `None` when there is none.
fn weather_condition(list: &[Phenomenon], of: Coverage) -> Option<Condition> {
    list.iter()
        .filter(|p| coverage(p) == Some(of))
        .filter_map(phenomenon)
        .max_by_key(|c| c.severity_rank())
}

/// An hour's weather by how sure it is.
#[derive(Clone, Copy, Default)]
struct Weather {
    firm: Option<Condition>,
    chance: Option<Condition>,
}

/// The chance of precipitation from which a "chance" of rain is the hour's
/// weather, as Open-Meteo's codes would show it: "it may rain at 3 AM" must not
/// vanish from the day.
const LIKELY_PERCENT: f32 = 50.0;

/// An hour's condition: its likely weather; a chance of it when the hour's
/// probability is at least [`LIKELY_PERCENT`]; else the sky cover.
fn hour_condition(weather: Weather, chance: Option<f32>, cover: Option<f32>) -> Condition {
    weather
        .firm
        .or(weather
            .chance
            .filter(|_| chance.is_some_and(|p| p >= LIKELY_PERCENT)))
        .unwrap_or_else(|| sky(cover))
}

/// The sky at the NWS's sky-condition limits (eighths: 0–1 clear, 2–3 mostly
/// clear, 4–5 partly cloudy, 6–7 mostly cloudy, 8 overcast).
fn sky(cover: Option<f32>) -> Condition {
    match cover {
        None => Condition::Unknown,
        Some(c) if c <= 12.5 => Condition::Clear,
        Some(c) if c <= 37.5 => Condition::MainlyClear,
        Some(c) if c <= 62.5 => Condition::PartlyCloudy,
        Some(c) if c <= 87.5 => Condition::Cloudy,
        Some(_) => Condition::Overcast,
    }
}

/// The sun at the place, one date at a time.
struct Sun {
    latitude: f64,
    longitude: f64,
    offset: UtcOffset,
    cache: Vec<(LocalDate, SunDay)>,
}

impl Sun {
    fn day(&mut self, date: LocalDate) -> SunDay {
        if let Some((_, day)) = self.cache.iter().find(|(d, _)| *d == date) {
            return *day;
        }
        let day = sun_day(date, self.latitude, self.longitude);
        self.cache.push((date, day));
        day
    }

    /// Whether the sun is up for most of the hour starting at `t`.
    fn is_day(&mut self, t: UnixTime) -> bool {
        let middle = UnixTime(t.0 + HOUR / 2);
        self.day(middle.local_date(self.offset)).is_up(middle)
    }
}

/// One day per local date of `hourly`, with the forecasters' highs and lows and
/// the sun's times.
fn days(hourly: &[Hourly], p: &Layers, sun: &mut Sun) -> Vec<Daily> {
    let offset = sun.offset;
    let steps: Vec<Step> = hourly
        .iter()
        .map(|h| Step {
            hourly: h.clone(),
            temp_min: None,
            temp_max: None,
        })
        .collect();
    let mut daily = daily_from_steps(&steps, offset, None);
    // The forecasters' highs (daytime) and lows (overnight), on the local date of
    // each period's middle: a low from 8 PM to 9 AM belongs to the morning.
    for (layer, high) in [(&p.max_temperature, true), (&p.min_temperature, false)] {
        for e in &layer.values {
            let (Some((from, to)), Some(v)) = (interval(&e.valid_time), e.value) else {
                continue;
            };
            let Some(v) = canonical(layer.uom.as_deref(), v) else {
                continue;
            };
            let date = UnixTime(from + (to - from) / 2).local_date(offset);
            if let Some(day) = daily.iter_mut().find(|d| d.date == date) {
                if high {
                    day.temp_max = Some(Celsius(v));
                } else {
                    day.temp_min = Some(Celsius(v));
                }
            }
        }
    }
    for day in &mut daily {
        if let SunDay::RiseSet { rise, set } = sun.day(day.date) {
            day.sunrise = Some(rise);
            day.sunset = Some(set);
            day.daylight_s = u32::try_from(set.0 - rise.0).ok();
        }
    }
    daily
}

/// Parses a gridpoint forecast into hours, days and the current hour.
pub(super) fn parse(
    body: &[u8],
    ctx: &FetchContext,
    out: &mut SnapshotBuilder,
) -> Result<(), ProviderError> {
    let grid: Gridpoint =
        serde_json::from_slice(body).map_err(|e| ProviderError::Malformed(e.to_string()))?;
    let p = grid.properties;
    if let Some(updated) = p.update_time.as_deref().and_then(parse_rfc3339)
        && ctx.now.0 - updated.0 > STALE_AFTER_S
    {
        return Err(ProviderError::Malformed(alloc::format!(
            "the forecast was last updated {} hours ago",
            (ctx.now.0 - updated.0) / HOUR
        )));
    }
    let hours = Hours::of(&p.temperature)
        .ok_or_else(|| ProviderError::Malformed("the forecast has no temperatures".into()))?;
    let location = out
        .location()
        .cloned()
        .ok_or_else(|| ProviderError::Malformed("the point lookup came first".into()))?;
    let offset = location.utc_offset;
    let mut sun = Sun {
        latitude: location.latitude,
        longitude: location.longitude,
        offset,
        cache: Vec::new(),
    };

    let temperature = hours.spread(&p.temperature, false);
    let apparent = hours.spread(&p.apparent_temperature, false);
    let dew_point = hours.spread(&p.dewpoint, false);
    let humidity = hours.spread(&p.relative_humidity, false);
    let cover = hours.spread(&p.sky_cover, false);
    let wind = hours.spread(&p.wind_speed, false);
    let gust = hours.spread(&p.wind_gust, false);
    let direction = hours.spread(&p.wind_direction, false);
    let chance = hours.spread(&p.probability_of_precipitation, false);
    let rain = hours.spread(&p.quantitative_precipitation, true);
    let visibility = hours.spread(&p.visibility, false);
    let pressure = hours.spread(&p.pressure, false);
    let weather = hours.weather(&p.weather);

    let mut hourly = Vec::new();
    let mut current = None;
    for i in 0..hours.count {
        let Some(t) = temperature[i] else { continue };
        let time = UnixTime(hours.time(i));
        let hour = Hourly {
            time,
            is_day: sun.is_day(time),
            condition: hour_condition(weather[i], chance[i], cover[i]),
            temperature: Some(Celsius(t)),
            apparent: apparent[i].map(Celsius),
            precip_probability: chance[i].map(Percent),
            precipitation: rain[i].map(Mm),
            wind_speed: wind[i].map(Mps),
            wind_gust: gust[i].map(Mps),
            wind_dir: direction[i].map(Degrees),
            humidity: humidity[i].map(Percent),
            pressure_msl: pressure[i].map(Hpa),
            uv_index: None,
            visibility: visibility[i].map(Meters),
            cloud_cover: cover[i].map(Percent),
        };
        if (time.0..time.0 + HOUR).contains(&ctx.now.0) {
            current = Some(Current {
                time,
                is_day: hour.is_day,
                condition: hour.condition,
                temperature: hour.temperature,
                apparent: hour.apparent,
                dew_point: dew_point[i].map(Celsius),
                humidity: hour.humidity,
                pressure_msl: hour.pressure_msl,
                cloud_cover: hour.cloud_cover,
                wind_speed: hour.wind_speed,
                wind_gust: hour.wind_gust,
                wind_dir: hour.wind_dir,
                visibility: hour.visibility,
                uv_index: None,
                precipitation: None,
            });
        }
        hourly.push(hour);
    }

    let daily = days(&hourly, &p, &mut sun);
    out.set_hourly(hourly);
    out.set_daily(daily);
    if let Some(current) = current {
        out.set_current(current);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_and_intervals() {
        assert_eq!(duration_secs("PT1H"), Some(3_600));
        assert_eq!(duration_secs("P7DT12H"), Some(7 * 86_400 + 12 * 3_600));
        assert_eq!(duration_secs("P1D"), Some(86_400));
        assert_eq!(duration_secs("PT30M"), Some(1_800));
        assert_eq!(duration_secs("P1M"), None, "months are not used");
        assert_eq!(duration_secs("PT1"), None);
        assert_eq!(duration_secs("1H"), None);
        assert_eq!(
            interval("2026-10-17T04:00:00+00:00/PT8H"),
            Some((1_792_209_600, 1_792_209_600 + 8 * 3_600))
        );
    }

    #[test]
    fn units_become_canonical() {
        assert_eq!(canonical(Some("wmoUnit:km_h-1"), 36.0), Some(10.0));
        assert_eq!(canonical(Some("wmoUnit:degF"), 212.0), Some(100.0));
        assert_eq!(canonical(Some("wmoUnit:Pa"), 101_325.0), Some(1_013.25));
        assert_eq!(canonical(Some("wmoUnit:km"), 1.5), Some(1_500.0));
        assert_eq!(canonical(Some("wmoUnit:furlong"), 1.0), None);
    }

    fn phenomena(json: &str) -> Vec<Phenomenon> {
        serde_json::from_str(json).unwrap()
    }

    fn weather_of(json: &str) -> Weather {
        let list = phenomena(json);
        Weather {
            firm: weather_condition(&list, Coverage::Firm),
            chance: weather_condition(&list, Coverage::Chance),
        }
    }

    #[test]
    fn likely_rain_is_rain_and_a_chance_counts_when_probable() {
        let showers = weather_of(
            r#"[{"coverage":"chance","weather":"rain_showers","intensity":"light","attributes":[]}]"#,
        );
        let light_showers = Condition::RainShowers(Intensity::Light);
        // A 30 % chance of showers at 3 AM is the sky; a 50 % chance shows.
        assert_eq!(
            hour_condition(showers, Some(30.0), Some(90.0)),
            Condition::Overcast
        );
        assert_eq!(
            hour_condition(showers, Some(50.0), Some(90.0)),
            light_showers
        );
        assert_eq!(
            hour_condition(showers, None, Some(90.0)),
            Condition::Overcast
        );
        let slight = weather_of(
            r#"[{"coverage":"slight_chance","weather":"rain","intensity":null,"attributes":[]}]"#,
        );
        assert_eq!(
            hour_condition(slight, Some(80.0), Some(10.0)),
            Condition::Clear
        );
        let likely = weather_of(
            r#"[{"coverage":"likely","weather":"rain","intensity":null,"attributes":[]},
                {"coverage":"chance","weather":"thunderstorms","intensity":null,"attributes":[]}]"#,
        );
        assert_eq!(
            hour_condition(likely, Some(10.0), None),
            Condition::Rain(Intensity::Moderate)
        );
        let hail = weather_of(
            r#"[{"coverage":"scattered","weather":"thunderstorms","intensity":"heavy","attributes":["small_hail"]}]"#,
        );
        assert_eq!(
            hour_condition(hail, Some(55.0), None),
            Condition::ThunderstormHail(Intensity::Heavy)
        );
        assert_eq!(sky(Some(85.0)), Condition::Cloudy);
        assert_eq!(sky(Some(10.0)), Condition::Clear);
        assert_eq!(sky(Some(100.0)), Condition::Overcast);
    }
}
