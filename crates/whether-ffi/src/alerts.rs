//! Official warnings the app gets elsewhere (Apple Weather outside the US), merged into a snapshot next to the providers' own.

use whether_core::model::{APPLE_ALERT_PREFIX, Alert, WeatherSnapshot};
use whether_core::scales::Level;
use whether_core::time::UnixTime;

use crate::{WhetherError, from_json};

/// How severe a warning is, in the terms of Apple's `WeatherSeverity` (the
/// common alerting protocol's scale).
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum AlertSeverity {
    /// Minimal or no known threat.
    Minor,
    /// Possible threat.
    Moderate,
    /// Significant threat.
    Severe,
    /// Extraordinary threat.
    Extreme,
    /// The source did not say.
    Unknown,
}

impl AlertSeverity {
    /// The core's scale: minor is Fair, moderate and unknown Moderate (as the
    /// providers' unrecognised events), severe a warning's `VeryPoor`, extreme Extreme.
    fn level(self) -> Level {
        match self {
            Self::Minor => Level::Fair,
            Self::Moderate | Self::Unknown => Level::Moderate,
            Self::Severe => Level::VeryPoor,
            Self::Extreme => Level::Extreme,
        }
    }
}

/// A warning from a source the app asks itself.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ExternalAlert {
    /// Tells alerts apart across fetches; `apple:` is put in front when missing.
    pub id: String,
    /// The event ("Flood Warning").
    pub title: String,
    /// Further text, such as the affected area; may be empty.
    pub summary: String,
    /// How severe it is.
    pub severity: AlertSeverity,
    /// When the app first saw the warning in force, Unix seconds.
    pub start: i64,
    /// When the app stops showing it, Unix seconds.
    pub end: i64,
    /// The issuing authority.
    pub source: String,
    /// A page with the details; anything but an http(s) address is dropped.
    pub url: Option<String>,
}

/// `snapshot_json` with `alerts` as its Apple alerts: the ones an earlier call
/// added (ids starting `apple:`) are replaced, the providers' own stay. No
/// alerts clear the earlier ones.
#[uniffi::export]
#[allow(clippy::needless_pass_by_value)] // UniFFI passes arguments by value
pub fn with_alerts(
    snapshot_json: String,
    alerts: Vec<ExternalAlert>,
) -> Result<String, WhetherError> {
    let mut snapshot: WeatherSnapshot = from_json("snapshot", &snapshot_json)?;
    snapshot.alerts.retain(|a| !a.is_apple());
    let mut added: Vec<Alert> = Vec::new();
    for a in alerts {
        let id = if a.id.starts_with(APPLE_ALERT_PREFIX) {
            a.id
        } else {
            format!("{APPLE_ALERT_PREFIX}{}", a.id)
        };
        if added.iter().any(|x| x.id.as_deref() == Some(id.as_str())) {
            continue;
        }
        added.push(Alert {
            id: Some(id),
            title: a.title,
            description: a.summary,
            severity: a.severity.level(),
            start: UnixTime(a.start),
            end: UnixTime(a.end),
            source: a.source,
            checked_at: Some(UnixTime(a.start)),
            url: a
                .url
                .filter(|u| u.starts_with("https://") || u.starts_with("http://")),
        });
    }
    snapshot.alerts.extend(added);
    serde_json::to_string(&snapshot).map_err(|e| WhetherError::InvalidJson {
        what: "snapshot".into(),
        message: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn external(id: &str, severity: AlertSeverity) -> ExternalAlert {
        ExternalAlert {
            id: id.into(),
            title: "Flood Warning".into(),
            summary: "Kent".into(),
            severity,
            start: 1_000,
            end: 4_600,
            source: "Environment Agency".into(),
            url: Some("https://example.invalid/warning".into()),
        }
    }

    /// The sample snapshot (two NWS-style alerts) as JSON.
    fn sample() -> String {
        serde_json::to_string(&whether_core::testdata::sample_snapshot_with_alerts()).unwrap()
    }

    fn alerts_of(json: &str) -> Vec<Alert> {
        from_json::<WeatherSnapshot>("snapshot", json)
            .unwrap()
            .alerts
    }

    #[test]
    fn external_alerts_join_the_providers_own() {
        let before = alerts_of(&sample());
        assert_eq!(before.len(), 2);
        let out = with_alerts(sample(), vec![external("a", AlertSeverity::Severe)]).unwrap();
        let after = alerts_of(&out);
        assert_eq!(after.len(), before.len() + 1);
        assert_eq!(
            &after[..before.len()],
            &before[..],
            "providers' alerts stay"
        );
        let added = after.last().unwrap();
        assert_eq!(added.id.as_deref(), Some("apple:a"), "the prefix is added");
        assert_eq!(added.title, "Flood Warning");
        assert_eq!(added.description, "Kent");
        assert_eq!(added.severity, Level::VeryPoor);
        assert_eq!((added.start, added.end), (UnixTime(1_000), UnixTime(4_600)));
        assert_eq!(
            added.url.as_deref(),
            Some("https://example.invalid/warning")
        );
        assert!(added.is_apple());
    }

    #[test]
    fn a_second_call_replaces_the_first_ones_and_none_clears_them() {
        let once = with_alerts(sample(), vec![external("apple:a", AlertSeverity::Minor)]).unwrap();
        let twice = with_alerts(
            once.clone(),
            vec![
                external("apple:b", AlertSeverity::Extreme),
                external("b", AlertSeverity::Extreme),
            ],
        )
        .unwrap();
        let ids: Vec<Option<String>> = alerts_of(&twice).into_iter().map(|a| a.id).collect();
        assert!(!ids.contains(&Some("apple:a".into())));
        assert_eq!(
            ids.iter()
                .filter(|i| i.as_deref() == Some("apple:b"))
                .count(),
            1,
            "the same id twice is one alert"
        );
        let cleared = with_alerts(twice, Vec::new()).unwrap();
        assert!(alerts_of(&cleared).iter().all(|a| !a.is_apple()));
        assert_eq!(alerts_of(&cleared), alerts_of(&sample()));
    }

    #[test]
    fn severity_maps_onto_the_core_scale() {
        let levels: Vec<Level> = [
            AlertSeverity::Minor,
            AlertSeverity::Moderate,
            AlertSeverity::Severe,
            AlertSeverity::Extreme,
            AlertSeverity::Unknown,
        ]
        .map(AlertSeverity::level)
        .to_vec();
        assert_eq!(
            levels,
            [
                Level::Fair,
                Level::Moderate,
                Level::VeryPoor,
                Level::Extreme,
                Level::Moderate
            ]
        );
    }

    #[test]
    fn an_unknown_severity_and_a_bad_link_are_kept_safely() {
        let mut a = external("x", AlertSeverity::Unknown);
        a.url = Some("javascript:alert(1)".into());
        let out = with_alerts(sample(), vec![a]).unwrap();
        let added = alerts_of(&out).pop().unwrap();
        assert_eq!(added.severity, Level::Moderate);
        assert_eq!(added.url, None, "only http(s) links are kept");
    }

    #[test]
    fn bad_json_is_an_error() {
        assert!(matches!(
            with_alerts("{".into(), Vec::new()),
            Err(WhetherError::InvalidJson { .. })
        ));
    }
}
