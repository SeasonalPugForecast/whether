//! The US National Hurricane Center: active tropical cyclones
//! (`CurrentStorms.json`), the guidance models' and the official forecast
//! tracks (the ATCF a-deck, gzip-compressed) and the observed track (the
//! ATCF b-deck). All of it is public domain.
//!
//! Like every provider here this is sans-IO: builders return
//! [`HttpRequest`]s and parsers take the response bytes. The requests are
//! the same for every place, so no coordinates are sent. They carry
//! [`Endpoint::Forecast`] and are not required: a missing hurricane layer
//! never fails a storm outlook.
//!
//! ATCF rows are comma separated: basin, number, the run's start
//! (`YYYYMMDDHH` UTC), technique number, aid code, forecast hour, latitude
//! and longitude in tenths of a degree with a hemisphere letter (`395N`,
//! `668W`), maximum wind (kt), central pressure (hPa) and storm type, then
//! wind radii. A position repeats on one row per wind radius (34, 50 and 64
//! kt); the first row of each forecast hour is kept. Rows that do not parse
//! are skipped.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use miniz_oxide::inflate::TINFLStatus;
use miniz_oxide::inflate::core::inflate_flags::TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF;
use miniz_oxide::inflate::core::{DecompressorOxide, decompress};
use serde::Deserialize;
use serde_json::Value;
use whether_core::storm::hurricane::OFFICIAL_AID;
use whether_core::storm::{Basin, Hurricane, TrackFix, wrap_longitude};
use whether_core::time::{UnixTime, days_from_civil, days_in_month, parse_rfc3339};

use crate::error::ProviderError;
use crate::http::{Endpoint, HttpRequest, Method};

/// The active storms summary.
pub const CURRENT_STORMS_URL: &str = "https://www.nhc.noaa.gov/CurrentStorms.json";
/// The directory of the public a-decks (`a<id>.dat.gz`).
pub const ADECK_BASE_URL: &str = "https://ftp.nhc.noaa.gov/atcf/aid_public";
/// The directory of the b-decks (`b<id>.dat`).
pub const BDECK_BASE_URL: &str = "https://ftp.nhc.noaa.gov/atcf/btk";

/// The most bytes a deck may inflate to. The 2026 public a-decks are 20 to
/// 640 KB compressed and a few MB inflated.
pub const MAX_DECK_BYTES: usize = 32 * 1024 * 1024;

/// Miles per hour in a knot.
const MPH_PER_KT: f64 = 1.150_779_448;

/// Guidance tracks by ATCF aid code.
pub type AidTracks = BTreeMap<String, Vec<TrackFix>>;

/// One active cyclone in `CurrentStorms.json`.
#[derive(Clone, Debug, PartialEq)]
pub struct ActiveStorm {
    /// NHC's id, lower case: basin, number and year, e.g. `al102026`.
    pub id: String,
    /// The name, e.g. "Josephine".
    pub name: String,
    /// NHC's classification code, e.g. `HU`, `TS` or `PTC`.
    pub classification: String,
    /// The basin, from the id.
    pub basin: Basin,
    /// The maximum sustained wind, in knots.
    pub wind_kt: Option<u16>,
    /// The central pressure, in hPa.
    pub pressure_hpa: Option<f32>,
    /// The centre's latitude, in degrees.
    pub latitude: f64,
    /// The centre's longitude, in degrees, in `-180.0..180.0`.
    pub longitude: f64,
    /// Where the centre is heading, in degrees clockwise from north.
    pub movement_deg: Option<f32>,
    /// How fast it moves, in knots (NHC gives miles per hour).
    pub movement_kt: Option<f32>,
    /// When the summary is valid (`lastUpdate`).
    pub last_update: UnixTime,
    /// The latest public advisory's number, e.g. `011` or `011A`.
    pub advisory_number: Option<String>,
    /// When the latest public advisory was issued.
    pub advisory_time: Option<UnixTime>,
    /// The latest public advisory's web page (https only).
    pub advisory_url: Option<String>,
}

