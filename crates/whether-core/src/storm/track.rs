//! Finding lows on a pressure grid and following them over time.
//!
//! [`find_lows`] picks the grid points that are local pressure minima at one
//! time step and refines each to a position between grid points.
//! [`track_lows`] links them from step to step into [`Track`]s, and
//! [`storm_track`] and [`keep_tracks`] pick the ones a map shows.
//!
//! **Detection.** A point qualifies when it is under [`MAX_LOW_HPA`], it is
//! the lowest of its 3×3 neighbourhood, and it lies at least
//! [`MIN_DEPTH_HPA`] below the mean of its 8 neighbours. Points on the grid's
//! edge never qualify, and neither does a point with a missing neighbour.
//!
//! A stricter rule, "at least 2 hPa below each of the 8 neighbours", loses
//! lows on a 2.5° grid: a centre midway between two grid points leaves those
//! two points nearly equal, so neither is 2 hPa below the other, and a broad
//! low dips the neighbours almost as much as the centre. Comparing with the
//! neighbours' mean keeps such lows while still rejecting flat fields and
//! gentle gradients. Among exactly equal lowest points, the first in row
//! order wins, so a tie yields one low and not two.
//!
//! **Refinement.** A parabola through the point and its two neighbours along
//! each axis places the centre within [`MAX_REFINE_CELLS`] of the point, and
//! the two fitted dips give the centre's pressure.
//!
//! **Linking.** Lows at consecutive steps join the same track when they lie
//! within [`MAX_STEP_KM`] per [`REFERENCE_STEP_SECS`], nearest pairs first.
//! Distances are great-circle distances, so tracks cross the antimeridian.

use alloc::string::String;
use alloc::vec::Vec;

use crate::storm::{PressureGrid, wrap_longitude};
use crate::time::UnixTime;

/// A low's centre must be under this sea-level pressure, in hPa.
pub const MAX_LOW_HPA: f32 = 1_008.0;

/// A low's grid point must lie this far below the mean of its 8 neighbours,
/// in hPa.
pub const MIN_DEPTH_HPA: f32 = 1.0;

/// The refined centre moves at most this many grid cells from its grid
/// point along each axis.
pub const MAX_REFINE_CELLS: f64 = 0.5;

/// The mean Earth radius used for distances, in km.
pub const EARTH_RADIUS_KM: f64 = 6_371.0;

/// A low's centre at one time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrackPoint {
    /// The grid's time step.
    pub time: UnixTime,
    /// Latitude in degrees.
    pub latitude: f64,
    /// Longitude in degrees, in `-180.0..180.0`.
    pub longitude: f64,
    /// The central sea-level pressure, in hPa.
    pub pressure_hpa: f32,
}

/// The great-circle distance between two `(latitude, longitude)` points in
/// degrees, in km (haversine on a sphere of [`EARTH_RADIUS_KM`]).
pub fn distance_km(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (lat1, lat2) = (a.0.to_radians(), b.0.to_radians());
    let dlat = lat2 - lat1;
    let dlon = (b.1 - a.1).to_radians();
    let sin_dlat = libm::sin(dlat / 2.0);
    let sin_dlon = libm::sin(dlon / 2.0);
    let haversine = sin_dlat * sin_dlat + libm::cos(lat1) * libm::cos(lat2) * sin_dlon * sin_dlon;
    2.0 * EARTH_RADIUS_KM * libm::asin(libm::sqrt(haversine.clamp(0.0, 1.0)))
}

/// The lows on `grid` at time index `step`, refined between grid points, in
/// row order. Empty when the step is missing.
pub fn find_lows(grid: &PressureGrid, step: usize) -> Vec<TrackPoint> {
    let (rows, cols) = (grid.lats.len(), grid.lons.len());
    let mut lows = Vec::new();
    let Some(&time) = grid.times.get(step) else {
        return lows;
    };
    for row in 1..rows.saturating_sub(1) {
        for col in 1..cols.saturating_sub(1) {
            if let Some(cell) = neighbourhood(grid, step, row, col)
                && is_low(&cell)
            {
                lows.push(refine(grid, time, row, col, &cell));
            }
        }
    }
    lows
}

/// The 3×3 values around an interior point as `[row][col]`, south to north
/// and west to east, or `None` when one is missing.
fn neighbourhood(
    grid: &PressureGrid,
    step: usize,
    row: usize,
    col: usize,
) -> Option<[[f32; 3]; 3]> {
    let mut cell = [[0.0; 3]; 3];
    for (i, line) in cell.iter_mut().enumerate() {
        for (j, value) in line.iter_mut().enumerate() {
            *value = grid.value(step, row + i - 1, col + j - 1)?;
        }
    }
    Some(cell)
}

