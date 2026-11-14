//! The storms view: several models side by side for the coming storms at one
//! place, already formatted.
//!
//! [`build_storms`] turns a [`StormOutlook`] into a [`StormsVm`]. Like
//! [`crate::view::ViewModel`], it carries display text plus the numbers charts
//! and maps need to place things (in display units); displays never convert or
//! format.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::models;
use crate::presentation::PresentationConfig;
use crate::scales::Level;
use crate::storm::{Agreement, ModelRole, ModelRun, PastRun, StormOutlook};
use crate::strings::{Strings, Text, weekday_text};
use crate::time::{UnixTime, UtcOffset};
use crate::view::format::{Formatter, Quantity};

/// Everything the storms screen shows.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct StormsVm {
    /// The place's name, e.g. "New York".
    pub place: String,
    /// The place's latitude in degrees, for the map pin.
    pub latitude: f64,
    /// The place's longitude in degrees.
    pub longitude: f64,
    /// One chip per deterministic model, in display order.
    pub models: Vec<ModelChipVm>,
    /// A sentence when there is nothing to show as an event, e.g. "No storm in
    /// the next 10 days".
    pub summary: Option<String>,
    /// The coming storms at the place, in time order.
    pub events: Vec<StormEventVm>,
    /// Low-pressure tracks on the map, per model.
    pub tracks: Vec<TrackVm>,
    /// The time scrubber's steps, oldest first.
    pub steps: Vec<StepVm>,
    /// The required data credits; every display shows them.
    pub attribution: Vec<String>,
}

/// A model's chip: what it is and which run is shown.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ModelChipVm {
    /// The Open-Meteo `models` id, e.g. `ecmwf_ifs025`.
    pub model: String,
    /// The full name, e.g. "ECMWF IFS 0.25°".
    pub label: String,
    /// The short name, e.g. "ECMWF".
    pub short: String,
    /// The color and dash role.
    pub role: ModelRole,
    /// The run's start in UTC, e.g. "12z", or the missing placeholder.
    pub run: String,
}

/// One coming storm at the place.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct StormEventVm {
    /// When it happens, e.g. "Sat night – Mon".
    pub title: String,
    /// The event window's start, for highlighting the scrubber.
    pub start: UnixTime,
    /// The event window's end.
    pub end: UnixTime,
    /// One row per model.
    pub rows: Vec<ModelRowVm>,
    /// How closely the models agree.
    pub agreement: AgreementVm,
    /// The ensembles' spread for the event.
    pub ensembles: Vec<EnsembleVm>,
    /// How each model's totals changed over its recent runs.
    pub run_to_run: Vec<RunToRunVm>,
}

/// One model's numbers for an event.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ModelRowVm {
    /// The Open-Meteo `models` id.
    pub model: String,
    /// The short name, e.g. "GFS".
    pub short: String,
    /// The color and dash role.
    pub role: ModelRole,
    /// Snowfall over the event, e.g. "14.0 in".
    pub snow: Quantity,
    /// Precipitation (as water) over the event, e.g. "1.73 in".
    pub precipitation: Quantity,
    /// The strongest gust, e.g. "51 mph".
    pub gust: Quantity,
    /// The lowest pressure at the place, e.g. "978 hPa".
    pub low: Quantity,
    /// When the lowest pressure comes, e.g. "Sun 1 PM".
    pub low_at: String,
    /// When the precipitation starts, e.g. "Sat 9 PM".
    pub onset: String,
}

/// The models' agreement on an event.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct AgreementVm {
    /// The agreement itself.
    pub agreement: Agreement,
    /// The severity level displays color it with.
    pub level: Level,
    /// Its name, e.g. "Low agreement".
    pub label: String,
    /// Why, e.g. "Euro brings the low within 60 mi; GFS keeps it 200 mi out".
    pub sentence: String,
}

/// One ensemble's spread for an event.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct EnsembleVm {
    /// The Ensemble API `models` id, e.g. `gfs025`.
    pub model: String,
    /// The short name, e.g. "GEFS".
    pub short: String,
    /// The color and dash role.
    pub role: ModelRole,
    /// How many members (the control included).
    pub members: u32,
    /// The range in words, e.g. "Most members: 4–12 in".
    pub text: String,
    /// The 10th percentile of the members' totals.
    pub p10: Quantity,
    /// The median of the members' totals.
    pub p50: Quantity,
    /// The 90th percentile of the members' totals.
    pub p90: Quantity,
    /// The members' running totals over the event.
    pub plume: PlumeVm,
}

