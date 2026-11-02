//! `sunrise/3.0/sun`: today's sunrise and sunset, applied to the matching day.

use alloc::string::String;

use serde::Deserialize;
use whether_core::builder::SnapshotBuilder;
use whether_core::time::{UnixTime, UtcOffset, parse_rfc3339};

use crate::error::ProviderError;

#[derive(Deserialize)]
struct Response {
    properties: Properties,
}

#[derive(Deserialize)]
struct Properties {
    #[serde(default)]
    sunrise: Option<Event>,
    #[serde(default)]
    sunset: Option<Event>,
}

#[derive(Deserialize)]
struct Event {
    /// `2026-10-05T07:03-04:00`: minutes only, with the requested offset.
    time: Option<String>,
}

/// Parses MET's times, which may omit the seconds.
pub(super) fn parse_time(s: &str) -> Option<UnixTime> {
    parse_rfc3339(s).or_else(|| {
        // "YYYY-MM-DDTHH:MM" + offset: insert ":00".
        let (head, tail) = (s.get(..16)?, s.get(16..)?);
        parse_rfc3339(&alloc::format!("{head}:00{tail}"))
    })
}

/// Sets sunrise, sunset and daylight on the day they fall on. Polar day or night
/// (no events) leaves the days unchanged.
pub(super) fn parse(
    body: &[u8],
    offset: UtcOffset,
    out: &mut SnapshotBuilder,
) -> Result<(), ProviderError> {
    let r: Response = serde_json::from_slice(body)
        .map_err(|e| ProviderError::Malformed(alloc::format!("MET Norway sun: {e}")))?;
    let at = |e: Option<Event>| e.and_then(|e| e.time).and_then(|t| parse_time(&t));
    let (Some(sunrise), Some(sunset)) = (at(r.properties.sunrise), at(r.properties.sunset)) else {
        return Ok(());
    };
    let date = sunrise.local_date(offset);
    if let Some(day) = out.daily_mut().iter_mut().find(|d| d.date == date) {
        day.sunrise = Some(sunrise);
        day.sunset = Some(sunset);
        day.daylight_s = u32::try_from(sunset.0 - sunrise.0).ok();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_without_seconds() {
        assert_eq!(
            parse_time("2026-10-05T07:03-04:00"),
            parse_rfc3339("2026-10-05T07:03:00-04:00")
        );
        assert_eq!(
            parse_time("2026-10-05T11:03:00Z"),
            parse_rfc3339("2026-10-05T11:03:00Z")
        );
        assert!(parse_time("2026-10-05").is_none());
    }
}