/// Whether the centre of `cell` is a low (see the module docs).
fn is_low(cell: &[[f32; 3]; 3]) -> bool {
    let centre = cell[1][1];
    if centre >= MAX_LOW_HPA {
        return false;
    }
    let mut sum = 0.0;
    for (k, value) in cell.iter().flatten().enumerate() {
        if k == 4 {
            continue;
        }
        // Neighbours before the centre in row order must be higher, the ones
        // after it at least as high, so a tie has one winner. A NaN fails
        // both comparisons.
        let higher = if k < 4 {
            centre < *value
        } else {
            centre <= *value
        };
        if !higher {
            return false;
        }
        sum += *value;
    }
    centre <= sum / 8.0 - MIN_DEPTH_HPA
}

/// The vertex of the parabola through `(-1, minus)`, `(0, centre)` and
/// `(1, plus)`, clamped to [`MAX_REFINE_CELLS`]: its offset in cells and the
/// fitted change from `centre` there, in hPa.
fn vertex(minus: f32, centre: f32, plus: f32) -> (f64, f64) {
    let (m, c, p) = (f64::from(minus), f64::from(centre), f64::from(plus));
    let slope = (p - m) / 2.0;
    let curve = f64::midpoint(m, p) - c;
    if curve <= 0.0 {
        return (0.0, 0.0);
    }
    let x = (-slope / (2.0 * curve)).clamp(-MAX_REFINE_CELLS, MAX_REFINE_CELLS);
    (x, slope * x + curve * x * x)
}

/// The low at interior point `(row, col)` refined by a parabola along each
/// axis of its neighbourhood `cell`.
fn refine(
    grid: &PressureGrid,
    time: UnixTime,
    row: usize,
    col: usize,
    cell: &[[f32; 3]; 3],
) -> TrackPoint {
    let (dy, dp_y) = vertex(cell[0][1], cell[1][1], cell[2][1]);
    let (dx, dp_x) = vertex(cell[1][0], cell[1][1], cell[1][2]);
    let lat = f64::from(grid.lats[row]);
    let row_step = f64::from(grid.lats[row + 1] - grid.lats[row - 1]) / 2.0;
    // The columns may wrap across the antimeridian between two neighbours.
    let col_step =
        wrap_longitude(f64::from(grid.lons[col + 1]) - f64::from(grid.lons[col - 1])) / 2.0;
    TrackPoint {
        time,
        latitude: lat + dy * row_step,
        longitude: wrap_longitude(f64::from(grid.lons[col]) + dx * col_step),
        pressure_hpa: (f64::from(cell[1][1]) + dp_y + dp_x) as f32,
    }
}

/// Lows at consecutive steps are linked when they lie at most this far apart
/// per [`REFERENCE_STEP_SECS`] between the steps, in km.
pub const MAX_STEP_KM: f64 = 900.0;

/// The time between steps that [`MAX_STEP_KM`] applies to, in seconds; the
/// reach scales with the actual time between steps.
pub const REFERENCE_STEP_SECS: i64 = 6 * 3_600;

/// Tracks with fewer points are dropped.
pub const MIN_TRACK_POINTS: usize = 3;

/// One low followed across a grid's time steps.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Track {
    /// The Open-Meteo `models` id of the grid it was found on.
    pub model: String,
    /// The centre at consecutive time steps, ascending.
    pub points: Vec<TrackPoint>,
}

impl Track {
    /// The point of lowest pressure, the earliest among equals; `None` for
    /// an empty track.
    pub fn lowest(&self) -> Option<&TrackPoint> {
        self.points.iter().reduce(|best, p| {
            if p.pressure_hpa < best.pressure_hpa {
                p
            } else {
                best
            }
        })
    }
}