impl ActiveStorm {
    /// The storm as a [`Hurricane`] with its tracks: `official` and `aids`
    /// from [`parse_adeck`], `past` from [`parse_bdeck`]. The advisory time
    /// is the public advisory's issuance, else the summary's time.
    pub fn hurricane(
        &self,
        official: Vec<TrackFix>,
        aids: AidTracks,
        past: Vec<TrackFix>,
    ) -> Hurricane {
        Hurricane {
            id: self.id.clone(),
            name: self.name.clone(),
            classification: self.classification.clone(),
            basin: self.basin,
            wind_kt: self.wind_kt,
            pressure_hpa: self.pressure_hpa,
            latitude: self.latitude,
            longitude: self.longitude,
            movement_deg: self.movement_deg,
            movement_kt: self.movement_kt,
            advisory_time: self.advisory_time.unwrap_or(self.last_update),
            advisory_url: self.advisory_url.clone(),
            official,
            aids,
            past,
        }
    }
}

/// A GET of an NHC file.
fn get(url: String) -> HttpRequest {
    HttpRequest {
        url,
        headers: vec![],
        endpoint: Endpoint::Forecast,
        required: false,
        metered: false,
        method: Method::Get,
        body: None,
    }
}

/// The active storms summary request.
pub fn current_storms_request() -> HttpRequest {
    get(CURRENT_STORMS_URL.to_string())
}

/// `id` checked to be an NHC storm id in one of its basins (`al`, `ep` or
/// `cp`, two digits for the number, four for the year), lower case.
fn storm_id(id: &str) -> Result<String, ProviderError> {
    let ok = id.len() == 8
        && Basin::of_id(id).is_some()
        && id.as_bytes()[2..].iter().all(u8::is_ascii_digit);
    if ok {
        Ok(id.to_ascii_lowercase())
    } else {
        Err(ProviderError::Config(format!(
            "not an NHC storm id: {id:?}"
        )))
    }
}

/// The public a-deck (guidance and official forecast tracks) of storm `id`,
/// e.g. `al102026`.
pub fn adeck_request(id: &str) -> Result<HttpRequest, ProviderError> {
    let id = storm_id(id)?;
    Ok(get(format!("{ADECK_BASE_URL}/a{id}.dat.gz")))
}

/// The b-deck (observed track) of storm `id`.
pub fn bdeck_request(id: &str) -> Result<HttpRequest, ProviderError> {
    let id = storm_id(id)?;
    Ok(get(format!("{BDECK_BASE_URL}/b{id}.dat")))
}

#[derive(Deserialize)]
struct CurrentStorms {
    #[serde(rename = "activeStorms")]
    active_storms: Vec<Value>,
}

/// One entry as NHC writes it. Numbers come as numbers or as strings, and
/// the reference spells the position fields `latitude_numeric`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawStorm {
    id: String,
    name: Option<String>,
    classification: Option<String>,
    intensity: Option<Value>,
    pressure: Option<Value>,
    latitude: Option<String>,
    longitude: Option<String>,
    latitude_numeric: Option<Value>,
    longitude_numeric: Option<Value>,
    #[serde(rename = "latitude_numeric")]
    latitude_numeric_snake: Option<Value>,
    #[serde(rename = "longitude_numeric")]
    longitude_numeric_snake: Option<Value>,
    movement_dir: Option<Value>,
    movement_speed: Option<Value>,
    last_update: Option<String>,
    public_advisory: Option<Product>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Product {
    adv_num: Option<String>,
    issuance: Option<String>,
    url: Option<String>,
}

/// A number given as a JSON number or a numeric string.
fn number(v: Option<&Value>) -> Option<f64> {
    let n = match v? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }?;
    n.is_finite().then_some(n)
}

/// A position string with a hemisphere letter, `17.1N` or `58.4W`, as
/// signed degrees.
fn hemisphere_degrees(s: &str, positive: u8, negative: u8) -> Option<f64> {
    let s = s.trim();
    let (value, letter) = s.split_at_checked(s.len().checked_sub(1)?)?;
    let sign = match letter.as_bytes().first()?.to_ascii_uppercase() {
        b if b == positive => 1.0,
        b if b == negative => -1.0,
        _ => return None,
    };
    let v: f64 = value.trim().parse().ok()?;
    v.is_finite().then_some(sign * v)
}

/// `lon` in `-180.0..180.0`, unchanged when it already is.
fn longitude(lon: f64) -> f64 {
    if (-180.0..180.0).contains(&lon) {
        lon
    } else {
        wrap_longitude(lon)
    }
}

fn wind_kt(v: f64) -> Option<u16> {
    (v > 0.0 && v < 300.0).then(|| libm::round(v) as u16)
}

fn pressure_hpa(v: f64) -> Option<f32> {
    (800.0..1_100.0).contains(&v).then_some(v as f32)
}

