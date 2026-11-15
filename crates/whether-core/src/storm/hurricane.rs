//! Tropical cyclones from the US National Hurricane Center: the advisory's
//! position and strength, the official forecast track, the guidance models'
//! tracks, the past track, and the forecast cone built from the official
//! track.
//!
//! Positions are `(latitude, longitude)` in degrees; winds are in knots and
//! pressures in hectopascals, as NHC publishes them. Only `view::storms`
//! converts and formats them.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use crate::storm::analysis::EARTH_RADIUS_KM;
use crate::storm::wrap_longitude;
use crate::time::UnixTime;

/// Kilometres in a nautical mile.
pub const KM_PER_NMI: f64 = 1.852;

/// The Earth's mean radius in nautical miles.
const EARTH_RADIUS_NMI: f64 = EARTH_RADIUS_KM / KM_PER_NMI;

/// The ATCF code of NHC's official forecast.
pub const OFFICIAL_AID: &str = "OFCL";

/// The guidance models drawn besides the official forecast, as ATCF aid
/// codes in display order: GFS, the Canadian GEM, ECMWF (interpolated, then
/// the raw run when only that is published), UKMET, HAFS-A and the track
/// consensus.
///
/// The interpolated aids (ending in `I`) are shifted to start at the
/// advisory's position, so they line up with the official track. ATCF's
/// `ICON` is an intensity consensus, not DWD's ICON model, and is not drawn.
pub const DRAWN_AIDS: [&str; 7] = ["AVNI", "CMCI", "EMXI", "EMX", "UKXI", "HFAI", "TVCN"];

/// The NHC basin a cyclone is in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum Basin {
    /// The North Atlantic, the Caribbean and the Gulf (`al`).
    #[default]
    Atlantic,
    /// The eastern North Pacific (`ep`).
    EastPacific,
    /// The central North Pacific (`cp`).
    CentralPacific,
}

impl Basin {
    /// The basin of an NHC storm id such as `al102026` (or ATCF's `AL`),
    /// from its first two letters; `None` for other basins.
    pub fn of_id(id: &str) -> Option<Self> {
        let prefix = id.get(..2)?;
        if prefix.eq_ignore_ascii_case("al") {
            Some(Self::Atlantic)
        } else if prefix.eq_ignore_ascii_case("ep") {
            Some(Self::EastPacific)
        } else if prefix.eq_ignore_ascii_case("cp") {
            Some(Self::CentralPacific)
        } else {
            None
        }
    }

    /// The basin's id prefix: `al`, `ep` or `cp`.
    pub fn code(self) -> &'static str {
        match self {
            Self::Atlantic => "al",
            Self::EastPacific => "ep",
            Self::CentralPacific => "cp",
        }
    }
}

/// One position of a cyclone: observed (past track) or forecast.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TrackFix {
    /// When the centre is there.
    pub time: UnixTime,
    /// Latitude in degrees.
    pub latitude: f64,
    /// Longitude in degrees, in `-180.0..180.0`.
    pub longitude: f64,
    /// The maximum sustained wind, in knots, when given.
    pub wind_kt: Option<u16>,
    /// The central sea-level pressure, in hPa, when given.
    pub pressure_hpa: Option<f32>,
}

impl TrackFix {
    /// The fix's position as `(latitude, longitude)`.
    pub fn position(&self) -> (f64, f64) {
        (self.latitude, self.longitude)
    }
}

/// One active cyclone: NHC's latest advisory with its tracks.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Hurricane {
    /// NHC's id: basin, number and year, e.g. `al102026`.
    pub id: String,
    /// The name, e.g. "Josephine", or the number for an unnamed depression.
    pub name: String,
    /// NHC's classification code: `HU` (hurricane), `TS` (tropical storm),
    /// `TD` (tropical depression), `STS` and `STD` (subtropical storm and
    /// depression), `PTC` (post-tropical cyclone), `PC` (potential tropical
    /// cyclone) or `TY` (typhoon).
    pub classification: String,
    /// The basin.
    pub basin: Basin,
    /// The maximum sustained wind, in knots.
    pub wind_kt: Option<u16>,
    /// The central pressure, in hPa.
    pub pressure_hpa: Option<f32>,
    /// The centre's latitude, in degrees.
    pub latitude: f64,
    /// The centre's longitude, in degrees, in `-180.0..180.0`.
    pub longitude: f64,
    /// Where the centre is heading, in degrees clockwise from north.
    pub movement_deg: Option<f32>,
    /// How fast the centre moves, in knots.
    pub movement_kt: Option<f32>,
    /// When the latest public advisory was issued.
    pub advisory_time: UnixTime,
    /// The latest public advisory's web page.
    pub advisory_url: Option<String>,
    /// NHC's official forecast track, from the advisory's position on.
    #[cfg_attr(feature = "serde", serde(default))]
    pub official: Vec<TrackFix>,
    /// The guidance models' latest tracks by ATCF aid code (see
    /// [`DRAWN_AIDS`]).
    #[cfg_attr(feature = "serde", serde(default))]
    pub aids: BTreeMap<String, Vec<TrackFix>>,
    /// The observed (best) track, oldest first.
    #[cfg_attr(feature = "serde", serde(default))]
    pub past: Vec<TrackFix>,
}

/// The season [`CONE_RADII_NMI`] is for.
pub const CONE_SEASON: u16 = 2026;