/// The lows on `grid` linked into tracks, ordered by their first time.
///
/// Each step's lows (see [`find_lows`]) extend the tracks that reached the
/// previous step: pairs within [`MAX_STEP_KM`] (scaled by the time between
/// the steps) are linked nearest first, each track and each low at most
/// once. A low left over starts a new track, and a track left over ends.
/// Tracks shorter than [`MIN_TRACK_POINTS`] are dropped.
pub fn track_lows(grid: &PressureGrid) -> Vec<Track> {
    let mut ended: Vec<Vec<TrackPoint>> = Vec::new();
    let mut open: Vec<Vec<TrackPoint>> = Vec::new();
    let mut previous: Option<UnixTime> = None;
    for (step, &time) in grid.times.iter().enumerate() {
        let lows = find_lows(grid, step);
        let reach = previous.map_or(0.0, |before| {
            MAX_STEP_KM * (time.0 - before.0) as f64 / REFERENCE_STEP_SECS as f64
        });
        let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
        for (t, track) in open.iter().enumerate() {
            let Some(end) = track.last() else { continue };
            for (l, low) in lows.iter().enumerate() {
                let km = distance_km(position(end), position(low));
                if km <= reach {
                    pairs.push((km, t, l));
                }
            }
        }
        pairs.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        let mut continues: Vec<Option<usize>> = alloc::vec![None; lows.len()];
        let mut taken = alloc::vec![false; open.len()];
        for (_, t, l) in pairs {
            if !taken[t] && continues[l].is_none() {
                taken[t] = true;
                continues[l] = Some(t);
            }
        }
        let mut tracks: Vec<Option<Vec<TrackPoint>>> = open.into_iter().map(Some).collect();
        let mut next = Vec::with_capacity(lows.len());
        for (low, track) in lows.into_iter().zip(continues) {
            let mut points = track.and_then(|t| tracks[t].take()).unwrap_or_default();
            points.push(low);
            next.push(points);
        }
        ended.extend(tracks.into_iter().flatten());
        open = next;
        previous = Some(time);
    }
    ended.extend(open);
    let mut kept: Vec<Track> = ended
        .into_iter()
        .filter(|points| points.len() >= MIN_TRACK_POINTS)
        .map(|points| Track {
            model: grid.model.clone(),
            points,
        })
        .collect();
    kept.sort_by_key(|track| track.points.first().map(|p| p.time));
    kept
}

/// A point's `(latitude, longitude)`.
fn position(point: &TrackPoint) -> (f64, f64) {
    (point.latitude, point.longitude)
}

/// The point of `track` closest to `place` (`(latitude, longitude)` in
/// degrees) and its distance in km; the earliest among equals, `None` for an
/// empty track.
pub fn closest_approach(track: &Track, place: (f64, f64)) -> Option<(TrackPoint, f64)> {
    nearest(track.points.iter(), place)
}

/// The nearest of `points` to `place`, the first among equals.
fn nearest<'a>(
    points: impl Iterator<Item = &'a TrackPoint>,
    place: (f64, f64),
) -> Option<(TrackPoint, f64)> {
    points
        .map(|p| (*p, distance_km(position(p), place)))
        .reduce(|best, next| if next.1 < best.1 { next } else { best })
}

/// The index of the storm among `tracks`: the track that passes closest to
/// `place` (`(latitude, longitude)` in degrees) at its points within
/// `window` (start inclusive, end exclusive). A deeper low breaks a tie, then
/// the earlier track. `None` when no track has a point in the window.
pub fn storm_track(
    tracks: &[Track],
    place: (f64, f64),
    window: (UnixTime, UnixTime),
) -> Option<usize> {
    tracks
        .iter()
        .enumerate()
        .filter_map(|(i, track)| {
            let inside = track
                .points
                .iter()
                .filter(|p| p.time >= window.0 && p.time < window.1);
            let (_, km) = nearest(inside, place)?;
            let lowest = track.lowest()?.pressure_hpa;
            Some((i, km, lowest))
        })
        .reduce(|best, next| {
            let closer = next.1.total_cmp(&best.1).then(next.2.total_cmp(&best.2));
            if closer.is_lt() { next } else { best }
        })
        .map(|(i, _, _)| i)
}

