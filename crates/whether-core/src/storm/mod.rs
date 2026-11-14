//! Storm outlook: several weather models side by side at one place.
//!
//! A [`StormOutlook`] holds, per model, the place's hourly series, ensemble
//! members, a coarse pressure grid around the place for following lows, and
//! earlier runs' totals. Each series is tagged with the [`ModelRun`] it comes
//! from. Raw values stay in the units their field names say (mm, cm, km/h,
//! hPa); only `view::storms` converts and formats them.

use alloc::string::String;
use alloc::vec::Vec;

use crate::model::{OffsetChange, ResolvedLocation};
use crate::scales::Level;
use crate::time::{UnixTime, UtcOffset};

/// The deterministic models compared by default, as Open-Meteo `models` ids.
pub const DETERMINISTIC_MODELS: [&str; 4] = [
    "ecmwf_ifs025",
    "gfs_seamless",
    "gem_seamless",
    "icon_seamless",
];

/// The ensembles fetched by default, as Open-Meteo Ensemble API `models` ids.
pub const ENSEMBLE_MODELS: [&str; 2] = ["ecmwf_ifs025", "gfs025"];

/// Points per side of the pressure grid.
pub const GRID_SIZE: usize = 7;

/// Spacing of the pressure grid, in degrees of latitude and longitude.
pub const GRID_SPACING_DEG: f32 = 2.5;

/// The grid snaps its centre to multiples of this many degrees.
pub const GRID_SNAP_DEG: f64 = 0.25;

/// No grid row lies further from the equator than this, in degrees.
pub const GRID_MAX_LATITUDE: f32 = 80.0;

/// One model run: when it started and when its data became available.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ModelRun {
    /// The Open-Meteo domain the run belongs to, e.g. `ncep_gfs025`.
    pub model: String,
    /// The run's initialisation time (its "12z").
    pub init: UnixTime,
    /// When the run's data became available.
    pub available: UnixTime,
}

/// One deterministic model's hourly forecast at the place.
///
/// Every value vector has one entry per entry of `times`; `None` is a value
/// the model did not supply.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PointSeries {
    /// The Open-Meteo `models` id, e.g. `ecmwf_ifs025`.
    pub model: String,
    /// The run the series comes from, when known.
    pub run: Option<ModelRun>,
    /// Ascending, hourly.
    pub times: Vec<UnixTime>,
    /// Precipitation (rain, showers and snow as water) in the hour before
    /// each time, in millimetres.
    pub precipitation_mm: Vec<Option<f32>>,
    /// Snowfall in the hour before each time, in centimetres of snow.
    pub snowfall_cm: Vec<Option<f32>>,
    /// Wind gusts at 10 m, in kilometres per hour.
    pub gust_kmh: Vec<Option<f32>>,
    /// Mean sea-level pressure, in hectopascals.
    pub pressure_hpa: Vec<Option<f32>>,
}

/// One ensemble's members at the place.
///
/// Member 0 is the control run. Each member's vector has one entry per entry
/// of `times`.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct EnsembleSeries {
    /// The Open-Meteo Ensemble API `models` id, e.g. `gfs025`.
    pub model: String,
    /// The run the members come from, when known.
    pub run: Option<ModelRun>,
    /// Ascending, hourly.
    pub times: Vec<UnixTime>,
    /// Precipitation in the hour before each time, in millimetres, as
    /// `[member][time]`.
    pub precipitation_mm: Vec<Vec<Option<f32>>>,
    /// Snowfall in the hour before each time, in centimetres, as
    /// `[member][time]`.
    pub snowfall_cm: Vec<Vec<Option<f32>>>,
}

impl EnsembleSeries {
    /// How many members (the control included) the ensemble has.
    pub fn member_count(&self) -> usize {
        self.precipitation_mm.len()
    }
}

