//! Time without a time-zone database.
//!
//! Instants are unix seconds; local time comes from a fixed UTC offset supplied by
//! the provider. Civil-date conversion uses Howard Hinnant's algorithms
//! ("chrono-Compatible Low-Level Date Algorithms").

use alloc::format;
use alloc::string::String;

const SECS_PER_DAY: i64 = 86_400;

/// Seconds since 1970-01-01T00:00:00Z.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(transparent)
)]
pub struct UnixTime(pub i64);

/// A fixed offset from UTC, in seconds east of UTC.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(transparent)
)]
pub struct UtcOffset(pub i32);

/// A calendar date in the location's local time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LocalDate {
    /// Proleptic Gregorian year.
    pub year: i32,
    /// Month, 1–12.
    pub month: u8,
    /// Day of the month, 1–31.
    pub day: u8,
}

/// Day of the week.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[allow(missing_docs)]
pub enum Weekday {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

/// A local date and time of day.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LocalDateTime {
    /// The calendar date.
    pub date: LocalDate,
    /// Hour, 0–23.
    pub hour: u8,
    /// Minute, 0–59.
    pub minute: u8,
    /// Second, 0–59.
    pub second: u8,
    /// Day of the week.
    pub weekday: Weekday,
}

impl Default for LocalDate {
    /// 1970-01-01.
    fn default() -> Self {
        Self {
            year: 1970,
            month: 1,
            day: 1,
        }
    }
}

impl LocalDate {
    /// Days since 1970-01-01 for this date.
    pub fn days_since_epoch(self) -> i64 {
        days_from_civil(self.year, self.month, self.day)
    }
}

/// Days since 1970-01-01 for a proleptic Gregorian date.
pub fn days_from_civil(year: i32, month: u8, day: u8) -> i64 {
    let (m, d) = (i64::from(month), i64::from(day));
    let y = i64::from(year) - i64::from(m <= 2);
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The proleptic Gregorian `(year, month, day)` for days since 1970-01-01.
pub fn civil_from_days(days: i64) -> (i32, u8, u8) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    // INVARIANT: month is 1..=12 and day is 1..=31; years beyond i32 are unreachable
    // for any timestamp this crate handles.
    (
        i32::try_from(year).unwrap_or(i32::MAX),
        u8::try_from(month).unwrap_or(1),
        u8::try_from(day).unwrap_or(1),
    )
}

/// The weekday of a day count since 1970-01-01 (a Thursday).
pub fn weekday_from_days(days: i64) -> Weekday {
    match (days + 3).rem_euclid(7) {
        0 => Weekday::Mon,
        1 => Weekday::Tue,
        2 => Weekday::Wed,
        3 => Weekday::Thu,
        4 => Weekday::Fri,
        5 => Weekday::Sat,
        _ => Weekday::Sun,
    }
}

/// Whether `year` is a Gregorian leap year.
pub fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// The number of days in `month` (1–12) of `year`, or 0 for an invalid month.
pub fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

impl UnixTime {
    /// Local date and time at the given offset.
    pub fn to_local(self, offset: UtcOffset) -> LocalDateTime {
        let secs = self.0 + i64::from(offset.0);
        let days = secs.div_euclid(SECS_PER_DAY);
        let sod = secs.rem_euclid(SECS_PER_DAY);
        let (year, month, day) = civil_from_days(days);
        // INVARIANT: sod < 86_400, so each component fits in u8.
        LocalDateTime {
            date: LocalDate { year, month, day },
            hour: u8::try_from(sod / 3_600).unwrap_or(0),
            minute: u8::try_from(sod % 3_600 / 60).unwrap_or(0),
            second: u8::try_from(sod % 60).unwrap_or(0),
            weekday: weekday_from_days(days),
        }
    }

    /// Local calendar date at the given offset.
    pub fn local_date(self, offset: UtcOffset) -> LocalDate {
        self.to_local(offset).date
    }
}

