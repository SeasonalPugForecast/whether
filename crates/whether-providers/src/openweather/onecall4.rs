//! One Call 4.0 (`/data/4.0/onecall/…`, "One Call by Call" subscription): current
//! conditions plus 15-minute, hourly and daily timelines. Every request is one
//! metered call, so the planner asks only for what the displays need and the
//! session sends each request only within the daily budget.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use serde::Deserialize;
use whether_core::builder::SnapshotBuilder;
use whether_core::model::{Alert, Current, Daily, Hourly, Minutely, ResolvedLocation};
use whether_core::requirements::DataRequirements;
use whether_core::scales::Level;
use whether_core::time::{LocalDate, UnixTime, UtcOffset};
use whether_core::units::{Celsius, Degrees, Hpa, Meters, Mm, Mps, Percent, finite};

use super::codes::from_owm_id;
use crate::error::ProviderError;
use crate::http::FetchContext;

/// Hourly records per page of `/timeline/1h`.
pub const HOURS_PER_PAGE: u8 = 20;
/// Hourly pages at most (48 hours).
pub const MAX_HOURLY_PAGES: u8 = 3;
/// 15-minute steps kept for the nowcast (the longest, eight hours; the view
/// shows the hours it is set to).
const NOWCAST_STEPS: usize = whether_core::requirements::MAX_NOWCAST_HOURS as usize * 4;

/// Pages of `/timeline/1h` needed for `req` (0 when no hourly data is needed).
pub fn hourly_pages(req: &DataRequirements) -> u8 {
    if req.hourly.is_empty() || req.hourly_hours == 0 {
        return 0;
    }
    req.hourly_hours
        .div_ceil(HOURS_PER_PAGE)
        .min(MAX_HOURLY_PAGES)
}

/// Longest alert id sent back to the API; longer ids are ignored.
pub const MAX_ALERT_ID_LEN: usize = 256;

/// Metered calls one fetch makes for `req`: current, hourly pages, daily and nowcast,
/// plus one call reserved for alert details when alerts are shown.
pub fn calls_per_fetch(req: &DataRequirements) -> u32 {
    1 + u32::from(hourly_pages(req))
        + u32::from(!req.daily.is_empty())
        + u32::from(req.minutely_hours > 0)
        + u32::from(req.alerts)
}

/// The start of the hour containing `now`, where hourly page 0 begins.
pub fn hour_start(now: UnixTime) -> UnixTime {
    UnixTime(now.0 - now.0.rem_euclid(3600))
}

#[derive(Deserialize)]
struct Weather {
    id: u16,
    #[serde(default)]
    icon: String,
}

/// Rain or snow as a rate in mm/h.
#[derive(Deserialize, Default)]
struct Rate {
    #[serde(rename = "1h")]
    one_hour: Option<f32>,
}

/// One record of `current` or a 15-minute or hourly timeline.
#[derive(Deserialize)]
struct Record {
    dt: i64,
    sunrise: Option<i64>,
    sunset: Option<i64>,
    temp: Option<f32>,
    feels_like: Option<f32>,
    pressure: Option<f32>,
    humidity: Option<f32>,
    dew_point: Option<f32>,
    uvi: Option<f32>,
    clouds: Option<f32>,
    visibility: Option<f32>,
    wind_speed: Option<f32>,
    wind_deg: Option<f32>,
    wind_gust: Option<f32>,
    pop: Option<f32>,
    #[serde(default)]
    rain: Option<Rate>,
    #[serde(default)]
    snow: Option<Rate>,
    #[serde(default)]
    weather: Vec<Weather>,
    /// Ids of the alerts in effect.
    #[serde(default)]
    alerts: Vec<String>,
}

impl Record {
    /// Rain plus snow in mm/h, if either is reported.
    fn precipitation_rate(&self) -> Option<f32> {
        let rain = self.rain.as_ref().and_then(|r| r.one_hour).and_then(finite);
        let snow = self.snow.as_ref().and_then(|r| r.one_hour).and_then(finite);
        match (rain, snow) {
            (None, None) => None,
            (r, s) => Some(r.unwrap_or(0.0) + s.unwrap_or(0.0)),
        }
    }

    fn condition(&self) -> (whether_core::condition::Condition, bool) {
        match self.weather.first() {
            Some(w) => (from_owm_id(w.id), !w.icon.ends_with('n')),
            None => (whether_core::condition::Condition::Unknown, true),
        }
    }
}

#[derive(Deserialize)]
struct DailyTemp {
    min: Option<f32>,
    max: Option<f32>,
}

