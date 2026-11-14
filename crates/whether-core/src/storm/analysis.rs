//! Storm analysis: finding the coming events in a [`StormOutlook`],
//! summarising what each model and ensemble says about them, and rating how
//! closely the models agree.
//!
//! Everything here is a pure function of the outlook and returns raw domain
//! values in the outlook's units (mm, cm, km/h, hPa); `view::storms` formats
//! them.
//!
//! **Windows.** A window `[start, end)` is a span of real time. Hourly amounts
//! are for the hour before their time, so the hours inside a window are those
//! stamped after `start` up to and including `end`. Gusts and pressure are
//! read at the same stamps.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use crate::storm::{Agreement, EnsembleSeries, PointSeries, StormOutlook};
use crate::time::UnixTime;

const HOUR: i64 = 3_600;

/// The length of the bins events are found in: 6 hours, aligned to 00, 06,
/// 12 and 18 UTC.
pub const BIN_SECS: i64 = 6 * HOUR;

/// A bin is wet when the models' mean precipitation in it reaches this, in mm.
pub const EVENT_MIN_BIN_MM: f32 = 2.0;

/// A bin is wet when the models' mean snowfall in it reaches this, in cm.
pub const EVENT_MIN_BIN_SNOW_CM: f32 = 1.0;

/// Runs of wet bins closer than this (end to start) join one event, in
/// seconds.
pub const EVENT_MERGE_GAP_SECS: i64 = 12 * HOUR;

/// Each event's window is widened by this on both sides, in seconds.
pub const EVENT_PAD_SECS: i64 = 6 * HOUR;

/// At most this many events are kept: the wettest ones.
pub const MAX_EVENTS: usize = 3;

/// An event has snow when the models' mean snowfall over it reaches this, in
/// cm.
pub const EVENT_SNOW_MIN_CM: f32 = 1.0;

/// A model's onset is the first hour with at least this much precipitation,
/// in mm.
pub const ONSET_MIN_MM: f32 = 0.5;

/// One coming storm at the place, with each model's view of it.
#[derive(Clone, Debug, PartialEq)]
pub struct StormEvent {
    /// The window's start (inclusive), usually on a 6-hour boundary.
    pub start: UnixTime,
    /// The window's end (exclusive).
    pub end: UnixTime,
    /// The mean of the models' precipitation totals over the window, in mm.
    pub mean_total_mm: f32,
    /// The mean of the models' snowfall totals over the window, in cm.
    pub mean_snow_cm: f32,
    /// Each point series' statistics over the window, in the outlook's order.
    pub models: Vec<ModelEventStats>,
}

impl StormEvent {
    /// Whether the models bring snow: their mean snowfall reaches
    /// [`EVENT_SNOW_MIN_CM`].
    pub fn has_snow(&self) -> bool {
        self.mean_snow_cm >= EVENT_SNOW_MIN_CM
    }
}

/// What one model says about an event window.
///
/// Missing hours are skipped: totals add the hours the model supplied, and a
/// field is `None` only when the model supplied no value for it in the
/// window.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelEventStats {
    /// The Open-Meteo `models` id.
    pub model: String,
    /// Precipitation over the window, in mm.
    pub total_mm: Option<f32>,
    /// Snowfall over the window, in cm.
    pub total_snow_cm: Option<f32>,
    /// The strongest gust in the window, in km/h.
    pub peak_gust_kmh: Option<f32>,
    /// The lowest sea-level pressure at the place in the window, in hPa.
    pub lowest_hpa: Option<f32>,
    /// When `lowest_hpa` occurs (the first time, on a tie).
    pub lowest_at: Option<UnixTime>,
    /// The start of the first hour with at least [`ONSET_MIN_MM`].
    pub onset: Option<UnixTime>,
}

/// Whether the hour stamped `t` lies inside `[start, end)`.
fn in_window(t: UnixTime, start: UnixTime, end: UnixTime) -> bool {
    t.0 > start.0 && t.0 <= end.0
}

/// The sum of the values of the hours inside `[start, end)`, or `None` when
/// none of them has a value.
fn window_sum(
    times: &[UnixTime],
    values: &[Option<f32>],
    start: UnixTime,
    end: UnixTime,
) -> Option<f32> {
    times
        .iter()
        .zip(values)
        .filter(|(t, _)| in_window(**t, start, end))
        .filter_map(|(_, v)| *v)
        .fold(None, |acc, v| Some(acc.unwrap_or(0.0) + v))
}

/// `series`' precipitation (mm) and snowfall (cm) over `[start, end)`, each
/// `None` when the series has no value in the window.
pub fn window_totals(
    series: &PointSeries,
    start: UnixTime,
    end: UnixTime,
) -> (Option<f32>, Option<f32>) {
    (
        window_sum(&series.times, &series.precipitation_mm, start, end),
        window_sum(&series.times, &series.snowfall_cm, start, end),
    )
}

/// The hours of `values` inside `[start, end)` that have a value.
fn window_values<'a>(
    times: &'a [UnixTime],
    values: &'a [Option<f32>],
    start: UnixTime,
    end: UnixTime,
) -> impl Iterator<Item = (UnixTime, f32)> + 'a {
    times
        .iter()
        .zip(values)
        .filter(move |(t, _)| in_window(**t, start, end))
        .filter_map(|(t, v)| v.map(|v| (*t, v)))
}

/// The lowest value and its first time.
fn lowest(values: impl Iterator<Item = (UnixTime, f32)>) -> Option<(UnixTime, f32)> {
    values.fold(None, |best, (t, v)| match best {
        Some((_, b)) if b <= v => best,
        _ => Some((t, v)),
    })
}