/// Parses `YYYY-MM-DDTHH:MM:SS` followed by `Z`, `+HH:MM` or `-HH:MM`.
/// Fractional seconds are accepted and ignored.
pub fn parse_rfc3339(s: &str) -> Option<UnixTime> {
    let b = s.as_bytes();
    if b.len() < 20
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
    {
        return None;
    }
    let year = i32::try_from(digits(&b[0..4])?).ok()?;
    let (month, day) = (two(&b[5..7])?, two(&b[8..10])?);
    let (hour, minute, second) = (two(&b[11..13])?, two(&b[14..16])?, two(&b[17..19])?);
    let mut rest = &s[19..];
    if let Some(frac) = rest.strip_prefix('.') {
        let end = frac
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(frac.len());
        if end == 0 {
            return None;
        }
        rest = &frac[end..];
    }
    let offset = parse_utc_offset(rest)?;
    let secs = civil_seconds(year, month, day, hour, minute, second)?;
    Some(UnixTime(secs - i64::from(offset.0)))
}

/// Parses an HTTP IMF-fixdate such as `Mon, 05 Oct 2026 11:12:32 GMT`.
pub fn parse_http_date(s: &str) -> Option<UnixTime> {
    let b = s.as_bytes();
    if b.len() != 29 || &b[3..5] != b", " || b[7] != b' ' || b[11] != b' ' || b[16] != b' ' {
        return None;
    }
    if &b[25..] != b" GMT" || b[19] != b':' || b[22] != b':' {
        return None;
    }
    let day = two(&b[5..7])?;
    let month = month_from_abbrev(&b[8..11])?;
    let year = i32::try_from(digits(&b[12..16])?).ok()?;
    let (hour, minute, second) = (two(&b[17..19])?, two(&b[20..22])?, two(&b[23..25])?);
    Some(UnixTime(civil_seconds(
        year, month, day, hour, minute, second,
    )?))
}