#[derive(Deserialize)]
struct DailyRecord {
    dt: i64,
    sunrise: Option<i64>,
    sunset: Option<i64>,
    moonrise: Option<i64>,
    moonset: Option<i64>,
    moon_phase: Option<f32>,
    temp: Option<DailyTemp>,
    wind_speed: Option<f32>,
    wind_deg: Option<f32>,
    wind_gust: Option<f32>,
    pop: Option<f32>,
    uvi: Option<f32>,
    #[serde(default)]
    weather: Vec<Weather>,
    /// Ids of the alerts in effect that day.
    #[serde(default)]
    alerts: Vec<String>,
}

/// `/alert/{id}`: one alert's details.
#[derive(Deserialize)]
struct AlertRecord {
    id: String,
    #[serde(default)]
    sender_name: String,
    #[serde(default)]
    event: String,
    start: i64,
    end: i64,
    #[serde(default)]
    description: Vec<AlertText>,
    #[serde(default)]
    tags: Vec<String>,
}

/// One language's text of an alert.
#[derive(Deserialize)]
struct AlertText {
    #[serde(default)]
    language: String,
    #[serde(default)]
    description: String,
}

/// The envelope every One Call 4.0 response shares.
#[derive(Deserialize)]
struct Envelope<T> {
    lat: f64,
    lon: f64,
    timezone: Option<String>,
    timezone_offset: i32,
    data: Vec<T>,
}

fn decode<'a, T: Deserialize<'a>>(body: &'a [u8]) -> Result<Envelope<T>, ProviderError> {
    serde_json::from_slice(body).map_err(|e| ProviderError::Malformed(e.to_string()))
}

fn percent_from_fraction(pop: Option<f32>) -> Option<Percent> {
    pop.and_then(finite)
        .map(|p| Percent((p * 100.0).clamp(0.0, 100.0)))
}

/// Notes the alert ids that `records` name, for the session to follow.
fn refer_alerts<'a>(ids: impl Iterator<Item = &'a String>, out: &mut SnapshotBuilder) {
    for id in ids {
        out.refer_alert(id);
    }
}

/// Sets the location from any response that has not set it yet.
fn locate<T>(envelope: &Envelope<T>, out: &mut SnapshotBuilder) {
    if out.location().is_none() {
        out.set_location(ResolvedLocation {
            name: String::new(),
            region: None,
            latitude: envelope.lat,
            longitude: envelope.lon,
            elevation_m: None,
            utc_offset: UtcOffset(envelope.timezone_offset),
            tz_name: envelope.timezone.clone(),
            tz_abbrev: None,
            station: None,
        });
    }
}

/// `/current`: current conditions and today's sunrise and sunset.
pub(super) fn parse_current(body: &[u8], out: &mut SnapshotBuilder) -> Result<(), ProviderError> {
    let envelope: Envelope<Record> = decode(body)?;
    locate(&envelope, out);
    let r = envelope
        .data
        .first()
        .ok_or_else(|| ProviderError::Malformed("current has no data".to_string()))?;
    refer_alerts(r.alerts.iter(), out);
    let (condition, is_day) = r.condition();
    out.set_current(Current {
        time: UnixTime(r.dt),
        is_day: match (r.sunrise, r.sunset) {
            (Some(rise), Some(set)) => (rise..set).contains(&r.dt),
            _ => is_day,
        },
        condition,
        temperature: r.temp.and_then(finite).map(Celsius),
        apparent: r.feels_like.and_then(finite).map(Celsius),
        dew_point: r.dew_point.and_then(finite).map(Celsius),
        humidity: r.humidity.and_then(finite).map(Percent),
        pressure_msl: r.pressure.and_then(finite).map(Hpa),
        cloud_cover: r.clouds.and_then(finite).map(Percent),
        wind_speed: r.wind_speed.and_then(finite).map(Mps),
        wind_gust: r.wind_gust.and_then(finite).map(Mps),
        wind_dir: r.wind_deg.and_then(finite).map(Degrees),
        visibility: r.visibility.and_then(finite).map(Meters),
        uv_index: r.uvi.and_then(finite),
        precipitation: r.precipitation_rate().map(Mm),
    });
    Ok(())
}

