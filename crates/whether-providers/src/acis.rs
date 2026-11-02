//! NOAA's Regional Climate Centers (ACIS, <https://data.rcc-acis.org>): US station
//! history, keyless, public domain. Queries are JSON POSTs.
//!
//! 1. `StnMeta` lists the stations near the place; the nearest with records
//!    since 1991 that still reports is chosen (or the configured one).
//! 2. `StnData` gives that station's observed days and its 1991–2020 normals
//!    over the window (observed values of future dates are `"M"`).
//! 3. Per-day statistics over 1991–2020 give the chance of precipitation.
//! 4. Records come from the station, then from its threaded city record
//!    (`NYCthr`: New York since 1869), which replaces them when it exists.
//!
//! Values are °F and inches; `"M"` is missing and `"T"` a trace.

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use serde_json::{Value, json};
use whether_core::history::{ClimateQuery, HistoryBuilder, Record, YearRange};
use whether_core::model::{ProviderId, Station};
use whether_core::requirements::HistoryRequirements;
use whether_core::time::LocalDate;
use whether_core::units::{Celsius, Mm, Percent};

use crate::error::ProviderError;
use crate::history::{HistoryContext, HistoryProvider};
use crate::http::{Attribution, Endpoint, HttpRequest, HttpResponse, Method};
use crate::regions::covers_us;
use crate::url::Coordinates;

/// The default API base URL.
pub const DEFAULT_BASE_URL: &str = "https://data.rcc-acis.org";

/// How far a station may be, in metres.
const MAX_DISTANCE_M: f64 = 60_000.0;
/// The box searched around the place, in degrees.
const SEARCH_DEGREES: f64 = 0.6;
/// A station must report no later than this many days before yesterday.
const RECENT_DAYS: i64 = 3;
/// The official normals period.
const NORMALS: YearRange = YearRange::NORMALS_1991_2020;

/// ACIS settings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcisSettings {
    /// API base URL.
    pub base_url: String,
    /// A station id to use instead of the nearest (`"LGA"`, `"USW00014732"`).
    pub station: Option<String>,
    /// A station id for the records (`"LGAthr"`) instead of the station's
    /// threaded record.
    pub records_station: Option<String>,
}

impl Default for AcisSettings {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_string(),
            station: None,
            records_station: None,
        }
    }
}

/// The ACIS history provider.
#[derive(Clone, Debug, Default)]
pub struct Acis {
    settings: AcisSettings,
}

/// `YYYY-MM-DD`.
fn iso(d: LocalDate) -> String {
    alloc::format!("{:04}-{:02}-{:02}", d.year, d.month, d.day)
}

/// `MM-DD`.
fn month_day(d: LocalDate) -> String {
    alloc::format!("{:02}-{:02}", d.month, d.day)
}

/// `YYYY-MM-DD` as a date.
fn parse_iso(s: &str) -> Option<LocalDate> {
    let mut parts = s.splitn(3, '-');
    let year = parts.next()?.parse().ok()?;
    let month = parts.next()?.parse().ok()?;
    let day = parts.next()?.get(..2)?.parse().ok()?;
    Some(LocalDate { year, month, day })
}

/// An ACIS value: a number (possibly with a flag letter, `0.12A`), `"T"` for a
/// trace, `"M"` or anything else for missing.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Reading {
    Value(f32),
    Trace,
    Missing,
}

fn reading(v: &Value) -> Reading {
    let Some(s) = v.as_str() else {
        #[allow(clippy::cast_possible_truncation)] // weather values are small
        return v
            .as_f64()
            .map_or(Reading::Missing, |n| Reading::Value(n as f32));
    };
    if s == "T" {
        return Reading::Trace;
    }
    let number = s.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    number
        .parse::<f32>()
        .ok()
        .filter(|n| n.is_finite())
        .map_or(Reading::Missing, Reading::Value)
}

fn fahrenheit(r: Reading) -> Option<Celsius> {
    match r {
        Reading::Value(f) => Some(Celsius((f - 32.0) * 5.0 / 9.0)),
        Reading::Trace | Reading::Missing => None,
    }
}