impl RawStorm {
    fn active(self) -> Option<ActiveStorm> {
        let id = storm_id(&self.id).ok()?;
        let basin = Basin::of_id(&id)?;
        let latitude = number(self.latitude_numeric.as_ref())
            .or_else(|| number(self.latitude_numeric_snake.as_ref()))
            .or_else(|| hemisphere_degrees(self.latitude.as_deref()?, b'N', b'S'))
            .filter(|lat| (-90.0..=90.0).contains(lat))?;
        let longitude = number(self.longitude_numeric.as_ref())
            .or_else(|| number(self.longitude_numeric_snake.as_ref()))
            .or_else(|| hemisphere_degrees(self.longitude.as_deref()?, b'E', b'W'))
            .filter(|lon| (-360.0..=360.0).contains(lon))
            .map(longitude)?;
        let advisory = self.public_advisory;
        let advisory_time = advisory
            .as_ref()
            .and_then(|a| parse_rfc3339(a.issuance.as_deref()?));
        let last_update = self
            .last_update
            .as_deref()
            .and_then(parse_rfc3339)
            .or(advisory_time)?;
        let name = self
            .name
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| id.to_ascii_uppercase());
        Some(ActiveStorm {
            name,
            classification: self
                .classification
                .map(|c| c.trim().to_ascii_uppercase())
                .unwrap_or_default(),
            basin,
            wind_kt: number(self.intensity.as_ref()).and_then(wind_kt),
            pressure_hpa: number(self.pressure.as_ref()).and_then(pressure_hpa),
            latitude,
            longitude,
            movement_deg: number(self.movement_dir.as_ref())
                .filter(|d| (0.0..=360.0).contains(d))
                .map(|d| d as f32),
            movement_kt: number(self.movement_speed.as_ref())
                .filter(|s| (0.0..200.0).contains(s))
                .map(|mph| (mph / MPH_PER_KT) as f32),
            last_update,
            advisory_number: advisory
                .as_ref()
                .and_then(|a| a.adv_num.clone())
                .map(|n| n.trim().to_string())
                .filter(|n| !n.is_empty()),
            advisory_time,
            advisory_url: advisory
                .and_then(|a| a.url)
                .filter(|u| u.starts_with("https://")),
            id,
        })
    }
}

/// The active storms in NHC's basins, in NHC's order. Entries in other
/// basins, or without an id, a position or a time, are skipped.
pub fn parse_current_storms(body: &[u8]) -> Result<Vec<ActiveStorm>, ProviderError> {
    let storms: CurrentStorms =
        serde_json::from_slice(body).map_err(|e| ProviderError::Malformed(e.to_string()))?;
    Ok(storms
        .active_storms
        .into_iter()
        .filter_map(|v| serde_json::from_value::<RawStorm>(v).ok()?.active())
        .collect())
}

/// The CRC-32 (IEEE) table, built at compile time.
const CRC_TABLE: [u32; 256] = {
    let mut table = [0_u32; 256];
    let mut n = 0;
    while n < 256 {
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 == 1 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        table[n] = c;
        n += 1;
    }
    table
};

fn crc32(bytes: &[u8]) -> u32 {
    !bytes.iter().fold(!0_u32, |c, b| {
        CRC_TABLE[((c ^ u32::from(*b)) & 0xFF) as usize] ^ (c >> 8)
    })
}

/// Whether `bytes` start like a gzip stream.
pub fn is_gzip(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x1F, 0x8B])
}

fn corrupt(what: &str) -> ProviderError {
    ProviderError::Malformed(format!("gzip: {what}"))
}

/// The length of the gzip member header at the start of `bytes`.
fn gzip_header_len(bytes: &[u8]) -> Result<usize, ProviderError> {
    const FHCRC: u8 = 0x02;
    const FEXTRA: u8 = 0x04;
    const FNAME: u8 = 0x08;
    const FCOMMENT: u8 = 0x10;
    if bytes.len() < 10 || !is_gzip(bytes) {
        return Err(corrupt("not a gzip stream"));
    }
    if bytes[2] != 8 {
        return Err(corrupt("not deflate"));
    }
    let flags = bytes[3];
    if flags & 0xE0 != 0 {
        return Err(corrupt("reserved flags"));
    }
    let mut pos = 10;
    if flags & FEXTRA != 0 {
        let len = bytes
            .get(pos..pos + 2)
            .ok_or_else(|| corrupt("truncated"))?;
        pos += 2 + usize::from(u16::from_le_bytes([len[0], len[1]]));
    }
    for flag in [FNAME, FCOMMENT] {
        if flags & flag != 0 {
            let rest = bytes.get(pos..).ok_or_else(|| corrupt("truncated"))?;
            let end = rest
                .iter()
                .position(|b| *b == 0)
                .ok_or_else(|| corrupt("truncated"))?;
            pos += end + 1;
        }
    }
    if flags & FHCRC != 0 {
        pos += 2;
    }
    if pos > bytes.len() {
        return Err(corrupt("truncated"));
    }
    Ok(pos)
}

