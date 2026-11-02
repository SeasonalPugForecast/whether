//! Incremental construction of a [`WeatherSnapshot`] by provider parsers.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::model::{
    AirQuality, Alert, Current, Daily, Hourly, Minutely, ProviderId, ResolvedLocation,
    WeatherSnapshot,
};
use crate::time::UnixTime;
use crate::units::{Celsius, Degrees, Hpa, Meters, Mm, Mps, Percent};

/// Why a snapshot could not be built.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ModelError {
    /// No provider response set the location.
    #[error("the snapshot has no location")]
    MissingLocation,
}

/// Collects the parts of a snapshot as provider responses are parsed.
#[derive(Clone, Debug)]
pub struct SnapshotBuilder {
    provider: ProviderId,
    location: Option<ResolvedLocation>,
    current: Option<Current>,
    hourly: Vec<Hourly>,
    daily: Vec<Daily>,
    minutely: Vec<Minutely>,
    air_quality: Option<AirQuality>,
    alerts: Vec<Alert>,
    alert_refs: Vec<String>,
    models: Vec<String>,
}

impl SnapshotBuilder {
    /// An empty builder for `provider`.
    pub fn new(provider: ProviderId) -> Self {
        Self {
            provider,
            location: None,
            current: None,
            hourly: Vec::new(),
            daily: Vec::new(),
            minutely: Vec::new(),
            air_quality: None,
            alerts: Vec::new(),
            alert_refs: Vec::new(),
            models: Vec::new(),
        }
    }

    /// The provider this builder collects for.
    pub fn provider(&self) -> ProviderId {
        self.provider
    }

    /// The location set so far, if any.
    pub fn location(&self) -> Option<&ResolvedLocation> {
        self.location.as_ref()
    }

    /// Sets the resolved location.
    pub fn set_location(&mut self, location: ResolvedLocation) {
        self.location = Some(location);
    }

    /// Sets the current conditions.
    pub fn set_current(&mut self, current: Current) {
        self.current = Some(current);
    }

    /// The current conditions collected so far, to amend (a station's
    /// observation over a forecast hour).
    pub fn current_mut(&mut self) -> Option<&mut Current> {
        self.current.as_mut()
    }

    /// The location collected so far, to amend.
    pub fn location_mut(&mut self) -> Option<&mut ResolvedLocation> {
        self.location.as_mut()
    }

    /// Appends one forecast step.
    pub fn push_hourly(&mut self, hourly: Hourly) {
        self.hourly.push(hourly);
    }

    /// Replaces the forecast steps.
    pub fn set_hourly(&mut self, hourly: Vec<Hourly>) {
        self.hourly = hourly;
    }

    /// Replaces the daily forecast.
    pub fn set_daily(&mut self, daily: Vec<Daily>) {
        self.daily = daily;
    }

    /// The daily forecast so far, for responses that complete it (e.g. a separate
    /// sunrise endpoint).
    pub fn daily_mut(&mut self) -> &mut [Daily] {
        &mut self.daily
    }

    /// Replaces the 15-minute precipitation steps.
    pub fn set_minutely(&mut self, minutely: Vec<Minutely>) {
        self.minutely = minutely;
    }

    /// Sets the air quality.
    pub fn set_air_quality(&mut self, air_quality: AirQuality) {
        self.air_quality = Some(air_quality);
    }

    /// Appends an alert.
    pub fn push_alert(&mut self, alert: Alert) {
        self.alerts.push(alert);
    }

    /// Notes that an alert identified by `id` applies, for providers whose responses
    /// list alerts only by id. The fetch session takes the ids with
    /// [`Self::take_alert_refs`] and requests the details it lacks.
    pub fn refer_alert(&mut self, id: &str) {
        if !self.alert_refs.iter().any(|r| r == id) {
            self.alert_refs.push(id.to_string());
        }
    }

    /// The alert ids noted since the last call, in the order first noted.
    pub fn take_alert_refs(&mut self) -> Vec<String> {
        core::mem::take(&mut self.alert_refs)
    }

    /// Records the weather models that supplied the data, in priority order.
    pub fn set_models(&mut self, models: Vec<String>) {
        self.models = models;
    }

    /// Discards everything collected, keeping the provider (used when falling back).
    pub fn reset(&mut self) {
        *self = Self::new(self.provider);
    }