fn inches(r: Reading) -> Option<Mm> {
    match r {
        Reading::Value(i) => Some(Mm(i * 25.4)),
        Reading::Trace => Some(Mm(0.0)),
        Reading::Missing => None,
    }
}

/// `PHILADELPHIA INTL AP` as `Philadelphia Intl AP`: words title-cased, short
/// ones (`AP`, `NW`) kept.
fn station_name(name: &str) -> String {
    name.split_whitespace()
        .map(|word| {
            if word.len() <= 2 || word.chars().any(|c| c.is_ascii_digit()) {
                return word.to_string();
            }
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                let mut out = String::new();
                out.extend(first.to_uppercase());
                out.push_str(&chars.as_str().to_lowercase());
                out
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The id of type `kind` in an ACIS `sids` list (`"LGA 3"` → `LGA`).
fn sid_of(sids: &[Value], kind: &str) -> Option<String> {
    sids.iter().filter_map(Value::as_str).find_map(|sid| {
        let (id, t) = sid.rsplit_once(' ')?;
        (t == kind).then(|| id.to_string())
    })
}

/// The id to ask ACIS for: the FAA id, else the COOP id, else the first.
fn best_sid(sids: &[Value]) -> Option<String> {
    sid_of(sids, "3")
        .or_else(|| sid_of(sids, "2"))
        .or_else(|| {
            sids.first()?
                .as_str()?
                .split(' ')
                .next()
                .map(ToString::to_string)
        })
        .filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric()))
}

/// The distance between two places, in metres (haversine).
fn distance_m(a: (f64, f64), b: (f64, f64)) -> f64 {
    const EARTH_RADIUS_M: f64 = 6_371_000.0;
    let (lat1, lat2) = (a.0.to_radians(), b.0.to_radians());
    let (dlat, dlon) = ((b.0 - a.0).to_radians(), (b.1 - a.1).to_radians());
    let h = libm::sin(dlat / 2.0) * libm::sin(dlat / 2.0)
        + libm::cos(lat1) * libm::cos(lat2) * libm::sin(dlon / 2.0) * libm::sin(dlon / 2.0);
    2.0 * EARTH_RADIUS_M * libm::asin(libm::sqrt(h).min(1.0))
}

/// `[longitude, latitude]` as `(latitude, longitude)`.
fn lat_lon(ll: Option<&Value>) -> Option<(f64, f64)> {
    let ll = ll?.as_array()?;
    Some((ll.get(1)?.as_f64()?, ll.first()?.as_f64()?))
}

fn malformed(e: &serde_json::Error) -> ProviderError {
    ProviderError::Malformed(e.to_string())
}

/// A response's JSON, or the error ACIS reports in a 200 (`{"error": …}`).
fn read(body: &[u8]) -> Result<Value, ProviderError> {
    let v: Value = serde_json::from_slice(body).map_err(|e| malformed(&e))?;
    match v.get("error").and_then(Value::as_str) {
        Some(error) => Err(ProviderError::Rejected(error.to_string())),
        None => Ok(v),
    }
}

/// A station near a place, as a `StnMeta` answer describes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NearbyStation {
    /// The id ACIS is asked with (FAA, else COOP, else the first).
    pub id: String,
    /// Its name, title-cased.
    pub name: String,
    /// How far it is from the place, in metres.
    pub distance_m: u32,
    /// The first day with temperatures.
    pub first: Option<LocalDate>,
    /// The last day with temperatures.
    pub last: Option<LocalDate>,
    /// Its temperatures go back to 1991, so it has normals and long records.
    pub long_record: bool,
    /// It reported yesterday.
    pub reported_yesterday: bool,
    /// It reported in the last days, so it can be used.
    pub usable: bool,
}

