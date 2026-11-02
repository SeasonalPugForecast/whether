//! Gap filling: merging a supplement provider's snapshot into the primary one
//!. The primary's values always win.

use alloc::vec::Vec;

use crate::model::{AirQuality, Current, Daily, Hourly, WeatherSnapshot};

/// What a supplement contributed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FillReport {
    /// Individual fields filled in existing current, hourly, daily and air-quality records.
    pub fields_filled: u32,
    /// Whole sections taken because the primary had none (current, hourly, nowcast,
    /// air quality).
    pub sections_added: u32,
    /// Days appended after the primary's last day.
    pub days_appended: u32,
    /// Hours added before the primary's first (earlier today) and after its
    /// last (One Call stops at 48).
    pub hours_appended: u32,
    /// Alerts added.
    pub alerts_added: u32,
}

impl FillReport {
    /// Whether the supplement contributed anything.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Copies each listed `Option` field from `src` into `dst` where `dst` is `None`,
/// counting the fields filled.
macro_rules! fill {
    ($dst:expr, $src:expr, $count:expr; $($field:ident),+ $(,)?) => {
        $(
            if $dst.$field.is_none() && $src.$field.is_some() {
                $dst.$field = $src.$field.clone();
                $count += 1;
            }
        )+
    };
}

fn fill_current(dst: &mut Current, src: &Current) -> u32 {
    let mut n = 0;
    fill!(dst, src, n; temperature, apparent, dew_point, humidity, pressure_msl, cloud_cover,
        wind_speed, wind_gust, wind_dir, visibility, uv_index, precipitation);
    n
}

fn fill_hourly(dst: &mut Hourly, src: &Hourly) -> u32 {
    let mut n = 0;
    fill!(dst, src, n; temperature, apparent, precip_probability, precipitation, wind_speed,
        wind_gust, wind_dir, humidity, pressure_msl, uv_index, visibility, cloud_cover);
    n
}

fn fill_daily(dst: &mut Daily, src: &Daily) -> u32 {
    let mut n = 0;
    fill!(dst, src, n; temp_max, temp_min, apparent_max, apparent_min, precip_sum, precip_probability_max, sunrise, sunset,
        daylight_s, uv_index_max, wind_speed_max, wind_gust_max, wind_dir_dominant, moon_phase,
        moonrise, moonset);
    n
}

fn fill_air(dst: &mut AirQuality, src: &AirQuality) -> u32 {
    let mut n = 0;
    fill!(dst, src, n; us_aqi, european_aqi, owm_index, pm2_5, pm10, o3, no2, so2, co);
    n
}

/// Maximum distance (degrees) between the primary and supplement locations.
const MAX_LOCATION_DELTA: f64 = 0.1;
/// Maximum age difference (seconds) between the two "current" observations.
const MAX_CURRENT_DELTA: i64 = 90 * 60;

impl WeatherSnapshot {
    /// Fills what this snapshot lacks from `other`. Ignores `other` entirely when the
    /// locations are more than 0.1° apart or the UTC offsets differ.
    pub fn fill_gaps_from(&mut self, other: &WeatherSnapshot) -> FillReport {
        let mut report = FillReport::default();
        let (a, b) = (&self.location, &other.location);
        if (a.latitude - b.latitude).abs() > MAX_LOCATION_DELTA
            || (a.longitude - b.longitude).abs() > MAX_LOCATION_DELTA
            || a.utc_offset != b.utc_offset
        {
            return report;
        }

        match (&mut self.current, &other.current) {
            (Some(dst), Some(src)) if (dst.time.0 - src.time.0).abs() <= MAX_CURRENT_DELTA => {
                report.fields_filled += fill_current(dst, src);
            }
            (None, Some(src)) => {
                self.current = Some(src.clone());
                report.sections_added += 1;
            }
            _ => {}
        }

        if self.hourly.is_empty() && !other.hourly.is_empty() {
            self.hourly.clone_from(&other.hourly);
            report.sections_added += 1;
        } else {
            for dst in &mut self.hourly {
                if let Ok(i) = other.hourly.binary_search_by_key(&dst.time, |h| h.time) {
                    report.fields_filled += fill_hourly(dst, &other.hourly[i]);
                }
            }
            if let (Some(first), Some(last)) = (
                self.hourly.first().map(|h| h.time),
                self.hourly.last().map(|h| h.time),
            ) {
                let before: Vec<Hourly> = other
                    .hourly
                    .iter()
                    .take_while(|h| h.time < first)
                    .cloned()
                    .collect();
                let after = other.hourly.iter().filter(|h| h.time > last).cloned();
                report.hours_appended += u32::try_from(before.len()).unwrap_or(u32::MAX);
                self.hourly.splice(0..0, before);
                for h in after {
                    self.hourly.push(h);
                    report.hours_appended += 1;
                }
            }
        }

        for dst in &mut self.daily {
            if let Some(src) = other.daily.iter().find(|d| d.date == dst.date) {
                report.fields_filled += fill_daily(dst, src);
            }
        }
        let last = self.daily.last().map(|d| d.date);
        for src in &other.daily {
            if last.is_none_or(|last| src.date > last) {
                self.daily.push(src.clone());
                report.days_appended += 1;
            }
        }

        if self.minutely_15.is_empty() && !other.minutely_15.is_empty() {
            self.minutely_15.clone_from(&other.minutely_15);
            report.sections_added += 1;
        }

        match (&mut self.air_quality, &other.air_quality) {
            (Some(dst), Some(src)) => report.fields_filled += fill_air(dst, src),
            (None, Some(src)) => {
                self.air_quality = Some(src.clone());
                report.sections_added += 1;
            }
            _ => {}
        }

        // Only the primary's own alerts can be the same alert from another
        // source; the supplement's are distinct from each other (two warnings
        // for two rivers share a title and start).
        let own = self.alerts.len();
        for alert in &other.alerts {
            if !self.alerts[..own].iter().any(|a| {
                (a.title == alert.title && a.start == alert.start)
                    || (a.id.is_some() && a.id == alert.id)
            }) {
                self.alerts.push(alert.clone());
                report.alerts_added += 1;
            }
        }

        if !report.is_empty()
            && other.provider != self.provider
            && !self.supplemented_by.contains(&other.provider)
        {
            self.supplemented_by.push(other.provider);
        }
        report
    }
}

#[cfg(test)]
mod tests {
    use crate::model::ProviderId;
    use crate::testdata::{sample_snapshot, sample_snapshot_owm_free};
    use crate::time::UtcOffset;