    /// Validates and returns the snapshot. Series are sorted and deduplicated by time
    /// (first occurrence wins) and non-finite values become `None`.
    pub fn finish(self, fetched_at: UnixTime) -> Result<WeatherSnapshot, ModelError> {
        let location = self.location.ok_or(ModelError::MissingLocation)?;
        let mut current = self.current;
        if let Some(c) = current.as_mut() {
            c.sanitize();
        }
        let mut hourly = sorted_unique(self.hourly, |h| h.time);
        hourly.iter_mut().for_each(Hourly::sanitize);
        let mut daily = sorted_unique(self.daily, |d| d.date);
        daily.iter_mut().for_each(Daily::sanitize);
        let mut minutely = sorted_unique(self.minutely, |m| m.time);
        for m in &mut minutely {
            keep_finite(&mut m.precipitation);
        }
        let mut air_quality = self.air_quality;
        if let Some(a) = air_quality.as_mut() {
            a.sanitize();
        }
        Ok(WeatherSnapshot {
            location,
            provider: self.provider,
            supplemented_by: Vec::new(),
            fetched_at,
            current,
            hourly,
            daily,
            minutely_15: minutely,
            minutely_1: Vec::new(),
            air_quality,
            alerts: unique_alerts(self.alerts),
            offset_changes: Vec::new(),
            models: self.models,
        })
    }
}

/// Keeps one alert per id, where it first appeared, with the details pushed last:
/// fresh details replace a cached copy.
fn unique_alerts(alerts: Vec<Alert>) -> Vec<Alert> {
    let mut out: Vec<Alert> = Vec::with_capacity(alerts.len());
    for alert in alerts {
        let earlier = alert
            .id
            .as_ref()
            .and_then(|id| out.iter().position(|a| a.id.as_ref() == Some(id)));
        match earlier {
            Some(i) => out[i] = alert,
            None => out.push(alert),
        }
    }
    out
}

/// Sorts by key (stable) and drops later items with an equal key.
fn sorted_unique<T, K: Ord + Copy>(mut items: Vec<T>, key: impl Fn(&T) -> K) -> Vec<T> {
    items.sort_by_key(|item| key(item));
    items.dedup_by(|later, earlier| key(later) == key(earlier));
    items
}

/// Values that can be checked for finiteness.
trait Finite {
    fn is_finite_value(&self) -> bool;
}

impl Finite for f32 {
    fn is_finite_value(&self) -> bool {
        self.is_finite()
    }
}

macro_rules! finite_newtype {
    ($($t:ty),+) => {$(
        impl Finite for $t {
            fn is_finite_value(&self) -> bool {
                self.0.is_finite()
            }
        }
    )+};
}
finite_newtype!(Celsius, Mps, Hpa, Mm, Meters, Percent, Degrees);

fn keep_finite<T: Finite>(value: &mut Option<T>) {
    if value.as_ref().is_some_and(|v| !v.is_finite_value()) {
        *value = None;
    }
}

macro_rules! sanitize_fields {
    ($ty:ty { $($field:ident),+ $(,)? }) => {
        impl $ty {
            fn sanitize(&mut self) {
                $(keep_finite(&mut self.$field);)+
            }
        }
    };
}