/// The stations of a `StnMeta` list within 60 km, best first: those that
/// reported in the last days, then those whose temperatures go back to 1991
/// (normals and long records), then those that reported yesterday (many
/// cooperative observers report a day or two late, which would leave
/// "yesterday" empty), then the nearest. The first usable one is the station
/// used when none is configured.
fn ranked(list: &Value, place: (f64, f64), ctx: &HistoryContext) -> Vec<NearbyStation> {
    let recent = iso(ctx.day(-1 - RECENT_DAYS));
    let yesterday = iso(ctx.day(-1));
    let normals_start = alloc::format!("{}-01-01", NORMALS.first);
    let mut stations: Vec<(f64, NearbyStation)> = list
        .get("meta")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|station| {
            let at = lat_lon(station.get("ll"))?;
            let distance = distance_m(place, at);
            let range = station.get("valid_daterange")?.get(0)?.as_array()?;
            let (start, end) = (range.first()?.as_str()?, range.get(1)?.as_str()?);
            if distance > MAX_DISTANCE_M {
                return None;
            }
            let id = best_sid(station.get("sids")?.as_array()?)?;
            let name = station_name(station.get("name").and_then(Value::as_str).unwrap_or(&id));
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // under 60 km
            let distance_m = libm::round(distance) as u32;
            Some((
                distance,
                NearbyStation {
                    id,
                    name,
                    distance_m,
                    first: parse_iso(start),
                    last: parse_iso(end),
                    long_record: start <= normals_start.as_str(),
                    reported_yesterday: end >= yesterday.as_str(),
                    usable: end >= recent.as_str(),
                },
            ))
        })
        .collect();
    stations.sort_by(|(da, a), (db, b)| {
        b.usable
            .cmp(&a.usable)
            .then(b.long_record.cmp(&a.long_record))
            .then(b.reported_yesterday.cmp(&a.reported_yesterday))
            .then(da.total_cmp(db))
    });
    stations.into_iter().map(|(_, s)| s).collect()
}

/// The station to use from a `StnMeta` list: the best usable one.
fn choose(list: &Value, place: (f64, f64), ctx: &HistoryContext) -> Option<String> {
    ranked(list, place, ctx)
        .into_iter()
        .find(|s| s.usable)
        .map(|s| s.id)
}

/// The station for a climate query from a `StnMeta` list: within 60 km, those
/// whose temperatures span all of `years` first, then those spanning most of
/// them, then the nearest.
fn choose_for_years(list: &Value, place: (f64, f64), years: YearRange) -> Option<String> {
    let mut candidates: Vec<(bool, i32, f64, String)> = list
        .get("meta")?
        .as_array()?
        .iter()
        .filter_map(|station| {
            let distance = distance_m(place, lat_lon(station.get("ll"))?);
            let range = station.get("valid_daterange")?.get(0)?.as_array()?;
            let (start, end) = (
                parse_iso(range.first()?.as_str()?)?,
                parse_iso(range.get(1)?.as_str()?)?,
            );
            if distance > MAX_DISTANCE_M {
                return None;
            }
            // Whole years covered.
            let (first_year, last_year) = (
                if (start.month, start.day) == (1, 1) {
                    start.year
                } else {
                    start.year + 1
                },
                if (end.month, end.day) == (12, 31) {
                    end.year
                } else {
                    end.year - 1
                },
            );
            let overlap = (last_year.min(years.last) - first_year.max(years.first) + 1).max(0);
            if overlap == 0 {
                return None;
            }
            let full = first_year <= years.first && last_year >= years.last;
            Some((
                full,
                overlap,
                distance,
                best_sid(station.get("sids")?.as_array()?)?,
            ))
        })
        .collect();
    candidates.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)).then(a.2.total_cmp(&b.2)));
    candidates.into_iter().next().map(|(.., sid)| sid)
}

/// One element summarized per calendar date of a window across the years.
fn per_day(name: &str, reduce: &Value, from: LocalDate, to: LocalDate) -> Value {
    json!({
        "name": name,
        "interval": "dly",
        "duration": "dly",
        "smry": reduce,
        "smry_only": 1,
        "groupby": ["year", month_day(from), month_day(to)],
    })
}