    fn alert(id: Option<&str>, title: &str, start: i64) -> crate::model::Alert {
        crate::model::Alert {
            id: id.map(Into::into),
            title: title.into(),
            description: alloc::string::String::new(),
            severity: crate::scales::Level::Poor,
            start: crate::time::UnixTime(start),
            end: crate::time::UnixTime(start + 3600),
            source: "test".into(),
            checked_at: None,
            url: None,
        }
    }

    #[test]
    fn alerts_are_unioned_by_id_or_title_and_start() {
        let mut primary = sample_snapshot_owm_free();
        primary.alerts = alloc::vec![alert(Some("a"), "Flood Watch", 0)];
        let mut other = sample_snapshot();
        other.alerts = alloc::vec![
            alert(Some("a"), "Flood Watch (updated)", 60),
            alert(None, "Flood Watch", 0),
            alert(Some("b"), "Wind Advisory", 0),
        ];
        let report = primary.fill_gaps_from(&other);
        assert_eq!(report.alerts_added, 1);
        let titles: alloc::vec::Vec<&str> =
            primary.alerts.iter().map(|a| a.title.as_str()).collect();
        assert_eq!(titles, ["Flood Watch", "Wind Advisory"]);
    }

    #[test]
    fn supplement_fills_what_the_free_api_lacks() {
        let mut primary = sample_snapshot_owm_free();
        let original = primary.clone();
        let report = primary.fill_gaps_from(&sample_snapshot());

        assert_eq!(primary.provider, ProviderId::OpenWeather);
        assert_eq!(primary.supplemented_by, [ProviderId::OpenMeteo]);
        assert!(
            primary.current.as_ref().unwrap().uv_index.is_some(),
            "UV filled"
        );
        assert!(primary.daily[0].moon_phase.is_some(), "moon filled");
        assert!(primary.daily[0].uv_index_max.is_some());
        assert_eq!(primary.daily.len(), 7, "days 6 and 7 appended");
        assert_eq!(report.days_appended, 2);
        assert_eq!(primary.minutely_15.len(), 8, "nowcast taken");
        let aq = primary.air_quality.as_ref().unwrap();
        assert_eq!(
            (aq.us_aqi, aq.european_aqi, aq.owm_index),
            (Some(42), Some(28), Some(2))
        );
        assert!(report.fields_filled > 0);

        // Hourly steps stay OpenWeather's 3-hour grid; only matching hours gain UV.
        assert_eq!(primary.hourly.len(), original.hourly.len());
        assert!(
            primary.hourly[4].uv_index.is_some(),
            "12:00 matches an Open-Meteo hour"
        );
    }

    #[test]
    fn primary_values_are_never_overwritten() {
        let mut primary = sample_snapshot_owm_free();
        primary.current.as_mut().unwrap().temperature = Some(crate::units::Celsius(99.0));
        let original = primary.clone();
        primary.fill_gaps_from(&sample_snapshot());
        assert_eq!(
            primary.current.as_ref().unwrap().temperature,
            Some(crate::units::Celsius(99.0))
        );
        for (after, before) in primary.hourly.iter().zip(&original.hourly) {
            assert_eq!(after.temperature, before.temperature);
            assert_eq!(after.condition, before.condition);
        }
        for (after, before) in primary.daily.iter().zip(&original.daily) {
            assert_eq!(after.temp_max, before.temp_max);
        }
    }

    #[test]
    fn mismatched_locations_are_ignored() {
        let mut primary = sample_snapshot_owm_free();
        let original = primary.clone();
        let mut far = sample_snapshot();
        far.location.latitude += 0.5;
        assert!(primary.fill_gaps_from(&far).is_empty());
        let mut other_zone = sample_snapshot();
        other_zone.location.utc_offset = UtcOffset(0);
        assert!(primary.fill_gaps_from(&other_zone).is_empty());
        assert_eq!(primary, original);
    }

    #[test]
    fn filling_twice_is_idempotent() {
        let mut primary = sample_snapshot_owm_free();
        let supplement = sample_snapshot();
        primary.fill_gaps_from(&supplement);
        let once = primary.clone();
        assert!(primary.fill_gaps_from(&supplement).is_empty());
        assert_eq!(primary, once);
    }
}