/// One page of `/timeline/1h`.
pub(super) fn parse_hourly(body: &[u8], out: &mut SnapshotBuilder) -> Result<(), ProviderError> {
    let envelope: Envelope<Record> = decode(body)?;
    locate(&envelope, out);
    refer_alerts(envelope.data.iter().flat_map(|r| &r.alerts), out);
    for r in &envelope.data {
        let (condition, is_day) = r.condition();
        out.push_hourly(Hourly {
            time: UnixTime(r.dt),
            is_day,
            condition,
            temperature: r.temp.and_then(finite).map(Celsius),
            apparent: r.feels_like.and_then(finite).map(Celsius),
            precip_probability: percent_from_fraction(r.pop),
            // A rate in mm/h is the amount for the hour.
            precipitation: r.precipitation_rate().map(Mm),
            wind_speed: r.wind_speed.and_then(finite).map(Mps),
            wind_gust: r.wind_gust.and_then(finite).map(Mps),
            wind_dir: r.wind_deg.and_then(finite).map(Degrees),
            humidity: r.humidity.and_then(finite).map(Percent),
            pressure_msl: r.pressure.and_then(finite).map(Hpa),
            uv_index: r.uvi.and_then(finite),
            visibility: r.visibility.and_then(finite).map(Meters),
            cloud_cover: r.clouds.and_then(finite).map(Percent),
        });
    }
    Ok(())
}

/// `/timeline/15min`: the first eight hours as 15-minute precipitation amounts.
pub(super) fn parse_nowcast(body: &[u8], out: &mut SnapshotBuilder) -> Result<(), ProviderError> {
    let envelope: Envelope<Record> = decode(body)?;
    locate(&envelope, out);
    refer_alerts(envelope.data.iter().flat_map(|r| &r.alerts), out);
    out.set_minutely(
        envelope
            .data
            .iter()
            .take(NOWCAST_STEPS)
            .map(|r| Minutely {
                time: UnixTime(r.dt),
                // mm/h over a quarter hour.
                precipitation: r.precipitation_rate().map(|rate| Mm(rate / 4.0)),
            })
            .collect(),
    );
    Ok(())
}

/// The calendar date a `/timeline/1day` record covers. The API stamps each day at
/// 00:00 UTC of the location's local date (recorded 2026-10-08 for 10001: `dt`
/// 2026-10-08T00:00Z, sunrise 07:05 EDT the same day), so reading `dt` at the
/// local offset would land a day early west of Greenwich. A `dt` that is not on a
/// UTC midnight is read as a local time.
fn day_of(dt: i64, offset: UtcOffset) -> LocalDate {
    if dt.rem_euclid(86_400) == 0 {
        UnixTime(dt).local_date(UtcOffset(0))
    } else {
        UnixTime(dt).local_date(offset)
    }
}

/// `/timeline/1day`. Daily precipitation is left to the supplement, because the
/// 4.0 daily records report only an hourly rate.
pub(super) fn parse_daily(body: &[u8], out: &mut SnapshotBuilder) -> Result<(), ProviderError> {
    let envelope: Envelope<DailyRecord> = decode(body)?;
    locate(&envelope, out);
    refer_alerts(envelope.data.iter().flat_map(|r| &r.alerts), out);
    let offset = UtcOffset(envelope.timezone_offset);
    out.set_daily(
        envelope
            .data
            .iter()
            .map(|d| {
                let condition = d
                    .weather
                    .first()
                    .map_or(whether_core::condition::Condition::Unknown, |w| {
                        from_owm_id(w.id)
                    });
                Daily {
                    date: day_of(d.dt, offset),
                    condition,
                    temp_max: d
                        .temp
                        .as_ref()
                        .and_then(|t| t.max)
                        .and_then(finite)
                        .map(Celsius),
                    temp_min: d
                        .temp
                        .as_ref()
                        .and_then(|t| t.min)
                        .and_then(finite)
                        .map(Celsius),
                    // Morning, day, evening and night "feels like" values, no
                    // extremes: they come from the hours (fill_derived) or the
                    // supplement.
                    apparent_max: None,
                    apparent_min: None,
                    precip_sum: None,
                    precip_probability_max: percent_from_fraction(d.pop),
                    sunrise: d.sunrise.map(UnixTime),
                    sunset: d.sunset.map(UnixTime),
                    daylight_s: match (d.sunrise, d.sunset) {
                        (Some(rise), Some(set)) if set > rise => u32::try_from(set - rise).ok(),
                        _ => None,
                    },
                    uv_index_max: d.uvi.and_then(finite),
                    wind_speed_max: d.wind_speed.and_then(finite).map(Mps),
                    wind_gust_max: d.wind_gust.and_then(finite).map(Mps),
                    wind_dir_dominant: d.wind_deg.and_then(finite).map(Degrees),
                    moon_phase: d.moon_phase.and_then(finite),
                    moonrise: d.moonrise.map(UnixTime),
                    moonset: d.moonset.map(UnixTime),
                }
            })
            .collect(),
    );
    Ok(())
}

