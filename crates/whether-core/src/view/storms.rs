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
use crate::storm::analysis::{
    self, AgreementResult, EnsembleEventStats, MemberSpread, ModelEventStats, Percentiles,
    StormCenter, StormEvent,
};
use crate::storm::track::{self, Track};
use crate::storm::{Agreement, ModelRole, ModelRun, PastRun, StormOutlook};
use crate::strings::{Strings, Text, weekday_text};
use crate::time::{UnixTime, UtcOffset, Weekday};
use crate::units::{Hpa, Meters, Mm, Mps, WindUnit};
use crate::view::format::{Formatter, Quantity, number};

const HOUR: i64 = 3_600;
const DAY: i64 = 24 * HOUR;

/// Besides the storm, the map shows at most this many other lows per model.
pub const MAP_OTHER_TRACKS: usize = 2;

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
    /// When the rain or snow falls (the window without its padding), e.g.
    /// "Sat night – Sun night" or "Sun afternoon".
    pub title: String,
    /// The event window's start, for highlighting the scrubber.
    pub start: UnixTime,
    /// The event window's end.
    pub end: UnixTime,
    /// One row per model.
    pub rows: Vec<ModelRowVm>,
    /// How closely the models agree; `None` when fewer than two models have
    /// a total or a storm centre to compare.
    pub agreement: Option<AgreementVm>,
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
    /// Why, e.g. "ECMWF brings the low within 55 mi of New York; GFS keeps
    /// it 200 mi away.", or without storm centres "ECMWF brings 14 in of
    /// snow; GFS 3 in."; empty when the models can't be told apart.
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
    /// The members' P10–P90 range and median in words, e.g. "Most members:
    /// 4–12 in (median 7 in)".
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
    /// The x positions, ascending: the event's start, where every total is
    /// zero, then the end of each 6-hour step, the last at the event's end.
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
    /// The run's start in UTC with its day, e.g. "Fri 06z", or the missing
    /// placeholder for a current run of unknown start.
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
///
/// The map shows, per pressure grid, the low that passes closest to the
/// place during the first event (flagged [`TrackVm::is_storm`]) and up to
/// [`MAP_OTHER_TRACKS`] other lows, deepest first. Without an event there is
/// no storm, only the other lows, and `summary` says so.
pub fn build_storms(
    outlook: &StormOutlook,
    past: &[PastRun],
    presentation: &PresentationConfig,
    strings: &Strings,
    now: UnixTime,
) -> StormsVm {
    let f = Formatter::new(presentation, strings);
    let events = analysis::find_events(outlook);
    let found: Vec<Vec<Track>> = outlook.grids.iter().map(track::track_lows).collect();
    StormsVm {
        place: outlook.place.name.clone(),
        latitude: outlook.place.latitude,
        longitude: outlook.place.longitude,
        models: chips(outlook, presentation),
        summary: summary(outlook, &events, &f),
        events: events
            .iter()
            .map(|e| event_vm(outlook, past, &found, e, &f))
            .collect(),
        tracks: tracks(outlook, &found, events.first(), &f),
        steps: steps(outlook, &f, now),
        attribution: attribution(outlook),
    }
}

/// A card warning of the first significant storm coming at the place.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct StormAheadVm {
    /// e.g. "Storm ahead · Sat night – Sun night", named like
    /// [`StormEventVm::title`].
    pub title: String,
    /// The headline amount, then how the models differ when they don't
    /// agree well, e.g. "Up to 14 in of snow · models disagree", "Up to
    /// 1.7 in of rain" or "Gusts to 52 mph".
    pub line: String,
    /// The severity level displays color the card with: the agreement's, or
    /// Moderate when the agreement is unknown.
    pub level: Level,
    /// The agreement's name, e.g. "Low agreement"; `None` when fewer than two
    /// models can be compared.
    pub agreement: Option<String>,
    /// The event window's start.
    pub start: UnixTime,
    /// The event window's end.
    pub end: UnixTime,
}

/// The "Storm ahead" card for `outlook` at `now`: the first event
/// [`analysis::significant_event`] finds, or `None` when nothing coming is
/// significant.
///
/// The headline is whichever of snow, precipitation and gusts goes furthest
/// past its significance threshold (snow, then precipitation, on a tie), as
/// the largest any model brings. The agreement uses the storm centres when
/// the outlook has pressure grids, else the totals alone.
pub fn storm_ahead(
    outlook: &StormOutlook,
    presentation: &PresentationConfig,
    strings: &Strings,
    now: UnixTime,
) -> Option<StormAheadVm> {
    let event = analysis::significant_event(outlook, now)?;
    let f = Formatter::new(presentation, strings);
    let (wet_start, wet_end) = wet_span(outlook, &event);
    let when = event_title(&f, outlook, wet_start, wet_end);
    let found: Vec<Vec<Track>> = outlook.grids.iter().map(track::track_lows).collect();
    let centers = storm_centers(outlook, &found, &event);
    let agreement = agreement_vm(outlook, &event, &centers, &f);
    let headline = headline(&f, &event);
    let differ = match agreement.as_ref().map(|a| a.agreement) {
        Some(Agreement::Medium) => Some(Text::StormAheadModelsDiffer),
        Some(Agreement::Low) => Some(Text::StormAheadModelsDisagree),
        Some(Agreement::High) | None => None,
    };
    let line = match differ {
        Some(text) => f.strings.fmt(
            Text::StormAheadLine,
            &[("headline", &headline), ("agreement", f.strings.get(text))],
        ),
        None => headline,
    };
    Some(StormAheadVm {
        title: f.strings.fmt(Text::StormAheadTitle, &[("when", &when)]),
        line,
        level: agreement
            .as_ref()
            .map_or(Level::Moderate, |a| a.agreement.level()),
        agreement: agreement.map(|a| a.label),
        start: event.start,
        end: event.end,
    })
}