/// What `series` says about the window `[start, end)`.
pub fn model_stats(series: &PointSeries, start: UnixTime, end: UnixTime) -> ModelEventStats {
    let (total_mm, total_snow_cm) = window_totals(series, start, end);
    let peak_gust_kmh = window_values(&series.times, &series.gust_kmh, start, end)
        .map(|(_, v)| v)
        .reduce(f32::max);
    let low = lowest(window_values(
        &series.times,
        &series.pressure_hpa,
        start,
        end,
    ));
    let onset = window_values(&series.times, &series.precipitation_mm, start, end)
        .find(|(_, v)| *v >= ONSET_MIN_MM)
        .map(|(t, _)| UnixTime(t.0 - HOUR));
    ModelEventStats {
        model: series.model.clone(),
        total_mm,
        total_snow_cm,
        peak_gust_kmh,
        lowest_hpa: low.map(|(_, v)| v),
        lowest_at: low.map(|(t, _)| t),
        onset,
    }
}

/// The mean of the values present, or `None` when there are none.
fn mean(values: impl Iterator<Item = Option<f32>>) -> Option<f32> {
    let (sum, n) = values
        .flatten()
        .fold((0.0_f32, 0_u32), |(s, n), v| (s + v, n + 1));
    (n > 0).then(|| sum / n as f32)
}

/// The start of the 6-hour bin holding the hour stamped `t`.
fn bin_of(t: UnixTime) -> i64 {
    (t.0 - HOUR).div_euclid(BIN_SECS) * BIN_SECS
}

/// Per-bin sums of one series' values; a bin is absent when the series has no
/// value in it.
fn bin_sums(times: &[UnixTime], values: &[Option<f32>]) -> BTreeMap<i64, f32> {
    let mut bins = BTreeMap::new();
    for (t, v) in times.iter().zip(values) {
        if let Some(v) = v {
            *bins.entry(bin_of(*t)).or_insert(0.0) += *v;
        }
    }
    bins
}

/// The multi-model mean of each bin, over the series with a value in it.
fn bin_means<'a>(
    outlook: &'a StormOutlook,
    values: impl Fn(&'a PointSeries) -> &'a [Option<f32>],
) -> BTreeMap<i64, f32> {
    let mut acc: BTreeMap<i64, (f32, u32)> = BTreeMap::new();
    for series in &outlook.points {
        for (bin, sum) in bin_sums(&series.times, values(series)) {
            let entry = acc.entry(bin).or_insert((0.0, 0));
            entry.0 += sum;
            entry.1 += 1;
        }
    }
    acc.into_iter()
        .map(|(bin, (sum, n))| (bin, sum / n as f32))
        .collect()
}

/// The span of real time the point series cover: from the start of the first
/// hour to the last stamp.
fn data_extent(outlook: &StormOutlook) -> Option<(i64, i64)> {
    let firsts = outlook.points.iter().filter_map(|p| p.times.first());
    let lasts = outlook.points.iter().filter_map(|p| p.times.last());
    Some((firsts.min()?.0 - HOUR, lasts.max()?.0))
}

/// The coming storms in `outlook`, in time order.
///
/// The models' mean precipitation and snowfall are taken in 6-hour bins
/// aligned to 00, 06, 12 and 18 UTC, each over the point series with a value
/// in the bin. A bin is wet when its mean reaches [`EVENT_MIN_BIN_MM`] or
/// [`EVENT_MIN_BIN_SNOW_CM`]. Runs of wet bins less than
/// [`EVENT_MERGE_GAP_SECS`] apart join, and each run is widened by
/// [`EVENT_PAD_SECS`] on both sides, within the data. The [`MAX_EVENTS`]
/// events with the largest mean total are kept (the earlier one on a tie).
pub fn find_events(outlook: &StormOutlook) -> Vec<StormEvent> {
    let Some((first, last)) = data_extent(outlook) else {
        return Vec::new();
    };
    let rain = bin_means(outlook, |p| &p.precipitation_mm);
    let snow = bin_means(outlook, |p| &p.snowfall_cm);
    let wet = |bin: &i64| {
        rain.get(bin).is_some_and(|mm| *mm >= EVENT_MIN_BIN_MM)
            || snow.get(bin).is_some_and(|cm| *cm >= EVENT_MIN_BIN_SNOW_CM)
    };
    let mut bins: Vec<i64> = rain
        .keys()
        .chain(snow.keys())
        .copied()
        .filter(wet)
        .collect();
    bins.sort_unstable();
    bins.dedup();

    let mut runs: Vec<(i64, i64)> = Vec::new();
    for bin in bins {
        match runs.last_mut() {
            Some(run) if bin - run.1 < EVENT_MERGE_GAP_SECS => run.1 = bin + BIN_SECS,
            _ => runs.push((bin, bin + BIN_SECS)),
        }
    }

    let mut events: Vec<StormEvent> = runs
        .into_iter()
        .map(|(start, end)| {
            let start = UnixTime((start - EVENT_PAD_SECS).max(first));
            let end = UnixTime((end + EVENT_PAD_SECS).min(last));
            let models: Vec<ModelEventStats> = outlook
                .points
                .iter()
                .map(|p| model_stats(p, start, end))
                .collect();
            StormEvent {
                start,
                end,
                mean_total_mm: mean(models.iter().map(|m| m.total_mm)).unwrap_or(0.0),
                mean_snow_cm: mean(models.iter().map(|m| m.total_snow_cm)).unwrap_or(0.0),
                models,
            }
        })
        .collect();
    // A stable sort keeps the earlier event first on a tie.
    events.sort_by(|a, b| b.mean_total_mm.total_cmp(&a.mean_total_mm));
    events.truncate(MAX_EVENTS);
    events.sort_by_key(|e| e.start);
    events
}

/// The lowest sea-level pressure at the place in any point series: the model,
/// the pressure in hPa and its time (the first model and time on a tie).
///
/// When [`find_events`] finds nothing, this is the strongest pressure dip to
/// mention instead.
pub fn deepest_low(outlook: &StormOutlook) -> Option<(String, f32, UnixTime)> {
    outlook
        .points
        .iter()
        .filter_map(|p| {
            let (t, hpa) = lowest(
                p.times
                    .iter()
                    .zip(&p.pressure_hpa)
                    .filter_map(|(t, v)| v.map(|v| (*t, v))),
            )?;
            Some((p, hpa, t))
        })
        .fold(
            None,
            |best: Option<(&PointSeries, f32, UnixTime)>, cur| match best {
                Some(b) if b.1 <= cur.1 => Some(b),
                _ => Some(cur),
            },
        )
        .map(|(p, hpa, t)| (p.model.clone(), hpa, t))
}