/// Inflates a gzip stream (one member or several back to back) of at most
/// `limit` inflated bytes, checking each member's CRC and length.
pub fn gunzip(bytes: &[u8], limit: usize) -> Result<Vec<u8>, ProviderError> {
    let mut out: Vec<u8> = Vec::new();
    let mut pos = 0;
    while pos < bytes.len() {
        // Padding after the last member.
        if !out.is_empty() && bytes[pos..].iter().all(|b| *b == 0) {
            break;
        }
        let input = &bytes[pos..];
        let start = gzip_header_len(input)?;
        let member = out.len();
        let mut decomp = Box::<DecompressorOxide>::default();
        let mut in_pos = start;
        let mut out_pos = member;
        out.resize(
            (member + input.len().saturating_mul(4).max(1 << 16)).min(limit),
            0,
        );
        loop {
            let (status, read, written) = decompress(
                &mut decomp,
                &input[in_pos..],
                &mut out,
                out_pos,
                TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF,
            );
            in_pos += read;
            out_pos += written;
            match status {
                TINFLStatus::Done => break,
                TINFLStatus::HasMoreOutput if out.len() < limit => {
                    let grown = out.len().saturating_mul(2).min(limit);
                    out.resize(grown, 0);
                }
                TINFLStatus::HasMoreOutput => {
                    return Err(corrupt("inflates past the size limit"));
                }
                _ => return Err(corrupt("truncated or corrupt")),
            }
        }
        out.truncate(out_pos);
        let trailer = input
            .get(in_pos..in_pos + 8)
            .ok_or_else(|| corrupt("truncated"))?;
        let crc = u32::from_le_bytes([trailer[0], trailer[1], trailer[2], trailer[3]]);
        let size = u32::from_le_bytes([trailer[4], trailer[5], trailer[6], trailer[7]]);
        let inflated = &out[member..];
        if crc32(inflated) != crc || inflated.len() as u32 != size {
            return Err(corrupt("checksum mismatch"));
        }
        pos += in_pos + 8;
    }
    if pos == 0 {
        return Err(corrupt("empty"));
    }
    Ok(out)
}

/// One parsed ATCF row.
#[derive(Clone, Debug, PartialEq)]
struct AtcfRow<'a> {
    aid: &'a str,
    init: UnixTime,
    tau: i64,
    fix: TrackFix,
}

/// `YYYYMMDDHH` in UTC.
fn atcf_time(s: &str) -> Option<UnixTime> {
    let b = s.as_bytes();
    if b.len() != 10 || !b.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let num = |r: core::ops::Range<usize>| s.get(r)?.parse::<u16>().ok();
    let year = i32::from(num(0..4)?);
    let month = u8::try_from(num(4..6)?).ok()?;
    let day = u8::try_from(num(6..8)?).ok()?;
    let hour = i64::from(num(8..10)?);
    if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) || hour > 23 {
        return None;
    }
    Some(UnixTime(
        days_from_civil(year, month, day) * 86_400 + hour * 3_600,
    ))
}

/// Tenths of a degree with a hemisphere letter: `395N`, `668W`.
fn atcf_degrees(s: &str, positive: u8, negative: u8, max: f64) -> Option<f64> {
    let (digits, letter) = s.split_at_checked(s.len().checked_sub(1)?)?;
    if digits.is_empty() || digits.len() > 4 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let value = f64::from(digits.parse::<u16>().ok()?) / 10.0;
    let sign = match letter.as_bytes().first()?.to_ascii_uppercase() {
        b if b == positive => 1.0,
        b if b == negative => -1.0,
        _ => return None,
    };
    (value <= max).then_some(sign * value)
}

/// Fields up to the storm type; a row with fewer is cut short and skipped.
const ATCF_FIELDS: usize = 11;