/// Parses `Z`, `+HH:MM` or `-HH:MM`.
pub fn parse_utc_offset(s: &str) -> Option<UtcOffset> {
    if s == "Z" || s == "z" {
        return Some(UtcOffset(0));
    }
    let b = s.as_bytes();
    if b.len() != 6 || b[3] != b':' {
        return None;
    }
    let sign = match b[0] {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let (hours, minutes) = (two(&b[1..3])?, two(&b[4..6])?);
    if hours > 23 || minutes > 59 {
        return None;
    }
    Some(UtcOffset(
        sign * (i32::from(hours) * 3_600 + i32::from(minutes) * 60),
    ))
}

/// Formats a duration as hours and minutes, e.g. `11h 20m` or `45m`.
pub fn format_hm(secs: u32) -> String {
    let (hours, minutes) = (secs / 3_600, secs % 3_600 / 60);
    if hours == 0 {
        format!("{minutes}m")
    } else {
        format!("{hours}h {minutes}m")
    }
}

fn civil_seconds(year: i32, month: u8, day: u8, hour: u8, minute: u8, second: u8) -> Option<i64> {
    if !(1..=12).contains(&month)
        || day == 0
        || day > days_in_month(year, month)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    let days = days_from_civil(year, month, day);
    Some(days * SECS_PER_DAY + i64::from(hour) * 3_600 + i64::from(minute) * 60 + i64::from(second))
}

fn digits(b: &[u8]) -> Option<u32> {
    if b.is_empty() || b.len() > 9 {
        return None;
    }
    b.iter().try_fold(0u32, |acc, &c| {
        c.is_ascii_digit().then(|| acc * 10 + u32::from(c - b'0'))
    })
}

fn two(b: &[u8]) -> Option<u8> {
    if b.len() == 2 {
        u8::try_from(digits(b)?).ok()
    } else {
        None
    }
}

fn month_from_abbrev(b: &[u8]) -> Option<u8> {
    const MONTHS: [&[u8; 3]; 12] = [
        b"Jan", b"Feb", b"Mar", b"Apr", b"May", b"Jun", b"Jul", b"Aug", b"Sep", b"Oct", b"Nov",
        b"Dec",
    ];
    MONTHS
        .iter()
        .position(|m| m.as_slice() == b)
        .and_then(|i| u8::try_from(i + 1).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn local(t: i64, off: i32) -> (i32, u8, u8, u8, u8, u8, Weekday) {
        let l = UnixTime(t).to_local(UtcOffset(off));
        (
            l.date.year,
            l.date.month,
            l.date.day,
            l.hour,
            l.minute,
            l.second,
            l.weekday,
        )
    }

    #[test]
    fn reference_vectors() {
        assert_eq!(local(0, 0), (1970, 1, 1, 0, 0, 0, Weekday::Thu));
        assert_eq!(local(-1, 0), (1969, 12, 31, 23, 59, 59, Weekday::Wed));
        assert_eq!(local(951_782_400, 0), (2000, 2, 29, 0, 0, 0, Weekday::Tue));
        assert_eq!(
            local(1_791_158_400, 0),
            (2026, 10, 5, 0, 0, 0, Weekday::Mon)
        );
        assert_eq!(local(4_102_444_800, 0), (2100, 1, 1, 0, 0, 0, Weekday::Fri));
        assert_eq!(
            local(253_402_300_799, 0),
            (9999, 12, 31, 23, 59, 59, Weekday::Fri)
        );
    }

    #[test]
    fn offsets_shift_the_local_date() {
        let d = UnixTime(1_791_151_200).local_date(UtcOffset(7_200));
        assert_eq!(
            d,
            LocalDate {
                year: 2026,
                month: 10,
                day: 5
            }
        );
        assert_eq!(
            local(1_791_212_400, -14_400),
            (2026, 10, 5, 11, 0, 0, Weekday::Mon)
        );
    }

    #[test]
    fn parses_http_dates() {
        assert_eq!(
            parse_http_date("Mon, 05 Oct 2026 11:12:32 GMT"),
            Some(UnixTime(1_791_198_752))
        );
        assert_eq!(parse_http_date("Mon, 05 Oct 2026 11:12:32 UTC"), None);
        assert_eq!(parse_http_date("Mon, 31 Feb 2026 11:12:32 GMT"), None);
        assert_eq!(parse_http_date("Mon, 05 Foo 2026 11:12:32 GMT"), None);
        assert_eq!(parse_http_date(""), None);
    }

    #[test]
    fn parses_rfc3339() {
        assert_eq!(
            parse_rfc3339("2026-10-05T11:00:00Z"),
            Some(UnixTime(1_791_198_000))
        );
        assert_eq!(
            parse_rfc3339("2026-10-05T13:00:00+02:00"),
            Some(UnixTime(1_791_198_000))
        );
        assert_eq!(
            parse_rfc3339("2026-10-05T07:00:00-04:00"),
            Some(UnixTime(1_791_198_000))
        );
        assert_eq!(
            parse_rfc3339("2026-10-05T11:00:00.250Z"),
            Some(UnixTime(1_791_198_000))
        );
        assert_eq!(parse_rfc3339("2026-10-05T11:00:00"), None);
        assert_eq!(parse_rfc3339("2026-13-05T11:00:00Z"), None);
        assert_eq!(parse_rfc3339("2026-10-05T24:00:00Z"), None);
        assert_eq!(parse_rfc3339("2026-10-05T11:00:00.Z"), None);
    }

    #[test]
    fn parses_offsets() {
        assert_eq!(parse_utc_offset("Z"), Some(UtcOffset(0)));
        assert_eq!(parse_utc_offset("+02:00"), Some(UtcOffset(7_200)));
        assert_eq!(parse_utc_offset("-10:00"), Some(UtcOffset(-36_000)));
        assert_eq!(parse_utc_offset("+05:45"), Some(UtcOffset(20_700)));
        assert_eq!(parse_utc_offset("+0200"), None);
        assert_eq!(parse_utc_offset("+24:00"), None);
    }

    #[test]
    fn formats_durations() {
        assert_eq!(format_hm(40_800), "11h 20m");
        assert_eq!(format_hm(2_700), "45m");
        assert_eq!(format_hm(0), "0m");
    }

    #[test]
    fn leap_years() {
        assert!(is_leap_year(2000) && is_leap_year(2024));
        assert!(!is_leap_year(1900) && !is_leap_year(2100) && !is_leap_year(2026));
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2026, 13), 0);
    }

    proptest! {
        #[test]
        fn civil_round_trip(y in -10_000i32..10_000, m in 1u8..=12, d in 1u8..=31) {
            prop_assume!(d <= days_in_month(y, m));
            prop_assert_eq!(civil_from_days(days_from_civil(y, m, d)), (y, m, d));
        }

        #[test]
        fn consecutive_days_are_consecutive(z in -4_000_000i64..4_000_000) {
            let (y, m, d) = civil_from_days(z);
            prop_assert_eq!(days_from_civil(y, m, d), z);
        }

        #[test]
        fn parsers_never_panic(s in "\\PC{0,40}") {
            let _ = parse_rfc3339(&s);
            let _ = parse_http_date(&s);
            let _ = parse_utc_offset(&s);
        }
    }
}