/// Agreement on totals is High when their spread is under this.
pub const SPREAD_HIGH_MAX: f32 = 0.35;

/// Agreement on totals is Medium when their spread is under this, and Low
/// otherwise.
pub const SPREAD_MEDIUM_MAX: f32 = 0.75;

/// The totals spread divides by the models' mean total, but never by less
/// than this, in mm, so light events don't look uncertain.
pub const SPREAD_FLOOR_MM: f32 = 5.0;

/// Agreement on the storm's centre is High when the two centres furthest
/// apart are closer than this, in km.
pub const CENTERS_HIGH_MAX_KM: f64 = 150.0;

/// Agreement on the storm's centre is Medium when the two centres furthest
/// apart are closer than this, in km, and Low otherwise.
pub const CENTERS_MEDIUM_MAX_KM: f64 = 400.0;

/// The Earth's mean radius, in km.
pub const EARTH_RADIUS_KM: f64 = 6_371.0;

/// Where one model puts the storm's low: its centre when it is deepest near
/// the place.
#[derive(Clone, Debug, PartialEq)]
pub struct StormCenter {
    /// The Open-Meteo `models` id.
    pub model: String,
    /// The centre's latitude, in degrees.
    pub latitude: f64,
    /// The centre's longitude, in degrees.
    pub longitude: f64,
    /// When the low is there.
    pub time: UnixTime,
    /// Its central pressure, in hPa.
    pub pressure_hpa: f32,
}

/// One model's precipitation total over an event.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelTotal {
    /// The Open-Meteo `models` id.
    pub model: String,
    /// Precipitation over the event, in mm.
    pub total_mm: f32,
}

/// How far one model's storm centre is from the place.
#[derive(Clone, Debug, PartialEq)]
pub struct CenterDistance {
    /// The Open-Meteo `models` id.
    pub model: String,
    /// The great-circle distance from the place, in km.
    pub distance_km: f64,
}

/// How closely the models agree on an event, with what a sentence about it
/// needs.
#[derive(Clone, Debug, PartialEq)]
pub struct AgreementResult {
    /// The worse of `totals` and `centers`, or the one that is known; `None`
    /// when neither is.
    pub verdict: Option<Agreement>,
    /// The verdict from the totals spread; `None` with fewer than 2 models'
    /// totals.
    pub totals: Option<Agreement>,
    /// `(max − min) / max(mean, SPREAD_FLOOR_MM)` over the models' totals.
    pub spread: Option<f32>,
    /// The model with the largest total (the first on a tie).
    pub wettest: Option<ModelTotal>,
    /// The model with the smallest total (the first on a tie).
    pub driest: Option<ModelTotal>,
    /// The verdict from the storm centres; `None` with fewer than 2 centres.
    pub centers: Option<Agreement>,
    /// The largest distance between two models' centres, in km.
    pub center_spread_km: Option<f64>,
    /// The centre nearest the place (the first on a tie).
    pub closest: Option<CenterDistance>,
    /// The centre furthest from the place (the first on a tie).
    pub farthest: Option<CenterDistance>,
}

/// The great-circle distance between two points given in degrees, in km
/// (the haversine formula on a sphere of [`EARTH_RADIUS_KM`]).
pub fn distance_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let half_dlat = (p2 - p1) / 2.0;
    let half_dlon = (lon2 - lon1).to_radians() / 2.0;
    let a = libm::sin(half_dlat) * libm::sin(half_dlat)
        + libm::cos(p1) * libm::cos(p2) * libm::sin(half_dlon) * libm::sin(half_dlon);
    2.0 * EARTH_RADIUS_KM * libm::asin(libm::sqrt(a.clamp(0.0, 1.0)))
}

/// The verdict for a totals spread.
pub fn totals_agreement(spread: f32) -> Agreement {
    if spread < SPREAD_HIGH_MAX {
        Agreement::High
    } else if spread < SPREAD_MEDIUM_MAX {
        Agreement::Medium
    } else {
        Agreement::Low
    }
}

/// The verdict for the largest distance between storm centres, in km.
pub fn centers_agreement(spread_km: f64) -> Agreement {
    if spread_km < CENTERS_HIGH_MAX_KM {
        Agreement::High
    } else if spread_km < CENTERS_MEDIUM_MAX_KM {
        Agreement::Medium
    } else {
        Agreement::Low
    }
}

/// The worse of two verdicts.
pub fn worse(a: Agreement, b: Agreement) -> Agreement {
    a.max(b)
}

/// The first item with the extreme key: the largest when `largest`, else the
/// smallest.
fn extreme<T, K: PartialOrd + Copy>(
    items: impl Iterator<Item = T>,
    key: impl Fn(&T) -> K,
    largest: bool,
) -> Option<T> {
    items.fold(None, |best, item| match best {
        Some(b) if (largest && key(&b) >= key(&item)) || (!largest && key(&b) <= key(&item)) => {
            Some(b)
        }
        _ => Some(item),
    })
}