/// One model's sea-level pressure on a regular grid around the place.
///
/// Points are stored row by row: the point at `row` (an index into `lats`)
/// and `col` (an index into `lons`) is `row * lons.len() + col`.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PressureGrid {
    /// The Open-Meteo `models` id.
    pub model: String,
    /// The run the grid comes from, when known.
    pub run: Option<ModelRun>,
    /// Row latitudes in degrees, south to north.
    pub lats: Vec<f32>,
    /// Column longitudes in degrees, west to east, each in `-180.0..180.0`;
    /// a grid across the antimeridian wraps from 180 to -180 between two
    /// columns.
    pub lons: Vec<f32>,
    /// Ascending, usually 6 hours apart.
    pub times: Vec<UnixTime>,
    /// Mean sea-level pressure in hectopascals, as `[time][point]`.
    pub pressure_hpa: Vec<Vec<Option<f32>>>,
}

impl PressureGrid {
    /// The point index of `row` and `col`.
    pub fn index(&self, row: usize, col: usize) -> usize {
        row * self.lons.len() + col
    }

    /// The pressure at time index `step`, `row` and `col`, if the grid has it.
    pub fn value(&self, step: usize, row: usize, col: usize) -> Option<f32> {
        if row >= self.lats.len() || col >= self.lons.len() {
            return None;
        }
        self.pressure_hpa
            .get(step)
            .and_then(|points| points.get(self.index(row, col)).copied().flatten())
    }
}

/// One earlier run's totals at the place over an event window.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PastRun {
    /// The Open-Meteo `models` id.
    pub model: String,
    /// The earlier run.
    pub run: ModelRun,
    /// The window's start (inclusive).
    pub window_start: UnixTime,
    /// The window's end (exclusive).
    pub window_end: UnixTime,
    /// Precipitation over the window, in millimetres; `None` when the run had
    /// no values for it.
    pub window_total_mm: Option<f32>,
    /// Snowfall over the window, in centimetres.
    pub window_snow_cm: Option<f32>,
}

/// Everything the storms view is built from, for one place.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StormOutlook {
    /// The place.
    pub place: ResolvedLocation,
    /// UTC offset changes (DST) within the period, ascending; empty when
    /// `place.utc_offset` applies throughout.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub offset_changes: Vec<OffsetChange>,
    /// The deterministic models at the place, in display order.
    pub points: Vec<PointSeries>,
    /// The ensembles at the place.
    pub ensembles: Vec<EnsembleSeries>,
    /// Pressure grids around the place, one per model.
    pub grids: Vec<PressureGrid>,
    /// Earlier runs' totals fetched with the outlook.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub past_runs: Vec<PastRun>,
    /// When the data was received.
    pub fetched_at: UnixTime,
}

impl StormOutlook {
    /// The UTC offset local times use at `t`: the last change at or before
    /// it, else the place's.
    pub fn offset_at(&self, t: UnixTime) -> UtcOffset {
        self.offset_changes
            .iter()
            .take_while(|c| c.at <= t)
            .last()
            .map_or(self.place.utc_offset, |c| c.offset)
    }
}

/// The model family a series belongs to; displays pick a color and a dash
/// pattern from it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum ModelRole {
    /// ECMWF IFS (deterministic and ensemble).
    Ecmwf,
    /// NOAA GFS and its ensemble, GEFS.
    Gfs,
    /// Environment Canada's GEM.
    Gem,
    /// DWD's ICON.
    Icon,
    /// Any other model.
    Other,
}

impl ModelRole {
    /// The role of an Open-Meteo model id or `meta.json` domain.
    pub fn of(model: &str) -> Self {
        let starts = |prefixes: &[&str]| prefixes.iter().any(|p| model.starts_with(p));
        if starts(&["ecmwf_ifs"]) {
            Self::Ecmwf
        } else if starts(&["gfs", "ncep_gfs", "ncep_gefs"]) {
            Self::Gfs
        } else if starts(&["gem_", "cmc_gem"]) {
            Self::Gem
        } else if starts(&["icon_", "dwd_icon"]) {
            Self::Icon
        } else {
            Self::Other
        }
    }