/// The members' running totals as a chart: every series has one value per
/// entry of `times`, in display units (`None` where a member has no value).
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PlumeVm {
    /// What the chart shows, e.g. "Snow".
    pub title: String,
    /// The values' unit symbol, e.g. "in".
    pub unit: &'static str,
    /// The x positions, ascending (6 hours apart).
    pub times: Vec<UnixTime>,
    /// Each member's running total, as `[member][time]`.
    pub members: Vec<Vec<Option<f32>>>,
    /// The 10th percentile at each time.
    pub p10: Vec<Option<f32>>,
    /// The median at each time.
    pub p50: Vec<Option<f32>>,
    /// The 90th percentile at each time.
    pub p90: Vec<Option<f32>>,
    /// The top of the y axis.
    pub y_max: f32,
    /// Labels along the x axis.
    pub x_labels: Vec<TimeTickVm>,
    /// Labels along the y axis.
    pub y_labels: Vec<ValueTickVm>,
}

/// A label at a time on a chart's x axis.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TimeTickVm {
    /// Where the label goes.
    pub time: UnixTime,
    /// The label, e.g. "Sun".
    pub text: String,
}

/// A label at a value on a chart's y axis.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ValueTickVm {
    /// Where the label goes, in display units.
    pub value: f32,
    /// The label, e.g. "6 in".
    pub text: String,
}

/// One model's totals for an event over its recent runs.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RunToRunVm {
    /// The Open-Meteo `models` id.
    pub model: String,
    /// The short name.
    pub short: String,
    /// The color and dash role.
    pub role: ModelRole,
    /// What the bars measure, e.g. "Snow".
    pub title: String,
    /// One bar per run, oldest first; the last is the current run.
    pub bars: Vec<RunBarVm>,
    /// The top of the bars' scale, in display units.
    pub max: f32,
}

/// One run's bar.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RunBarVm {
    /// The run, e.g. "12z Fri".
    pub label: String,
    /// The run's initialisation time.
    pub init: UnixTime,
    /// The bar's height in display units.
    pub value: f32,
    /// The value as text, e.g. "11.0 in".
    pub text: String,
    /// Whether this is the run the rest of the screen shows.
    pub current: bool,
}

/// A low's track in one model.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TrackVm {
    /// The Open-Meteo `models` id.
    pub model: String,
    /// The short name.
    pub short: String,
    /// The color and dash role.
    pub role: ModelRole,
    /// Whether this low is the storm at the place (the others are context).
    pub is_storm: bool,
    /// The low's positions, oldest first.
    pub points: Vec<TrackPointVm>,
}

/// One position of a low.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TrackPointVm {
    /// Latitude in degrees.
    pub latitude: f32,
    /// Longitude in degrees, in `-180.0..180.0`.
    pub longitude: f32,
    /// When the low is there; matches a [`StepVm::time`].
    pub time: UnixTime,
    /// The central pressure, e.g. "978 hPa" or "28.88 inHg".
    pub pressure: Quantity,
}

/// One step of the time scrubber.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct StepVm {
    /// The step's time.
    pub time: UnixTime,
    /// e.g. "Sun 7 PM · +72 h"; the first step reads "… · Now".
    pub label: String,
}

/// Builds the storms view of `outlook` at `now`. `past` adds earlier runs'
/// totals to those the outlook carries, for the run-to-run bars.
pub fn build_storms(
    outlook: &StormOutlook,
    past: &[PastRun],
    presentation: &PresentationConfig,
    strings: &Strings,
    now: UnixTime,
) -> StormsVm {
    let f = Formatter::new(presentation, strings);
    // Earlier runs feed the run-to-run bars, which come with the events.
    let _ = past;
    StormsVm {
        place: outlook.place.name.clone(),
        latitude: outlook.place.latitude,
        longitude: outlook.place.longitude,
        models: chips(outlook, presentation),
        summary: None,
        events: Vec::new(),
        tracks: Vec::new(),
        steps: steps(outlook, &f, now),
        attribution: attribution(outlook),
    }
}

/// The run's start in UTC, "12z".
fn run_text(run: Option<&ModelRun>, presentation: &PresentationConfig) -> String {
    run.map_or_else(
        || presentation.missing.clone(),
        |r| format!("{:02}z", r.init.to_local(UtcOffset(0)).hour),
    )
}