/// How closely the models agree on an event at the place
/// `(place_latitude, place_longitude)`, from each model's `stats` over it and
/// where each puts the storm's centre.
///
/// The totals spread is `(max − min) / max(mean, SPREAD_FLOOR_MM)` over the
/// models with a total: High under [`SPREAD_HIGH_MAX`], Medium under
/// [`SPREAD_MEDIUM_MAX`], Low otherwise. The centres' spread is the largest
/// great-circle distance between two of them: High under
/// [`CENTERS_HIGH_MAX_KM`], Medium under [`CENTERS_MEDIUM_MAX_KM`], Low
/// otherwise. The verdict is the worse of the two; with fewer than 2 centres
/// it is the totals' alone.
pub fn agreement(
    stats: &[ModelEventStats],
    centers: &[StormCenter],
    place_latitude: f64,
    place_longitude: f64,
) -> AgreementResult {
    let totals: Vec<ModelTotal> = stats
        .iter()
        .filter_map(|s| {
            s.total_mm.map(|total_mm| ModelTotal {
                model: s.model.clone(),
                total_mm,
            })
        })
        .collect();
    let wettest = extreme(totals.iter(), |t| t.total_mm, true).cloned();
    let driest = extreme(totals.iter(), |t| t.total_mm, false).cloned();
    let spread = match (&wettest, &driest) {
        (Some(max), Some(min)) if totals.len() >= 2 => {
            let mean = totals.iter().map(|t| t.total_mm).sum::<f32>() / totals.len() as f32;
            Some((max.total_mm - min.total_mm) / mean.max(SPREAD_FLOOR_MM))
        }
        _ => None,
    };

    let center_spread_km = (centers.len() >= 2).then(|| {
        centers
            .iter()
            .enumerate()
            .flat_map(|(i, a)| {
                centers[i + 1..]
                    .iter()
                    .map(move |b| distance_km(a.latitude, a.longitude, b.latitude, b.longitude))
            })
            .fold(0.0, f64::max)
    });
    let distances: Vec<CenterDistance> = centers
        .iter()
        .map(|c| CenterDistance {
            model: c.model.clone(),
            distance_km: distance_km(place_latitude, place_longitude, c.latitude, c.longitude),
        })
        .collect();
    let closest = extreme(distances.iter(), |d| d.distance_km, false).cloned();
    let farthest = extreme(distances.iter(), |d| d.distance_km, true).cloned();

    let totals_verdict = spread.map(totals_agreement);
    let centers_verdict = center_spread_km.map(centers_agreement);
    let verdict = match (totals_verdict, centers_verdict) {
        (Some(a), Some(b)) => Some(worse(a, b)),
        (a, b) => a.or(b),
    };
    AgreementResult {
        verdict,
        totals: totals_verdict,
        spread,
        wettest,
        driest,
        centers: centers_verdict,
        center_spread_km,
        closest,
        farthest,
    }
}

/// The 10th, 50th and 90th percentiles of a set of values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Percentiles {
    /// The 10th percentile.
    pub p10: f32,
    /// The median.
    pub p50: f32,
    /// The 90th percentile.
    pub p90: f32,
}

/// The `p`th quantile (`p` in `0.0..=1.0`) of `sorted`, which must be in
/// ascending order; `None` when it is empty.
///
/// The method interpolates linearly between order statistics (Hyndman and
/// Fan's type 7, the default in R and NumPy): with `n` values and
/// `h = (n − 1)·p`, the result is `x[⌊h⌋] + (h − ⌊h⌋)·(x[⌊h⌋ + 1] − x[⌊h⌋])`,
/// counting from 0. The 0th and 100th percentiles are the extremes.
pub fn quantile(sorted: &[f32], p: f32) -> Option<f32> {
    let last = sorted.len().checked_sub(1)?;
    let h = last as f32 * p.clamp(0.0, 1.0);
    let lo = (libm::floorf(h) as usize).min(last);
    let hi = (lo + 1).min(last);
    let (a, b) = (sorted[lo], sorted[hi]);
    Some(a + (h - lo as f32) * (b - a))
}

/// The P10, P50 and P90 of `values` by [`quantile`], ignoring NaNs; `None`
/// when there is no value.
pub fn percentiles(values: impl IntoIterator<Item = f32>) -> Option<Percentiles> {
    let mut sorted: Vec<f32> = values.into_iter().filter(|v| !v.is_nan()).collect();
    sorted.sort_unstable_by(f32::total_cmp);
    Some(Percentiles {
        p10: quantile(&sorted, 0.1)?,
        p50: quantile(&sorted, 0.5)?,
        p90: quantile(&sorted, 0.9)?,
    })
}

/// One quantity (precipitation or snowfall) across an ensemble's members
/// over an event.
#[derive(Clone, Debug, PartialEq)]
pub struct MemberSpread {
    /// Each member's total over the window, the control first; `None` for a
    /// member with no value in it.
    pub totals: Vec<Option<f32>>,
    /// The percentiles of the members' totals.
    pub percentiles: Option<Percentiles>,
    /// Each member's running total at each of the steps, as
    /// `[member][step]`; `None` until the member's first value.
    pub plumes: Vec<Vec<Option<f32>>>,
    /// The members' 10th percentile at each step.
    pub plume_p10: Vec<Option<f32>>,
    /// The members' median at each step.
    pub plume_p50: Vec<Option<f32>>,
    /// The members' 90th percentile at each step.
    pub plume_p90: Vec<Option<f32>>,
}

/// One ensemble's spread over an event.
#[derive(Clone, Debug, PartialEq)]
pub struct EnsembleEventStats {
    /// The Ensemble API `models` id.
    pub model: String,
    /// How many members (the control included) the ensemble has.
    pub members: usize,
    /// The plume's steps: the end of each 6-hour step across the window, the
    /// last one at the window's end.
    pub steps: Vec<UnixTime>,
    /// Precipitation, in mm.
    pub precipitation: MemberSpread,
    /// Snowfall in cm, when the event has snow.
    pub snowfall: Option<MemberSpread>,
}

/// The ends of the 6-hour steps across `[start, end)`; the last one is `end`.
pub fn plume_steps(start: UnixTime, end: UnixTime) -> Vec<UnixTime> {
    let mut steps = Vec::new();
    let mut t = start.0 + BIN_SECS;
    while t < end.0 {
        steps.push(UnixTime(t));
        t += BIN_SECS;
    }
    if end > start {
        steps.push(end);
    }
    steps
}

/// One member's running total at each step from `start`.
fn plume(
    times: &[UnixTime],
    values: &[Option<f32>],
    start: UnixTime,
    steps: &[UnixTime],
) -> Vec<Option<f32>> {
    let mut from = start;
    let mut running: Option<f32> = None;
    steps
        .iter()
        .map(|&to| {
            if let Some(sum) = window_sum(times, values, from, to) {
                running = Some(running.unwrap_or(0.0) + sum);
            }
            from = to;
            running
        })
        .collect()
}