    /// The key displays use for theme roles: `ecmwf`, `gfs`, `gem`, `icon` or
    /// `other`.
    pub fn key(self) -> &'static str {
        match self {
            Self::Ecmwf => "ecmwf",
            Self::Gfs => "gfs",
            Self::Gem => "gem",
            Self::Icon => "icon",
            Self::Other => "other",
        }
    }
}

/// How closely the models agree on an event.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum Agreement {
    /// The models tell the same story.
    High,
    /// They differ in amounts or placement.
    Medium,
    /// They tell different stories.
    Low,
}

impl Agreement {
    /// The severity level displays color it with.
    pub fn level(self) -> Level {
        match self {
            Self::High => Level::Good,
            Self::Medium => Level::Moderate,
            Self::Low => Level::Poor,
        }
    }
}

/// The pressure grid's row latitudes and column longitudes around a place:
/// [`GRID_SIZE`] points per side, [`GRID_SPACING_DEG`] apart, centred on the
/// place snapped to [`GRID_SNAP_DEG`].
///
/// The centre latitude is clamped so no row passes [`GRID_MAX_LATITUDE`], and
/// longitudes wrap into `-180.0..180.0`.
pub fn grid_axes(latitude: f64, longitude: f64) -> (Vec<f32>, Vec<f32>) {
    let snap = |x: f64| libm::round(x / GRID_SNAP_DEG) * GRID_SNAP_DEG;
    let half = (GRID_SIZE / 2) as f64 * f64::from(GRID_SPACING_DEG);
    let limit = f64::from(GRID_MAX_LATITUDE) - half;
    let centre_lat = snap(latitude).clamp(-limit, limit);
    let centre_lon = snap(longitude);
    let offsets = (0..GRID_SIZE).map(|i| i as f64 * f64::from(GRID_SPACING_DEG) - half);
    let lats = offsets.clone().map(|d| (centre_lat + d) as f32).collect();
    let lons = offsets
        .map(|d| wrap_longitude(centre_lon + d) as f32)
        .collect();
    (lats, lons)
}

