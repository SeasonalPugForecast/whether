//! Sunrise and sunset by calculation (the sunrise equation with NOAA's
//! constants; within about two minutes away from the poles), for providers that
//! report neither, such as the US National Weather Service; the same
//! equation at other altitudes gives the twilights and golden hours.

use crate::time::{LocalDate, UnixTime, days_from_civil};

/// The sun's centre at sunrise and sunset: 0.833° below the horizon
/// (refraction and its radius).
pub const SUNRISE: f64 = -0.833;
/// The end of civil twilight: the sun 6° below the horizon.
pub const CIVIL: f64 = -6.0;
/// The end of nautical twilight: 12° below.
pub const NAUTICAL: f64 = -12.0;
/// The end of astronomical twilight (full night): 18° below.
pub const ASTRONOMICAL: f64 = -18.0;
/// The edge of the golden hour: the sun 6° above the horizon.
pub const GOLDEN: f64 = 6.0;

const J2000: f64 = 2_451_545.0;
const UNIX_EPOCH_JD: f64 = 2_440_587.5;

/// The sun on one local date.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SunDay {
    /// It rises and sets.
    RiseSet {
        /// Sunrise.
        rise: UnixTime,
        /// Sunset.
        set: UnixTime,
    },
    /// It stays up all day (midnight sun).
    AlwaysUp,
    /// It stays down all day (polar night).
    AlwaysDown,
}

/// `x` in 0..360 (`rem_euclid` needs std).
fn degrees(x: f64) -> f64 {
    let r = libm::fmod(x, 360.0);
    if r < 0.0 { r + 360.0 } else { r }
}

fn sin_deg(x: f64) -> f64 {
    libm::sin(x.to_radians())
}

fn cos_deg(x: f64) -> f64 {
    libm::cos(x.to_radians())
}

/// The sun on `date` at `latitude`, `longitude` (degrees, east positive).
pub fn sun_day(date: LocalDate, latitude: f64, longitude: f64) -> SunDay {
    sun_crossing(date, latitude, longitude, SUNRISE)
}

/// Solar noon on `date` at `longitude`: the sun at its highest.
pub fn solar_noon(date: LocalDate, longitude: f64) -> UnixTime {
    unix(solar(date, longitude).0)
}

#[allow(clippy::cast_possible_truncation)] // Unix seconds of a date fit in i64
fn unix(jd: f64) -> UnixTime {
    UnixTime(libm::round((jd - UNIX_EPOCH_JD) * 86_400.0) as i64)
}

/// When the sun's centre crosses `altitude` degrees (negative below the
/// horizon) on `date`: `rise` going up in the morning, `set` going down in the
/// evening; `AlwaysUp` or `AlwaysDown` when it stays above or below all day.
pub fn sun_crossing(date: LocalDate, latitude: f64, longitude: f64, altitude: f64) -> SunDay {
    let (transit, declination) = solar(date, longitude);
    let cos_hour = (sin_deg(altitude) - sin_deg(latitude) * libm::sin(declination))
        / (cos_deg(latitude) * libm::cos(declination));
    if cos_hour > 1.0 {
        return SunDay::AlwaysDown;
    }
    if cos_hour < -1.0 {
        return SunDay::AlwaysUp;
    }
    let half_day = libm::acos(cos_hour).to_degrees() / 360.0;
    SunDay::RiseSet {
        rise: unix(transit - half_day),
        set: unix(transit + half_day),
    }
}

/// The Julian day of solar noon on `date` at `longitude`, and the sun's
/// declination (radians).
fn solar(date: LocalDate, longitude: f64) -> (f64, f64) {
    // Julian day at noon UTC of the date, then the mean solar noon at the place.
    #[allow(clippy::cast_precision_loss)] // day counts are far below 2^52
    let noon = days_from_civil(date.year, date.month, date.day) as f64 + UNIX_EPOCH_JD + 0.5;
    let n = libm::round(noon - J2000 + 0.0008);
    let mean_noon = n - longitude / 360.0;
    let anomaly = degrees(357.529_1 + 0.985_600_28 * mean_noon);
    let center = 1.914_8 * sin_deg(anomaly)
        + 0.02 * sin_deg(2.0 * anomaly)
        + 0.000_3 * sin_deg(3.0 * anomaly);
    let ecliptic = degrees(anomaly + center + 180.0 + 102.937_2);
    let transit =
        J2000 + mean_noon + 0.005_3 * sin_deg(anomaly) - 0.006_9 * sin_deg(2.0 * ecliptic);
    let declination = libm::asin(sin_deg(ecliptic) * sin_deg(23.439_7));
    (transit, declination)
}