/// The primary subtag of a language tag, lower case (`"en-GB"` → `"en"`).
fn primary_language(tag: &str) -> String {
    tag.split(['-', '_'])
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
}

/// `/alert/{id}`: one alert's details. The API gives no severity, so it comes
/// from the event name. The text is the one in `ctx.language` (by primary subtag),
/// else English, else the first. The details count as checked at
/// `ctx.now`.
pub(super) fn parse_alert(
    body: &[u8],
    ctx: &FetchContext,
    out: &mut SnapshotBuilder,
) -> Result<(), ProviderError> {
    let language = ctx.language.as_str();
    let r: AlertRecord =
        serde_json::from_slice(body).map_err(|e| ProviderError::Malformed(e.to_string()))?;
    let event = r.event.trim();
    let title = if !event.is_empty() {
        event.to_string()
    } else if !r.tags.is_empty() {
        r.tags.join(", ")
    } else {
        r.sender_name.clone()
    };
    let in_language = |wanted: &str| {
        r.description
            .iter()
            .find(|t| primary_language(&t.language) == wanted)
    };
    let description = in_language(&primary_language(language))
        .or_else(|| in_language("en"))
        .or_else(|| r.description.first())
        .map(|t| t.description.trim().to_string())
        .unwrap_or_default();
    out.push_alert(Alert {
        id: Some(r.id),
        severity: severity_of(event),
        title,
        description,
        start: UnixTime(r.start),
        end: UnixTime(r.end),
        source: r.sender_name,
        checked_at: Some(ctx.now),
        url: None,
    });
    Ok(())
}