/// `lon` in degrees, wrapped into `-180.0..180.0`.
pub fn wrap_longitude(lon: f64) -> f64 {
    let r = libm::fmod(lon + 180.0, 360.0);
    let shifted = if r < 0.0 { r + 360.0 } else { r };
    let wrapped = shifted - 180.0;
    // Adding 360 to a tiny negative remainder can round up to exactly 360.
    if wrapped >= 180.0 {
        wrapped - 360.0
    } else {
        wrapped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec;

    #[test]
    fn grid_axes_centre_on_new_york() {
        let (lats, lons) = grid_axes(40.7143, -74.005_97);
        assert_eq!(lats, [33.25, 35.75, 38.25, 40.75, 43.25, 45.75, 48.25]);
        assert_eq!(lons, [-81.5, -79.0, -76.5, -74.0, -71.5, -69.0, -66.5]);
    }

    #[test]
    fn grid_axes_clamp_near_the_poles_and_wrap_the_antimeridian() {
        let (lats, _) = grid_axes(78.2, 15.6);
        assert_eq!(lats.last().copied(), Some(80.0));
        assert_eq!(lats.first().copied(), Some(65.0));
        let (lats, _) = grid_axes(-89.0, 0.0);
        assert_eq!(lats.first().copied(), Some(-80.0));
        let (_, lons) = grid_axes(-17.7, 178.4);
        assert_eq!(lons, [171.0, 173.5, 176.0, 178.5, -179.0, -176.5, -174.0]);
        let (_, lons) = grid_axes(64.8, -179.9);
        assert_eq!(lons[0], 172.5);
        assert!(lons.iter().all(|l| (-180.0..180.0).contains(l)));
    }

    #[test]
    fn longitudes_wrap_into_range() {
        assert_eq!(wrap_longitude(180.0), -180.0);
        assert_eq!(wrap_longitude(-180.0), -180.0);
        assert_eq!(wrap_longitude(181.5), -178.5);
        assert_eq!(wrap_longitude(-200.0), 160.0);
    }

    #[test]
    fn roles_cover_model_ids_and_run_domains() {
        for (id, role) in [
            ("ecmwf_ifs025", ModelRole::Ecmwf),
            ("ecmwf_ifs025_ensemble", ModelRole::Ecmwf),
            ("gfs_seamless", ModelRole::Gfs),
            ("gfs025", ModelRole::Gfs),
            ("ncep_gfs025", ModelRole::Gfs),
            ("ncep_gefs025", ModelRole::Gfs),
            ("gem_seamless", ModelRole::Gem),
            ("cmc_gem_gdps", ModelRole::Gem),
            ("icon_seamless", ModelRole::Icon),
            ("dwd_icon", ModelRole::Icon),
            ("ecmwf_aifs025_single", ModelRole::Other),
            ("ncep_aigfs025", ModelRole::Other),
            ("ncep_hrrr_conus", ModelRole::Other),
        ] {
            assert_eq!(ModelRole::of(id), role, "{id}");
        }
        assert_eq!(
            DETERMINISTIC_MODELS.map(|m| ModelRole::of(m).key()),
            ["ecmwf", "gfs", "gem", "icon"]
        );
        assert_eq!(
            ENSEMBLE_MODELS.map(|m| ModelRole::of(m).key()),
            ["ecmwf", "gfs"]
        );
    }

    #[test]
    fn agreement_maps_to_levels_in_order() {
        assert_eq!(Agreement::High.level(), Level::Good);
        assert_eq!(Agreement::Medium.level(), Level::Moderate);
        assert_eq!(Agreement::Low.level(), Level::Poor);
        assert!(Agreement::High.level() < Agreement::Low.level());
    }

    #[test]
    fn grid_values_are_indexed_row_by_row() {
        let grid = PressureGrid {
            model: "gfs_seamless".to_string(),
            run: None,
            lats: vec![40.0, 42.5],
            lons: vec![-75.0, -72.5, -70.0],
            times: vec![UnixTime(0)],
            pressure_hpa: vec![vec![
                Some(1.0),
                Some(2.0),
                Some(3.0),
                Some(4.0),
                None,
                Some(6.0),
            ]],
        };
        assert_eq!(grid.value(0, 1, 0), Some(4.0));
        assert_eq!(grid.value(0, 0, 2), Some(3.0));
        assert_eq!(grid.value(0, 1, 1), None);
        assert_eq!(grid.value(0, 0, 3), None);
        assert_eq!(grid.value(1, 0, 0), None);
    }

    #[test]
    fn offsets_follow_the_changes() {
        let outlook = StormOutlook {
            place: ResolvedLocation {
                utc_offset: UtcOffset(-14_400),
                ..ResolvedLocation::default()
            },
            offset_changes: vec![OffsetChange {
                at: UnixTime(1_000),
                offset: UtcOffset(-18_000),
            }],
            ..StormOutlook::default()
        };
        assert_eq!(outlook.offset_at(UnixTime(999)), UtcOffset(-14_400));
        assert_eq!(outlook.offset_at(UnixTime(1_000)), UtcOffset(-18_000));
    }

    /// Caches without the optional lists still load.
    #[cfg(feature = "serde")]
    #[test]
    fn outlooks_load_without_optional_lists() {
        let json = r#"{"place":{"name":"New York","region":null,"latitude":40.7143,
            "longitude":-74.00597,"elevation_m":null,"utc_offset":-18000,"tz_name":null,
            "tz_abbrev":null},"points":[],"ensembles":[],"grids":[],"fetched_at":5}"#;
        let outlook: StormOutlook = serde_json::from_str(json).unwrap();
        assert!(outlook.past_runs.is_empty() && outlook.offset_changes.is_empty());
        assert_eq!(outlook.fetched_at, UnixTime(5));
        let back = serde_json::to_string(&outlook).unwrap();
        assert!(!back.contains("past_runs") && !back.contains("offset_changes"));
    }
}
