//! Turning the application state into a `ViewModel`, and the host clock.

use std::time::{SystemTime, UNIX_EPOCH};

use whether_core::presentation::PresentationConfig;
use whether_core::presentation::WidgetKind;
use whether_core::strings::Strings;
use whether_core::time::UnixTime;
use whether_core::view::{LocationView, StatusInfo, ViewInput, ViewModel, build_view, radar_vm};

use crate::state::{AppState, FetchStatus};

/// The current time: the only place the host stack reads the system clock.
pub fn now() -> UnixTime {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    UnixTime(i64::try_from(secs).unwrap_or(i64::MAX))
}

/// The view's status for a location.
pub fn status_info(status: &FetchStatus) -> StatusInfo {
    StatusInfo {
        last_success: status.last_success,
        error: status.error.clone(),
        stale: status.stale,
        refreshing: status.in_flight,
    }
}

/// A display's presentation while location `active` is shown: its `units` and
/// `presentation` keys on top.
pub fn presentation_at(
    state: &AppState,
    active: usize,
    presentation: &PresentationConfig,
) -> PresentationConfig {
    match state.locations.get(active) {
        Some(l) => whether_config::for_location(presentation, &l.config),
        None => presentation.clone(),
    }
}

/// The view model for `state`, showing location `active` with its own settings
/// over `presentation`.
pub fn view_for(
    state: &AppState,
    active: usize,
    presentation: &PresentationConfig,
    strings: &Strings,
    now: UnixTime,
) -> ViewModel {
    let presentation = presentation_at(state, active, presentation);
    view_with(state, active, &presentation, strings, now)
}

/// [`view_for`] with `presentation` already resolved for the location (from
/// [`presentation_at`]), for displays that adjust it further, such as a units
/// toggle.
pub fn view_with(
    state: &AppState,
    active: usize,
    presentation: &PresentationConfig,
    strings: &Strings,
    now: UnixTime,
) -> ViewModel {
    let statuses: Vec<StatusInfo> = state
        .locations
        .iter()
        .map(|l| status_info(&l.status))
        .collect();
    let locations: Vec<LocationView<'_>> = state
        .locations
        .iter()
        .zip(&statuses)
        .map(|(l, status)| LocationView {
            name: &l.name,
            snapshot: l.snapshot.as_deref(),
            status,
            history: l.history.as_deref(),
        })
        .collect();
    let mut vm = build_view(&ViewInput {
        locations: &locations,
        active,
        presentation,
        strings,
        now,
    });
    // Radar imagery travels beside the snapshot; the view gets its labels.
    let shown = state.locations.get(vm.active);
    match shown.and_then(|l| l.radar.as_ref()) {
        Some(radar) if presentation.shows(WidgetKind::Radar) => {
            let times: Vec<UnixTime> = radar.0.frames.iter().map(|f| f.valid).collect();
            let offset = |t: UnixTime| {
                shown.and_then(|l| l.snapshot.as_deref()).map_or_else(
                    || {
                        shown
                            .and_then(|l| l.resolved.as_ref())
                            .map(|r| r.utc_offset)
                            .unwrap_or_default()
                    },
                    |s| s.offset_at(t),
                )
            };
            vm.radar = Some(radar_vm(
                &times,
                offset,
                radar.0.attribution,
                presentation,
                strings,
            ));
        }
        _ => vm.widgets.retain(|w| *w != WidgetKind::Radar),
    }
    vm
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use whether_config::LocationConfig;
    use whether_core::testdata;

    use crate::state::LocationState;

    #[test]
    fn matches_the_core_builder() {
        let snapshot = testdata::sample_snapshot();
        let mut location = LocationState::new(LocationConfig {
            name: Some("New York".into()),
            ..Default::default()
        });
        location.snapshot = Some(Arc::new(snapshot.clone()));
        location.status.last_success = Some(testdata::SAMPLE_NOW);
        location.status.in_flight = true;
        let state = AppState {
            locations: vec![location],
            ..Default::default()
        };
        let p = PresentationConfig::default();
        let s = Strings::default();
        let now = testdata::SAMPLE_NOW;

        let status = StatusInfo {
            last_success: Some(now),
            error: None,
            stale: false,
            refreshing: true,
        };
        let expected = build_view(&ViewInput {
            locations: &[LocationView {
                name: "New York",
                snapshot: Some(&snapshot),
                status: &status,
                history: None,
            }],
            active: 0,
            presentation: &p,
            strings: &s,
            now,
        });
        assert_eq!(view_for(&state, 0, &p, &s, now), expected);
    }

    #[test]
    fn each_location_shows_with_its_own_settings() {
        use whether_config::PresentationToml;
        use whether_core::units::{UnitPrefs, UnitSystem};
        let place = |name: &str, units, widgets: Option<Vec<WidgetKind>>| {
            let mut l = LocationState::new(LocationConfig {
                name: Some(name.into()),
                units,
                presentation: widgets.map(|w| PresentationToml {
                    widgets: Some(w),
                    ..PresentationToml::default()
                }),
                ..Default::default()
            });
            l.snapshot = Some(Arc::new(testdata::sample_snapshot()));
            l
        };
        let state = AppState {
            locations: vec![
                place("New York", None, None),
                place(
                    "Abroad",
                    Some(UnitSystem::Metric),
                    Some(vec![WidgetKind::Current]),
                ),
            ],
            ..Default::default()
        };
        let p = PresentationConfig {
            units: UnitPrefs::IMPERIAL,
            ..PresentationConfig::default()
        };
        let s = Strings::default();
        let home = view_for(&state, 0, &p, &s, testdata::SAMPLE_NOW);
        let abroad = view_for(&state, 1, &p, &s, testdata::SAMPLE_NOW);
        assert!(
            home.hourly.is_some() && abroad.hourly.is_none(),
            "abroad shows only now"
        );
        assert_eq!(presentation_at(&state, 1, &p).units, UnitPrefs::METRIC);
        assert_eq!(presentation_at(&state, 0, &p).units, UnitPrefs::IMPERIAL);
        assert_eq!(presentation_at(&state, 9, &p), p, "no such location");
        assert_ne!(
            home.current.as_ref().unwrap().temperature.value,
            abroad.current.as_ref().unwrap().temperature.value,
            "°F at home, °C abroad"
        );
    }

    #[test]
    fn a_location_s_history_reaches_the_view() {
        let mut location = LocationState::new(LocationConfig {
            name: Some("New York".into()),
            ..Default::default()
        });
        location.snapshot = Some(Arc::new(testdata::sample_snapshot()));
        location.history = Some(Arc::new(testdata::history_10001()));
        let state = AppState {
            locations: vec![location],
            ..Default::default()
        };
        let p = PresentationConfig {
            widgets: vec![whether_core::presentation::WidgetKind::History],
            ..PresentationConfig::default()
        };
        let vm = view_for(&state, 0, &p, &Strings::default(), testdata::SAMPLE_NOW);
        assert_eq!(vm.history.unwrap().days.len(), 7);
        assert!(vm.attribution.contains("ACIS"));
    }

    #[test]
    fn no_locations_still_builds() {
        let vm = view_for(
            &AppState::default(),
            3,
            &PresentationConfig::default(),
            &Strings::default(),
            UnixTime(0),
        );
        assert_eq!(vm.tabs, [] as [whether_core::view::TabVm; 0]);
    }

    #[test]
    fn the_clock_is_after_2026() {
        assert!(now().0 > 1_767_225_600);
    }
}