/// The largest of a significant event's quantities, relative to its
/// threshold: "Up to 14 in of snow", "Up to 1.7 in of rain" or "Gusts to
/// 52 mph", each the most any model brings.
fn headline(f: &Formatter<'_>, event: &StormEvent) -> String {
    let most = |pick: fn(&ModelEventStats) -> Option<f32>| {
        event.models.iter().filter_map(pick).reduce(f32::max)
    };
    let snow = most(|m| m.total_snow_cm).map(|cm| {
        (
            event.mean_snow_cm / analysis::SIGNIFICANT_SNOW_CM,
            f.strings.fmt(
                Text::StormAheadSnow,
                &[("amount", &rough_amount(f, true, cm))],
            ),
        )
    });
    let rain = most(|m| m.total_mm).map(|mm| {
        (
            event.mean_total_mm / analysis::SIGNIFICANT_PRECIP_MM,
            f.strings.fmt(
                Text::StormAheadRain,
                &[("amount", &rough_amount(f, false, mm))],
            ),
        )
    });
    let gusts = most(|m| m.peak_gust_kmh).map(|kmh| {
        let speed = f.wind(Mps::from_unit(kmh, WindUnit::Kmh));
        (
            analysis::mean_peak_gust_kmh(event).unwrap_or(0.0) / analysis::SIGNIFICANT_GUST_KMH,
            f.strings
                .fmt(Text::StormAheadGusts, &[("speed", &speed.text)]),
        )
    });
    [snow, rain, gusts]
        .into_iter()
        .flatten()
        .fold(None, |best: Option<(f32, String)>, cur| match best {
            Some(b) if b.0 >= cur.0 => Some(b),
            _ => Some(cur),
        })
        .map_or_else(String::new, |(_, text)| text)
}

/// The place as `(latitude, longitude)`.
fn place_of(outlook: &StormOutlook) -> (f64, f64) {
    (outlook.place.latitude, outlook.place.longitude)
}

/// The day and the part of it a forecaster would name for `t`: morning
/// (06–12), afternoon (12–18) or night (18–06, named after its evening).
fn part_of_day(t: UnixTime, offset: UtcOffset) -> (Weekday, Text) {
    let local = t.to_local(offset);
    match local.hour {
        0..=5 => (
            UnixTime(t.0 - 6 * HOUR).to_local(offset).weekday,
            Text::StormNight,
        ),
        6..=11 => (local.weekday, Text::StormMorning),
        12..=17 => (local.weekday, Text::StormAfternoon),
        _ => (local.weekday, Text::StormNight),
    }
}

/// "Sun afternoon", "Tue night".
fn day_part_label(f: &Formatter<'_>, t: UnixTime, offset: UtcOffset) -> String {
    let (day, part) = part_of_day(t, offset);
    f.strings
        .fmt(part, &[("day", f.strings.get(weekday_text(day)))])
}

/// "Sun 1 PM" or "Sun 13:00".
fn day_time(f: &Formatter<'_>, t: UnixTime, offset: UtcOffset) -> String {
    let day = f.strings.get(weekday_text(t.to_local(offset).weekday));
    format!("{day} {}", f.hour_label(t, offset))
}

/// When the rain or snow of `event` falls: its window without the padding
/// [`analysis::find_events`] adds, except where the window meets the edge of
/// the data and was not padded.
fn wet_span(outlook: &StormOutlook, event: &StormEvent) -> (UnixTime, UnixTime) {
    let first = outlook.points.iter().filter_map(|p| p.times.first()).min();
    let last = outlook.points.iter().filter_map(|p| p.times.last()).max();
    let pad = analysis::EVENT_PAD_SECS;
    let start = match first {
        Some(first) if event.start.0 > first.0 - HOUR => event.start.0 + pad,
        _ => event.start.0,
    };
    let end = match last {
        Some(last) if event.end.0 < last.0 => event.end.0 - pad,
        _ => event.end.0,
    };
    if end > start {
        (UnixTime(start), UnixTime(end))
    } else {
        (event.start, event.end)
    }
}

/// The span `[start, end)` in parts of days: "Sun afternoon", "Sun morning –
/// Sun night", or for a day or more "Sat night – Mon" (the end's part is
/// kept when it is a night, which runs into the next day).
fn event_title(
    f: &Formatter<'_>,
    outlook: &StormOutlook,
    start: UnixTime,
    end: UnixTime,
) -> String {
    let last = UnixTime((end.0 - 1).max(start.0));
    let long = end.0 - start.0 >= DAY;
    let from = day_part_label(f, start, outlook.offset_at(start));
    let (start_day, start_part) = part_of_day(start, outlook.offset_at(start));
    let (end_day, end_part) = part_of_day(last, outlook.offset_at(last));
    if (start_day, start_part) == (end_day, end_part) {
        return from;
    }
    let to = if long && end_part != Text::StormNight {
        f.strings.get(weekday_text(end_day)).to_string()
    } else {
        day_part_label(f, last, outlook.offset_at(last))
    };
    f.strings
        .fmt(Text::StormSpan, &[("start", &from), ("end", &to)])
}

/// One event: its span, each model's numbers, how well they agree, the
/// ensembles' spread and the totals of earlier runs.
fn event_vm(
    outlook: &StormOutlook,
    past: &[PastRun],
    found: &[Vec<Track>],
    event: &StormEvent,
    f: &Formatter<'_>,
) -> StormEventVm {
    let centers = storm_centers(outlook, found, event);
    let (wet_start, wet_end) = wet_span(outlook, event);
    StormEventVm {
        title: event_title(f, outlook, wet_start, wet_end),
        start: event.start,
        end: event.end,
        rows: event.models.iter().map(|m| row(outlook, m, f)).collect(),
        agreement: agreement_vm(outlook, event, &centers, f),
        ensembles: analysis::ensemble_stats(outlook, event)
            .iter()
            .map(|stats| ensemble_vm(outlook, event, stats, f))
            .collect(),
        run_to_run: run_to_run(outlook, past, event, f),
    }
}

/// An ensemble's short name: "ECMWF ENS", "GEFS".
fn ensemble_short(model: &str) -> &str {
    match ModelRole::of(model) {
        ModelRole::Ecmwf => "ECMWF ENS",
        ModelRole::Gfs => "GEFS",
        _ => models::short_name(model),
    }
}

/// `x` rounded to 2 decimals, for chart values.
fn chart_value(x: f32) -> f32 {
    let r = libm::roundf(x * 100.0) / 100.0;
    if r == 0.0 { 0.0 } else { r }
}

/// What the event's amounts show: snow when it has snow, else
/// precipitation.
fn amount_title(f: &Formatter<'_>, snow: bool) -> String {
    let text = if snow {
        Text::PrecipSnow
    } else {
        Text::PrecipGeneric
    };
    f.strings.get(text).to_string()
}

