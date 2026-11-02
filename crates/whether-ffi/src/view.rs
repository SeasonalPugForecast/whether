//! The display contract for the app, as JSON.

use whether_core::model::WeatherSnapshot;
use whether_core::strings::Strings;
use whether_core::time::UnixTime;
use whether_core::view::{LocationView, StatusInfo, ViewInput, build_view};

use crate::settings::{Place, Settings};
use crate::{WhetherError, from_json};

/// One location as the app holds it.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct LocationEntry {
    /// The place.
    pub place: Place,
    /// The cached snapshot ([`crate::FetchOutcome::snapshot_json`]), if any.
    pub snapshot_json: Option<String>,
    /// When the last fetch succeeded (Unix seconds).
    pub last_success: Option<i64>,
    /// The last fetch's error, if it failed.
    pub error: Option<String>,
    /// A fetch is running.
    pub refreshing: bool,
    /// The kept history ([`crate::HistoryOutcome::history_json`]), if any.
    #[uniffi(default = None)]
    pub history_json: Option<String>,
}

/// The `ViewModel` for `locations`, showing `active`, as JSON: every value
/// formatted for the place's units and the settings' clock.
#[uniffi::export]
#[allow(clippy::needless_pass_by_value)] // UniFFI passes arguments by value
pub fn view_model(
    locations: Vec<LocationEntry>,
    active: u32,
    settings: Settings,
    now: i64,
) -> Result<String, WhetherError> {
    let snapshots = locations
        .iter()
        .map(|l| {
            l.snapshot_json
                .as_deref()
                .map(|json| from_json::<WeatherSnapshot>("snapshot", json))
                .transpose()
        })
        .collect::<Result<Vec<_>, _>>()?;
    // History that does not read is left out: it never costs the forecast.
    let histories: Vec<_> = locations
        .iter()
        .map(|l| {
            l.history_json
                .as_deref()
                .and_then(|json| crate::history::kept(json).ok())
                .map(|k| k.history)
        })
        .collect();
    let stale_after = i64::from(settings.refresh_minutes.max(5)) * 60 * 2;
    let statuses: Vec<StatusInfo> = locations
        .iter()
        .map(|l| StatusInfo {
            last_success: l.last_success.map(UnixTime),
            error: l.error.clone(),
            stale: l.last_success.is_some_and(|t| now - t > stale_after),
            refreshing: l.refreshing,
        })
        .collect();
    let views: Vec<LocationView<'_>> = locations
        .iter()
        .zip(&snapshots)
        .zip(&statuses)
        .zip(&histories)
        .map(|(((l, snapshot), status), history)| LocationView {
            name: &l.place.name,
            snapshot: snapshot.as_ref(),
            status,
            history: history.as_ref(),
        })
        .collect();
    let active = usize::try_from(active).unwrap_or(0);
    let place = locations.get(active).map(|l| &l.place);
    let presentation = place.map_or_else(
        || settings.presentation(&placeholder()),
        |p| settings.presentation(p),
    );
    let strings = Strings::localized(&settings.language, &std::collections::BTreeMap::default()).0;
    let vm = build_view(&ViewInput {
        locations: &views,
        active,
        presentation: &presentation,
        strings: &strings,
        now: UnixTime(now),
    });
    serde_json::to_string(&vm).map_err(|e| WhetherError::InvalidJson {
        what: "view model".into(),
        message: e.to_string(),
    })
}

/// A precipitation chance as the core writes it (`"40%"`), for fields the view
/// model keeps numeric (`precip_probability`).
#[uniffi::export]
pub fn chance_text(percent: u8) -> String {
    whether_core::view::format::chance(percent)
}

/// An alert's severity in words for its `level` (`"very_poor"`, as the view model
/// writes it), in `language`: the sign beside the alert's color when the app shows
/// patterns and symbols. `None` for a level the core doesn't know.
#[uniffi::export]
#[allow(clippy::needless_pass_by_value)] // UniFFI passes arguments by value
pub fn severity_word(level: String, language: String) -> Option<String> {
    let level: whether_core::scales::Level =
        serde_json::from_value(serde_json::Value::String(level)).ok()?;
    let strings = Strings::localized(&language, &std::collections::BTreeMap::default()).0;
    Some(
        strings
            .get(whether_core::strings::severity_text(level))
            .to_string(),
    )
}

/// The radar legend's five steps in words, light to heavy, in `language`.
#[uniffi::export]
#[allow(clippy::needless_pass_by_value)] // UniFFI passes arguments by value
pub fn radar_step_words(language: String) -> Vec<String> {
    let strings = Strings::localized(&language, &std::collections::BTreeMap::default()).0;
    whether_core::strings::RADAR_STEPS
        .iter()
        .map(|t| strings.get(*t).to_string())
        .collect()
}

/// A place without overrides, for an empty list.
fn placeholder() -> Place {
    Place {
        name: String::new(),
        latitude: 0.0,
        longitude: 0.0,
        units: None,
        utc_offset_seconds: None,
        offset_changes: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_words_by_level_and_language() {
        let en = |l: &str| severity_word(l.into(), "en".into());
        assert_eq!(en("fair").as_deref(), Some("Minor"));
        assert_eq!(en("moderate").as_deref(), Some("Moderate"));
        assert_eq!(en("very_poor").as_deref(), Some("Severe"));
        assert_eq!(en("extreme").as_deref(), Some("Extreme"));
        assert_eq!(en("unheard_of"), None);
        assert_eq!(
            severity_word("very_poor".into(), "de-DE".into()).as_deref(),
            Some("Schwer")
        );
    }

    #[test]
    fn radar_steps_are_five_words_light_to_heavy() {
        assert_eq!(
            radar_step_words("en".into()),
            ["Light", "Moderate", "Heavy", "Very heavy", "Extreme"]
        );
        assert_eq!(radar_step_words("fr".into()).len(), 5);
    }
}