impl Acis {
    /// A provider with these settings.
    pub fn new(settings: AcisSettings) -> Self {
        Self { settings }
    }

    /// The `StnMeta` request for the stations around `loc` (US places only).
    pub fn stations_request(
        &self,
        loc: &Coordinates,
        ctx: &HistoryContext,
    ) -> Result<HttpRequest, ProviderError> {
        if !covers_us(loc) {
            return Err(ProviderError::OutOfArea("ACIS"));
        }
        let d = SEARCH_DEGREES;
        let bbox = alloc::format!(
            "{:.4},{:.4},{:.4},{:.4}",
            loc.longitude - d,
            loc.latitude - d,
            loc.longitude + d,
            loc.latitude + d
        );
        let body = json!({
            "bbox": bbox,
            "elems": "maxt,mint,pcpn",
            "meta": "name,sids,ll,valid_daterange",
        });
        Ok(self.post("StnMeta", &body, Endpoint::Stations, true, ctx))
    }

    /// The stations within 60 km in the answer to [`Self::stations_request`],
    /// best first; the first usable one is used when no station is configured.
    pub fn nearby_stations(
        resp: &HttpResponse<'_>,
        loc: &Coordinates,
        ctx: &HistoryContext,
    ) -> Result<Vec<NearbyStation>, ProviderError> {
        crate::error::check_status(resp)?;
        let list = read(resp.body)?;
        Ok(ranked(&list, (loc.latitude, loc.longitude), ctx))
    }

    fn post(
        &self,
        path: &str,
        body: &Value,
        endpoint: Endpoint,
        required: bool,
        ctx: &HistoryContext,
    ) -> HttpRequest {
        HttpRequest {
            url: alloc::format!("{}/{path}", self.settings.base_url.trim_end_matches('/')),
            headers: vec![("User-Agent", ctx.user_agent.clone())],
            endpoint,
            required,
            metered: false,
            method: Method::Post,
            body: Some(body.to_string()),
        }
    }

    /// The station's requests: days and normals, the chance of precipitation,
    /// and records (the station's, then `records`', which replace them).
    fn station_requests(
        &self,
        sid: &str,
        records: Option<String>,
        req: HistoryRequirements,
        ctx: &HistoryContext,
    ) -> Vec<HttpRequest> {
        let (first, last) = (ctx.first_date(&req), ctx.last_date(&req));
        let days = json!({
            "sid": sid,
            "sdate": iso(first),
            "edate": iso(last),
            "elems": [
                {"name": "maxt"}, {"name": "mint"}, {"name": "pcpn"}, {"name": "snow"},
                {"name": "maxt", "normal": "1"},
                {"name": "mint", "normal": "1"},
                {"name": "pcpn", "normal": "1"},
            ],
            "meta": "name,sids,ll",
        });
        let mut out = vec![self.post("StnData", &days, Endpoint::Climate, true, ctx)];
        if req.climate_days == 0 {
            return out;
        }
        let (from, to) = (ctx.today, ctx.day(i64::from(req.climate_days) - 1));
        // Records reach back to the past days too, for what each one compares with.
        let records_from = ctx.first_date(&req).min(from);
        let chance = json!({
            "sid": sid,
            "sdate": alloc::format!("{}-01-01", NORMALS.first),
            "edate": alloc::format!("{}-12-31", NORMALS.last),
            "elems": [
                per_day("pcpn", &json!({"reduce": "cnt_ge_0.01"}), from, to),
                per_day("pcpn", &json!({"reduce": "cnt_ge_0.00"}), from, to),
            ],
        });
        out.push(self.post("StnData", &chance, Endpoint::Climate, false, ctx));
        if !req.records {
            return out;
        }
        let records_of = |sid: &str| {
            json!({
                "sid": sid,
                "sdate": "por",
                "edate": "por",
                "elems": [
                    per_day("maxt", &json!({"reduce": "max", "add": "date"}), records_from, to),
                    per_day("maxt", &json!({"reduce": "min", "add": "date"}), records_from, to),
                    per_day("mint", &json!({"reduce": "min", "add": "date"}), records_from, to),
                    per_day("mint", &json!({"reduce": "max", "add": "date"}), records_from, to),
                    per_day("pcpn", &json!({"reduce": "max", "add": "date"}), records_from, to),
                ],
                "meta": "name,sids",
            })
        };
        // The station's own records first; a threaded (or configured) records
        // station's replace them when it answers.
        out.push(self.post("StnData", &records_of(sid), Endpoint::Climate, false, ctx));
        let records = self.settings.records_station.clone().or(records);
        if let Some(records) = records.filter(|r| r != sid) {
            out.push(self.post(
                "StnData",
                &records_of(&records),
                Endpoint::Climate,
                false,
                ctx,
            ));
        }
        out
    }