/// One ensemble's spread over an event: snow when the event has snow, else
/// precipitation.
fn ensemble_vm(
    outlook: &StormOutlook,
    event: &StormEvent,
    stats: &EnsembleEventStats,
    f: &Formatter<'_>,
) -> EnsembleVm {
    let (spread, snow) = match &stats.snowfall {
        Some(snowfall) => (snowfall, true),
        None => (&stats.precipitation, false),
    };
    let unit = amount(f, snow, None).unit;
    // Both conversions are linear from zero.
    let factor = amount(f, snow, Some(1.0)).value.unwrap_or(1.0);
    let quantile =
        |pick: fn(&Percentiles) -> f32| amount(f, snow, spread.percentiles.as_ref().map(pick));
    let text = spread.percentiles.map_or_else(
        || f.presentation.missing.clone(),
        |p| {
            let (low, high, median) = (p.p10 * factor, p.p90 * factor, p.p50 * factor);
            f.strings.fmt(
                Text::StormMostMembers,
                &[
                    ("low", &rough_number(low, unit, snow)),
                    (
                        "high",
                        &format!("{} {unit}", rough_number(high, unit, snow)),
                    ),
                    (
                        "median",
                        &format!("{} {unit}", rough_number(median, unit, snow)),
                    ),
                ],
            )
        },
    );
    EnsembleVm {
        model: stats.model.clone(),
        short: ensemble_short(&stats.model).to_string(),
        role: ModelRole::of(&stats.model),
        members: u32::try_from(stats.members).unwrap_or(u32::MAX),
        text,
        p10: quantile(|p| p.p10),
        p50: quantile(|p| p.p50),
        p90: quantile(|p| p.p90),
        plume: plume_vm(outlook, event, stats, spread, snow, f),
    }
}

/// The members' running totals as a chart from the event's start, where
/// every total is zero, to its end.
fn plume_vm(
    outlook: &StormOutlook,
    event: &StormEvent,
    stats: &EnsembleEventStats,
    spread: &MemberSpread,
    snow: bool,
    f: &Formatter<'_>,
) -> PlumeVm {
    let unit = amount(f, snow, None).unit;
    let factor = amount(f, snow, Some(1.0)).value.unwrap_or(1.0);
    let line = |values: &[Option<f32>]| -> Vec<Option<f32>> {
        let start = values.iter().any(Option::is_some).then_some(0.0);
        core::iter::once(start)
            .chain(values.iter().map(|v| v.map(|v| chart_value(v * factor))))
            .collect()
    };
    let members: Vec<Vec<Option<f32>>> = spread.plumes.iter().map(|m| line(m)).collect();
    let top = members
        .iter()
        .flatten()
        .flatten()
        .fold(0.0_f32, |a, b| a.max(*b));
    let (y_max, y_labels) = value_axis(top, unit);
    let times: Vec<UnixTime> = core::iter::once(event.start)
        .chain(stats.steps.iter().copied())
        .collect();
    let x_labels = match (times.first(), times.last()) {
        (Some(from), Some(to)) => time_ticks(outlook, f, *from, *to),
        _ => Vec::new(),
    };
    PlumeVm {
        title: amount_title(f, snow),
        unit,
        times,
        p10: line(&spread.plume_p10),
        p50: line(&spread.plume_p50),
        p90: line(&spread.plume_p90),
        members,
        y_max,
        x_labels,
        y_labels,
    }
}

/// A round step for an axis from 0 to `max` with about 5 intervals: 1, 2,
/// 2.5 or 5 times a power of ten (1 for an empty axis).
fn nice_step(max: f32) -> f32 {
    if !max.is_finite() || max <= 0.0 {
        return 1.0;
    }
    let raw = max / 5.0;
    let magnitude = libm::powf(10.0, libm::floorf(libm::log10f(raw)));
    let norm = raw / magnitude;
    let multiple = [1.0, 2.0, 2.5, 5.0]
        .into_iter()
        .find(|m| norm <= m * 1.000_1)
        .unwrap_or(10.0);
    multiple * magnitude
}

/// The decimals `step` needs: 0 for whole numbers, 1 for 2.5, 2 for 0.25.
fn step_decimals(step: f32) -> u8 {
    (0..3_u8)
        .find(|d| {
            let scaled = step * libm::powf(10.0, f32::from(*d));
            libm::fabsf(scaled - libm::roundf(scaled)) < 1e-3
        })
        .unwrap_or(3)
}

/// A y axis from 0 past `max`: its top and its labelled ticks, in `unit`.
fn value_axis(max: f32, unit: &'static str) -> (f32, Vec<ValueTickVm>) {
    let step = nice_step(max);
    let decimals = step_decimals(step);
    let scale = libm::powf(10.0, f32::from(decimals));
    let intervals = libm::ceilf(max / step - 1e-4).max(1.0) as u32;
    let ticks = (0..=intervals)
        .map(|i| {
            let value = libm::roundf(step * i as f32 * scale) / scale;
            ValueTickVm {
                value,
                text: format!("{} {unit}", number(value, decimals)),
            }
        })
        .collect();
    let top = libm::roundf(step * intervals as f32 * scale) / scale;
    (top, ticks)
}

/// Whole local hours in `[from, to]` whose hour is a multiple of `every`.
fn local_hours(outlook: &StormOutlook, from: UnixTime, to: UnixTime, every: u8) -> Vec<UnixTime> {
    let offset = i64::from(outlook.offset_at(from).0);
    let local = from.0 + offset;
    let past = local.rem_euclid(HOUR);
    let mut t = if past == 0 {
        from.0
    } else {
        from.0 + HOUR - past
    };
    let mut out = Vec::new();
    while t <= to.0 {
        let time = UnixTime(t);
        let at = time.to_local(outlook.offset_at(time));
        if at.minute == 0 && at.hour.is_multiple_of(every) {
            out.push(time);
        }
        t += HOUR;
    }
    out
}

/// Labels along a chart's time axis: the day at each local midnight, or
/// with fewer than two midnights, the hour every 6 hours.
fn time_ticks(
    outlook: &StormOutlook,
    f: &Formatter<'_>,
    from: UnixTime,
    to: UnixTime,
) -> Vec<TimeTickVm> {
    let midnights = local_hours(outlook, from, to, 24);
    if midnights.len() >= 2 {
        return midnights
            .into_iter()
            .map(|t| TimeTickVm {
                time: t,
                text: f
                    .strings
                    .get(weekday_text(t.to_local(outlook.offset_at(t)).weekday))
                    .to_string(),
            })
            .collect();
    }
    local_hours(outlook, from, to, 6)
        .into_iter()
        .map(|t| TimeTickVm {
            time: t,
            text: f.hour_label(t, outlook.offset_at(t)),
        })
        .collect()
}

/// A run's start in UTC with its day: "Fri 06z".
fn run_label(f: &Formatter<'_>, init: UnixTime) -> String {
    let at = init.to_local(UtcOffset(0));
    format!(
        "{} {:02}z",
        f.strings.get(weekday_text(at.weekday)),
        at.hour
    )
}