/// Severity from an event name in the US National Weather Service's terms
/// (emergency, warning, watch, advisory, statement); anything else is Moderate.
pub(crate) fn severity_of(event: &str) -> Level {
    let event = event.to_ascii_lowercase();
    let has = |word: &str| event.contains(word);
    if has("emergency") {
        Level::Extreme
    } else if has("warning") {
        Level::VeryPoor
    } else if has("watch") {
        Level::Poor
    } else if has("advisory") {
        Level::Moderate
    } else if has("statement") {
        Level::Fair
    } else {
        Level::Moderate
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use whether_core::requirements::{DailyFields, HourlyFields};

    #[test]
    fn severity_follows_the_event_name() {
        assert_eq!(severity_of("Tornado Emergency"), Level::Extreme);
        assert_eq!(severity_of("Flood Warning"), Level::VeryPoor);
        assert_eq!(severity_of("Winter Storm Watch"), Level::Poor);
        assert_eq!(severity_of("wind advisory"), Level::Moderate);
        assert_eq!(severity_of("Special Weather Statement"), Level::Fair);
        assert_eq!(severity_of(""), Level::Moderate);
    }

    fn ctx_in(language: &str) -> FetchContext {
        FetchContext {
            now: UnixTime(1_000),
            user_agent: "test".into(),
            utc_offset_hint: None,
            if_modified_since: None,
            language: language.into(),
        }
    }

    #[test]
    fn alert_title_and_text_fall_back() {
        let parse_in = |body: &str, language: &str| {
            let mut out = SnapshotBuilder::new(whether_core::model::ProviderId::OpenWeather);
            out.set_location(ResolvedLocation::default());
            parse_alert(body.as_bytes(), &ctx_in(language), &mut out).unwrap();
            out.finish(UnixTime(0)).unwrap().alerts.remove(0)
        };
        let parse = |body: &str| parse_in(body, "en");
        let a = parse(
            r#"{"id":"x","sender_name":"METEO-FRANCE","event":"","start":1,"end":2,
               "description":[{"language":"fr-FR","description":"Orages"},
                              {"language":"en-GB","description":" Storms "}],
               "tags":["Thunderstorm","Wind"]}"#,
        );
        assert_eq!(
            (a.title.as_str(), a.description.as_str(), a.severity),
            ("Thunderstorm, Wind", "Storms", Level::Moderate)
        );
        let a = parse(
            r#"{"id":"y","sender_name":"DWD","start":1,"end":2,
                          "description":[{"language":"de","description":"Sturm"}]}"#,
        );
        assert_eq!((a.title.as_str(), a.description.as_str()), ("DWD", "Sturm"));
        let mut out = SnapshotBuilder::new(whether_core::model::ProviderId::OpenWeather);
        assert!(
            parse_alert(b"{\"id\":\"z\"}", &ctx_in("en"), &mut out).is_err(),
            "times are required"
        );
    }

    #[test]
    fn alert_text_follows_the_language() {
        let body = r#"{"id":"x","sender_name":"METEO-FRANCE","event":"Orages","start":1,"end":2,
               "description":[{"language":"fr-FR","description":"Orages"},
                              {"language":"en-GB","description":"Storms"}]}"#;
        let text = |language: &str| {
            let mut out = SnapshotBuilder::new(whether_core::model::ProviderId::OpenWeather);
            out.set_location(ResolvedLocation::default());
            parse_alert(body.as_bytes(), &ctx_in(language), &mut out).unwrap();
            out.finish(UnixTime(0))
                .unwrap()
                .alerts
                .remove(0)
                .description
        };
        assert_eq!(text("fr"), "Orages");
        let mut out = SnapshotBuilder::new(whether_core::model::ProviderId::OpenWeather);
        out.set_location(ResolvedLocation::default());
        parse_alert(body.as_bytes(), &ctx_in("en"), &mut out).unwrap();
        let checked = out.finish(UnixTime(0)).unwrap().alerts[0].checked_at;
        assert_eq!(
            checked,
            Some(UnixTime(1_000)),
            "checked at the fetch's time"
        );
        assert_eq!(text("FR_ca"), "Orages", "primary subtag, any case");
        assert_eq!(text("en-US"), "Storms");
        assert_eq!(text("de"), "Storms", "English when the language is missing");
        assert_eq!(text(""), "Storms");
    }

    #[test]
    fn daily_records_are_dated_by_their_utc_midnight() {
        let edt = UtcOffset(-14_400);
        let oct8 = LocalDate {
            year: 2026,
            month: 10,
            day: 8,
        };
        // 2026-10-08T00:00Z, as the API stamps the 8th.
        assert_eq!(day_of(1_791_417_600, edt), oct8);
        assert_eq!(day_of(1_791_417_600, UtcOffset(32_400)), oct8);
        // 12:00 EDT on the 8th.
        assert_eq!(day_of(1_791_475_200, edt), oct8);
    }

    fn req(hours: u8, daily: bool, nowcast: bool) -> DataRequirements {
        DataRequirements {
            hourly: if hours > 0 {
                HourlyFields::TEMPERATURE
            } else {
                HourlyFields::empty()
            },
            hourly_hours: hours,
            daily: if daily {
                DailyFields::TEMP_MAX
            } else {
                DailyFields::empty()
            },
            daily_days: 5,
            minutely_hours: if nowcast { 2 } else { 0 },
            ..Default::default()
        }
    }

    #[test]
    fn pages_and_calls() {
        assert_eq!(hourly_pages(&req(0, false, false)), 0);
        assert_eq!(hourly_pages(&req(12, false, false)), 1);
        assert_eq!(hourly_pages(&req(20, false, false)), 1);
        assert_eq!(hourly_pages(&req(24, false, false)), 2);
        assert_eq!(hourly_pages(&req(48, false, false)), 3);
        assert_eq!(calls_per_fetch(&req(0, false, false)), 1);
        assert_eq!(calls_per_fetch(&req(24, true, false)), 4);
        let no_alerts = DataRequirements {
            alerts: false,
            ..DataRequirements::full()
        };
        assert_eq!(calls_per_fetch(&no_alerts), 6);
        assert_eq!(
            calls_per_fetch(&DataRequirements::full()),
            7,
            "one call reserved for alert details"
        );
    }

    #[test]
    fn hours_start_on_the_hour() {
        assert_eq!(hour_start(UnixTime(1_791_217_800)), UnixTime(1_791_216_000));
        assert_eq!(hour_start(UnixTime(1_791_216_000)), UnixTime(1_791_216_000));
    }

    #[test]
    fn precipitation_rates_add_rain_and_snow() {
        let r: Record =
            serde_json::from_str(r#"{"dt":0,"rain":{"1h":1.5},"snow":{"1h":0.5}}"#).unwrap();
        assert_eq!(r.precipitation_rate(), Some(2.0));
        let dry: Record =
            serde_json::from_str(r#"{"dt":0,"rain":{"1h":null},"snow":{"1h":null}}"#).unwrap();
        assert_eq!(dry.precipitation_rate(), None);
    }
}