/// One a-deck or b-deck row, or `None` when it doesn't parse. Rows with a
/// negative forecast hour or a position of 0°, 0° are skipped.
fn atcf_row(line: &str) -> Option<AtcfRow<'_>> {
    let mut fields = [""; ATCF_FIELDS];
    let mut parts = line.split(',');
    for slot in &mut fields {
        *slot = parts.next()?.trim();
    }
    let [basin, _, init, _, aid, tau, lat, lon, wind, pressure, _] = fields;
    if Basin::of_id(basin).is_none() || aid.is_empty() {
        return None;
    }
    let init = atcf_time(init)?;
    let tau: i64 = tau.parse().ok().filter(|t| (0..=240).contains(t))?;
    let latitude = atcf_degrees(lat, b'N', b'S', 90.0)?;
    let longitude = longitude(atcf_degrees(lon, b'E', b'W', 180.0)?);
    if libm::fabs(latitude) < 1e-9 && libm::fabs(longitude) < 1e-9 {
        return None;
    }
    Some(AtcfRow {
        aid,
        init,
        tau,
        fix: TrackFix {
            time: UnixTime(init.0 + tau * 3_600),
            latitude,
            longitude,
            wind_kt: wind.parse::<f64>().ok().and_then(wind_kt),
            pressure_hpa: pressure.parse::<f64>().ok().and_then(pressure_hpa),
        },
    })
}

/// The deck's text: inflated when `body` is gzip, else as it is (a host
/// may have inflated it already).
fn deck_text(body: &[u8]) -> Result<alloc::borrow::Cow<'_, [u8]>, ProviderError> {
    if is_gzip(body) {
        gunzip(body, MAX_DECK_BYTES).map(alloc::borrow::Cow::Owned)
    } else {
        Ok(alloc::borrow::Cow::Borrowed(body))
    }
}

/// Every row of `text` that parses.
fn atcf_rows(text: &[u8]) -> impl Iterator<Item = AtcfRow<'_>> {
    text.split(|b| *b == b'\n')
        .filter_map(|line| core::str::from_utf8(line).ok())
        .filter_map(atcf_row)
}

/// The error for a non-empty deck without a single row that parses.
fn no_rows(text: &[u8]) -> Result<(), ProviderError> {
    if text.iter().all(u8::is_ascii_whitespace) {
        Ok(())
    } else {
        Err(ProviderError::Malformed("no ATCF rows".to_string()))
    }
}

/// The official forecast and the `aids` tracks of an a-deck (gzip as NHC
/// serves it, or already inflated). Each aid's newest run is kept, one fix
/// per forecast hour, in time order; aids without rows are left out. The
/// official track is empty before NHC's first advisory.
pub fn parse_adeck(
    body: &[u8],
    aids: &[&str],
) -> Result<(Vec<TrackFix>, AidTracks), ProviderError> {
    let text = deck_text(body)?;
    let mut newest: BTreeMap<String, (UnixTime, BTreeMap<i64, TrackFix>)> = BTreeMap::new();
    let mut rows = 0_usize;
    for row in atcf_rows(&text) {
        rows += 1;
        let key = if row.aid.eq_ignore_ascii_case(OFFICIAL_AID) {
            OFFICIAL_AID
        } else if let Some(aid) = aids.iter().find(|a| a.eq_ignore_ascii_case(row.aid)) {
            *aid
        } else {
            continue;
        };
        let run = newest
            .entry(key.to_string())
            .or_insert_with(|| (row.init, BTreeMap::new()));
        if row.init > run.0 {
            *run = (row.init, BTreeMap::new());
        }
        if row.init == run.0 {
            run.1.entry(row.tau).or_insert(row.fix);
        }
    }
    if rows == 0 {
        no_rows(&text)?;
    }
    let mut tracks: AidTracks = newest
        .into_iter()
        .map(|(aid, (_, fixes))| (aid, fixes.into_values().collect()))
        .collect();
    let official = tracks.remove(OFFICIAL_AID).unwrap_or_default();
    Ok((official, tracks))
}