/// How each model's total for `event` changed over its earlier runs: the
/// runs in `outlook.past_runs` and `past` whose window overlaps the event,
/// oldest first, then the current run over the latest of those windows.
fn run_to_run(
    outlook: &StormOutlook,
    past: &[PastRun],
    event: &StormEvent,
    f: &Formatter<'_>,
) -> Vec<RunToRunVm> {
    let snow = event.has_snow();
    let unit = amount(f, snow, None).unit;
    let mut out: Vec<RunToRunVm> = Vec::new();
    for point in &outlook.points {
        if out.iter().any(|r| r.model == point.model) {
            continue;
        }
        let current = point.run.as_ref().map(|r| r.init);
        let mut runs: Vec<&PastRun> = outlook
            .past_runs
            .iter()
            .chain(past)
            .filter(|r| {
                r.model == point.model
                    && r.window_start < event.end
                    && r.window_end > event.start
                    && current.is_none_or(|init| r.run.init < init)
            })
            .collect();
        // A stable sort keeps the outlook's copy of a run first.
        runs.sort_by_key(|r| r.run.init);
        runs.dedup_by_key(|r| r.run.init);
        let Some(latest) = runs.last() else {
            continue;
        };
        let bar = |raw: Option<f32>, label: String, init: UnixTime, current: bool| {
            let q = amount(f, snow, raw);
            q.value.map(|value| RunBarVm {
                label,
                init,
                value: chart_value(value),
                text: q.text,
                current,
            })
        };
        let mut bars: Vec<RunBarVm> = runs
            .iter()
            .filter_map(|r| {
                let raw = if snow {
                    r.window_snow_cm
                } else {
                    r.window_total_mm
                };
                bar(raw, run_label(f, r.run.init), r.run.init, false)
            })
            .collect();
        let (water, snowfall) =
            analysis::window_totals(point, latest.window_start, latest.window_end);
        let label = current.map_or_else(|| f.presentation.missing.clone(), |t| run_label(f, t));
        let init = current.unwrap_or(outlook.fetched_at);
        bars.extend(bar(if snow { snowfall } else { water }, label, init, true));
        if bars.is_empty() {
            continue;
        }
        let top = bars.iter().fold(0.0_f32, |a, b| a.max(b.value));
        out.push(RunToRunVm {
            model: point.model.clone(),
            short: models::short_name(&point.model).to_string(),
            role: ModelRole::of(&point.model),
            title: amount_title(f, snow),
            bars,
            max: value_axis(top, unit).0,
        });
    }
    out
}

/// One model's row: snow, precipitation, peak gust, the lowest pressure and
/// when it comes, and the onset.
fn row(outlook: &StormOutlook, stats: &ModelEventStats, f: &Formatter<'_>) -> ModelRowVm {
    let at = |t: Option<UnixTime>| {
        t.map_or_else(
            || f.presentation.missing.clone(),
            |t| day_time(f, t, outlook.offset_at(t)),
        )
    };
    ModelRowVm {
        model: stats.model.clone(),
        short: models::short_name(&stats.model).to_string(),
        role: ModelRole::of(&stats.model),
        snow: f.snowfall(stats.total_snow_cm),
        precipitation: f.precipitation(stats.total_mm.map(Mm)),
        gust: f.wind(
            stats
                .peak_gust_kmh
                .and_then(|kmh| Mps::from_unit(kmh, WindUnit::Kmh)),
        ),
        low: f.pressure(stats.lowest_hpa.map(Hpa)),
        low_at: at(stats.lowest_at),
        onset: at(stats.onset),
    }
}

/// Where each grid's model puts the storm of `event`: the lowest point of
/// its storm track inside the event window.
fn storm_centers(
    outlook: &StormOutlook,
    found: &[Vec<Track>],
    event: &StormEvent,
) -> Vec<StormCenter> {
    let place = place_of(outlook);
    outlook
        .grids
        .iter()
        .zip(found)
        .filter_map(|(grid, lows)| {
            let storm = track::storm_track(lows, place, (event.start, event.end))?;
            let deepest = lows[storm]
                .points
                .iter()
                .filter(|p| p.time >= event.start && p.time < event.end)
                .reduce(|best, p| {
                    if p.pressure_hpa < best.pressure_hpa {
                        p
                    } else {
                        best
                    }
                })?;
            Some(StormCenter {
                model: grid.model.clone(),
                latitude: deepest.latitude,
                longitude: deepest.longitude,
                time: deepest.time,
                pressure_hpa: deepest.pressure_hpa,
            })
        })
        .collect()
}

/// An event amount: snowfall from centimetres when `snow`, else
/// precipitation from millimetres.
fn amount(f: &Formatter<'_>, snow: bool, raw: Option<f32>) -> Quantity {
    if snow {
        f.snowfall(raw)
    } else {
        f.precipitation(raw.map(Mm))
    }
}

/// How sentences round an amount in display units: whole numbers from 1
/// up and one decimal below, with one more decimal for inches of rain.
fn rough_number(value: f32, unit: &str, snow: bool) -> String {
    let extra = u8::from(!snow && unit == "in");
    number(value, u8::from(value < 1.0) + extra)
}

/// An amount rounded for a sentence: "14 in" of snow, "1.7 in" or "44 mm"
/// of rain.
fn rough_amount(f: &Formatter<'_>, snow: bool, raw: f32) -> String {
    let q = amount(f, snow, Some(raw));
    let value = q.value.unwrap_or(0.0);
    format!("{} {}", rough_number(value, q.unit, snow), q.unit)
}

/// A distance in km, rounded for a sentence to the nearest 5 (or whole
/// units under 10), in the presentation's distance unit: "55 mi".
fn rough_distance(f: &Formatter<'_>, km: f64) -> String {
    let unit = f.presentation.units.distance;
    let value = Meters((km * 1_000.0) as f32).to(unit);
    let rounded = if value < 10.0 {
        libm::roundf(value)
    } else {
        libm::roundf(value / 5.0) * 5.0
    };
    format!("{} {}", number(rounded, 0), unit.symbol())
}

/// The models' agreement on `event`, with a sentence on where they differ.
fn agreement_vm(
    outlook: &StormOutlook,
    event: &StormEvent,
    centers: &[StormCenter],
    f: &Formatter<'_>,
) -> Option<AgreementVm> {
    let (lat, lon) = place_of(outlook);
    let result = analysis::agreement(&event.models, centers, lat, lon);
    let verdict = result.verdict?;
    let label = match verdict {
        Agreement::High => Text::StormAgreementHigh,
        Agreement::Medium => Text::StormAgreementMedium,
        Agreement::Low => Text::StormAgreementLow,
    };
    Some(AgreementVm {
        agreement: verdict,
        level: verdict.level(),
        label: f.strings.get(label).to_string(),
        sentence: agreement_sentence(outlook, event, &result, f),
    })
}