    /// The climate of `query`'s dates at station `sid` over its years: per
    /// date, the mean high, low and precipitation, the counts of wet and of
    /// reported days, and the extremes with their dates.
    fn climate_request(
        &self,
        sid: &str,
        query: &ClimateQuery,
        ctx: &HistoryContext,
    ) -> HttpRequest {
        let (from, to) = (query.from, query.to);
        let each = |name: &str, reduce: Value| per_day(name, &reduce, from, to);
        let body = json!({
            "sid": sid,
            "sdate": alloc::format!("{}-01-01", query.years.first),
            "edate": alloc::format!("{}-12-31", query.years.last),
            "elems": [
                each("maxt", json!({"reduce": "mean"})),
                each("mint", json!({"reduce": "mean"})),
                each("pcpn", json!({"reduce": "mean"})),
                each("pcpn", json!({"reduce": "cnt_ge_0.01"})),
                each("pcpn", json!({"reduce": "cnt_ge_0.00"})),
                each("maxt", json!({"reduce": "max", "add": "date"})),
                each("maxt", json!({"reduce": "min", "add": "date"})),
                each("mint", json!({"reduce": "min", "add": "date"})),
                each("mint", json!({"reduce": "max", "add": "date"})),
                each("pcpn", json!({"reduce": "max", "add": "date"})),
            ],
            "meta": "name,sids,ll",
        });
        self.post("StnData", &body, Endpoint::Climate, true, ctx)
    }

    /// The threaded record of a station: its FAA id plus `thr`.
    fn threaded(faa: Option<String>) -> Option<String> {
        faa.filter(|id| (3..=4).contains(&id.len()))
            .map(|id| id + "thr")
    }
}

/// The station a `StnData` answer's `meta` names, and how far it is.
fn station_from_meta(v: &Value, out: &mut HistoryBuilder) {
    let Some(meta) = v.get("meta") else {
        return;
    };
    let place = out.location().map(|l| (l.latitude, l.longitude));
    let sids = meta.get("sids").and_then(Value::as_array);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // within 60 km
    let distance = place
        .zip(lat_lon(meta.get("ll")))
        .map(|(a, b)| libm::round(distance_m(a, b)) as u32);
    out.set_station(Station {
        id: sids.and_then(|s| best_sid(s)).unwrap_or_default(),
        name: station_name(meta.get("name").and_then(Value::as_str).unwrap_or("")),
        distance_m: distance,
    });
}

/// Days and normals: `[date, maxt, mint, pcpn, snow, normal maxt, mint, pcpn]`.
fn parse_days(v: &Value, ctx: &HistoryContext, out: &mut HistoryBuilder) {
    station_from_meta(v, out);
    out.set_period(NORMALS);
    for row in v
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(row) = row.as_array() else { continue };
        let Some(date) = row.first().and_then(Value::as_str).and_then(parse_iso) else {
            continue;
        };
        let at = |i: usize| row.get(i).map_or(Reading::Missing, reading);
        if date < ctx.today {
            let (high, low, rain, snow) = (at(1), at(2), at(3), at(4));
            if [high, low, rain, snow]
                .iter()
                .any(|r| *r != Reading::Missing)
            {
                let day = out.day_mut(date);
                day.temp_max = fahrenheit(high);
                day.temp_min = fahrenheit(low);
                day.precip_sum = inches(rain);
                day.trace = rain == Reading::Trace;
                day.snow_sum = inches(snow);
            }
        }
        let normal = (fahrenheit(at(5)), fahrenheit(at(6)), inches(at(7)));
        if normal != (None, None, None) {
            let c = out.climate_mut(date);
            c.normal_max = normal.0;
            c.normal_min = normal.1;
            c.normal_precip = normal.2;
        }
    }
}