sanitize_fields!(Current {
    temperature,
    apparent,
    dew_point,
    humidity,
    pressure_msl,
    cloud_cover,
    wind_speed,
    wind_gust,
    wind_dir,
    visibility,
    uv_index,
    precipitation,
});
sanitize_fields!(Hourly {
    temperature,
    apparent,
    precip_probability,
    precipitation,
    wind_speed,
    wind_gust,
    wind_dir,
    humidity,
    pressure_msl,
    uv_index,
    visibility,
    cloud_cover,
});
sanitize_fields!(Daily {
    temp_max,
    temp_min,
    apparent_max,
    apparent_min,
    precip_sum,
    precip_probability_max,
    uv_index_max,
    wind_speed_max,
    wind_gust_max,
    wind_dir_dominant,
    moon_phase,
});
sanitize_fields!(AirQuality {
    pm2_5,
    pm10,
    o3,
    no2,
    so2,
    co
});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::LocalDate;
    use alloc::vec;

    fn hour(t: i64, temp: f32) -> Hourly {
        Hourly {
            time: UnixTime(t),
            temperature: Some(Celsius(temp)),
            ..Hourly::default()
        }
    }

    fn location() -> ResolvedLocation {
        ResolvedLocation {
            name: "New York".into(),
            ..ResolvedLocation::default()
        }
    }

    fn alert(id: Option<&str>, title: &str) -> Alert {
        Alert {
            id: id.map(Into::into),
            title: title.into(),
            description: String::new(),
            severity: crate::scales::Level::Moderate,
            start: UnixTime(0),
            end: UnixTime(3600),
            source: "test".into(),
            checked_at: None,
            url: None,
        }
    }

    #[test]
    fn alert_refs_are_taken_once_and_in_order() {
        let mut b = SnapshotBuilder::new(ProviderId::OpenWeather);
        b.refer_alert("b");
        b.refer_alert("a");
        b.refer_alert("b");
        assert_eq!(b.take_alert_refs(), ["b", "a"]);
        assert_eq!(b.take_alert_refs(), [] as [String; 0]);
    }

    #[test]
    fn alerts_with_the_same_id_are_kept_once() {
        let mut b = SnapshotBuilder::new(ProviderId::OpenWeather);
        b.set_location(location());
        b.push_alert(alert(Some("x"), "first"));
        b.push_alert(alert(Some("y"), "other"));
        b.push_alert(alert(Some("x"), "again"));
        b.push_alert(alert(None, "no id"));
        b.push_alert(alert(None, "no id"));
        let titles: Vec<String> = b
            .finish(UnixTime(0))
            .unwrap()
            .alerts
            .into_iter()
            .map(|a| a.title)
            .collect();
        assert_eq!(
            titles,
            ["again", "other", "no id", "no id"],
            "the last details, in the first place"
        );
    }

    #[test]
    fn models_are_kept_in_order_and_cleared_by_reset() {
        let mut b = SnapshotBuilder::new(ProviderId::OpenMeteo);
        b.set_location(location());
        b.set_models(vec!["ncep_hrrr_conus".into(), "ncep_nbm_conus".into()]);
        let snap = b.clone().finish(UnixTime(0)).unwrap();
        assert_eq!(snap.models, ["ncep_hrrr_conus", "ncep_nbm_conus"]);
        b.reset();
        b.set_location(location());
        assert_eq!(b.finish(UnixTime(0)).unwrap().models, Vec::<String>::new());
    }

    #[test]
    fn location_is_required() {
        let b = SnapshotBuilder::new(ProviderId::OpenMeteo);
        assert_eq!(
            b.finish(UnixTime(0)).unwrap_err(),
            ModelError::MissingLocation
        );
    }

    #[test]
    fn sorts_and_dedupes_series() {
        let mut b = SnapshotBuilder::new(ProviderId::OpenMeteo);
        b.set_location(location());
        b.set_hourly(vec![
            hour(7_200, 3.0),
            hour(0, 1.0),
            hour(3_600, 2.0),
            hour(0, 9.0),
        ]);
        let d = |day| Daily {
            date: LocalDate {
                year: 2026,
                month: 10,
                day,
            },
            ..Daily::default()
        };
        b.set_daily(vec![d(6), d(5), d(6)]);
        let s = b.finish(UnixTime(10)).unwrap();
        let times: Vec<i64> = s.hourly.iter().map(|h| h.time.0).collect();
        assert_eq!(times, [0, 3_600, 7_200]);
        assert_eq!(
            s.hourly[0].temperature,
            Some(Celsius(1.0)),
            "first occurrence wins"
        );
        assert_eq!(s.daily.len(), 2);
        assert_eq!(s.daily[0].date.day, 5);
        assert_eq!(s.fetched_at, UnixTime(10));
    }

    #[test]
    fn non_finite_values_become_none() {
        let mut b = SnapshotBuilder::new(ProviderId::OpenWeather);
        b.set_location(location());
        b.set_current(Current {
            temperature: Some(Celsius(f32::NAN)),
            uv_index: Some(f32::INFINITY),
            humidity: Some(Percent(50.0)),
            ..Current::default()
        });
        b.set_air_quality(AirQuality {
            pm2_5: Some(f32::NAN),
            ..AirQuality::default()
        });
        let s = b.finish(UnixTime(0)).unwrap();
        let c = s.current.unwrap();
        assert_eq!(c.temperature, None);
        assert_eq!(c.uv_index, None);
        assert_eq!(c.humidity, Some(Percent(50.0)));
        assert_eq!(s.air_quality.unwrap().pm2_5, None);
    }

    #[test]
    fn reset_keeps_only_the_provider() {
        let mut b = SnapshotBuilder::new(ProviderId::MetNorway);
        b.set_location(location());
        b.push_hourly(hour(0, 1.0));
        b.reset();
        assert_eq!(b.provider(), ProviderId::MetNorway);
        assert!(b.location().is_none());
        assert_eq!(
            b.finish(UnixTime(0)).unwrap_err(),
            ModelError::MissingLocation
        );
    }
}