/// Which model brings the storm's centre closest to the place and which
/// keeps it farthest; without centres, which brings the most snow (or rain)
/// and which the least.
fn agreement_sentence(
    outlook: &StormOutlook,
    event: &StormEvent,
    result: &AgreementResult,
    f: &Formatter<'_>,
) -> String {
    if let (Some(near), Some(far)) = (&result.closest, &result.farthest)
        && near.model != far.model
    {
        return f.strings.fmt(
            Text::StormLowNearFar,
            &[
                ("near", models::short_name(&near.model)),
                ("near_distance", &rough_distance(f, near.distance_km)),
                ("place", &outlook.place.name),
                ("far", models::short_name(&far.model)),
                ("far_distance", &rough_distance(f, far.distance_km)),
            ],
        );
    }
    if let (Some(wet), Some(dry)) = (&result.wettest, &result.driest)
        && wet.model != dry.model
    {
        let snow_of = |model: &str| {
            event
                .models
                .iter()
                .find(|m| m.model == model)
                .and_then(|m| m.total_snow_cm)
        };
        let (text, wet_amount, dry_amount) =
            match (event.has_snow(), snow_of(&wet.model), snow_of(&dry.model)) {
                (true, Some(a), Some(b)) => (
                    Text::StormSnowRange,
                    rough_amount(f, true, a),
                    rough_amount(f, true, b),
                ),
                _ => (
                    Text::StormRainRange,
                    rough_amount(f, false, wet.total_mm),
                    rough_amount(f, false, dry.total_mm),
                ),
            };
        return f.strings.fmt(
            text,
            &[
                ("wet", models::short_name(&wet.model)),
                ("wet_amount", &wet_amount),
                ("dry", models::short_name(&dry.model)),
                ("dry_amount", &dry_amount),
            ],
        );
    }
    String::new()
}

/// "No storm in the next 10 days. Lowest pressure: 1004 hPa Tue night
/// (GFS).", when there is no event; `None` with events or without data.
fn summary(outlook: &StormOutlook, events: &[StormEvent], f: &Formatter<'_>) -> Option<String> {
    if !events.is_empty() {
        return None;
    }
    let first = outlook
        .points
        .iter()
        .filter_map(|p| p.times.first())
        .min()?;
    let last = outlook.points.iter().filter_map(|p| p.times.last()).max()?;
    // Hourly amounts cover the hour before their time.
    let days = ((last.0 - first.0 + HOUR + DAY / 2) / DAY).max(1);
    let mut text = f.strings.fmt(Text::StormNone, &[("n", &days.to_string())]);
    if let Some((model, hpa, at)) = analysis::deepest_low(outlook) {
        text.push(' ');
        text.push_str(&f.strings.fmt(
            Text::StormLowest,
            &[
                ("pressure", &f.pressure(Some(Hpa(hpa))).text),
                ("when", &day_part_label(f, at, outlook.offset_at(at))),
                ("model", models::short_name(&model)),
            ],
        ));
    }
    Some(text)
}