/// The observed track of a b-deck: one fix per time, oldest first.
pub fn parse_bdeck(body: &[u8]) -> Result<Vec<TrackFix>, ProviderError> {
    let text = deck_text(body)?;
    let mut fixes: BTreeMap<UnixTime, TrackFix> = BTreeMap::new();
    let mut rows = 0_usize;
    for row in atcf_rows(&text) {
        rows += 1;
        if row.tau == 0 {
            fixes.entry(row.fix.time).or_insert(row.fix);
        }
    }
    if rows == 0 {
        no_rows(&text)?;
    }
    Ok(fixes.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_matches_the_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn ids_are_checked() {
        assert!(adeck_request("al102026").is_ok());
        assert_eq!(
            bdeck_request("EP092023").unwrap().url,
            "https://ftp.nhc.noaa.gov/atcf/btk/bep092023.dat"
        );
        for bad in ["wp102026", "al1020", "al10202x", "../etc/x", "al102026/"] {
            assert!(adeck_request(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn times_and_positions_parse() {
        assert_eq!(atcf_time("2026102218"), Some(UnixTime(1_792_692_000)));
        assert_eq!(atcf_time("2026022918"), None);
        assert_eq!(atcf_time("2026102224"), None);
        assert_eq!(atcf_time("202610221"), None);
        assert_eq!(atcf_degrees("395N", b'N', b'S', 90.0), Some(39.5));
        assert_eq!(atcf_degrees("668W", b'E', b'W', 180.0), Some(-66.8));
        assert_eq!(atcf_degrees("1795E", b'E', b'W', 180.0), Some(179.5));
        assert_eq!(atcf_degrees("1805E", b'E', b'W', 180.0), None);
        assert_eq!(atcf_degrees("39.5N", b'N', b'S', 90.0), None);
        assert_eq!(atcf_degrees("N", b'N', b'S', 90.0), None);
        assert_eq!(atcf_degrees("", b'N', b'S', 90.0), None);
        assert_eq!(hemisphere_degrees("17.1N", b'N', b'S'), Some(17.1));
        assert_eq!(hemisphere_degrees("58.4W", b'E', b'W'), Some(-58.4));
        assert_eq!(hemisphere_degrees("58.4", b'E', b'W'), None);
    }

    #[test]
    fn rows_need_every_field_to_the_storm_type() {
        let row = "AL, 10, 2026102218, 03, OFCL,  12, 413N,  627W,  65,    0, EX,  34, NEQ";
        let parsed = atcf_row(row).unwrap();
        assert_eq!((parsed.aid, parsed.tau), ("OFCL", 12));
        assert_eq!(parsed.fix.time, UnixTime(1_792_692_000 + 12 * 3_600));
        assert_eq!((parsed.fix.latitude, parsed.fix.longitude), (41.3, -62.7));
        assert_eq!(
            (parsed.fix.wind_kt, parsed.fix.pressure_hpa),
            (Some(65), None)
        );
        // Cut before the storm type: the wind and pressure may be cut too.
        assert_eq!(
            atcf_row("AL, 10, 2026102218, 03, OFCL,  12, 413N,  627W,  6"),
            None
        );
        assert_eq!(
            atcf_row("WP, 10, 2026102218, 03, OFCL,  12, 413N,  627W,  65, 0, EX"),
            None
        );
        assert_eq!(
            atcf_row("AL, 10, 2026102218, 01, CARQ, -24, 310N,  714W,  40, 0, EX"),
            None
        );
        assert_eq!(atcf_row(""), None);
    }

    #[test]
    fn current_storms_take_numbers_as_text_or_numbers() {
        let body = br#"{"activeStorms":[
            {"id":"al052017","name":"Irma","classification":"HU","intensity":125,
             "pressure":941,"latitude":"22.9N","longitude":"79.9W","latitude_numeric":22.9,
             "longitude_numeric":-79.9,"movementDir":270,"movementSpeed":9,
             "lastUpdate":"2017-09-09T16:00:00.000Z"},
            {"id":"wp012017","name":"Other","latitudeNumeric":1.0,"longitudeNumeric":1.0,
             "lastUpdate":"2017-09-09T16:00:00.000Z"},
            {"id":"al062017","name":"No position","lastUpdate":"2017-09-09T16:00:00.000Z"},
            {"name":"No id"},
            "not an object"
        ]}"#;
        let storms = parse_current_storms(body).unwrap();
        assert_eq!(storms.len(), 1);
        let irma = &storms[0];
        assert_eq!((irma.wind_kt, irma.pressure_hpa), (Some(125), Some(941.0)));
        assert_eq!((irma.latitude, irma.longitude), (22.9, -79.9));
        assert!((irma.movement_kt.unwrap() - 7.82).abs() < 0.01);
        assert_eq!(irma.advisory_url, None);
        assert!(parse_current_storms(b"[]").is_err());
        assert!(parse_current_storms(b"{}").is_err());
        assert_eq!(
            parse_current_storms(br#"{"activeStorms":[]}"#),
            Ok(Vec::new())
        );
    }
}
