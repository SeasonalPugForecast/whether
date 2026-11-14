//! Finding lows on a pressure grid.
//!
//! [`find_lows`] picks the grid points that are local pressure minima at one
//! time step and refines each to a position between grid points.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storm::testdata::{SyntheticLow, grid_times, pressure_field};
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
}