fn chips(outlook: &StormOutlook, presentation: &PresentationConfig) -> Vec<ModelChipVm> {
    let mut chips: Vec<ModelChipVm> = Vec::new();
    for point in &outlook.points {
        if chips.iter().any(|c| c.model == point.model) {
            continue;
        }
        let run = point.run.as_ref().or_else(|| {
            outlook
                .grids
                .iter()
                .find(|g| g.model == point.model)
                .and_then(|g| g.run.as_ref())
        });
        chips.push(ModelChipVm {
            model: point.model.clone(),
            label: models::lookup(&point.model)
                .map_or_else(|| point.model.clone(), |m| m.name.to_string()),
            short: models::short_name(&point.model).to_string(),
            role: ModelRole::of(&point.model),
            run: run_text(run, presentation),
        });
    }
    chips
}

/// The scrubber steps: the grids' times (else the point series' times on
/// 6-hour boundaries), from the last one at or before `now`.
fn steps(outlook: &StormOutlook, f: &Formatter<'_>, now: UnixTime) -> Vec<StepVm> {
    const SIX_HOURS: i64 = 6 * 3_600;
    let mut times: Vec<UnixTime> = outlook
        .grids
        .iter()
        .flat_map(|g| g.times.iter().copied())
        .collect();
    if times.is_empty() {
        times = outlook
            .points
            .iter()
            .flat_map(|p| p.times.iter().copied())
            .filter(|t| t.0.rem_euclid(SIX_HOURS) == 0)
            .collect();
    }
    times.sort_unstable();
    times.dedup();
    let first = times.iter().rposition(|t| *t <= now).unwrap_or(0);
    times
        .iter()
        .skip(first)
        .map(|t| StepVm {
            time: *t,
            label: step_label(*t, outlook.offset_at(*t), f, now),
        })
        .collect()
}

fn step_label(t: UnixTime, offset: UtcOffset, f: &Formatter<'_>, now: UnixTime) -> String {
    let day = f.strings.get(weekday_text(t.to_local(offset).weekday));
    let when = if t <= now {
        f.strings.get(Text::Now).to_string()
    } else {
        // Hours ahead, rounded to the nearest hour.
        let hours = (t.0 - now.0 + 1_800) / 3_600;
        format!(
            "+{}",
            f.strings
                .fmt(Text::HoursShort, &[("n", &hours.to_string())])
        )
    };
    format!("{day} {} · {when}", f.hour_label(t, offset))
}