/// Where [`CONE_RADII_NMI`] comes from.
pub const CONE_SOURCE: &str = "https://www.nhc.noaa.gov/aboutcone.shtml";

/// The radius of NHC's forecast cone circles for the [`CONE_SEASON`]
/// season, in nautical miles, by forecast hour: `(hour, Atlantic, eastern
/// and central Pacific)`. NHC sets them each spring so that two thirds of
/// the official forecast errors of the previous five seasons fall inside;
/// update them with each season's values.
pub const CONE_RADII_NMI: [(u16, u16, u16); 8] = [
    (12, 25, 25),
    (24, 39, 37),
    (36, 49, 48),
    (48, 62, 56),
    (60, 77, 66),
    (72, 95, 78),
    (96, 134, 106),
    (120, 200, 138),
];

/// The radius at the advisory's position (hour 0), in nautical miles, so
/// the cone starts as a narrow cap around the current position.
pub const CONE_START_RADIUS_NMI: f64 = 10.0;

/// The cone's circle radius at `hours` into the forecast, in nautical miles:
/// [`CONE_START_RADIUS_NMI`] at hour 0, then linear between the hours of
/// [`CONE_RADII_NMI`] and its last radius beyond them.
pub fn cone_radius_nmi(hours: f64, basin: Basin) -> f64 {
    let radius = |row: &(u16, u16, u16)| match basin {
        Basin::Atlantic => f64::from(row.1),
        Basin::EastPacific | Basin::CentralPacific => f64::from(row.2),
    };
    let mut prev = (0.0, CONE_START_RADIUS_NMI);
    for row in &CONE_RADII_NMI {
        let (h, r) = (f64::from(row.0), radius(row));
        if hours <= h {
            let span = h - prev.0;
            let f = ((hours - prev.0) / span).clamp(0.0, 1.0);
            return prev.1 + f * (r - prev.1);
        }
        prev = (h, r);
    }
    prev.1
}

/// An azimuthal equidistant projection around a centre, in nautical miles:
/// distances and bearings from the centre are exact, and shapes within a
/// few thousand miles of it are distorted by a few percent at most.
#[derive(Clone, Copy, Debug)]
struct Projection {
    sin_lat0: f64,
    cos_lat0: f64,
    lon0: f64,
}

impl Projection {
    fn new(centre: (f64, f64)) -> Self {
        let lat0 = centre.0.to_radians();
        Self {
            sin_lat0: libm::sin(lat0),
            cos_lat0: libm::cos(lat0),
            lon0: centre.1,
        }
    }

    /// `(x east, y north)` in nautical miles.
    fn forward(&self, point: (f64, f64)) -> (f64, f64) {
        let lat = point.0.to_radians();
        let dlon = wrap_longitude(point.1 - self.lon0).to_radians();
        let (sin_lat, cos_lat) = (libm::sin(lat), libm::cos(lat));
        let cos_c =
            (self.sin_lat0 * sin_lat + self.cos_lat0 * cos_lat * libm::cos(dlon)).clamp(-1.0, 1.0);
        let c = libm::acos(cos_c);
        let k = if c < 1e-9 { 1.0 } else { c / libm::sin(c) };
        let x = k * cos_lat * libm::sin(dlon);
        let y = k * (self.cos_lat0 * sin_lat - self.sin_lat0 * cos_lat * libm::cos(dlon));
        (x * EARTH_RADIUS_NMI, y * EARTH_RADIUS_NMI)
    }

    /// The `(latitude, longitude)` of plane point `(x, y)`.
    fn inverse(&self, (x, y): (f64, f64)) -> (f64, f64) {
        let rho = libm::hypot(x, y);
        let lat0 = libm::atan2(self.sin_lat0, self.cos_lat0);
        if rho < 1e-9 {
            return (lat0.to_degrees(), wrap_longitude(self.lon0));
        }
        let c = rho / EARTH_RADIUS_NMI;
        let (sin_c, cos_c) = (libm::sin(c), libm::cos(c));
        let lat =
            libm::asin((cos_c * self.sin_lat0 + y * sin_c * self.cos_lat0 / rho).clamp(-1.0, 1.0));
        let dlon = libm::atan2(
            x * sin_c,
            rho * self.cos_lat0 * cos_c - y * self.sin_lat0 * sin_c,
        );
        (
            lat.to_degrees(),
            wrap_longitude(self.lon0 + dlon.to_degrees()),
        )
    }
}

/// A circle moving in a straight line from `a` (radius `ra`) to `b` (radius
/// `rb`), in plane coordinates. The area it sweeps is the convex hull of the
/// two end circles.
#[derive(Clone, Copy, Debug)]
struct Swept {
    a: (f64, f64),
    ra: f64,
    b: (f64, f64),
    rb: f64,
}

impl Swept {
    /// How far `p` is outside the circle at fraction `f` of the way:
    /// negative inside it.
    fn gap_at(&self, p: (f64, f64), f: f64) -> f64 {
        let c = (
            self.a.0 + f * (self.b.0 - self.a.0),
            self.a.1 + f * (self.b.1 - self.a.1),
        );
        libm::hypot(p.0 - c.0, p.1 - c.1) - (self.ra + f * (self.rb - self.ra))
    }

