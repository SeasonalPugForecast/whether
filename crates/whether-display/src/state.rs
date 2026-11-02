//! The application state every display reads, and the commands they send back.

use std::sync::Arc;

use whether_config::{Issue, LinuxConfig, LocationConfig};
use whether_core::history::History;
use whether_core::model::{ProviderId, ResolvedLocation, WeatherSnapshot};
use whether_core::time::UnixTime;
use whether_providers::GeocodeResult;

/// Everything the service knows, published as an immutable snapshot.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AppState {
    /// One entry per configured location, in config order.
    pub locations: Vec<LocationState>,
    /// The latest place search, if any.
    pub search: Option<SearchState>,
    /// Warnings from loading the configuration (or errors from a failed reload).
    pub config_issues: Vec<Issue>,
    /// The configuration in effect; every successful reload replaces it.
    pub config: Option<LoadedConfig>,
    /// Increases with every successful reload (0: the start-up configuration).
    pub config_epoch: u64,
    /// Increases with every published change.
    pub epoch: u64,
}

/// A loaded configuration, shared. Two are equal when they are the same load, which
/// keeps comparing states cheap.
#[derive(Clone)]
pub struct LoadedConfig(pub Arc<LinuxConfig>);

impl PartialEq for LoadedConfig {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl std::fmt::Debug for LoadedConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadedConfig")
            .field("sources", &self.0.sources)
            .finish_non_exhaustive()
    }
}

/// One location's data and fetch status.
#[derive(Clone, Debug, PartialEq)]
pub struct LocationState {
    /// The configured name.
    pub name: String,
    /// The configuration entry.
    pub config: LocationConfig,
    /// Coordinates and time zone, once resolved.
    pub resolved: Option<ResolvedLocation>,
    /// The latest snapshot (shared, so publishing does not copy it).
    pub snapshot: Option<Arc<WeatherSnapshot>>,
    /// Fetch bookkeeping.
    pub status: FetchStatus,
    /// Radar imagery, when the radar widget is on and the location is covered.
    pub radar: Option<SharedRadar>,
    /// Past days, normals and records, when a presentation shows them.
    pub history: Option<Arc<History>>,
}

/// Radar imagery for one location: an OpenStreetMap base and NEXRAD
/// frames, RGBA, all `width × height`, frames oldest first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RadarImages {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// The base map, opaque.
    pub base: Arc<[u8]>,
    /// The frames, transparent where there is no echo.
    pub frames: Vec<RadarFrame>,
    /// The location in pixels.
    pub marker: (u32, u32),
    /// The required credit.
    pub attribution: &'static str,
}

/// One radar frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RadarFrame {
    /// When the composite was valid.
    pub valid: UnixTime,
    /// RGBA pixels.
    pub rgba: Arc<[u8]>,
}

/// Radar imagery, shared; equal when it is the same fetch, so comparing states does
/// not compare pixels.
#[derive(Clone, Debug)]
pub struct SharedRadar(pub Arc<RadarImages>);

impl PartialEq for SharedRadar {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl LocationState {
    /// A location with no data yet.
    pub fn new(config: LocationConfig) -> Self {
        Self {
            name: config.name.clone().unwrap_or_default(),
            config,
            resolved: None,
            snapshot: None,
            status: FetchStatus::default(),
            radar: None,
            history: None,
        }
    }
}

/// Fetch bookkeeping for one location.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FetchStatus {
    /// When the last fetch started.
    pub last_attempt: Option<UnixTime>,
    /// When a fetch last succeeded.
    pub last_success: Option<UnixTime>,
    /// When the next fetch is due.
    pub next_due: Option<UnixTime>,
    /// A fetch is running.
    pub in_flight: bool,
    /// The snapshot is older than the refresh interval.
    pub stale: bool,
    /// Why the last fetch failed.
    pub error: Option<String>,
    /// Partial failures of the last fetch (e.g. "OpenWeather: unauthorized").
    pub notes: Vec<String>,
    /// The provider that served the snapshot.
    pub provider: Option<ProviderId>,
}

/// A place search started from a display.
#[derive(Clone, Debug, PartialEq)]
pub enum SearchState {
    /// Waiting for results.
    Searching {
        /// What was searched for.
        query: String,
    },
    /// Results arrived.
    Results {
        /// What was searched for.
        query: String,
        /// Matches, best first.
        results: Vec<GeocodeResult>,
    },
    /// The search failed.
    Failed {
        /// What was searched for.
        query: String,
        /// Why.
        error: String,
    },
}

/// Requests from displays to the service.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Fetch now (one location, or all), subject to the refresh floor and call budgets.
    Refresh {
        /// Location index, or `None` for all.
        location: Option<usize>,
    },
    /// Search for places.
    Search {
        /// Name or postal code.
        query: String,
    },
    /// Append a search result to the configuration.
    AddLocation(GeocodeResult),
    /// Remove a location from the configuration.
    RemoveLocation(usize),
    /// Reload the configuration files.
    ReloadConfig,
    /// Set dotted keys (`general.units`, `display.gui.sections`) in the configuration
    /// file in one edit, then reload; a result that does not load is undone.
    SetConfig(Vec<(String, toml::Value)>),
    /// Stop the service.
    Quit,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_locations_take_their_configured_name() {
        let loc = LocationState::new(LocationConfig {
            name: Some("New York".into()),
            ..Default::default()
        });
        assert_eq!(loc.name, "New York");
        assert!(loc.snapshot.is_none() && !loc.status.in_flight);
    }
}