/// The date with `month` and `day` nearest today (within half a year): this
/// year's, last year's or next year's.
fn date_near(ctx: &HistoryContext, month: u8, day: u8) -> Option<LocalDate> {
    let today = ctx.today.days_since_epoch();
    [ctx.today.year - 1, ctx.today.year, ctx.today.year + 1]
        .into_iter()
        .map(|year| LocalDate { year, month, day })
        .filter(|d| day <= whether_core::time::days_in_month(d.year, month))
        .min_by_key(|d| (d.days_since_epoch() - today).abs())
}

/// Per-day summaries: the chance of precipitation (two counts), or records
/// (five `[value, date]` lists).
fn parse_summary(
    v: &Value,
    ctx: &HistoryContext,
    out: &mut HistoryBuilder,
) -> Result<(), ProviderError> {
    let smry = v
        .get("smry")
        .and_then(Value::as_array)
        .ok_or_else(|| ProviderError::Malformed("no smry".into()))?;
    // One value per date of the window, which starts today.
    let days = smry.first().and_then(Value::as_array).map_or(0, Vec::len);
    let dates: Vec<LocalDate> = (0..days)
        .map(|i| ctx.day(i64::try_from(i).unwrap_or(0)))
        .collect();
    let is_records = smry
        .first()
        .and_then(Value::as_array)
        .and_then(|d| d.first())
        .is_some_and(Value::is_array);
    if !is_records {
        let (wet, all) = (smry.first(), smry.get(1));
        for (i, date) in dates.iter().enumerate() {
            let count = |list: Option<&Value>| match list.and_then(|l| l.get(i)).map(reading) {
                Some(Reading::Value(n)) => Some(n),
                _ => None,
            };
            if let (Some(wet), Some(all)) = (count(wet), count(all))
                && all > 0.0
            {
                out.climate_mut(*date).precip_chance = Some(Percent(100.0 * wet / all));
            }
        }
        return Ok(());
    }
    // Each record carries the date it was set: its month and day say which
    // calendar date it belongs to (the window may start before today).
    let record = |elem: usize, i: usize, precip: bool| -> Option<(LocalDate, Record)> {
        let pair = smry.get(elem)?.get(i)?.as_array()?;
        let set = parse_iso(pair.get(1)?.as_str()?)?;
        let value = if precip {
            inches(reading(pair.first()?))?.0
        } else {
            fahrenheit(reading(pair.first()?))?.0
        };
        Some((
            date_near(ctx, set.month, set.day)?,
            Record {
                value,
                year: set.year,
            },
        ))
    };
    let mut any = false;
    for i in 0..days {
        for elem in 0..5 {
            let Some((date, r)) = record(elem, i, elem == 4) else {
                continue;
            };
            any = true;
            let c = out.climate_mut(date);
            *match elem {
                0 => &mut c.record_max,
                1 => &mut c.record_low_max,
                2 => &mut c.record_min,
                3 => &mut c.record_high_min,
                _ => &mut c.record_precip,
            } = Some(r);
        }
    }
    if let Some(meta) = v.get("meta")
        && any
    {
        let sids = meta.get("sids").and_then(Value::as_array);
        // A threaded record (type 9) is named as the records' station.
        if let Some(thread) = sids.and_then(|s| sid_of(s, "9")) {
            out.set_records_station(Station {
                id: thread,
                name: station_name(meta.get("name").and_then(Value::as_str).unwrap_or("")),
                distance_m: None,
            });
        }
    }
    Ok(())
}