    /// The smallest [`Self::gap_at`] over the way and the fraction where it
    /// is. The gap is convex in the fraction, so this is also the signed
    /// distance from `p` to the swept area wherever it is positive.
    fn nearest(&self, p: (f64, f64)) -> (f64, f64) {
        let delta = (self.b.0 - self.a.0, self.b.1 - self.a.1);
        let len = libm::hypot(delta.0, delta.1);
        let ends = || {
            let (g0, g1) = (self.gap_at(p, 0.0), self.gap_at(p, 1.0));
            if g1 < g0 { (g1, 1.0) } else { (g0, 0.0) }
        };
        if len < 1e-9 {
            return ends();
        }
        // Radius change per mile travelled; at 1 or more one end circle
        // contains the other, and the nearest circle is at an end.
        let k = (self.rb - self.ra) / len;
        if libm::fabs(k) >= 1.0 {
            return ends();
        }
        let dir = (delta.0 / len, delta.1 / len);
        let rel = (p.0 - self.a.0, p.1 - self.a.1);
        let along = rel.0 * dir.0 + rel.1 * dir.1;
        let across = libm::fabs(rel.0 * dir.1 - rel.1 * dir.0);
        // Where d/dτ (|p − c(τ)| − r(τ)) is zero, τ being the distance along.
        let tau = (along + k * across / libm::sqrt(1.0 - k * k)).clamp(0.0, len);
        let frac = tau / len;
        (self.gap_at(p, frac), frac)
    }
}

/// The official track's fixes with their cone radii: `(position, radius in
/// n mi, time)`, ascending in time, skipping fixes that are not finite.
fn cone_fixes(official: &[TrackFix], basin: Basin) -> Vec<((f64, f64), f64, UnixTime)> {
    let mut fixes: Vec<&TrackFix> = official
        .iter()
        .filter(|f| f.latitude.is_finite() && f.longitude.is_finite())
        .collect();
    fixes.sort_by_key(|f| f.time);
    fixes.dedup_by_key(|f| f.time);
    let Some(first) = fixes.first().map(|f| f.time) else {
        return Vec::new();
    };
    fixes
        .iter()
        .map(|f| {
            let hours = (f.time.0 - first.0) as f64 / 3_600.0;
            (f.position(), cone_radius_nmi(hours, basin), f.time)
        })
        .collect()
}

/// The swept circles between consecutive fixes, in the plane of `proj`;
/// one still circle for a single fix.
fn swept_segments(fixes: &[((f64, f64), f64, UnixTime)], proj: &Projection) -> Vec<Swept> {
    let plane: Vec<((f64, f64), f64)> = fixes
        .iter()
        .map(|(p, r, _)| (proj.forward(*p), *r))
        .collect();
    match plane.as_slice() {
        [] => Vec::new(),
        [(a, ra)] => alloc::vec![Swept {
            a: *a,
            ra: *ra,
            b: *a,
            rb: *ra,
        }],
        _ => plane
            .windows(2)
            .map(|w| Swept {
                a: w[0].0,
                ra: w[0].1,
                b: w[1].0,
                rb: w[1].1,
            })
            .collect(),
    }
}