/// The percentile line `pick` of the members' plumes at each step.
fn plume_line(
    plumes: &[Vec<Option<f32>>],
    steps: usize,
    pick: fn(&Percentiles) -> f32,
) -> Vec<Option<f32>> {
    (0..steps)
        .map(|i| {
            percentiles(plumes.iter().filter_map(|m| m.get(i).copied().flatten())).map(|p| pick(&p))
        })
        .collect()
}

/// The members' spread from `start` over `steps`, the last of which ends the
/// window. Each total is its plume's last value, so the two always match.
fn member_spread(
    times: &[UnixTime],
    members: &[Vec<Option<f32>>],
    start: UnixTime,
    steps: &[UnixTime],
) -> MemberSpread {
    let plumes: Vec<Vec<Option<f32>>> = members
        .iter()
        .map(|m| plume(times, m, start, steps))
        .collect();
    let totals: Vec<Option<f32>> = plumes.iter().map(|p| p.last().copied().flatten()).collect();
    MemberSpread {
        percentiles: percentiles(totals.iter().flatten().copied()),
        totals,
        plume_p10: plume_line(&plumes, steps.len(), |p| p.p10),
        plume_p50: plume_line(&plumes, steps.len(), |p| p.p50),
        plume_p90: plume_line(&plumes, steps.len(), |p| p.p90),
        plumes,
    }
}

/// `series`' spread over the window `[start, end)`: precipitation, and
/// snowfall when `with_snow`.
pub fn ensemble_event_stats(
    series: &EnsembleSeries,
    start: UnixTime,
    end: UnixTime,
    with_snow: bool,
) -> EnsembleEventStats {
    let steps = plume_steps(start, end);
    let spread =
        |members: &[Vec<Option<f32>>]| member_spread(&series.times, members, start, &steps);
    EnsembleEventStats {
        model: series.model.clone(),
        members: series.member_count(),
        precipitation: spread(&series.precipitation_mm),
        snowfall: with_snow.then(|| spread(&series.snowfall_cm)),
        steps,
    }
}