/// A climate query's answer (see [`Acis::climate_request`]), one value per
/// date of the query.
fn parse_climate_summary(
    v: &Value,
    query: &ClimateQuery,
    out: &mut HistoryBuilder,
) -> Result<(), ProviderError> {
    let smry = v
        .get("smry")
        .and_then(Value::as_array)
        .filter(|s| s.len() == 10)
        .ok_or_else(|| ProviderError::Malformed("no smry".into()))?;
    station_from_meta(v, out);
    out.set_period(query.years);
    let value = |elem: usize, i: usize| smry.get(elem)?.get(i).map(reading);
    let record = |elem: usize, i: usize, precip: bool| -> Option<Record> {
        let pair = smry.get(elem)?.get(i)?.as_array()?;
        let year = pair.get(1)?.as_str()?.get(..4)?.parse().ok()?;
        let r = reading(pair.first()?);
        let value = if precip {
            inches(r)?.0
        } else {
            fahrenheit(r)?.0
        };
        Some(Record { value, year })
    };
    for (i, date) in query.dates().into_iter().enumerate() {
        let count = |elem| match value(elem, i) {
            Some(Reading::Value(n)) => Some(n),
            _ => None,
        };
        let c = out.climate_mut(date);
        c.normal_max = value(0, i).and_then(fahrenheit);
        c.normal_min = value(1, i).and_then(fahrenheit);
        c.normal_precip = value(2, i).and_then(inches);
        if let (Some(wet), Some(all)) = (count(3), count(4))
            && all > 0.0
        {
            c.precip_chance = Some(Percent(100.0 * wet / all));
        }
        c.record_max = record(5, i, false);
        c.record_low_max = record(6, i, false);
        c.record_min = record(7, i, false);
        c.record_high_min = record(8, i, false);
        c.record_precip = record(9, i, true);
    }
    Ok(())
}

impl HistoryProvider for Acis {
    fn id(&self) -> ProviderId {
        ProviderId::Acis
    }

    fn attribution(&self) -> Attribution {
        Attribution {
            text: ProviderId::Acis.attribution(),
            url: "https://www.rcc-acis.org",
            license: "Public domain (US government work)",
        }
    }

    fn plan(
        &self,
        loc: &Coordinates,
        req: &HistoryRequirements,
        ctx: &HistoryContext,
    ) -> Result<Vec<HttpRequest>, ProviderError> {
        if !covers_us(loc) {
            return Err(ProviderError::OutOfArea("ACIS"));
        }
        if let Some(station) = &self.settings.station {
            let threaded = Self::threaded(Some(station.clone()));
            return Ok(self.station_requests(station, threaded, *req, ctx));
        }
        Ok(vec![self.stations_request(loc, ctx)?])
    }

    fn follow_up(
        &self,
        endpoint: Endpoint,
        resp: &HttpResponse<'_>,
        loc: &Coordinates,
        req: &HistoryRequirements,
        ctx: &HistoryContext,
    ) -> Result<Vec<HttpRequest>, ProviderError> {
        if endpoint != Endpoint::Stations {
            return Ok(Vec::new());
        }
        let list = read(resp.body)?;
        let sid = choose(&list, (loc.latitude, loc.longitude), ctx).ok_or_else(|| {
            ProviderError::Malformed("no station with recent data within 60 km".into())
        })?;
        // The FAA id of the chosen station, for its threaded record.
        let faa = list
            .get("meta")
            .and_then(Value::as_array)
            .and_then(|stations| {
                stations.iter().find_map(|s| {
                    let sids = s.get("sids")?.as_array()?;
                    (best_sid(sids).as_deref() == Some(sid.as_str()))
                        .then(|| sid_of(sids, "3"))
                        .flatten()
                })
            });
        Ok(self.station_requests(&sid, Self::threaded(faa), *req, ctx))
    }