/// The point halfway between the fixes' extremes, as a projection centre.
fn track_centre(fixes: &[((f64, f64), f64, UnixTime)]) -> (f64, f64) {
    let first_lon = fixes.first().map_or(0.0, |f| f.0.1);
    let (mut lat_lo, mut lat_hi) = (f64::INFINITY, f64::NEG_INFINITY);
    let (mut lon_lo, mut lon_hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for ((lat, lon), _, _) in fixes {
        // Longitudes relative to the first fix, so a track across the
        // antimeridian stays together.
        let lon = first_lon + wrap_longitude(lon - first_lon);
        lat_lo = lat_lo.min(*lat);
        lat_hi = lat_hi.max(*lat);
        lon_lo = lon_lo.min(lon);
        lon_hi = lon_hi.max(lon);
    }
    (
        f64::midpoint(lat_lo, lat_hi),
        wrap_longitude(f64::midpoint(lon_lo, lon_hi)),
    )
}

/// The finest grid step the cone outline uses, in nautical miles.
const MIN_CONE_STEP_NMI: f64 = 2.0;
/// The coarsest grid step: no more than the start radius, so the narrow end
/// of the cone keeps its shape.
const MAX_CONE_STEP_NMI: f64 = CONE_START_RADIUS_NMI;
/// About this many grid cells span the cone's longer side.
const CONE_CELLS: f64 = 160.0;
/// The outline is simplified until no dropped point is further than this
/// from it, in nautical miles.
const CONE_TOLERANCE_NMI: f64 = 1.0;

/// NHC's forecast cone around the official track `official`: the outline of
/// the area swept by circles centred on the track, with radii from
/// [`cone_radius_nmi`] (the first fix being hour 0).
///
/// Between two fixes the circle moves in a straight line and its radius
/// changes linearly, so each stretch sweeps the convex hull of its two end
/// circles. The union is outlined on a grid in an azimuthal equidistant
/// projection around the track (marching squares on the distance to the
/// swept area), then simplified to within [`CONE_TOLERANCE_NMI`].
///
/// The ring is `(latitude, longitude)` points, counter-clockwise, not
/// closed (the last point joins the first), with longitudes in
/// `-180.0..180.0`. A track that loops back on itself can enclose a hole;
/// only the outer outline is returned. Empty without fixes.
pub fn cone(official: &[TrackFix], basin: Basin) -> Vec<(f64, f64)> {
    let fixes = cone_fixes(official, basin);
    if fixes.is_empty() {
        return Vec::new();
    }
    let proj = Projection::new(track_centre(&fixes));
    let segments = swept_segments(&fixes, &proj);
    let r_max = fixes.iter().map(|f| f.1).fold(0.0, f64::max);
    let (mut x_lo, mut x_hi) = (f64::INFINITY, f64::NEG_INFINITY);
    let (mut y_lo, mut y_hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for s in &segments {
        for p in [s.a, s.b] {
            x_lo = x_lo.min(p.0);
            x_hi = x_hi.max(p.0);
            y_lo = y_lo.min(p.1);
            y_hi = y_hi.max(p.1);
        }
    }
    let span = (x_hi - x_lo).max(y_hi - y_lo) + 2.0 * r_max;
    let step = (span / CONE_CELLS).clamp(MIN_CONE_STEP_NMI, MAX_CONE_STEP_NMI);
    let margin = r_max + 2.0 * step;
    let grid = Grid {
        x0: x_lo - margin,
        y0: y_lo - margin,
        step,
        nx: ((x_hi - x_lo + 2.0 * margin) / step) as usize + 2,
        ny: ((y_hi - y_lo + 2.0 * margin) / step) as usize + 2,
    };
    let values: Vec<f64> = (0..grid.ny)
        .flat_map(|j| (0..grid.nx).map(move |i| (i, j)))
        .map(|(i, j)| {
            let p = grid.point(i as f64, j as f64);
            segments
                .iter()
                .map(|s| s.nearest(p).0)
                .fold(f64::INFINITY, f64::min)
        })
        .collect();
    let Some(outline) = grid.outer_contour(&values) else {
        return Vec::new();
    };
    simplify_ring(&outline, CONE_TOLERANCE_NMI)
        .into_iter()
        .map(|p| proj.inverse(p))
        .collect()
}

/// A regular grid of nodes in the plane.
#[derive(Clone, Copy, Debug)]
struct Grid {
    x0: f64,
    y0: f64,
    step: f64,
    nx: usize,
    ny: usize,
}

/// One cell edge of a [`Grid`]: horizontal from node `(i, j)` to `(i + 1,
/// j)`, or vertical from `(i, j)` to `(i, j + 1)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Edge {
    Horizontal(usize, usize),
    Vertical(usize, usize),
}

impl Grid {
    fn point(&self, i: f64, j: f64) -> (f64, f64) {
        (self.x0 + i * self.step, self.y0 + j * self.step)
    }

    fn id(&self, edge: Edge) -> usize {
        match edge {
            Edge::Horizontal(i, j) => j * (self.nx - 1) + i,
            Edge::Vertical(i, j) => (self.nx - 1) * self.ny + j * self.nx + i,
        }
    }

    fn edge_count(&self) -> usize {
        (self.nx - 1) * self.ny + self.nx * (self.ny - 1)
    }

    /// Where the zero of `values` lies on `edge`, by linear interpolation.
    fn crossing(&self, values: &[f64], edge: Edge) -> (f64, f64) {
        let ((i0, j0), (i1, j1)) = match edge {
            Edge::Horizontal(i, j) => ((i, j), (i + 1, j)),
            Edge::Vertical(i, j) => ((i, j), (i, j + 1)),
        };
        let (v0, v1) = (values[j0 * self.nx + i0], values[j1 * self.nx + i1]);
        let t = if libm::fabs(v0 - v1) < 1e-12 {
            0.5
        } else {
            (v0 / (v0 - v1)).clamp(0.0, 1.0)
        };
        let lerp = |a: usize, b: usize| a as f64 + t * (b as f64 - a as f64);
        self.point(lerp(i0, i1), lerp(j0, j1))
    }

    /// The contour segments of cell `(i, j)` where `values` is negative
    /// inside, as pairs of edges (marching squares; a saddle is resolved by
    /// the mean of the four corners).
    fn cell_segments(&self, values: &[f64], i: usize, j: usize, out: &mut Vec<[Edge; 2]>) {
        let v = |di: usize, dj: usize| values[(j + dj) * self.nx + i + di];
        let corners = [v(0, 0), v(1, 0), v(1, 1), v(0, 1)];
        let inside = corners.map(|c| c < 0.0);
        let case = inside
            .iter()
            .enumerate()
            .fold(0_u8, |acc, (k, inside)| acc | (u8::from(*inside) << k));
        let bottom = Edge::Horizontal(i, j);
        let right = Edge::Vertical(i + 1, j);
        let top = Edge::Horizontal(i, j + 1);
        let left = Edge::Vertical(i, j);
        let centre_inside = corners.iter().sum::<f64>() < 0.0;
        match case {
            0 | 15 => {}
            1 | 14 => out.push([left, bottom]),
            2 | 13 => out.push([bottom, right]),
            3 | 12 => out.push([left, right]),
            4 | 11 => out.push([right, top]),
            6 | 9 => out.push([bottom, top]),
            7 | 8 => out.push([left, top]),
            // Bottom-left and top-right inside.
            5 => {
                if centre_inside {
                    out.extend([[bottom, right], [top, left]]);
                } else {
                    out.extend([[left, bottom], [right, top]]);
                }
            }
            // Bottom-right and top-left inside.
            _ => {
                if centre_inside {
                    out.extend([[left, bottom], [right, top]]);
                } else {
                    out.extend([[bottom, right], [top, left]]);
                }
            }
        }
    }

    /// The longest closed contour of `values` around the area where they
    /// are negative, counter-clockwise; `None` when nothing is inside. The
    /// grid's border must be outside.
    fn outer_contour(&self, values: &[f64]) -> Option<Vec<(f64, f64)>> {
        if self.nx < 2 || self.ny < 2 || values.len() != self.nx * self.ny {
            return None;
        }
        let mut segments: Vec<[Edge; 2]> = Vec::new();
        for j in 0..self.ny - 1 {
            for i in 0..self.nx - 1 {
                self.cell_segments(values, i, j, &mut segments);
            }
        }
        // Each crossed edge borders two cells, so two segments meet on it.
        let mut on_edge = alloc::vec![[usize::MAX; 2]; self.edge_count()];
        for (k, seg) in segments.iter().enumerate() {
            for edge in seg {
                let slot = &mut on_edge[self.id(*edge)];
                if slot[0] == usize::MAX {
                    slot[0] = k;
                } else {
                    slot[1] = k;
                }
            }
        }
        let mut used = alloc::vec![false; segments.len()];
        let mut best: Option<(f64, Vec<(f64, f64)>)> = None;
        for start in 0..segments.len() {
            if used[start] {
                continue;
            }
            let first_edge = segments[start][0];
            let mut ring = Vec::new();
            let (mut seg, mut edge) = (start, first_edge);
            let mut closed = false;
            for _ in 0..segments.len() {
                used[seg] = true;
                ring.push(self.crossing(values, edge));
                let next_edge = if segments[seg][0] == edge {
                    segments[seg][1]
                } else {
                    segments[seg][0]
                };
                if next_edge == first_edge {
                    closed = true;
                    break;
                }
                let pair = on_edge[self.id(next_edge)];
                let Some(next) = pair.into_iter().find(|s| *s != seg && *s != usize::MAX) else {
                    break;
                };
                (seg, edge) = (next, next_edge);
            }
            if !closed || ring.len() < 3 {
                continue;
            }
            let area = signed_area(&ring);
            if best
                .as_ref()
                .is_none_or(|(a, _)| libm::fabs(area) > libm::fabs(*a))
            {
                if area < 0.0 {
                    ring.reverse();
                }
                best = Some((area, ring));
            }
        }
        best.map(|(_, ring)| ring)
    }
}

/// The shoelace area of a ring, positive counter-clockwise.
fn signed_area(ring: &[(f64, f64)]) -> f64 {
    let n = ring.len();
    (0..n)
        .map(|k| {
            let (a, b) = (ring[k], ring[(k + 1) % n]);
            a.0 * b.1 - b.0 * a.1
        })
        .sum::<f64>()
        / 2.0
}

/// How far `p` is from the segment `a`–`b`.
fn segment_distance(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let dir = (b.0 - a.0, b.1 - a.1);
    let len2 = dir.0 * dir.0 + dir.1 * dir.1;
    let frac = if len2 < 1e-18 {
        0.0
    } else {
        (((p.0 - a.0) * dir.0 + (p.1 - a.1) * dir.1) / len2).clamp(0.0, 1.0)
    };
    libm::hypot(p.0 - (a.0 + frac * dir.0), p.1 - (a.1 + frac * dir.1))
}

/// `ring` with points dropped (Douglas–Peucker) while none of them is more
/// than `tolerance` from the simplified outline.
fn simplify_ring(ring: &[(f64, f64)], tolerance: f64) -> Vec<(f64, f64)> {
    let n = ring.len();
    if n < 4 {
        return ring.to_vec();
    }
    // Split the ring at its first point and the point furthest from it.
    let far = (1..n)
        .max_by(|a, b| {
            let d = |k: usize| libm::hypot(ring[k].0 - ring[0].0, ring[k].1 - ring[0].1);
            d(*a).total_cmp(&d(*b))
        })
        .unwrap_or(n / 2);
    let mut keep = alloc::vec![false; n];
    keep[0] = true;
    keep[far] = true;
    let mut stack = alloc::vec![(0, far), (far, n)];
    while let Some((from, to)) = stack.pop() {
        let end = ring[to % n];
        let mut worst = (0.0, None);
        for (k, p) in ring.iter().enumerate().take(to).skip(from + 1) {
            let d = segment_distance(*p, ring[from], end);
            if d > worst.0 {
                worst = (d, Some(k));
            }
        }
        if let (d, Some(k)) = worst
            && d > tolerance
        {
            keep[k] = true;
            stack.push((from, k));
            stack.push((k, to));
        }
    }
    ring.iter()
        .zip(keep)
        .filter_map(|(p, k)| k.then_some(*p))
        .collect()
}

/// Whether `point` lies inside `ring` (both `(latitude, longitude)`; the
/// ring as [`cone`] returns it). Longitudes are taken relative to the point,
/// so a ring across the antimeridian works.
pub fn ring_contains(ring: &[(f64, f64)], point: (f64, f64)) -> bool {
    let n = ring.len();
    if n < 3 {
        return false;
    }
    let rel = |p: (f64, f64)| (wrap_longitude(p.1 - point.1), p.0 - point.0);
    let mut inside = false;
    for k in 0..n {
        let (a, b) = (rel(ring[k]), rel(ring[(k + 1) % n]));
        // Edges that straddle the antimeridian seen from the point are
        // skipped; they never cross the ray to the east of it.
        if libm::fabs(a.0 - b.0) > 180.0 {
            continue;
        }
        if (a.1 > 0.0) != (b.1 > 0.0) {
            let x = a.0 + (0.0 - a.1) * (b.0 - a.0) / (b.1 - a.1);
            if x > 0.0 {
                inside = !inside;
            }
        }
    }
    inside
}

/// Bisection steps when timing where a circle's edge passes a place.
const EDGE_STEPS: u32 = 48;

/// The fraction in `[lo, hi]` where `g` changes sign, `g(lo)` and `g(hi)`
/// having opposite signs.
fn root(g: impl Fn(f64) -> f64, mut lo: f64, mut hi: f64) -> f64 {
    let lo_inside = g(lo) <= 0.0;
    for _ in 0..EDGE_STEPS {
        let mid = f64::midpoint(lo, hi);
        if (g(mid) <= 0.0) == lo_inside {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    f64::midpoint(lo, hi)
}

/// When the official forecast puts `place` inside the cone, as `[start,
/// end]` intervals in time order: the times at which the place lies in the
/// circle the cone sweeps along the track (see [`cone`]).
pub fn cone_coverage(h: &Hurricane, place: (f64, f64)) -> Vec<(UnixTime, UnixTime)> {
    let fixes = cone_fixes(&h.official, h.basin);
    let proj = Projection::new(place);
    let segments = swept_segments(&fixes, &proj);
    let origin = (0.0, 0.0);
    let mut out: Vec<(UnixTime, UnixTime)> = Vec::new();
    for (k, s) in segments.iter().enumerate() {
        let (t0, t1) = match fixes.len() {
            1 => (fixes[0].2, fixes[0].2),
            _ => (fixes[k].2, fixes[k + 1].2),
        };
        let (gap, f_min) = s.nearest(origin);
        if gap > 0.0 {
            continue;
        }
        let g = |f: f64| s.gap_at(origin, f);
        let from = if g(0.0) <= 0.0 {
            0.0
        } else {
            root(g, 0.0, f_min)
        };
        let to = if g(1.0) <= 0.0 {
            1.0
        } else {
            root(g, f_min, 1.0)
        };
        let at = |f: f64| UnixTime(t0.0 + libm::round(f * (t1.0 - t0.0) as f64) as i64);
        let (start, end) = (at(from), at(to));
        match out.last_mut() {
            Some(last) if start.0 <= last.1.0 + 1 => last.1 = last.1.max(end),
            _ => out.push((start, end)),
        }
    }
    out
}

/// The first time from `now` to `now + horizon_secs` at which the official
/// forecast cone of `h` covers `place` (`(latitude, longitude)`): `now`
/// when the place is inside it already, `None` when the cone misses it in
/// that span.
pub fn hurricane_reaches(
    h: &Hurricane,
    place: (f64, f64),
    now: UnixTime,
    horizon_secs: i64,
) -> Option<UnixTime> {
    let until = now.0 + horizon_secs;
    cone_coverage(h, place)
        .into_iter()
        .find(|(start, end)| end.0 >= now.0 && start.0 <= until)
        .map(|(start, _)| start.max(now))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec;

    const HOUR: i64 = 3_600;
    const T0: UnixTime = UnixTime(1_790_000_000);

    /// A straight track due north along 70° W from 25° N at 15 kt, to 120 h.
    fn northbound() -> Vec<TrackFix> {
        [0, 12, 24, 36, 48, 60, 72, 96, 120]
            .into_iter()
            .map(|h| TrackFix {
                time: UnixTime(T0.0 + h * HOUR),
                latitude: 25.0 + 15.0 * h as f64 / 60.0,
                longitude: -70.0,
                wind_kt: Some(80),
                pressure_hpa: None,
            })
            .collect()
    }

    fn hurricane(official: Vec<TrackFix>) -> Hurricane {
        Hurricane {
            id: "al992026".to_string(),
            name: "Sample".to_string(),
            classification: "HU".to_string(),
            official,
            ..Hurricane::default()
        }
    }

    /// The point `nmi` nautical miles from `from` on `bearing` degrees.
    fn offset(from: (f64, f64), bearing: f64, nmi: f64) -> (f64, f64) {
        let b = bearing.to_radians();
        Projection::new(from).inverse((nmi * libm::sin(b), nmi * libm::cos(b)))
    }

    /// The cone's width through `centre` across a track heading north: the
    /// distance between the outline's crossings of the east–west line.
    fn width_across(ring: &[(f64, f64)], centre: (f64, f64)) -> f64 {
        let proj = Projection::new(centre);
        let plane: Vec<(f64, f64)> = ring.iter().map(|p| proj.forward(*p)).collect();
        let n = plane.len();
        let mut xs: Vec<f64> = (0..n)
            .filter_map(|k| {
                let (a, b) = (plane[k], plane[(k + 1) % n]);
                ((a.1 > 0.0) != (b.1 > 0.0)).then(|| a.0 + (0.0 - a.1) * (b.0 - a.0) / (b.1 - a.1))
            })
            .collect();
        xs.sort_by(f64::total_cmp);
        let west = xs
            .iter()
            .copied()
            .filter(|x| *x < 0.0)
            .fold(f64::NEG_INFINITY, f64::max);
        let east = xs
            .iter()
            .copied()
            .filter(|x| *x > 0.0)
            .fold(f64::INFINITY, f64::min);
        east - west
    }

    #[test]
    fn basins_come_from_ids() {
        assert_eq!(Basin::of_id("al102026"), Some(Basin::Atlantic));
        assert_eq!(Basin::of_id("EP092023"), Some(Basin::EastPacific));
        assert_eq!(Basin::of_id("cp012026"), Some(Basin::CentralPacific));
        assert_eq!(Basin::of_id("wp012026"), None);
        assert_eq!(Basin::of_id("a"), None);
        assert_eq!(Basin::EastPacific.code(), "ep");
    }

    #[test]
    fn radii_follow_the_season_table() {
        assert_eq!(cone_radius_nmi(0.0, Basin::Atlantic), CONE_START_RADIUS_NMI);
        assert_eq!(cone_radius_nmi(12.0, Basin::Atlantic), 25.0);
        assert_eq!(cone_radius_nmi(72.0, Basin::Atlantic), 95.0);
        assert_eq!(cone_radius_nmi(72.0, Basin::EastPacific), 78.0);
        assert_eq!(cone_radius_nmi(120.0, Basin::CentralPacific), 138.0);
        assert!((cone_radius_nmi(84.0, Basin::Atlantic) - 114.5).abs() < 1e-9);
        assert!((cone_radius_nmi(3.0, Basin::Atlantic) - 13.75).abs() < 1e-9);
        assert_eq!(cone_radius_nmi(150.0, Basin::Atlantic), 200.0);
        assert!(
            CONE_RADII_NMI
                .windows(2)
                .all(|w| w[0].0 < w[1].0 && w[0].1 < w[1].1 && w[0].2 < w[1].2)
        );
    }

    #[test]
    fn the_projection_round_trips() {
        let proj = Projection::new((40.0, -70.0));
        for p in [(40.0, -70.0), (25.0, -80.0), (55.0, -40.0), (10.0, -100.0)] {
            let back = proj.inverse(proj.forward(p));
            assert!(
                (back.0 - p.0).abs() < 1e-9 && (back.1 - p.1).abs() < 1e-9,
                "{p:?} {back:?}"
            );
        }
        // One degree of latitude is 60 n mi.
        let (x, y) = proj.forward((41.0, -70.0));
        assert!(x.abs() < 1e-9 && (y - 60.04).abs() < 0.05, "{x} {y}");
    }

    #[test]
    fn the_cone_holds_every_official_point() {
        let track = northbound();
        let ring = cone(&track, Basin::Atlantic);
        assert!(ring.len() > 20, "{}", ring.len());
        for fix in &track {
            assert!(ring_contains(&ring, fix.position()), "{fix:?}");
        }
        assert!(signed_area(&ring.iter().map(|p| (p.1, p.0)).collect::<Vec<_>>()) > 0.0);
        assert!(ring.iter().all(|p| (-180.0..180.0).contains(&p.1)));
    }

    #[test]
    fn the_cone_is_as_wide_as_its_circles() {
        let track = northbound();
        let ring = cone(&track, Basin::Atlantic);
        let at = |h: usize| track[h].position();
        // At 72 h the circle is 95 n mi; the tangent to the next, larger
        // circle widens it a little.
        let w72 = width_across(&ring, at(6));
        assert!((w72 - 2.0 * 95.0).abs() < 2.0 * 95.0 * 0.03, "{w72}");
        let w24 = width_across(&ring, at(2));
        assert!((w24 - 2.0 * 39.0).abs() < 2.0 * 39.0 * 0.05, "{w24}");
        let pacific = cone(&track, Basin::EastPacific);
        let w72 = width_across(&pacific, at(6));
        assert!((w72 - 2.0 * 78.0).abs() < 2.0 * 78.0 * 0.03, "{w72}");
    }

    #[test]
    fn places_off_the_track_are_outside() {
        let track = northbound();
        let ring = cone(&track, Basin::Atlantic);
        let at48 = track[4].position();
        assert!(!ring_contains(&ring, offset(at48, 90.0, 300.0)));
        assert!(!ring_contains(&ring, offset(at48, 270.0, 300.0)));
        assert!(ring_contains(&ring, offset(at48, 90.0, 55.0)));
        assert!(!ring_contains(&ring, offset(at48, 90.0, 70.0)));
        // Behind the start and past the end.
        assert!(!ring_contains(
            &ring,
            offset(track[0].position(), 180.0, 30.0)
        ));
        assert!(!ring_contains(
            &ring,
            offset(track[8].position(), 0.0, 230.0)
        ));
    }

    #[test]
    fn a_curving_track_keeps_one_outline() {
        // North, then a sharp turn east: a hook that the circles fill.
        let points = [
            (25.0, -75.0),
            (28.0, -75.5),
            (31.0, -75.0),
            (32.0, -71.0),
            (31.5, -66.0),
            (30.0, -62.0),
        ];
        let track: Vec<TrackFix> = points
            .iter()
            .zip([0, 12, 24, 48, 72, 96])
            .map(|(p, h)| TrackFix {
                time: UnixTime(T0.0 + h * HOUR),
                latitude: p.0,
                longitude: p.1,
                ..TrackFix::default()
            })
            .collect();
        let ring = cone(&track, Basin::Atlantic);
        for fix in &track {
            assert!(ring_contains(&ring, fix.position()), "{fix:?}");
        }
        assert!(!ring_contains(&ring, (28.0, -69.0)));
    }

    #[test]
    fn a_cone_across_the_antimeridian_wraps() {
        let track: Vec<TrackFix> = [(15.0, -176.0), (16.0, -179.0), (17.0, 178.0), (18.0, 175.0)]
            .iter()
            .zip([0, 24, 48, 72])
            .map(|(p, h)| TrackFix {
                time: UnixTime(T0.0 + h * HOUR),
                latitude: p.0,
                longitude: p.1,
                ..TrackFix::default()
            })
            .collect();
        let ring = cone(&track, Basin::CentralPacific);
        assert!(ring.iter().all(|p| (-180.0..180.0).contains(&p.1)));
        for fix in &track {
            assert!(ring_contains(&ring, fix.position()), "{fix:?}");
        }
        assert!(!ring_contains(&ring, (16.0, 170.0)));
    }

    #[test]
    fn short_tracks_still_have_a_cone() {
        assert_eq!(cone(&[], Basin::Atlantic), Vec::<(f64, f64)>::new());
        let one = &northbound()[..1];
        let ring = cone(one, Basin::Atlantic);
        assert!(ring_contains(&ring, one[0].position()));
        assert!(!ring_contains(&ring, offset(one[0].position(), 0.0, 12.0)));
        let bad = [TrackFix {
            latitude: f64::NAN,
            ..TrackFix::default()
        }];
        assert_eq!(cone(&bad, Basin::Atlantic), Vec::<(f64, f64)>::new());
    }

    #[test]
    fn the_cone_reaches_a_place_on_the_track_when_its_edge_does() {
        let h = hurricane(northbound());
        // On the track at 48 h, 180 n mi past the 36 h circle's centre: the
        // cone's edge reaches the place about 4 hours before the centre.
        let place = h.official[4].position();
        let reach = hurricane_reaches(&h, place, T0, 5 * 24 * HOUR).unwrap();
        let hours = (reach.0 - T0.0) as f64 / 3_600.0;
        assert!((43.0..47.0).contains(&hours), "{hours}");
        let spans = cone_coverage(&h, place);
        assert_eq!(spans.len(), 1);
        assert!(spans[0].1.0 > h.official[4].time.0);
        // Inside already: now.
        let later = UnixTime(T0.0 + 47 * HOUR);
        assert_eq!(hurricane_reaches(&h, place, later, HOUR), Some(later));
        // Too far ahead for the horizon, or passed.
        assert_eq!(hurricane_reaches(&h, place, T0, 24 * HOUR), None);
        assert_eq!(
            hurricane_reaches(&h, place, UnixTime(T0.0 + 100 * HOUR), 24 * HOUR),
            None
        );
    }

    #[test]
    fn places_beside_the_cone_are_not_reached() {
        let h = hurricane(northbound());
        let place = offset(h.official[4].position(), 90.0, 300.0);
        assert_eq!(hurricane_reaches(&h, place, T0, 10 * 24 * HOUR), None);
        // Inside the edge at 48 h (62 n mi) only around 48 h.
        let near = offset(h.official[4].position(), 90.0, 55.0);
        let spans = cone_coverage(&h, near);
        assert_eq!(spans.len(), 1);
        let (start, end) = spans[0];
        assert!(start.0 < h.official[4].time.0 && end.0 > h.official[4].time.0);
        assert!(ring_contains(&cone(&h.official, h.basin), near));
        let none = hurricane(Vec::new());
        assert_eq!(hurricane_reaches(&none, (0.0, 0.0), T0, HOUR), None);
    }

    #[test]
    fn rings_contain_by_crossings() {
        let square = [(0.0, 0.0), (0.0, 10.0), (10.0, 10.0), (10.0, 0.0)];
        assert!(ring_contains(&square, (5.0, 5.0)));
        assert!(!ring_contains(&square, (15.0, 5.0)));
        assert!(!ring_contains(&square[..2], (5.0, 5.0)));
        let across = [(0.0, 175.0), (0.0, -175.0), (10.0, -175.0), (10.0, 175.0)];
        assert!(ring_contains(&across, (5.0, 179.5)));
        assert!(ring_contains(&across, (5.0, -179.5)));
        assert!(!ring_contains(&across, (5.0, 170.0)));
    }

    #[test]
    fn simplifying_keeps_corners() {
        let mut ring = Vec::new();
        for k in 0..10 {
            ring.push((f64::from(k), 0.0));
        }
        for k in 0..10 {
            ring.push((10.0, f64::from(k)));
        }
        for k in 0..10 {
            ring.push((10.0 - f64::from(k), 10.0));
        }
        for k in 0..10 {
            ring.push((0.0, 10.0 - f64::from(k)));
        }
        let simple = simplify_ring(&ring, 0.1);
        assert_eq!(
            simple,
            vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn hurricanes_load_without_tracks() {
        let json = r#"{"id":"al102026","name":"Josephine","classification":"STS",
            "basin":"atlantic","wind_kt":55,"pressure_hpa":988.0,"latitude":39.5,
            "longitude":-66.8,"movement_deg":55.0,"movement_kt":11.0,
            "advisory_time":1792702800,"advisory_url":null}"#;
        let h: Hurricane = serde_json::from_str(json).unwrap();
        assert!(h.official.is_empty() && h.aids.is_empty() && h.past.is_empty());
        assert_eq!(h.basin, Basin::Atlantic);
        let back = serde_json::to_value(&h).unwrap();
        assert_eq!(back["basin"], "atlantic");
    }
}