/// The lows the map shows: per grid, in the outlook's order, the storm of
/// `event` first, then the others (see [`track::keep_tracks`]).
fn tracks(
    outlook: &StormOutlook,
    found: &[Vec<Track>],
    event: Option<&StormEvent>,
    f: &Formatter<'_>,
) -> Vec<TrackVm> {
    let place = place_of(outlook);
    let mut out = Vec::new();
    for (grid, lows) in outlook.grids.iter().zip(found) {
        let storm = event.and_then(|e| track::storm_track(lows, place, (e.start, e.end)));
        for (low, is_storm) in track::keep_tracks(lows, storm, MAP_OTHER_TRACKS) {
            out.push(TrackVm {
                model: grid.model.clone(),
                short: models::short_name(&grid.model).to_string(),
                role: ModelRole::of(&grid.model),
                is_storm,
                points: low
                    .points
                    .iter()
                    .map(|p| TrackPointVm {
                        latitude: p.latitude as f32,
                        longitude: p.longitude as f32,
                        time: p.time,
                        pressure: f.pressure(Some(Hpa(p.pressure_hpa))),
                    })
                    .collect(),
            });
        }
    }
    out
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
        GRID_STEP_SECS, STORM_NOW, STORM_OFFSET, STORM_PEAK, dry_week, noreaster,
        noreaster_past_runs,
    };
    use crate::units::UnitPrefs;

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
        }
    }

    fn imperial() -> PresentationConfig {
        PresentationConfig {
            units: UnitPrefs::IMPERIAL,
            time_format: TimeFormat::H12,
            ..PresentationConfig::default()
        }
    }

    #[test]
    fn each_model_tracks_the_storm() {
        let vm = build(&noreaster(), &imperial());
        let storms: Vec<(&str, &str, bool)> = vm
            .tracks
            .iter()
            .map(|t| (t.short.as_str(), t.role.key(), t.is_storm))
            .collect();
        assert_eq!(
            storms,
            [
                ("ECMWF", "ecmwf", true),
                ("GFS", "gfs", true),
                ("GEM", "gem", true),
                ("ICON", "icon", true),
            ]
        );
        for track in &vm.tracks {
            assert!(track.points.len() >= 3, "{}", track.model);
            assert!(track.points.windows(2).all(|w| w[0].time < w[1].time));
            // The lows travel northeast past the place.
            let (first, last) = (&track.points[0], &track.points[track.points.len() - 1]);
            assert!(last.latitude > first.latitude && last.longitude > first.longitude);
        }
        let deepest = |t: &TrackVm| {
            t.points
                .iter()
                .filter_map(|p| p.pressure.value)
                .fold(f32::INFINITY, f32::min)
        };
        // ECMWF's 978 hPa low is near 28.9 inHg; GFS's 990 hPa near 29.2.
        assert!((deepest(&vm.tracks[0]) - 28.9).abs() < 0.1);
        assert!((deepest(&vm.tracks[1]) - 29.25).abs() < 0.1);
        assert!(vm.tracks[0].points[0].pressure.text.ends_with(" inHg"));
        let metric = build(&noreaster(), &PresentationConfig::default());
        assert!(metric.tracks[0].points[0].pressure.text.ends_with(" hPa"));
        assert_eq!(vm.summary, None);
    }

    #[test]
    fn other_lows_are_kept_without_an_event() {
        let mut outlook = noreaster();
        // The same lows, but no precipitation: no event, so no storm.
        for point in &mut outlook.points {
            point
                .precipitation_mm
                .iter_mut()
                .for_each(|v| *v = Some(0.0));
            point.snowfall_cm.iter_mut().for_each(|v| *v = Some(0.0));
        }
        let vm = build(&outlook, &PresentationConfig::default());
        assert_eq!(vm.tracks.len(), 4);
        assert!(vm.tracks.iter().all(|t| !t.is_storm));
        let summary = vm.summary.unwrap_or_default();
        assert!(
            summary.starts_with("No storm in the next 10 days. Lowest pressure: 9"),
            "{summary}"
        );
        assert!(summary.ends_with("Sun afternoon (ECMWF)."), "{summary}");
    }

    #[test]
    fn a_dry_week_has_a_summary_and_no_storm() {
        let vm = build(&dry_week(), &PresentationConfig::default());
        assert_eq!(vm.events, []);
        assert_eq!(vm.tracks, []);
        let summary = vm.summary.unwrap_or_default();
        assert!(
            summary.starts_with("No storm in the next 10 days. Lowest pressure: 10"),
            "{summary}"
        );
        assert!(summary.contains(" hPa "), "{summary}");
        let vm = build(&dry_week(), &imperial());
        assert!(vm.summary.is_some_and(|s| s.contains(" inHg ")));
        let empty = build(&StormOutlook::default(), &PresentationConfig::default());
        assert_eq!(empty.summary, None);
    }

    #[test]
    fn the_noreaster_rows_compare_the_models() {
        let vm = build(&noreaster(), &imperial());
        assert_eq!(vm.events.len(), 1);
        let event = &vm.events[0];
        assert_eq!(event.title, "Sat night – Sun night");
        let rows: Vec<[&str; 7]> = event
            .rows
            .iter()
            .map(|r| {
                [
                    r.short.as_str(),
                    r.snow.text.as_str(),
                    r.precipitation.text.as_str(),
                    r.gust.text.as_str(),
                    r.low.text.as_str(),
                    r.low_at.as_str(),
                    r.onset.as_str(),
                ]
            })
            .collect();
        assert_eq!(
            rows,
            [
                [
                    "ECMWF",
                    "13.9 in",
                    "1.72 in",
                    "52 mph",
                    "28.92 inHg",
                    "Sun 1 PM",
                    "Sat 10 PM"
                ],
                [
                    "GFS",
                    "3.0 in",
                    "0.54 in",
                    "29 mph",
                    "29.50 inHg",
                    "Sun 4 PM",
                    "Sun 8 AM"
                ],
                [
                    "GEM",
                    "8.4 in",
                    "1.17 in",
                    "39 mph",
                    "29.22 inHg",
                    "Sun 1 PM",
                    "Sun 12 AM"
                ],
                [
                    "ICON",
                    "10.9 in",
                    "1.49 in",
                    "47 mph",
                    "29.04 inHg",
                    "Sun 10 AM",
                    "Sat 8 PM"
                ],
            ]
        );
        let metric = build(&noreaster(), &PresentationConfig::default());
        let ecmwf = &metric.events[0].rows[0];
        assert_eq!(
            [
                ecmwf.snow.text.as_str(),
                ecmwf.precipitation.text.as_str(),
                ecmwf.gust.text.as_str()
            ],
            ["35.2 cm", "43.6 mm", "84 km/h"]
        );
        assert_eq!(
            (ecmwf.low.text.as_str(), ecmwf.low_at.as_str()),
            ("979 hPa", "Sun 13:00")
        );
    }

    #[test]
    fn the_noreaster_agreement_is_low() {
        let vm = build(&noreaster(), &imperial());
        let agreement = vm.events[0].agreement.clone().unwrap();
        assert_eq!(agreement.agreement, Agreement::Low);
        assert_eq!(agreement.level, Level::Poor);
        assert_eq!(agreement.label, "Low agreement");
        assert_eq!(
            agreement.sentence,
            "ECMWF brings the low within 55 mi of New York; GFS keeps it 210 mi away."
        );
        let metric = build(&noreaster(), &PresentationConfig::default());
        assert_eq!(
            metric.events[0].agreement.clone().unwrap().sentence,
            "ECMWF brings the low within 85 km of New York; GFS keeps it 340 km away."
        );
    }

    #[test]
    fn without_tracks_the_sentence_compares_totals() {
        let mut outlook = noreaster();
        outlook.grids.clear();
        let vm = build(&outlook, &imperial());
        assert_eq!(
            vm.events[0].agreement.clone().unwrap().sentence,
            "ECMWF brings 14 in of snow; GFS 3 in."
        );
        for point in &mut outlook.points {
            point.snowfall_cm.iter_mut().for_each(|v| *v = Some(0.0));
        }
        let vm = build(&outlook, &PresentationConfig::default());
        let agreement = vm.events[0].agreement.clone().unwrap();
        assert_eq!(agreement.sentence, "ECMWF brings 44 mm of rain; GFS 14 mm.");
        let vm = build(&outlook, &imperial());
        assert_eq!(
            vm.events[0].agreement.clone().unwrap().sentence,
            "ECMWF brings 1.7 in of rain; GFS 0.54 in."
        );
    }

    #[test]
    fn missing_values_show_the_placeholder() {
        let mut outlook = noreaster();
        outlook.points.truncate(1);
        outlook.grids.clear();
        let point = &mut outlook.points[0];
        point.gust_kmh.iter_mut().for_each(|v| *v = None);
        point.pressure_hpa.iter_mut().for_each(|v| *v = None);
        let vm = build(&outlook, &PresentationConfig::default());
        let row = &vm.events[0].rows[0];
        assert_eq!(
            (
                row.gust.text.as_str(),
                row.low.text.as_str(),
                row.low_at.as_str()
            ),
            ("—", "—", "—")
        );
        assert_eq!(row.gust.unit, "km/h");
        // One model has nothing to agree with.
        assert_eq!(vm.events[0].agreement, None);
    }

    #[test]
    fn ensembles_give_the_members_range_and_plume() {
        let vm = build(&noreaster(), &imperial());
        let ensembles = &vm.events[0].ensembles;
        let summary: Vec<(&str, u32, &str)> = ensembles
            .iter()
            .map(|e| (e.short.as_str(), e.members, e.text.as_str()))
            .collect();
        assert_eq!(
            summary,
            [
                ("ECMWF ENS", 51, "Most members: 6–18 in (median 12 in)"),
                ("GEFS", 31, "Most members: 2–6 in (median 3 in)"),
            ]
        );
        for e in ensembles {
            let plume = &e.plume;
            assert_eq!(plume.title, "Snow");
            assert_eq!(plume.unit, "in");
            assert_eq!(plume.members.len() as u32, e.members);
            assert_eq!(plume.times[0], vm.events[0].start);
            assert_eq!(plume.times.last().copied(), Some(vm.events[0].end));
            for series in plume
                .members
                .iter()
                .chain([&plume.p10, &plume.p50, &plume.p90])
            {
                assert_eq!(series.len(), plume.times.len());
                assert_eq!(series[0], Some(0.0));
                let values: Vec<f32> = series.iter().flatten().copied().collect();
                assert!(values.windows(2).all(|w| w[0] <= w[1]), "{values:?}");
                assert!(values.iter().all(|v| *v <= plume.y_max));
            }
            assert_eq!(
                plume.p50.last().copied().flatten(),
                e.p50.value.map(chart_value)
            );
            assert_eq!(plume.y_labels.last().map(|t| t.value), Some(plume.y_max));
            let days: Vec<&str> = plume.x_labels.iter().map(|t| t.text.as_str()).collect();
            assert_eq!(days, ["Sun", "Mon"]);
        }
        let ticks = |e: &EnsembleVm| -> Vec<String> {
            e.plume.y_labels.iter().map(|t| t.text.clone()).collect()
        };
        assert_eq!(
            ticks(&ensembles[0]),
            ["0 in", "5 in", "10 in", "15 in", "20 in"]
        );
        assert_eq!(
            ticks(&ensembles[1]),
            ["0 in", "2 in", "4 in", "6 in", "8 in"]
        );
        assert_eq!(ensembles[0].p50.text, "12.0 in");
    }

    #[test]
    fn run_to_run_bars_end_with_the_current_run() {
        let vm = build(&noreaster(), &imperial());
        let runs = &vm.events[0].run_to_run;
        let models: Vec<&str> = runs.iter().map(|r| r.short.as_str()).collect();
        assert_eq!(models, ["ECMWF", "GFS"]);
        for r in runs {
            assert_eq!(r.title, "Snow");
            assert_eq!(r.bars.len(), 5);
            assert!(r.bars.windows(2).all(|w| w[0].init < w[1].init));
            assert_eq!(
                r.bars.iter().map(|b| b.current).collect::<Vec<_>>(),
                [false, false, false, false, true]
            );
            assert!(r.bars.iter().all(|b| b.value <= r.max));
        }
        let values = |r: &RunToRunVm| r.bars.iter().map(|b| b.value).collect::<Vec<_>>();
        assert!(
            values(&runs[0]).windows(2).all(|w| w[0] < w[1]),
            "{:?}",
            values(&runs[0])
        );
        assert!(
            values(&runs[1]).windows(2).all(|w| w[0] > w[1]),
            "{:?}",
            values(&runs[1])
        );
        let labels: Vec<&str> = runs[0].bars.iter().map(|b| b.label.as_str()).collect();
        assert_eq!(
            labels,
            ["Thu 12z", "Thu 18z", "Fri 00z", "Fri 06z", "Fri 12z"]
        );
        let texts: Vec<&str> = runs[0].bars.iter().map(|b| b.text.as_str()).collect();
        assert_eq!(texts, ["5.5 in", "8.3 in", "11.0 in", "13.0 in", "13.9 in"]);
        assert_eq!(runs[0].max, 15.0);
    }

    #[test]
    fn run_to_run_merges_and_filters_earlier_runs() {
        let mut outlook = noreaster();
        let past = noreaster_past_runs();
        // The outlook carries two of them already; `past` repeats them.
        outlook.past_runs = past[..2].to_vec();
        let vm = build(&outlook, &PresentationConfig::default());
        assert_eq!(vm.events[0].run_to_run[0].bars.len(), 5);
        // Runs for another window, or not before the current run, are left out.
        let mut other = past.clone();
        for run in &mut other {
            run.window_start = UnixTime(run.window_start.0 + 7 * DAY);
            run.window_end = UnixTime(run.window_end.0 + 7 * DAY);
        }
        let vm = build_storms(
            &noreaster(),
            &other,
            &PresentationConfig::default(),
            &Strings::default(),
            STORM_NOW,
        );
        assert_eq!(vm.events[0].run_to_run, []);
        let mut late = past;
        for run in &mut late {
            run.run.init = STORM_PEAK;
        }
        let vm = build_storms(
            &noreaster(),
            &late,
            &PresentationConfig::default(),
            &Strings::default(),
            STORM_NOW,
        );
        assert_eq!(vm.events[0].run_to_run, []);
    }

    #[test]
    fn axes_use_round_steps() {
        let texts = |max: f32, unit| {
            let (top, ticks) = value_axis(max, unit);
            let labels: Vec<String> = ticks.into_iter().map(|t| t.text).collect();
            (top, labels)
        };
        assert_eq!(
            texts(17.3, "in"),
            (
                20.0,
                ["0 in", "5 in", "10 in", "15 in", "20 in"]
                    .map(String::from)
                    .to_vec()
            )
        );
        assert_eq!(texts(1.9, "in").0, 2.0);
        assert_eq!(texts(1.9, "in").1[1], "0.5 in");
        assert_eq!(texts(0.9, "in").1[1], "0.2 in");
        assert_eq!(
            texts(44.0, "mm").1.last().map(String::as_str),
            Some("50 mm")
        );
        assert_eq!(
            texts(0.0, "mm"),
            (1.0, ["0 mm", "1 mm"].map(String::from).to_vec())
        );
        assert_eq!(nice_step(f32::NAN), 1.0);
    }

    #[test]
    fn short_plumes_label_every_six_hours() {
        let p = PresentationConfig::default();
        let s = Strings::default();
        let f = Formatter::new(&p, &s);
        let outlook = noreaster();
        let ticks = time_ticks(
            &outlook,
            &f,
            UnixTime(STORM_PEAK.0),
            UnixTime(STORM_PEAK.0 + 12 * HOUR),
        );
        let labels: Vec<&str> = ticks.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(labels, ["18:00", "00:00"]);
    }

    #[cfg(feature = "serde")]
    fn pretty<T: serde::Serialize>(vm: &T) -> String {
        serde_json::to_string_pretty(vm).unwrap()
    }

    fn ahead(outlook: &StormOutlook, presentation: &PresentationConfig) -> Option<StormAheadVm> {
        storm_ahead(outlook, presentation, &Strings::default(), STORM_NOW)
    }

    #[test]
    fn the_noreaster_is_a_storm_ahead() {
        let vm = ahead(&noreaster(), &imperial()).unwrap();
        assert_eq!(vm.title, "Storm ahead · Sat night – Sun night");
        assert_eq!(vm.line, "Up to 14 in of snow · models disagree");
        assert_eq!(vm.level, Level::Poor);
        assert_eq!(vm.agreement.as_deref(), Some("Low agreement"));
        let event = &build(&noreaster(), &imperial()).events[0];
        assert_eq!((vm.start, vm.end), (event.start, event.end));
        let metric = ahead(&noreaster(), &PresentationConfig::default()).unwrap();
        assert_eq!(metric.line, "Up to 35 cm of snow · models disagree");
        let (de, _) = Strings::localized("de", &alloc::collections::BTreeMap::new());
        let german = storm_ahead(&noreaster(), &PresentationConfig::default(), &de, STORM_NOW);
        let german = german.unwrap();
        assert_eq!(german.title, "Unwetter in Sicht · Sa Nacht – So Nacht");
        assert_eq!(german.line, "Bis zu 35 cm Schnee · Modelle uneins");
        // Without grids the totals alone rate the agreement.
        let mut points_only = noreaster();
        points_only.grids.clear();
        points_only.ensembles.clear();
        let vm = ahead(&points_only, &imperial()).unwrap();
        assert_eq!(vm.agreement.as_deref(), Some("Low agreement"));
    }

    #[test]
    fn a_dry_week_has_no_storm_ahead() {
        assert_eq!(ahead(&dry_week(), &imperial()), None);
        assert_eq!(ahead(&StormOutlook::default(), &imperial()), None);
    }

    #[test]
    fn rain_and_wind_head_the_line_when_they_lead() {
        // The noreaster's models with their snow taken away and the totals
        // brought together: rain heads the line, and agreement is high.
        let mut rain = noreaster();
        rain.grids.clear();
        for point in &mut rain.points {
            point.snowfall_cm.iter_mut().for_each(|v| *v = Some(0.0));
        }
        let ecmwf = rain.points[0].clone();
        for point in &mut rain.points[1..] {
            point.precipitation_mm.clone_from(&ecmwf.precipitation_mm);
            point.gust_kmh.iter_mut().for_each(|v| *v = Some(20.0));
        }
        let vm = ahead(&rain, &imperial()).unwrap();
        assert_eq!(vm.line, "Up to 1.7 in of rain");
        assert_eq!(vm.level, Level::Good);
        assert_eq!(vm.agreement.as_deref(), Some("High agreement"));

        // One model with 40 % less: the models differ.
        let mut differ = rain.clone();
        differ.points[1]
            .precipitation_mm
            .iter_mut()
            .for_each(|v| *v = v.map(|mm| mm * 0.6));
        let vm = ahead(&differ, &imperial()).unwrap();
        assert_eq!(vm.line, "Up to 1.7 in of rain · models differ");
        assert_eq!(vm.level, Level::Moderate);
        assert_eq!(vm.agreement.as_deref(), Some("Medium agreement"));

        // Light rain under strong gusts: the gusts head the line.
        let mut wind = rain;
        for point in &mut wind.points {
            point
                .precipitation_mm
                .iter_mut()
                .for_each(|v| *v = v.map(|mm| mm / 4.0));
            point.gust_kmh.clone_from(&ecmwf.gust_kmh);
        }
        let vm = ahead(&wind, &imperial()).unwrap();
        assert_eq!(vm.line, "Gusts to 52 mph");
        let vm = ahead(&wind, &PresentationConfig::default()).unwrap();
        assert_eq!(vm.line, "Gusts to 84 km/h");
    }

    #[test]
    fn a_single_model_has_no_agreement() {
        // One model has nothing to agree with: no wording, a moderate level.
        let mut single = noreaster();
        single.points.truncate(1);
        single.grids.clear();
        let vm = ahead(&single, &PresentationConfig::default()).unwrap();
        assert_eq!(vm.agreement, None);
        assert_eq!(vm.level, Level::Moderate);
        assert_eq!(vm.line, "Up to 36 cm of snow");
    }

    #[cfg(feature = "serde")]
    #[test]
    fn snapshot_storm_ahead_noreaster_imperial() {
        let vm = ahead(&noreaster(), &imperial());
        insta::assert_snapshot!("storm_ahead_noreaster_imperial", pretty(&vm));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn snapshot_storm_ahead_noreaster_metric() {
        let vm = ahead(&noreaster(), &PresentationConfig::default());
        insta::assert_snapshot!("storm_ahead_noreaster_metric", pretty(&vm));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn snapshot_noreaster_imperial() {
        insta::assert_snapshot!(
            "noreaster_imperial",
            pretty(&build(&noreaster(), &imperial()))
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn snapshot_noreaster_metric() {
        let vm = build(&noreaster(), &PresentationConfig::default());
        insta::assert_snapshot!("noreaster_metric", pretty(&vm));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn snapshot_dry_week() {
        let vm = build(&dry_week(), &PresentationConfig::default());
        insta::assert_snapshot!("dry_week", pretty(&vm));
    }

    #[test]
    fn titles_name_parts_of_days() {
        let p = PresentationConfig::default();
        let s = Strings::default();
        let f = Formatter::new(&p, &s);
        let outlook = noreaster();
        let title = |from: i64, to: i64| {
            event_title(
                &f,
                &outlook,
                UnixTime(STORM_PEAK.0 + from * HOUR),
                UnixTime(STORM_PEAK.0 + to * HOUR),
            )
        };
        // STORM_PEAK is Sunday 1 PM EST.
        assert_eq!(title(0, 5), "Sun afternoon");
        assert_eq!(title(-6, 5), "Sun morning – Sun afternoon");
        assert_eq!(title(-6, 6), "Sun morning – Sun night");
        assert_eq!(title(0, 12), "Sun afternoon – Sun night");
        assert_eq!(title(-18, 18), "Sat night – Mon");
        assert_eq!(title(-6, 29), "Sun morning – Mon");
        assert_eq!(title(-6, 30), "Sun morning – Mon night");
        assert_eq!(title(-18, 12), "Sat night – Sun night");
        assert_eq!(title(0, 0), "Sun afternoon");
    }

    #[test]
    fn parts_of_the_day_name_the_evening_for_the_night() {
        let p = PresentationConfig::default();
        let s = Strings::default();
        let f = Formatter::new(&p, &s);
        // STORM_PEAK is Sunday 1 PM EST.
        let at =
            |hours: i64| day_part_label(&f, UnixTime(STORM_PEAK.0 + hours * HOUR), STORM_OFFSET);
        assert_eq!(at(0), "Sun afternoon");
        assert_eq!(at(-2), "Sun morning");
        assert_eq!(at(-8), "Sat night");
        assert_eq!(at(-14), "Sat night");
        assert_eq!(at(4), "Sun afternoon");
        assert_eq!(at(5), "Sun night");
        assert_eq!(at(16), "Sun night");
        assert_eq!(at(17), "Mon morning");
        let (de, _) = Strings::localized("de", &alloc::collections::BTreeMap::new());
        let f = Formatter::new(&p, &de);
        assert_eq!(
            day_part_label(&f, STORM_PEAK, STORM_OFFSET),
            "So Nachmittag"
        );
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