    fn parse(
        &self,
        endpoint: Endpoint,
        resp: &HttpResponse<'_>,
        ctx: &HistoryContext,
        out: &mut HistoryBuilder,
    ) -> Result<(), ProviderError> {
        let v = read(resp.body)?;
        match endpoint {
            // The list only names the station (`follow_up`).
            Endpoint::Stations => Ok(()),
            _ if v.get("data").is_some() => {
                parse_days(&v, ctx, out);
                Ok(())
            }
            _ => parse_summary(&v, ctx, out),
        }
    }

    fn plan_climate(
        &self,
        loc: &Coordinates,
        query: &ClimateQuery,
        ctx: &HistoryContext,
    ) -> Result<Vec<HttpRequest>, ProviderError> {
        match &self.settings.station {
            Some(station) if covers_us(loc) => Ok(vec![self.climate_request(station, query, ctx)]),
            _ => Ok(vec![self.stations_request(loc, ctx)?]),
        }
    }

    fn follow_up_climate(
        &self,
        endpoint: Endpoint,
        resp: &HttpResponse<'_>,
        loc: &Coordinates,
        query: &ClimateQuery,
        ctx: &HistoryContext,
    ) -> Result<Vec<HttpRequest>, ProviderError> {
        if endpoint != Endpoint::Stations {
            return Ok(Vec::new());
        }
        let list = read(resp.body)?;
        let sid = choose_for_years(&list, (loc.latitude, loc.longitude), query.years).ok_or_else(
            || ProviderError::Malformed("no station with those years within 60 km".into()),
        )?;
        Ok(vec![self.climate_request(&sid, query, ctx)])
    }

    fn parse_climate(
        &self,
        endpoint: Endpoint,
        resp: &HttpResponse<'_>,
        query: &ClimateQuery,
        _ctx: &HistoryContext,
        out: &mut HistoryBuilder,
    ) -> Result<(), ProviderError> {
        let v = read(resp.body)?;
        match endpoint {
            Endpoint::Stations => Ok(()),
            _ => parse_climate_summary(&v, query, out),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use whether_core::time::{UnixTime, UtcOffset};

    fn ctx(month: u8, day: u8) -> HistoryContext {
        HistoryContext {
            now: UnixTime(0),
            today: LocalDate {
                year: 2026,
                month,
                day,
            },
            utc_offset: UtcOffset(-14_400),
            user_agent: "test".into(),
            normals: None,
        }
    }

    #[test]
    fn readings() {
        assert_eq!(reading(&json!("67")), Reading::Value(67.0));
        assert_eq!(reading(&json!("0.12A")), Reading::Value(0.12));
        assert_eq!(reading(&json!("T")), Reading::Trace);
        assert_eq!(reading(&json!("M")), Reading::Missing);
        assert_eq!(inches(Reading::Trace), Some(Mm(0.0)));
        assert_eq!(fahrenheit(Reading::Value(212.0)), Some(Celsius(100.0)));
    }

    #[test]
    fn names_and_ids() {
        assert_eq!(station_name("PHILADELPHIA INTL AP"), "Philadelphia Intl AP");
        assert_eq!(station_name("QUEENS 2 NW"), "Queens 2 NW");
        let sids = json!(["14732 1", "305811 2", "LGA 3", "USW00014732 6"]);
        assert_eq!(best_sid(sids.as_array().unwrap()).as_deref(), Some("LGA"));
        let coop = json!(["369464 2", "USC00369464 6"]);
        assert_eq!(
            best_sid(coop.as_array().unwrap()).as_deref(),
            Some("369464")
        );
        assert_eq!(
            Acis::threaded(Some("LGA".into())).as_deref(),
            Some("LGAthr")
        );
        assert_eq!(Acis::threaded(Some("369464".into())), None);
    }

    #[test]
    fn windows_cross_the_new_year_in_one_request() {
        let acis = Acis::default();
        let req = HistoryRequirements {
            past_days: 0,
            climate_days: 7,
            records: true,
            month_to_date: false,
        };
        let requests = acis.station_requests("LGA", None, req, &ctx(12, 30));
        let records = requests.last().unwrap().body.as_deref().unwrap();
        assert!(
            records.contains(r#""groupby":["year","12-30","01-05"]"#),
            "{records}"
        );
    }
}