/// "Models: ECMWF (CC BY 4.0), NOAA GFS, ECCC GEM, DWD ICON via Open-Meteo.com
/// (CC BY 4.0)", naming the model families the outlook uses.
fn attribution(outlook: &StormOutlook) -> Vec<String> {
    let mut roles: Vec<(ModelRole, &str)> = Vec::new();
    let ids = outlook
        .points
        .iter()
        .map(|p| p.model.as_str())
        .chain(outlook.ensembles.iter().map(|e| e.model.as_str()))
        .chain(outlook.grids.iter().map(|g| g.model.as_str()));
    for id in ids {
        let role = ModelRole::of(id);
        let known = |r: &(ModelRole, &str)| match role {
            ModelRole::Other => r.1 == id,
            _ => r.0 == role,
        };
        if !roles.iter().any(known) {
            roles.push((role, id));
        }
    }
    roles.sort_by_key(|r| r.0);
    let credits: Vec<&str> = roles
        .iter()
        .map(|(role, id)| match role {
            ModelRole::Ecmwf => "ECMWF (CC BY 4.0)",
            ModelRole::Gfs => "NOAA GFS",
            ModelRole::Gem => "ECCC GEM",
            ModelRole::Icon => "DWD ICON",
            ModelRole::Other => models::short_name(id),
        })
        .collect();
    let line = if credits.is_empty() {
        String::from("Model data via Open-Meteo.com (CC BY 4.0)")
    } else {
        format!(
            "Models: {} via Open-Meteo.com (CC BY 4.0)",
            credits.join(", ")
        )
    };
    alloc::vec![line]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presentation::TimeFormat;
    use crate::storm::testdata::{
        GRID_STEP_SECS, STORM_NOW, dry_week, noreaster, noreaster_past_runs,
    };

    fn build(outlook: &StormOutlook, presentation: &PresentationConfig) -> StormsVm {
        build_storms(
            outlook,
            &noreaster_past_runs(),
            presentation,
            &Strings::default(),
            STORM_NOW,
        )
    }

    #[test]
    fn chips_name_the_four_models_and_their_runs() {
        for outlook in [noreaster(), dry_week()] {
            let vm = build(&outlook, &PresentationConfig::default());
            let chips: Vec<(&str, &str, &str, &str)> = vm
                .models
                .iter()
                .map(|c| {
                    (
                        c.short.as_str(),
                        c.role.key(),
                        c.run.as_str(),
                        c.model.as_str(),
                    )
                })
                .collect();
            assert_eq!(
                chips,
                [
                    ("ECMWF", "ecmwf", "12z", "ecmwf_ifs025"),
                    ("GFS", "gfs", "12z", "gfs_seamless"),
                    ("GEM", "gem", "12z", "gem_seamless"),
                    ("ICON", "icon", "12z", "icon_seamless"),
                ]
            );
            assert_eq!(vm.models[0].label, "ECMWF IFS 0.25°");
            assert_eq!(vm.place, "New York");
            assert!((vm.latitude - 40.7143).abs() < 1e-9);
            assert!(vm.events.is_empty() && vm.tracks.is_empty());
        }
    }

    #[test]
    fn chips_show_the_placeholder_without_a_run() {
        let mut outlook = noreaster();
        outlook.points[1].run = None;
        outlook.grids.retain(|g| g.model != "gfs_seamless");
        outlook.points[2].run = None;
        let vm = build(&outlook, &PresentationConfig::default());
        let runs: Vec<&str> = vm.models.iter().map(|c| c.run.as_str()).collect();
        // GEM's grid still knows its run.
        assert_eq!(runs, ["12z", "—", "12z", "12z"]);
    }

    #[test]
    fn steps_start_now_and_count_hours_ahead() {
        let vm = build(&noreaster(), &PresentationConfig::default());
        // Fetched Fri 14:00 EST; grid steps are at 00, 06, 12 and 18 UTC.
        assert_eq!(vm.steps.len(), 40 - 3);
        assert_eq!(vm.steps[0].label, "Fri 13:00 · Now");
        assert_eq!(vm.steps[1].label, "Fri 19:00 · +5 h");
        assert!(
            vm.steps
                .windows(2)
                .all(|w| w[1].time.0 - w[0].time.0 == GRID_STEP_SECS)
        );
        let twelve = PresentationConfig {
            time_format: TimeFormat::H12,
            ..PresentationConfig::default()
        };
        let vm = build(&noreaster(), &twelve);
        assert_eq!(vm.steps[13].label, "Mon 7 PM · +77 h");
    }

    #[test]
    fn steps_fall_back_to_the_point_times() {
        let mut outlook = dry_week();
        outlook.grids.clear();
        let vm = build(&outlook, &PresentationConfig::default());
        assert_eq!(vm.steps.len(), 37);
        assert_eq!(vm.steps[0].label, "Fri 13:00 · Now");
        let empty = build(&StormOutlook::default(), &PresentationConfig::default());
        assert_eq!(empty.steps.len(), 0);
    }

    #[test]
    fn attribution_credits_each_model_family_once() {
        let vm = build(&noreaster(), &PresentationConfig::default());
        assert_eq!(
            vm.attribution,
            [
                "Models: ECMWF (CC BY 4.0), NOAA GFS, ECCC GEM, DWD ICON via Open-Meteo.com (CC BY 4.0)"
            ]
        );
        let mut outlook = dry_week();
        outlook.points.truncate(1);
        outlook.grids.clear();
        outlook.ensembles.clear();
        outlook.points[0].model = "ukmo_seamless".to_string();
        let vm = build(&outlook, &PresentationConfig::default());
        assert_eq!(
            vm.attribution,
            ["Models: UKMO via Open-Meteo.com (CC BY 4.0)"]
        );
        let vm = build(&StormOutlook::default(), &PresentationConfig::default());
        assert_eq!(
            vm.attribution,
            ["Model data via Open-Meteo.com (CC BY 4.0)"]
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn the_view_serializes_role_keys() {
        let vm = build(&noreaster(), &PresentationConfig::default());
        let json = serde_json::to_value(&vm).unwrap();
        assert_eq!(json["models"][1]["role"], "gfs");
        assert_eq!(json["models"][1]["run"], "12z");
        assert_eq!(json["steps"][0]["time"], STORM_NOW.0 - 3_600);
    }
}