impl SunDay {
    /// Whether the sun is up at `t` (a time on this day).
    pub fn is_up(self, t: UnixTime) -> bool {
        match self {
            Self::RiseSet { rise, set } => rise <= t && t < set,
            Self::AlwaysUp => true,
            Self::AlwaysDown => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(year: i32, month: u8, day: u8) -> LocalDate {
        LocalDate { year, month, day }
    }

    fn near(a: UnixTime, b: i64, tolerance: i64) -> bool {
        (a.0 - b).abs() <= tolerance
    }

    #[test]
    fn new_york_matches_open_meteo_within_two_minutes() {
        // Open-Meteo for 10001 on 2026-11-02: 11:27:32 and 21:51:05 UTC.
        let SunDay::RiseSet { rise, set } = sun_day(date(2026, 11, 2), 40.7143, -74.0060) else {
            panic!("the sun rises in New York");
        };
        assert!(near(rise, 1_793_618_852, 120), "{rise:?}");
        assert!(near(set, 1_793_656_265, 120), "{set:?}");
        assert!(sun_day(date(2026, 11, 2), 40.7, -74.0).is_up(UnixTime(1_793_637_000)));
        assert!(!sun_day(date(2026, 11, 2), 40.7, -74.0).is_up(UnixTime(1_793_610_000)));
    }

    #[test]
    fn twilights_come_in_order_around_the_sunrise() {
        let d = date(2026, 10, 17);
        let at = |altitude| match sun_crossing(d, 40.7143, -74.0060, altitude) {
            SunDay::RiseSet { rise, set } => (rise.0, set.0),
            other => panic!("{other:?}"),
        };
        let (astro, nautical, civil, sun, golden) = (
            at(ASTRONOMICAL),
            at(NAUTICAL),
            at(CIVIL),
            at(SUNRISE),
            at(GOLDEN),
        );
        // Mornings: each deeper twilight starts earlier; evenings mirror them.
        assert!(
            astro.0 < nautical.0 && nautical.0 < civil.0 && civil.0 < sun.0 && sun.0 < golden.0
        );
        assert!(
            golden.1 < sun.1 && sun.1 < civil.1 && civil.1 < nautical.1 && nautical.1 < astro.1
        );
        // Civil twilight lasts about 26 minutes at 40° N in October.
        assert!(
            (20 * 60..32 * 60).contains(&(sun.0 - civil.0)),
            "{}",
            sun.0 - civil.0
        );
        // Solar noon halves the day.
        let noon = solar_noon(d, -74.0060).0;
        assert!((noon - i64::midpoint(sun.0, sun.1)).abs() <= 60, "{noon}");
    }

    #[test]
    fn deep_twilight_can_last_all_night_in_summer_up_north() {
        // Oslo at midsummer: never 18° below, so no astronomical night.
        assert_eq!(
            sun_crossing(date(2026, 6, 21), 59.91, 10.75, ASTRONOMICAL),
            SunDay::AlwaysUp
        );
    }

    #[test]
    fn the_poles_have_whole_days_and_nights() {
        assert_eq!(
            sun_day(date(2026, 6, 21), 78.2, 15.6),
            SunDay::AlwaysUp,
            "Svalbard"
        );
        assert_eq!(sun_day(date(2026, 12, 21), 78.2, 15.6), SunDay::AlwaysDown);
        assert!(matches!(
            sun_day(date(2026, 12, 21), -33.87, 151.21),
            SunDay::RiseSet { .. }
        ));
    }
}