/// The tracks a map shows: the storm (index `storm` into `tracks`) first,
/// flagged `true`, then up to `max_others` of the rest, deepest first (the
/// earlier track among equals), flagged `false`. An index past the end
/// counts as no storm.
pub fn keep_tracks(
    tracks: &[Track],
    storm: Option<usize>,
    max_others: usize,
) -> Vec<(Track, bool)> {
    let storm = storm.filter(|i| *i < tracks.len());
    let mut others: Vec<(usize, f32)> = tracks
        .iter()
        .enumerate()
        .filter(|(i, _)| Some(*i) != storm)
        .map(|(i, track)| (i, track.lowest().map_or(f32::INFINITY, |p| p.pressure_hpa)))
        .collect();
    others.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
    storm
        .map(|i| (i, true))
        .into_iter()
        .chain(others.into_iter().take(max_others).map(|(i, _)| (i, false)))
        .map(|(i, is_storm)| (tracks[i].clone(), is_storm))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storm::testdata::{
        SyntheticLow, dry_week, grid_times, noreaster, noreaster_window, pressure_field,
    };
    use alloc::string::ToString;
    use alloc::vec;

    const NONE: [TrackPoint; 0] = [];

    /// A grid of `lows` on the background with rows from `south` and columns
    /// from `west`, `spacing` degrees apart, at `times`, rounded to 0.1 hPa.
    pub(super) fn field_grid(
        (south, west): (f32, f32),
        (rows, cols): (usize, usize),
        spacing: f32,
        times: &[UnixTime],
        lows: &[SyntheticLow],
    ) -> PressureGrid {
        let row_lats: Vec<f32> = (0..rows).map(|r| south + r as f32 * spacing).collect();
        let col_lons: Vec<f32> = (0..cols)
            .map(|c| wrap_longitude(f64::from(west + c as f32 * spacing)) as f32)
            .collect();
        let pressure_hpa = times
            .iter()
            .map(|t| {
                row_lats
                    .iter()
                    .flat_map(|lat| {
                        col_lons.iter().map(|lon| {
                            let p = pressure_field(lows, *t, *lat, *lon);
                            Some(libm::roundf(p * 10.0) / 10.0)
                        })
                    })
                    .collect()
            })
            .collect();
        PressureGrid {
            model: "test".to_string(),
            run: None,
            lats: row_lats,
            lons: col_lons,
            times: times.to_vec(),
            pressure_hpa,
        }
    }

    pub(super) fn still_low(
        latitude: f32,
        longitude: f32,
        central_hpa: f32,
        radius_km: f32,
    ) -> SyntheticLow {
        SyntheticLow {
            latitude,
            longitude,
            at: grid_times()[0],
            central_hpa,
            radius_km,
            speed_kmh: 0.0,
            bearing_deg: 0.0,
            lifetime_h: 1.0e6,
        }
    }

    /// Where the field of `lows` is lowest at `t` near `around`, to about
    /// 0.005°. The background's gradient moves it a little from the lows'
    /// centres.
    pub(super) fn field_min(lows: &[SyntheticLow], t: UnixTime, around: (f32, f32)) -> (f64, f64) {
        let search = |centre: (f32, f32), step: f32, n: i32| {
            let mut best = (f32::INFINITY, centre);
            for i in -n..=n {
                for j in -n..=n {
                    let lat = centre.0 + i as f32 * step;
                    let lon = wrap_longitude(f64::from(centre.1 + j as f32 * step)) as f32;
                    let p = pressure_field(lows, t, lat, lon);
                    if p < best.0 {
                        best = (p, (lat, lon));
                    }
                }
            }
            best.1
        };
        let (lat, lon) = search(search(around, 0.05, 30), 0.005, 10);
        (f64::from(lat), f64::from(lon))
    }

    /// Whether `cell`'s centre is 2 hPa below each of its 8 neighbours.
    fn strictly_low(cell: &[[f32; 3]; 3]) -> bool {
        let centre = cell[1][1];
        cell.iter()
            .flatten()
            .enumerate()
            .all(|(k, v)| k == 4 || centre <= *v - 2.0)
    }

    #[test]
    fn distances_follow_great_circles() {
        // One degree of a great circle is about 111.2 km.
        assert!((distance_km((0.0, 0.0), (1.0, 0.0)) - 111.195).abs() < 0.01);
        assert!((distance_km((0.0, 179.5), (0.0, -179.5)) - 111.195).abs() < 0.01);
        assert!((distance_km((60.0, 10.0), (60.0, 12.0)) - 111.195).abs() < 0.1);
        assert!(distance_km((40.7, -74.0), (40.7, -74.0)) < 1e-9);
        assert!((distance_km((-90.0, 0.0), (90.0, 0.0)) - 20_015.1).abs() < 0.1);
    }

    #[test]
    fn a_still_low_is_found_where_it_is_wherever_it_sits_between_points() {
        let t = [grid_times()[0]];
        let (mut worst_km, mut worst_hpa) = (0.0_f64, 0.0_f32);
        // Every half degree across one 2.5° cell, from a grid point to the
        // far corner, for narrow and broad lows, deep and shallow.
        for i in 0..6 {
            for j in 0..6 {
                for radius in [300.0, 450.0, 600.0] {
                    for central in [975.0, 1_000.0] {
                        let low = still_low(
                            40.0 + i as f32 * 0.5,
                            -75.0 + j as f32 * 0.5,
                            central,
                            radius,
                        );
                        let grid = field_grid((30.0, -90.0), (9, 13), 2.5, &t, &[low]);
                        let lows = find_lows(&grid, 0);
                        assert_eq!(lows.len(), 1, "{low:?}: {lows:?}");
                        let truth = field_min(&[low], t[0], (low.latitude, low.longitude));
                        let km = distance_km((lows[0].latitude, lows[0].longitude), truth);
                        worst_km = worst_km.max(km);
                        let truth_hpa =
                            pressure_field(&[low], t[0], truth.0 as f32, truth.1 as f32);
                        worst_hpa = worst_hpa.max((lows[0].pressure_hpa - truth_hpa).abs());
                    }
                }
            }
        }
        // About 16 km and 1.6 hPa at worst: the parabola is a little
        // shallower than a narrow, deep dip.
        assert!(worst_km < 20.0, "{worst_km} km");
        assert!(worst_hpa < 2.0, "{worst_hpa} hPa");
    }

    #[test]
    fn a_low_midway_between_points_fails_the_strict_rule_but_is_found() {
        let t = [grid_times()[0]];
        let low = still_low(41.25, -73.75, 992.0, 350.0);
        let grid = field_grid((30.0, -90.0), (9, 13), 2.5, &t, &[low]);
        let strict = (1..8)
            .flat_map(|r| (1..12).map(move |c| (r, c)))
            .filter(|(r, c)| neighbourhood(&grid, 0, *r, *c).is_some_and(|n| strictly_low(&n)))
            .count();
        assert_eq!(strict, 0);
        let lows = find_lows(&grid, 0);
        assert_eq!(lows.len(), 1, "{lows:?}");
        let truth = field_min(&[low], t[0], (low.latitude, low.longitude));
        let km = distance_km((lows[0].latitude, lows[0].longitude), truth);
        assert!(km < 50.0, "{km} km");
    }

    #[test]
    fn flat_fields_gradients_and_high_dips_give_no_lows() {
        let t = [grid_times()[0]];
        let flat = PressureGrid {
            pressure_hpa: vec![vec![Some(1_000.0); 49]],
            ..field_grid((30.0, -90.0), (7, 7), 2.5, &t, &[])
        };
        assert_eq!(find_lows(&flat, 0), NONE);
        // The background alone falls to the south, with no minimum inside.
        let gradient = field_grid((-20.0, -90.0), (7, 7), 2.5, &t, &[]);
        assert_eq!(find_lows(&gradient, 0), NONE);
        // A deep dip whose centre stays above the pressure limit.
        let high = field_grid(
            (70.0, -90.0),
            (7, 7),
            2.5,
            &t,
            &[still_low(77.5, -82.5, 1_009.0, 350.0)],
        );
        assert_eq!(find_lows(&high, 0), NONE);
        assert_eq!(find_lows(&flat, 1), NONE);
    }

    #[test]
    fn a_point_must_lie_below_its_neighbours_mean() {
        let t = [grid_times()[0]];
        let dip = |neighbours: f32| {
            let mut grid = PressureGrid {
                pressure_hpa: vec![vec![Some(neighbours); 9]],
                ..field_grid((30.0, -90.0), (3, 3), 2.5, &t, &[])
            };
            grid.pressure_hpa[0][4] = Some(1_000.0);
            find_lows(&grid, 0)
        };
        assert_eq!(dip(1_000.5), NONE);
        assert_eq!(dip(1_000.0 + MIN_DEPTH_HPA - 0.01), NONE);
        // Accepted, although it is not 2 hPa below its neighbours.
        let lows = dip(1_001.5);
        assert_eq!(lows.len(), 1, "{lows:?}");
        assert!((lows[0].latitude - 32.5).abs() < 1e-9 && (lows[0].longitude + 87.5).abs() < 1e-9);
    }

    #[test]
    fn edges_and_missing_values_never_qualify() {
        let t = [grid_times()[0]];
        // The centre on the grid's west edge.
        let edge = field_grid(
            (30.0, -75.0),
            (9, 9),
            2.5,
            &t,
            &[still_low(40.0, -75.0, 980.0, 350.0)],
        );
        assert_eq!(find_lows(&edge, 0), NONE);
        let mut gap = field_grid(
            (30.0, -90.0),
            (9, 9),
            2.5,
            &t,
            &[still_low(40.0, -80.0, 980.0, 350.0)],
        );
        let i = gap.index(4, 5);
        gap.pressure_hpa[0][i] = None;
        assert_eq!(find_lows(&gap, 0), NONE);
        gap.pressure_hpa[0][i] = Some(f32::NAN);
        assert_eq!(find_lows(&gap, 0), NONE);
    }

    #[test]
    fn two_equal_lowest_points_give_one_low() {
        let t = [grid_times()[0]];
        let mut grid = PressureGrid {
            pressure_hpa: vec![vec![Some(1_010.0); 49]],
            ..field_grid((30.0, -90.0), (7, 7), 2.5, &t, &[])
        };
        for (r, c) in [(3, 2), (3, 3)] {
            let i = grid.index(r, c);
            grid.pressure_hpa[0][i] = Some(1_000.0);
        }
        let lows = find_lows(&grid, 0);
        assert_eq!(lows.len(), 1, "{lows:?}");
        // The parabola is flat toward the tie, so the fit leans halfway.
        assert!(
            (lows[0].longitude - (-85.0 + 0.5 * 2.5)).abs() < 1e-6,
            "{lows:?}"
        );
    }

    #[test]
    fn a_refined_centre_wraps_across_the_antimeridian() {
        let t = [grid_times()[0]];
        let low = still_low(50.0, -179.2, 980.0, 350.0);
        let grid = field_grid((40.0, 170.0), (9, 9), 2.5, &t, &[low]);
        assert!(grid.lons.contains(&-180.0) && grid.lons.contains(&177.5));
        let lows = find_lows(&grid, 0);
        assert_eq!(lows.len(), 1, "{lows:?}");
        let km = distance_km((lows[0].latitude, lows[0].longitude), (50.0, -179.2));
        assert!(
            km < 50.0 && (-180.0..180.0).contains(&lows[0].longitude),
            "{lows:?}"
        );
        let low = still_low(50.0, 179.2, 980.0, 350.0);
        let grid = field_grid((40.0, 170.0), (9, 9), 2.5, &t, &[low]);
        let lows = find_lows(&grid, 0);
        assert!(lows.len() == 1 && lows[0].longitude > 178.0, "{lows:?}");
    }

    /// A low moving at `speed_kmh` toward `bearing_deg`, deepest (`central_hpa`)
    /// at `at`.
    fn moving_low(
        (latitude, longitude): (f32, f32),
        at: UnixTime,
        central_hpa: f32,
        speed_kmh: f32,
        bearing_deg: f32,
    ) -> SyntheticLow {
        SyntheticLow {
            latitude,
            longitude,
            at,
            central_hpa,
            radius_km: 400.0,
            speed_kmh,
            bearing_deg,
            lifetime_h: 30.0,
        }
    }

    /// Whether `(lat, lon)` lies at least one cell inside `grid`'s edges.
    fn inside(grid: &PressureGrid, (lat, lon): (f64, f64)) -> bool {
        let (rows, cols) = (grid.lats.len(), grid.lons.len());
        let west = f64::from(grid.lons[1]);
        let east_of_west = wrap_longitude(lon - west);
        let width = wrap_longitude(f64::from(grid.lons[cols - 2]) - west);
        (f64::from(grid.lats[1])..=f64::from(grid.lats[rows - 2])).contains(&lat)
            && (0.0..=width).contains(&east_of_west)
    }

    /// The worst distance in km between `track` and the field minimum of
    /// `low`, checking that the track has a point within 50 km at every step
    /// where that minimum is inside the grid and under [`MAX_LOW_HPA`].
    fn worst_error(grid: &PressureGrid, track: &Track, low: &SyntheticLow) -> f64 {
        let mut worst = 0.0_f64;
        for t in &grid.times {
            let truth = field_min(&[*low], *t, low.centre(*t));
            let truth_hpa = pressure_field(&[*low], *t, truth.0 as f32, truth.1 as f32);
            if !inside(grid, truth) || truth_hpa >= MAX_LOW_HPA {
                continue;
            }
            let point = track.points.iter().find(|p| p.time == *t);
            let km = point.map_or(f64::INFINITY, |p| distance_km(position(p), truth));
            assert!(km < 50.0, "{t:?}: {point:?} vs {truth:?}");
            worst = worst.max(km);
        }
        worst
    }

    #[test]
    fn a_fast_low_is_followed_on_and_off_the_grid_points() {
        let times = &grid_times()[..9];
        let mut worst = 0.0_f64;
        // Deepest on a grid point, midway between four, and off both.
        for centre in [(40.0, -80.0), (41.25, -78.75), (40.6, -79.4)] {
            // 600 km per 6 h toward the east-northeast.
            let low = moving_low(centre, times[4], 975.0, 100.0, 60.0);
            let grid = field_grid((25.0, -110.0), (13, 25), 2.5, times, &[low]);
            let tracks = track_lows(&grid);
            assert_eq!(tracks.len(), 1, "{centre:?}: {tracks:?}");
            assert!(tracks[0].points.len() >= 7, "{centre:?}: {tracks:?}");
            assert!(tracks[0].points.windows(2).all(|w| w[0].time < w[1].time));
            worst = worst.max(worst_error(&grid, &tracks[0], &low));
        }
        // About 8 km at worst.
        assert!(worst < 25.0, "{worst} km");
    }

    #[test]
    fn the_reach_scales_with_the_time_between_steps() {
        // 1,200 km between steps: too far 6 hours apart, near enough 12.
        let six_hourly = &grid_times()[..5];
        let low = moving_low((40.0, -80.0), six_hourly[2], 975.0, 200.0, 90.0);
        let grid = field_grid((30.0, -120.0), (9, 33), 2.5, six_hourly, &[low]);
        assert_eq!(track_lows(&grid), []);
        let twelve_hourly: Vec<UnixTime> = (0..5)
            .map(|k| UnixTime(six_hourly[0].0 + k * 12 * 3_600))
            .collect();
        let low = SyntheticLow {
            at: twelve_hourly[2],
            speed_kmh: 100.0,
            ..low
        };
        let grid = field_grid((30.0, -120.0), (9, 33), 2.5, &twelve_hourly, &[low]);
        let tracks = track_lows(&grid);
        assert_eq!(tracks.len(), 1, "{tracks:?}");
        assert!(tracks[0].points.len() >= 3);
    }

    #[test]
    fn parallel_lows_keep_their_own_tracks() {
        let times = &grid_times()[..7];
        // 1,500 km apart along their path, both moving east 600 km per 6 h:
        // the leading low's last position is 900 km from where the trailing
        // one arrives.
        let west = moving_low((45.0, -95.0), times[3], 980.0, 100.0, 90.0);
        let degrees = 1_500.0 / (111.195 * libm::cosf(45.0_f32.to_radians()));
        let east = SyntheticLow {
            longitude: west.longitude + degrees,
            ..west
        };
        let grid = field_grid((35.0, -125.0), (9, 37), 2.5, times, &[west, east]);
        let tracks = track_lows(&grid);
        assert_eq!(tracks.len(), 2, "{tracks:?}");
        let mut own = [false; 2];
        for track in &tracks {
            let low = if track.points[0].longitude < -100.0 {
                west
            } else {
                east
            };
            let i = usize::from(low.longitude > west.longitude);
            own[i] = true;
            assert!(track.points.len() >= 5, "{track:?}");
            for p in &track.points {
                let km = distance_km(position(p), field_min(&[low], p.time, low.centre(p.time)));
                assert!(km < 50.0, "{p:?}");
            }
        }
        assert_eq!(own, [true, true]);
    }

    #[test]
    fn flat_fields_give_no_tracks() {
        let times = &grid_times()[..8];
        let flat = PressureGrid {
            pressure_hpa: vec![vec![Some(1_000.0); 49]; 8],
            ..field_grid((30.0, -90.0), (7, 7), 2.5, times, &[])
        };
        assert_eq!(track_lows(&flat), []);
        assert_eq!(
            track_lows(&field_grid((30.0, -90.0), (7, 7), 2.5, times, &[])),
            []
        );
        assert_eq!(track_lows(&PressureGrid::default()), []);
    }

    #[test]
    fn a_low_crossing_the_antimeridian_keeps_one_track() {
        let times = &grid_times()[..11];
        // 300 km per 6 h due east, deepest just west of 180°.
        let low = moving_low((50.3, 179.3), times[5], 975.0, 50.0, 90.0);
        let grid = field_grid((40.0, 150.0), (9, 25), 2.5, times, &[low]);
        assert!(grid.lons.contains(&-180.0) && grid.lons.contains(&177.5));
        let tracks = track_lows(&grid);
        assert_eq!(tracks.len(), 1, "{tracks:?}");
        let points = &tracks[0].points;
        assert!(points.len() >= 9, "{points:?}");
        assert!(points.iter().any(|p| p.longitude > 170.0));
        assert!(points.iter().any(|p| p.longitude < -170.0));
        assert!(
            points
                .iter()
                .all(|p| (-180.0..180.0).contains(&p.longitude))
        );
        for w in points.windows(2) {
            let km = distance_km(position(&w[0]), position(&w[1]));
            assert!((200.0..400.0).contains(&km), "{w:?}");
        }
        let worst = worst_error(&grid, &tracks[0], &low);
        assert!(worst < 25.0, "{worst} km");
    }

    fn point(hour: i64, latitude: f64, longitude: f64, pressure_hpa: f32) -> TrackPoint {
        TrackPoint {
            time: UnixTime(hour * 3_600),
            latitude,
            longitude,
            pressure_hpa,
        }
    }

    fn track(points: &[TrackPoint]) -> Track {
        Track {
            model: "test".to_string(),
            points: points.to_vec(),
        }
    }

    #[test]
    fn the_lowest_point_and_the_closest_approach() {
        let t = track(&[
            point(0, 40.0, -76.0, 1_000.0),
            point(6, 40.0, -75.0, 990.0),
            point(12, 40.0, -74.0, 990.0),
        ]);
        assert_eq!(t.lowest().map(|p| p.time), Some(UnixTime(6 * 3_600)));
        let (p, km) = closest_approach(&t, (41.0, -74.0)).unwrap_or_else(|| panic!());
        assert_eq!(p.time, UnixTime(12 * 3_600));
        assert!((km - 111.195).abs() < 0.01, "{km}");
        assert_eq!(track(&[]).lowest(), None);
        assert_eq!(closest_approach(&track(&[]), (0.0, 0.0)), None);
    }

    #[test]
    fn the_storm_is_the_track_closest_within_the_window() {
        let place = (0.0, 0.0);
        let window = (UnixTime(12 * 3_600), UnixTime(36 * 3_600));
        // Passes over the place, but only before the window.
        let early = track(&[
            point(0, 0.0, 0.0, 980.0),
            point(6, 1.0, 4.0, 985.0),
            point(12, 2.0, 8.0, 990.0),
        ]);
        // 2.5° south of the place in the window.
        let south = track(&[
            point(12, -2.5, 0.0, 995.0),
            point(18, -2.5, 1.0, 995.0),
            point(24, -2.5, 2.0, 995.0),
        ]);
        // As far north, and deeper.
        let north = track(&[
            point(18, 2.5, 0.0, 985.0),
            point(24, 2.5, 1.0, 990.0),
            point(30, 2.5, 2.0, 995.0),
        ]);
        // Never in the window.
        let late = track(&[
            point(36, 0.0, 0.0, 970.0),
            point(42, 0.0, 1.0, 975.0),
            point(48, 0.0, 2.0, 980.0),
        ]);
        let tracks = [early.clone(), south.clone(), late.clone()];
        assert_eq!(storm_track(&tracks, place, window), Some(1));
        let tracks = [early, south.clone(), north, late.clone()];
        assert_eq!(storm_track(&tracks, place, window), Some(2));
        // An exact tie in both goes to the earlier track.
        assert_eq!(storm_track(&[south.clone(), south], place, window), Some(0));
        assert_eq!(storm_track(&[late], place, window), None);
        assert_eq!(storm_track(&[], place, window), None);
    }

    #[test]
    fn the_storm_comes_first_then_the_deepest_others() {
        let tracks: Vec<Track> = [1_000.0, 980.0, 990.0, 975.0, 995.0]
            .iter()
            .map(|hpa| track(&[point(0, 40.0, -74.0, *hpa)]))
            .collect();
        let depths = |kept: &[(Track, bool)]| -> Vec<(f32, bool)> {
            kept.iter()
                .map(|(t, s)| (t.points[0].pressure_hpa, *s))
                .collect()
        };
        assert_eq!(
            depths(&keep_tracks(&tracks, Some(4), 2)),
            [(995.0, true), (975.0, false), (980.0, false)]
        );
        assert_eq!(
            depths(&keep_tracks(&tracks, None, 2)),
            [(975.0, false), (980.0, false)]
        );
        assert_eq!(depths(&keep_tracks(&tracks, Some(9), 1)), [(975.0, false)]);
        assert_eq!(depths(&keep_tracks(&tracks, Some(3), 0)), [(975.0, true)]);
        assert_eq!(keep_tracks(&[], Some(0), 2), []);
    }

    #[test]
    fn noreaster_storm_tracks_follow_the_models() {
        let outlook = noreaster();
        let place = (outlook.place.latitude, outlook.place.longitude);
        let window = noreaster_window();
        let mut approach = Vec::new();
        for grid in &outlook.grids {
            let tracks = track_lows(grid);
            let storm = storm_track(&tracks, place, window);
            let Some(storm) = storm else {
                panic!("{}: no storm in {tracks:?}", grid.model);
            };
            let track = &tracks[storm];
            assert_eq!(track.model, grid.model);
            let (_, km) = closest_approach(track, place).unwrap_or_else(|| panic!());
            approach.push(km);
        }
        // The sample's ECMWF low passes about 88 km from the place and its
        // GFS low about 320 km; GEM and ICON fall between.
        assert!(approach[0] < 150.0 && approach[1] > 250.0, "{approach:?}");
        assert!((approach[0] - 88.0).abs() < 25.0, "{approach:?}");
        assert!((approach[1] - 320.0).abs() < 40.0, "{approach:?}");
        assert!(
            approach[2..]
                .iter()
                .all(|km| (approach[0]..approach[1]).contains(km))
        );
    }

    #[test]
    fn a_dry_week_has_no_tracks() {
        for grid in &dry_week().grids {
            assert_eq!(track_lows(grid), [], "{}", grid.model);
        }
    }
}