/// Every ensemble's spread over `event`, in the outlook's order, with
/// snowfall when the event has snow.
pub fn ensemble_stats(outlook: &StormOutlook, event: &StormEvent) -> Vec<EnsembleEventStats> {
    outlook
        .ensembles
        .iter()
        .map(|e| ensemble_event_stats(e, event.start, event.end, event.has_snow()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storm::testdata::{STORM_PEAK, STORM_START, dry_week, noreaster, noreaster_window};
    use alloc::string::ToString;
    use alloc::vec;

    const CM_PER_IN: f32 = 2.54;
    /// Saturday 2027-01-16 00:00 UTC.
    const SATURDAY: i64 = STORM_START.0 + 24 * HOUR;

    /// An hourly series stamped from an hour after `STORM_START`, one stamp
    /// per precipitation value, with no snow, a steady 1012 hPa and 20 km/h
    /// gusts.
    fn series(model: &str, precipitation_mm: Vec<Option<f32>>) -> PointSeries {
        let n = precipitation_mm.len();
        PointSeries {
            model: model.to_string(),
            run: None,
            times: (1..=i64::try_from(n).unwrap())
                .map(|h| UnixTime(STORM_START.0 + h * HOUR))
                .collect(),
            precipitation_mm,
            snowfall_cm: vec![Some(0.0); n],
            gust_kmh: vec![Some(20.0); n],
            pressure_hpa: vec![Some(1_012.0); n],
        }
    }

    /// 96 dry hours with 1 mm an hour in the 6-hour bins listed.
    fn wet_bins(bins: &[usize]) -> Vec<Option<f32>> {
        (0..96)
            .map(|h| Some(if bins.contains(&(h / 6)) { 1.0 } else { 0.0 }))
            .collect()
    }

    fn outlook(points: Vec<PointSeries>) -> StormOutlook {
        StormOutlook {
            points,
            ..StormOutlook::default()
        }
    }

    fn bin(k: i64) -> UnixTime {
        UnixTime(STORM_START.0 + k * BIN_SECS)
    }

    #[test]
    fn the_noreaster_is_one_event_from_saturday_to_monday() {
        let events = find_events(&noreaster());
        assert_eq!(events.len(), 1, "{events:?}");
        let event = &events[0];
        // Saturday 18:00 UTC (1 PM EST) to Monday 12:00 UTC (7 AM EST).
        assert_eq!((event.start, event.end), noreaster_window(), "{event:?}");
        assert!(event.has_snow());

        let snow_in: Vec<f32> = event
            .models
            .iter()
            .map(|m| m.total_snow_cm.unwrap_or_default() / CM_PER_IN)
            .collect();
        assert!((snow_in[0] - 14.0).abs() < 0.3, "{snow_in:?}");
        assert!((snow_in[1] - 3.0).abs() < 0.2, "{snow_in:?}");
        let ecmwf = &event.models[0];
        assert_eq!(ecmwf.model, "ecmwf_ifs025");
        assert!(ecmwf.lowest_hpa.is_some_and(|p| p < 982.0), "{ecmwf:?}");
        assert!(ecmwf.peak_gust_kmh.is_some_and(|g| g > 70.0), "{ecmwf:?}");
        // The snow starts on Saturday night: 10 PM EST.
        assert_eq!(ecmwf.onset, Some(UnixTime(STORM_PEAK.0 - 15 * HOUR)));
        // GFS's lighter, later storm starts later.
        assert!(event.models[1].onset > ecmwf.onset, "{event:?}");
        let lowest_at = ecmwf.lowest_at.unwrap_or_default();
        assert!((lowest_at.0 - STORM_PEAK.0).abs() <= 6 * HOUR, "{ecmwf:?}");
        let mean = event.models.iter().filter_map(|m| m.total_mm).sum::<f32>() / 4.0;
        assert!((event.mean_total_mm - mean).abs() < 1e-3);
    }

    #[test]
    fn a_dry_week_has_no_events_but_a_deepest_low() {
        let outlook = dry_week();
        assert_eq!(find_events(&outlook), []);
        let (model, hpa, at) = deepest_low(&outlook).unwrap();
        let lowest = outlook
            .points
            .iter()
            .flat_map(|p| p.pressure_hpa.iter().flatten())
            .fold(f32::INFINITY, |a, b| a.min(*b));
        assert_eq!(hpa, lowest);
        let series = outlook.points.iter().find(|p| p.model == model).unwrap();
        let i = series.times.iter().position(|t| *t == at).unwrap();
        assert_eq!(series.pressure_hpa[i], Some(hpa));
    }

    #[test]
    fn the_deepest_low_in_the_noreaster_is_ecmwfs() {
        let (model, hpa, at) = deepest_low(&noreaster()).unwrap();
        assert_eq!(model, "ecmwf_ifs025");
        assert!(hpa < 982.0);
        assert!((at.0 - STORM_PEAK.0).abs() <= 6 * HOUR);
        assert_eq!(deepest_low(&StormOutlook::default()), None);
    }

    #[test]
    fn wet_bins_a_short_gap_apart_join() {
        // Bins 4 and 6: one dry bin (6 h) between them.
        let events = find_events(&outlook(vec![series("a", wet_bins(&[4, 6]))]));
        assert_eq!(events.len(), 1, "{events:?}");
        assert_eq!((events[0].start, events[0].end), (bin(3), bin(8)));
        assert_eq!(events[0].mean_total_mm, 12.0);
        assert!(!events[0].has_snow());
    }

    #[test]
    fn wet_bins_a_long_gap_apart_stay_apart() {
        // Bins 4 and 8: three dry bins (18 h) between them.
        let events = find_events(&outlook(vec![series("a", wet_bins(&[4, 8]))]));
        let windows: Vec<_> = events.iter().map(|e| (e.start, e.end)).collect();
        assert_eq!(windows, [(bin(3), bin(6)), (bin(7), bin(10))]);
        // Exactly 12 hours apart is not under the merge gap.
        let events = find_events(&outlook(vec![series("a", wet_bins(&[4, 7]))]));
        assert_eq!(events.len(), 2, "{events:?}");
    }

    #[test]
    fn padding_stays_within_the_data() {
        let events = find_events(&outlook(vec![series("a", wet_bins(&[0, 15]))]));
        let windows: Vec<_> = events.iter().map(|e| (e.start, e.end)).collect();
        assert_eq!(windows, [(bin(0), bin(2)), (bin(14), bin(16))]);
    }

    #[test]
    fn only_the_three_wettest_events_are_kept_in_time_order() {
        let mut mm = wet_bins(&[1, 5, 9, 13]);
        // The bin-9 storm is the driest: 2 mm an hour everywhere else.
        for (h, v) in mm.iter_mut().enumerate() {
            if *v == Some(1.0) && h / 6 != 9 {
                *v = Some(2.0);
            }
        }
        let events = find_events(&outlook(vec![series("a", mm)]));
        let starts: Vec<UnixTime> = events.iter().map(|e| e.start).collect();
        assert_eq!(starts, [bin(0), bin(4), bin(12)]);
    }

    #[test]
    fn snow_alone_makes_an_event() {
        let mut s = series("a", vec![Some(0.1); 48]);
        s.snowfall_cm = (0..48)
            .map(|h| Some(if h / 6 == 3 { 0.2 } else { 0.0 }))
            .collect();
        let events = find_events(&outlook(vec![s]));
        assert_eq!(events.len(), 1, "{events:?}");
        assert!((events[0].mean_snow_cm - 1.2).abs() < 1e-4);
        assert!(events[0].has_snow());
    }

    #[test]
    fn the_bin_mean_counts_only_models_with_data() {
        // "b" has no values in bins 4 and 5, so "a" alone decides them; with
        // "b"'s zeros counted, bin 4 would fall to 1.5 mm and stay dry.
        let a = series(
            "a",
            wet_bins(&[4]).iter().map(|v| v.map(|x| x / 2.0)).collect(),
        );
        let b_mm = (0..96)
            .map(|h| {
                if (24..36).contains(&h) {
                    None
                } else {
                    Some(0.0)
                }
            })
            .collect();
        let events = find_events(&outlook(vec![a, series("b", b_mm)]));
        assert_eq!(events.len(), 1, "{events:?}");
        assert_eq!((events[0].start, events[0].end), (bin(3), bin(6)));
        assert_eq!(events[0].models[1].total_mm, Some(0.0));
    }

    #[test]
    fn a_model_with_missing_hours_keeps_the_hours_it_has() {
        let mut mm = vec![Some(0.0); 24];
        mm[6] = Some(0.4);
        mm[7] = None;
        mm[8] = Some(2.0);
        mm[9] = Some(1.0);
        let mut s = series("a", mm);
        s.gust_kmh[8] = None;
        s.gust_kmh[9] = Some(55.0);
        s.pressure_hpa[9] = Some(1_001.0);
        s.pressure_hpa[10] = None;
        s.pressure_hpa[12] = Some(1_001.0);
        s.snowfall_cm = vec![None; 24];
        let stats = model_stats(&s, STORM_START, UnixTime(STORM_START.0 + 24 * HOUR));
        assert!(
            (stats.total_mm.unwrap_or_default() - 3.4).abs() < 1e-5,
            "{stats:?}"
        );
        assert_eq!(stats.total_snow_cm, None);
        assert_eq!(stats.peak_gust_kmh, Some(55.0));
        assert_eq!(stats.lowest_hpa, Some(1_001.0));
        // Hour 9 is stamped 10 hours after the start; the first tie wins.
        assert_eq!(stats.lowest_at, Some(UnixTime(STORM_START.0 + 10 * HOUR)));
        // The hour stamped 9 h in (index 8) starts 8 h in.
        assert_eq!(stats.onset, Some(UnixTime(STORM_START.0 + 8 * HOUR)));

        let empty = series("b", vec![None; 24]);
        let stats = model_stats(&empty, STORM_START, UnixTime(STORM_START.0 + 3 * HOUR));
        assert_eq!((stats.total_mm, stats.onset), (None, None));
    }

    #[test]
    fn window_totals_take_the_hours_inside_the_window() {
        let s = series("a", (1..=6).map(|h| Some(h as f32)).collect());
        // Stamps 1..=6 h after the start; [1 h, 4 h) holds stamps 2, 3 and 4.
        let (mm, cm) = window_totals(
            &s,
            UnixTime(STORM_START.0 + HOUR),
            UnixTime(STORM_START.0 + 4 * HOUR),
        );
        assert_eq!((mm, cm), (Some(9.0), Some(0.0)));
        let (mm, cm) = window_totals(&s, UnixTime(0), UnixTime(1));
        assert_eq!((mm, cm), (None, None));
    }

    #[test]
    fn bins_align_to_six_hours_utc() {
        // The hour stamped 06:00 belongs to the 00–06 bin, 07:00 to the next.
        assert_eq!(bin_of(UnixTime(SATURDAY + 6 * HOUR)), SATURDAY);
        assert_eq!(bin_of(UnixTime(SATURDAY + 7 * HOUR)), SATURDAY + BIN_SECS);
        assert_eq!(bin_of(UnixTime(SATURDAY)), SATURDAY - BIN_SECS);
        assert_eq!(find_events(&StormOutlook::default()), []);
    }

    /// New York (Manhattan).
    const NYC: (f64, f64) = (40.7128, -74.0060);

    fn totals(mm: &[Option<f32>]) -> Vec<ModelEventStats> {
        mm.iter()
            .zip([
                "ecmwf_ifs025",
                "gfs_seamless",
                "gem_seamless",
                "icon_seamless",
            ])
            .map(|(total_mm, model)| ModelEventStats {
                model: model.to_string(),
                total_mm: *total_mm,
                total_snow_cm: None,
                peak_gust_kmh: None,
                lowest_hpa: None,
                lowest_at: None,
                onset: None,
            })
            .collect()
    }

    fn center(model: &str, latitude: f64, longitude: f64) -> StormCenter {
        StormCenter {
            model: model.to_string(),
            latitude,
            longitude,
            time: STORM_PEAK,
            pressure_hpa: 990.0,
        }
    }

    fn verdict(mm: &[f32], centers: &[StormCenter]) -> Option<Agreement> {
        let mm: Vec<Option<f32>> = mm.iter().copied().map(Some).collect();
        agreement(&totals(&mm), centers, NYC.0, NYC.1).verdict
    }

    #[test]
    fn new_york_to_boston_is_about_306_km() {
        let km = distance_km(NYC.0, NYC.1, 42.3601, -71.0589);
        assert!((km - 306.0).abs() < 5.0, "{km}");
        assert_eq!(distance_km(NYC.0, NYC.1, NYC.0, NYC.1), 0.0);
        // One degree of longitude on the equator, across the antimeridian.
        let km = distance_km(0.0, 179.5, 0.0, -179.5);
        assert!((km - 111.2).abs() < 0.1, "{km}");
    }

    #[test]
    fn the_noreaster_models_disagree() {
        let outlook = noreaster();
        let events = find_events(&outlook);
        let place = &outlook.place;
        let result = agreement(&events[0].models, &[], place.latitude, place.longitude);
        assert_eq!(result.verdict, Some(Agreement::Low));
        assert_eq!(result.totals, Some(Agreement::Low));
        assert_eq!((result.centers, result.center_spread_km), (None, None));
        assert!(
            result.spread.is_some_and(|s| s > SPREAD_MEDIUM_MAX),
            "{result:?}"
        );
        let wettest = result.wettest.unwrap();
        let driest = result.driest.unwrap();
        assert_eq!(
            (wettest.model.as_str(), driest.model.as_str()),
            ("ecmwf_ifs025", "gfs_seamless")
        );
        assert!(wettest.total_mm > 40.0 && driest.total_mm < 15.0);

        // Where each model's low is deepest: ECMWF close in, GFS offshore.
        let centers = [
            center("ecmwf_ifs025", 40.2, -73.2),
            center("gfs_seamless", 39.0, -71.0),
        ];
        let result = agreement(&events[0].models, &centers, place.latitude, place.longitude);
        assert_eq!(result.centers, Some(Agreement::Medium));
        assert_eq!(result.verdict, Some(Agreement::Low));
        let closest = result.closest.unwrap();
        let farthest = result.farthest.unwrap();
        assert_eq!(closest.model, "ecmwf_ifs025");
        assert!((closest.distance_km - 87.0).abs() < 10.0, "{closest:?}");
        assert_eq!(farthest.model, "gfs_seamless");
        assert!((farthest.distance_km - 320.0).abs() < 20.0, "{farthest:?}");
        let spread = result.center_spread_km.unwrap();
        assert!((spread - 240.0).abs() < 20.0, "{spread}");
    }

    #[test]
    fn the_worse_verdict_wins() {
        let near = [center("a", 40.0, -74.0), center("b", 40.3, -74.0)];
        let far = [center("a", 40.0, -74.0), center("b", 40.0, -68.0)];
        assert_eq!(verdict(&[20.0, 21.0, 22.0], &near), Some(Agreement::High));
        assert_eq!(verdict(&[20.0, 21.0, 22.0], &far), Some(Agreement::Low));
        assert_eq!(verdict(&[5.0, 40.0], &near), Some(Agreement::Low));
        assert_eq!(verdict(&[20.0, 30.0], &near), Some(Agreement::Medium));
        // One centre has nothing to compare with: the totals decide.
        assert_eq!(verdict(&[20.0, 21.0], &far[1..]), Some(Agreement::High));
        assert_eq!(verdict(&[20.0], &far), Some(Agreement::Low));
        assert_eq!(verdict(&[20.0], &[]), None);
        assert_eq!(worse(Agreement::High, Agreement::Medium), Agreement::Medium);
        assert_eq!(worse(Agreement::Low, Agreement::Medium), Agreement::Low);
    }

    #[test]
    fn the_thresholds_are_exclusive_upper_bounds() {
        assert_eq!(totals_agreement(0.349), Agreement::High);
        assert_eq!(totals_agreement(SPREAD_HIGH_MAX), Agreement::Medium);
        assert_eq!(totals_agreement(SPREAD_MEDIUM_MAX), Agreement::Low);
        assert_eq!(centers_agreement(149.9), Agreement::High);
        assert_eq!(centers_agreement(CENTERS_HIGH_MAX_KM), Agreement::Medium);
        assert_eq!(centers_agreement(CENTERS_MEDIUM_MAX_KM), Agreement::Low);
    }

    #[test]
    fn light_events_divide_by_the_floor() {
        let result = agreement(&totals(&[Some(1.0), Some(3.0)]), &[], NYC.0, NYC.1);
        // 2 mm apart over the 5 mm floor, not over the 2 mm mean.
        assert_eq!(result.spread, Some(0.4));
        assert_eq!(result.verdict, Some(Agreement::Medium));
    }

    #[test]
    fn models_without_totals_are_left_out() {
        let result = agreement(
            &totals(&[Some(10.0), None, Some(10.0), Some(12.0)]),
            &[],
            NYC.0,
            NYC.1,
        );
        assert_eq!(
            result.driest.map(|d| d.model).as_deref(),
            Some("ecmwf_ifs025")
        );
        assert_eq!(
            result.wettest.map(|d| d.model).as_deref(),
            Some("icon_seamless")
        );
        assert_eq!(result.verdict, Some(Agreement::High));
        let one = agreement(&totals(&[None, Some(10.0)]), &[], NYC.0, NYC.1);
        assert_eq!((one.spread, one.verdict), (None, None));
        assert_eq!(one.wettest.map(|d| d.total_mm), Some(10.0));
    }

    fn close(a: Option<f32>, b: f32) -> bool {
        a.is_some_and(|a| (a - b).abs() < 1e-4)
    }

    #[test]
    fn percentiles_interpolate_between_order_statistics() {
        let p = percentiles((1..=10).rev().map(|v| v as f32)).unwrap();
        assert!(close(Some(p.p10), 1.9), "{p:?}");
        assert!(close(Some(p.p50), 5.5), "{p:?}");
        assert!(close(Some(p.p90), 9.1), "{p:?}");
        let sorted: Vec<f32> = (1..=10).map(|v| v as f32).collect();
        assert_eq!(quantile(&sorted, 0.0), Some(1.0));
        assert_eq!(quantile(&sorted, 1.0), Some(10.0));
        let one = percentiles([4.0, f32::NAN]).unwrap();
        assert_eq!((one.p10, one.p50, one.p90), (4.0, 4.0, 4.0));
        assert_eq!(percentiles([]), None);
        assert_eq!(quantile(&[], 0.5), None);
    }

    #[test]
    fn plume_steps_run_six_hours_to_the_end() {
        assert_eq!(plume_steps(bin(2), bin(5)), [bin(3), bin(4), bin(5)]);
        let end = UnixTime(bin(1).0 + 2 * HOUR);
        assert_eq!(plume_steps(bin(0), end), [bin(1), end]);
        assert_eq!(plume_steps(bin(1), bin(1)), []);
    }

    #[test]
    fn the_noreaster_ensembles_spread_around_their_controls() {
        let outlook = noreaster();
        let event = &find_events(&outlook)[0];
        let stats = ensemble_stats(&outlook, event);
        let models: Vec<(&str, usize)> = stats
            .iter()
            .map(|s| (s.model.as_str(), s.members))
            .collect();
        assert_eq!(models, [("ecmwf_ifs025", 51), ("gfs025", 31)]);
        for (e, control) in stats.iter().zip([&event.models[0], &event.models[1]]) {
            assert_eq!(e.steps, plume_steps(event.start, event.end));
            assert_eq!(e.steps.len(), 7);
            for spread in [Some(&e.precipitation), e.snowfall.as_ref()] {
                let spread = spread.unwrap();
                assert_eq!(spread.totals.len(), e.members);
                assert_eq!(spread.plumes.len(), e.members);
                let p = spread.percentiles.unwrap();
                assert!(p.p10 < p.p50 && p.p50 < p.p90, "{}: {p:?}", e.model);
                for (plume, total) in spread.plumes.iter().zip(&spread.totals) {
                    assert_eq!(plume.len(), e.steps.len());
                    assert!(plume.windows(2).all(|w| w[0] <= w[1]), "{plume:?}");
                    assert_eq!(plume.last().copied().flatten(), *total);
                }
                let last = e.steps.len() - 1;
                assert!(close(spread.plume_p10[last], p.p10));
                assert!(close(spread.plume_p50[last], p.p50));
                assert!(close(spread.plume_p90[last], p.p90));
                assert!(
                    spread
                        .plume_p10
                        .iter()
                        .zip(&spread.plume_p90)
                        .all(|(a, b)| a <= b)
                );
            }
            // The control is the deterministic run.
            let total = e.precipitation.totals[0].unwrap();
            assert!(
                (total - control.total_mm.unwrap()).abs() < 1.0,
                "{}",
                e.model
            );
        }
        // ECMWF's members bring more than GFS's.
        let medians: Vec<f32> = stats
            .iter()
            .map(|s| s.precipitation.percentiles.unwrap().p50)
            .collect();
        assert!(medians[0] > medians[1], "{medians:?}");
    }

    #[test]
    fn ensembles_skip_snow_without_it_and_missing_members() {
        let mut series = dry_week().ensembles.remove(1);
        series.precipitation_mm[1] = vec![None; series.times.len()];
        series.precipitation_mm[2][10] = Some(3.0);
        let start = STORM_START;
        let end = UnixTime(STORM_START.0 + 24 * HOUR);
        let stats = ensemble_event_stats(&series, start, end, false);
        assert_eq!(stats.snowfall, None);
        let spread = &stats.precipitation;
        assert_eq!(spread.totals[1], None);
        assert_eq!(spread.plumes[1], [None; 4]);
        assert_eq!(spread.totals[2], Some(3.0));
        // Stamped 10 h in: the second step's running total and after.
        assert_eq!(
            spread.plumes[2],
            [Some(0.0), Some(3.0), Some(3.0), Some(3.0)]
        );
        // 30 members with a total: 29 zeros and one 3 mm.
        let p = spread.percentiles.unwrap();
        assert_eq!((p.p50, p.p90), (0.0, 0.0));
    }
}
