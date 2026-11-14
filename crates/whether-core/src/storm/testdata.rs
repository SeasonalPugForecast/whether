//! Synthetic storm outlooks for New York (ZIP 10001) in January, for tests in
//! this and other crates.
//!
//! Every value comes from a formula or a small linear congruential generator,
//! so each call returns the same outlook. Lows are Gaussian pressure dips that
//! move in a straight line and deepen and fill over a day or two; the place's
//! pressure is the same field sampled at the place, so the series and the
//! grids agree.

// Indices and member numbers here are small loop counters.
#![allow(clippy::cast_possible_wrap)]

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::model::ResolvedLocation;
use crate::storm::{
    DETERMINISTIC_MODELS, ENSEMBLE_MODELS, EnsembleSeries, ModelRun, PastRun, PointSeries,
    PressureGrid, StormOutlook, grid_axes, wrap_longitude,
};
use crate::time::{UnixTime, UtcOffset};

/// Where the series start: 2027-01-15 00:00 UTC (Friday).
pub const STORM_START: UnixTime = UnixTime(1_799_971_200);
/// The deterministic runs' initialisation: 2027-01-15 12:00 UTC ("12z").
pub const STORM_RUN_INIT: UnixTime = UnixTime(1_800_014_400);
/// The ensembles' initialisation: 2027-01-15 06:00 UTC ("06z").
pub const STORM_ENSEMBLE_INIT: UnixTime = UnixTime(1_800_014_400 - 6 * HOUR);
/// When the outlook was fetched: 2027-01-15 19:00 UTC (2 PM EST).
pub const STORM_NOW: UnixTime = UnixTime(1_800_039_600);
/// When the ECMWF low passes closest to the place: Sunday 2027-01-17
/// 18:00 UTC (1 PM EST).
pub const STORM_PEAK: UnixTime = UnixTime(1_800_208_800);
/// EST.
pub const STORM_OFFSET: UtcOffset = UtcOffset(-18_000);
/// Hourly steps in each point and ensemble series (10 days).
pub const STORM_HOURS: usize = 240;
/// Seconds between pressure grid steps.
pub const GRID_STEP_SECS: i64 = 6 * HOUR;
/// Pressure grid steps (10 days at 6 h).
pub const GRID_STEPS: usize = 40;
/// Members of the ECMWF ensemble, the control included.
pub const ECMWF_MEMBERS: usize = 51;
/// Members of the GFS ensemble (GEFS), the control included.
pub const GFS_MEMBERS: usize = 31;

const HOUR: i64 = 3_600;
const KM_PER_DEG: f32 = 111.195;

/// New York in winter (EST).
pub fn storm_location() -> ResolvedLocation {
    ResolvedLocation {
        name: "New York".to_string(),
        region: Some("New York, US".to_string()),
        latitude: 40.7143,
        longitude: -74.005_97,
        elevation_m: Some(71.0),
        utc_offset: STORM_OFFSET,
        tz_name: Some("America/New_York".to_string()),
        tz_abbrev: Some("EST".to_string()),
        station: None,
    }
}

/// The run of `model` (an Open-Meteo `models` id) in the sample outlooks,
/// tagged with its `meta.json` domain.
pub fn model_run(model: &str) -> ModelRun {
    let (domain, init, delay_h) = match model {
        "ecmwf_ifs025" => ("ecmwf_ifs025", STORM_RUN_INIT, 6),
        "gfs_seamless" => ("ncep_gfs025", STORM_RUN_INIT, 4),
        "gem_seamless" => ("cmc_gem_gdps", STORM_RUN_INIT, 5),
        "icon_seamless" => ("dwd_icon", STORM_RUN_INIT, 3),
        _ => (model, STORM_RUN_INIT, 6),
    };
    ModelRun {
        model: domain.to_string(),
        init,
        available: UnixTime(init.0 + delay_h * HOUR),
    }
}

/// The run of an ensemble (an Ensemble API `models` id).
pub fn ensemble_run(model: &str) -> ModelRun {
    let domain = match model {
        "ecmwf_ifs025" => "ecmwf_ifs025_ensemble",
        "gfs025" => "ncep_gefs025",
        other => other,
    };
    ModelRun {
        model: domain.to_string(),
        init: STORM_ENSEMBLE_INIT,
        available: UnixTime(STORM_ENSEMBLE_INIT.0 + 7 * HOUR),
    }
}

/// Hourly times from [`STORM_START`].
pub fn hourly_times() -> Vec<UnixTime> {
    (0..STORM_HOURS)
        .map(|h| UnixTime(STORM_START.0 + h as i64 * HOUR))
        .collect()
}

/// Pressure grid times from [`STORM_START`], [`GRID_STEP_SECS`] apart.
pub fn grid_times() -> Vec<UnixTime> {
    (0..GRID_STEPS)
        .map(|k| UnixTime(STORM_START.0 + k as i64 * GRID_STEP_SECS))
        .collect()
}

/// A low that moves in a straight line and deepens, then fills.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SyntheticLow {
    /// Centre latitude at `at`, in degrees.
    pub latitude: f32,
    /// Centre longitude at `at`, in degrees.
    pub longitude: f32,
    /// When the low is deepest.
    pub at: UnixTime,
    /// The central pressure at `at`, in hPa.
    pub central_hpa: f32,
    /// The dip's Gaussian radius, in km.
    pub radius_km: f32,
    /// How fast the centre moves, in km/h.
    pub speed_kmh: f32,
    /// Where it moves to, in degrees clockwise from north.
    pub bearing_deg: f32,
    /// The Gaussian half-width of its life in hours: the dip is about 60% as
    /// deep this long before and after `at`.
    pub lifetime_h: f32,
}

/// The background sea-level pressure at `lat`, in hPa: a weak north-south
/// gradient around 1016 hPa.
pub fn background_hpa(lat: f32) -> f32 {
    1_016.0 + 0.2 * (lat - 40.0)
}

impl SyntheticLow {
    /// Hours from the deepest moment to `t`.
    fn hours_from_peak(&self, t: UnixTime) -> f32 {
        (t.0 - self.at.0) as f32 / HOUR as f32
    }

    /// The centre at `t`, as `(latitude, longitude)` in degrees.
    pub fn centre(&self, t: UnixTime) -> (f32, f32) {
        let km = self.speed_kmh * self.hours_from_peak(t);
        let bearing = self.bearing_deg.to_radians();
        let lat = self.latitude + km * libm::cosf(bearing) / KM_PER_DEG;
        let lon_km = KM_PER_DEG * libm::cosf(self.latitude.to_radians());
        let lon = f64::from(self.longitude + km * libm::sinf(bearing) / lon_km);
        (lat, wrap_longitude(lon) as f32)
    }

    /// How far below the background the centre is at `t`, in hPa.
    pub fn depth(&self, t: UnixTime) -> f32 {
        let peak = background_hpa(self.latitude) - self.central_hpa;
        let x = self.hours_from_peak(t) / self.lifetime_h;
        peak * libm::expf(-0.5 * x * x)
    }

    /// How much the low lowers the pressure at `(lat, lon)` at `t`, in hPa.
    pub fn drop_at(&self, t: UnixTime, lat: f32, lon: f32) -> f32 {
        let (clat, clon) = self.centre(t);
        let mean_lat = f32::midpoint(lat, clat).to_radians();
        let dlon = wrap_longitude(f64::from(lon - clon)) as f32;
        let dx = dlon * KM_PER_DEG * libm::cosf(mean_lat);
        let dy = (lat - clat) * KM_PER_DEG;
        let r2 = (dx * dx + dy * dy) / (self.radius_km * self.radius_km);
        self.depth(t) * libm::expf(-0.5 * r2)
    }
}

/// The sea-level pressure at `(lat, lon)` and `t` with `lows` on the
/// background, in hPa.
pub fn pressure_field(lows: &[SyntheticLow], t: UnixTime, lat: f32, lon: f32) -> f32 {
    background_hpa(lat) - lows.iter().map(|l| l.drop_at(t, lat, lon)).sum::<f32>()
}

/// A grid of [`pressure_field`] with `systems` around `(latitude, longitude)` (see
/// [`grid_axes`]) at `times`, rounded to 0.1 hPa like Open-Meteo's values.
pub fn gaussian_grid(
    model: &str,
    run: Option<ModelRun>,
    latitude: f64,
    longitude: f64,
    times: &[UnixTime],
    systems: &[SyntheticLow],
) -> PressureGrid {
    let (lats, lons) = grid_axes(latitude, longitude);
    let pressure_hpa = times
        .iter()
        .map(|t| {
            lats.iter()
                .flat_map(|lat| {
                    lons.iter()
                        .map(|lon| Some(round_to(pressure_field(systems, *t, *lat, *lon), 1)))
                })
                .collect()
        })
        .collect();
    PressureGrid {
        model: model.to_string(),
        run,
        lats,
        lons,
        times: times.to_vec(),
        pressure_hpa,
    }
}

/// `x` rounded to `decimals` places.
fn round_to(x: f32, decimals: i32) -> f32 {
    let scale = libm::powf(10.0, decimals as f32);
    let r = libm::roundf(x * scale) / scale;
    if r == 0.0 { 0.0 } else { r }
}

/// A tiny linear congruential generator (Numerical Recipes constants).
#[derive(Clone, Copy, Debug)]
struct Lcg(u32);

impl Lcg {
    /// The next value in `0.0..1.0`.
    fn next_unit(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / 16_777_216.0
    }
}

/// `total` spread over `times` as a Gaussian bump around `centre`, rounded
/// to `decimals` places.
fn bump(times: &[UnixTime], centre: UnixTime, sigma_h: f32, total: f32, decimals: i32) -> Vec<f32> {
    let weights: Vec<f32> = times
        .iter()
        .map(|t| {
            let x = (t.0 - centre.0) as f32 / HOUR as f32 / sigma_h;
            libm::expf(-0.5 * x * x)
        })
        .collect();
    let sum: f32 = weights.iter().sum();
    weights
        .iter()
        .map(|w| round_to(total * w / sum, decimals))
        .collect()
}

/// One deterministic model's storm.
struct ModelStorm {
    model: &'static str,
    low: SyntheticLow,
    precipitation_mm: f32,
    snowfall_cm: f32,
    sigma_h: f32,
}

/// The four models' versions of the nor'easter: ECMWF brings a 978 hPa low
/// about 90 km from the place with about 14 in of snow, GFS keeps a 990 hPa
/// low about 320 km offshore with about 3 in, and GEM and ICON fall between.
fn noreaster_storms() -> [ModelStorm; 4] {
    let low = |latitude, longitude, central_hpa| SyntheticLow {
        latitude,
        longitude,
        at: STORM_PEAK,
        central_hpa,
        radius_km: 350.0,
        speed_kmh: 35.0,
        bearing_deg: 40.0,
        lifetime_h: 30.0,
    };
    [
        ModelStorm {
            model: DETERMINISTIC_MODELS[0],
            low: low(40.2, -73.2, 978.0),
            precipitation_mm: 44.0,
            snowfall_cm: 35.6,
            sigma_h: 8.0,
        },
        ModelStorm {
            model: DETERMINISTIC_MODELS[1],
            low: SyntheticLow {
                at: UnixTime(STORM_PEAK.0 + 3 * HOUR),
                ..low(39.0, -71.0, 990.0)
            },
            precipitation_mm: 14.0,
            snowfall_cm: 7.6,
            sigma_h: 7.0,
        },
        ModelStorm {
            model: DETERMINISTIC_MODELS[2],
            low: low(39.6, -72.3, 985.0),
            precipitation_mm: 30.0,
            snowfall_cm: 21.6,
            sigma_h: 8.0,
        },
        ModelStorm {
            model: DETERMINISTIC_MODELS[3],
            low: SyntheticLow {
                at: UnixTime(STORM_PEAK.0 - 3 * HOUR),
                ..low(39.9, -72.9, 981.0)
            },
            precipitation_mm: 38.0,
            snowfall_cm: 27.9,
            sigma_h: 8.0,
        },
    ]
}

/// Gusts in km/h from the place's pressure: stronger the deeper the dip,
/// with a little hour-to-hour noise.
fn gusts(pressure: &[f32], rng: &mut Lcg) -> Vec<Option<f32>> {
    pressure
        .iter()
        .map(|p| {
            let gust = 16.0 + 1.8 * (1_014.0 - p).max(0.0) + 6.0 * rng.next_unit();
            Some(round_to(gust, 1))
        })
        .collect()
}

fn point_series(storm: &ModelStorm, times: &[UnixTime], seed: u32) -> PointSeries {
    let place = storm_location();
    let (lat, lon) = (place.latitude as f32, place.longitude as f32);
    let pressure: Vec<f32> = times
        .iter()
        .map(|t| round_to(pressure_field(&[storm.low], *t, lat, lon), 1))
        .collect();
    let precipitation = bump(
        times,
        storm.low.at,
        storm.sigma_h,
        storm.precipitation_mm,
        1,
    );
    let snowfall = bump(times, storm.low.at, storm.sigma_h, storm.snowfall_cm, 2);
    PointSeries {
        model: storm.model.to_string(),
        run: Some(model_run(storm.model)),
        times: times.to_vec(),
        precipitation_mm: precipitation.into_iter().map(Some).collect(),
        snowfall_cm: snowfall.into_iter().map(Some).collect(),
        gust_kmh: gusts(&pressure, &mut Lcg(seed)),
        pressure_hpa: pressure.into_iter().map(Some).collect(),
    }
}

/// An ensemble of `members` around one storm: member 0 (the control) has the
/// given totals and timing; the others scale the totals by a factor in
/// `factors` and shift the timing by up to 6 hours.
fn ensemble(
    model: &str,
    members: usize,
    storm: &ModelStorm,
    factors: (f32, f32),
    seed: u32,
) -> EnsembleSeries {
    let times = hourly_times();
    let mut rng = Lcg(seed);
    let (mut precipitation_mm, mut snowfall_cm) = (Vec::new(), Vec::new());
    for member in 0..members {
        let (factor, shift_h) = if member == 0 {
            (1.0, 0)
        } else {
            let factor = factors.0 + (factors.1 - factors.0) * rng.next_unit();
            (factor, libm::roundf((rng.next_unit() - 0.5) * 12.0) as i64)
        };
        let centre = UnixTime(storm.low.at.0 + shift_h * HOUR);
        let total = storm.precipitation_mm * factor;
        let snow = storm.snowfall_cm * factor;
        let p = bump(&times, centre, storm.sigma_h, total, 1);
        let s = bump(&times, centre, storm.sigma_h, snow, 2);
        precipitation_mm.push(p.into_iter().map(Some).collect());
        snowfall_cm.push(s.into_iter().map(Some).collect());
    }
    EnsembleSeries {
        model: model.to_string(),
        run: Some(ensemble_run(model)),
        times,
        precipitation_mm,
        snowfall_cm,
    }
}

/// A January nor'easter at New York, fetched at [`STORM_NOW`].
///
/// ECMWF brings a 978 hPa low within about 90 km (55 mi) and about 14 in
/// (35.6 cm) of snow; GFS keeps a 990 hPa low about 320 km (200 mi) offshore
/// with about 3 in (7.6 cm); GEM (985 hPa, 8.5 in) and ICON (981 hPa, 11 in)
/// fall between. The snow falls from Saturday night to Monday, heaviest
/// around [`STORM_PEAK`]. ECMWF (51 members) and GFS (31) have ensembles; each
/// model has a 7×7 pressure grid at 2.5° with its moving low.
pub fn noreaster() -> StormOutlook {
    let place = storm_location();
    let times = hourly_times();
    let grid_times = grid_times();
    let storms = noreaster_storms();
    let points = storms
        .iter()
        .zip(1_u32..)
        .map(|(storm, seed)| point_series(storm, &times, seed))
        .collect();
    let grids = storms
        .iter()
        .map(|storm| {
            gaussian_grid(
                storm.model,
                Some(model_run(storm.model)),
                place.latitude,
                place.longitude,
                &grid_times,
                &[storm.low],
            )
        })
        .collect();
    let ensembles = [
        ensemble(
            ENSEMBLE_MODELS[0],
            ECMWF_MEMBERS,
            &storms[0],
            (0.3, 1.4),
            101,
        ),
        ensemble(ENSEMBLE_MODELS[1], GFS_MEMBERS, &storms[1], (0.5, 2.6), 202),
    ]
    .into();
    StormOutlook {
        place,
        offset_changes: Vec::new(),
        points,
        ensembles,
        grids,
        past_runs: Vec::new(),
        fetched_at: STORM_NOW,
    }
}

/// The window the nor'easter's earlier runs are totalled over: Saturday
/// 18:00 UTC to Monday 12:00 UTC.
pub fn noreaster_window() -> (UnixTime, UnixTime) {
    (
        UnixTime(STORM_PEAK.0 - 24 * HOUR),
        UnixTime(STORM_PEAK.0 + 18 * HOUR),
    )
}

/// Earlier ECMWF and GFS runs' totals for the nor'easter, oldest first: ECMWF
/// has trended snowier over four runs, GFS drier.
pub fn noreaster_past_runs() -> Vec<PastRun> {
    let (window_start, window_end) = noreaster_window();
    let series: [(&str, [(f32, f32); 4]); 2] = [
        (
            DETERMINISTIC_MODELS[0],
            [(20.0, 14.0), (27.5, 21.0), (35.0, 28.0), (41.0, 33.0)],
        ),
        (
            DETERMINISTIC_MODELS[1],
            [(23.0, 16.5), (19.5, 13.0), (16.0, 9.5), (15.0, 8.4)],
        ),
    ];
    series
        .iter()
        .flat_map(|(model, totals)| {
            let current = model_run(model);
            totals.iter().enumerate().map(move |(i, (mm, cm))| {
                // Four runs, 6 hours apart, before the current one.
                let init = UnixTime(current.init.0 - (4 - i as i64) * 6 * HOUR);
                PastRun {
                    model: (*model).to_string(),
                    run: ModelRun {
                        model: current.model.clone(),
                        init,
                        available: UnixTime(init.0 + (current.available.0 - current.init.0)),
                    },
                    window_start,
                    window_end,
                    window_total_mm: Some(*mm),
                    window_snow_cm: Some(*cm),
                }
            })
        })
        .collect()
}

/// A dry, breezy week under high pressure: no hour above 0.2 mm, no snow and
/// no low under 1008 hPa anywhere on the grids. Fetched at [`STORM_NOW`].
pub fn dry_week() -> StormOutlook {
    let place = storm_location();
    let times = hourly_times();
    let grid_times = grid_times();
    // A broad high: a negative "low" raises the pressure.
    let high = SyntheticLow {
        latitude: 42.0,
        longitude: -78.0,
        at: UnixTime(STORM_START.0 + 96 * HOUR),
        central_hpa: background_hpa(42.0) + 12.0,
        radius_km: 900.0,
        speed_kmh: 15.0,
        bearing_deg: 100.0,
        lifetime_h: 120.0,
    };
    let (lat, lon) = (place.latitude as f32, place.longitude as f32);
    let points = DETERMINISTIC_MODELS
        .iter()
        .zip(11_u32..)
        .map(|(model, seed)| {
            let mut rng = Lcg(seed);
            let pressure: Vec<f32> = times
                .iter()
                .map(|t| round_to(pressure_field(&[high], *t, lat, lon), 1))
                .collect();
            let precipitation = times
                .iter()
                .map(|_| Some(if rng.next_unit() < 0.03 { 0.1 } else { 0.0 }))
                .collect();
            PointSeries {
                model: (*model).to_string(),
                run: Some(model_run(model)),
                times: times.clone(),
                precipitation_mm: precipitation,
                snowfall_cm: times.iter().map(|_| Some(0.0)).collect(),
                gust_kmh: gusts(&pressure, &mut rng),
                pressure_hpa: pressure.into_iter().map(Some).collect(),
            }
        })
        .collect();
    let ensembles = ENSEMBLE_MODELS
        .iter()
        .zip([ECMWF_MEMBERS, GFS_MEMBERS])
        .map(|(model, members)| {
            let zeros: Vec<Option<f32>> = times.iter().map(|_| Some(0.0)).collect();
            EnsembleSeries {
                model: (*model).to_string(),
                run: Some(ensemble_run(model)),
                times: times.clone(),
                precipitation_mm: (0..members).map(|_| zeros.clone()).collect(),
                snowfall_cm: (0..members).map(|_| zeros.clone()).collect(),
            }
        })
        .collect();
    let grids = DETERMINISTIC_MODELS
        .iter()
        .map(|model| {
            gaussian_grid(
                model,
                Some(model_run(model)),
                place.latitude,
                place.longitude,
                &grid_times,
                &[high],
            )
        })
        .collect();
    StormOutlook {
        place,
        offset_changes: Vec::new(),
        points,
        ensembles,
        grids,
        past_runs: Vec::new(),
        fetched_at: STORM_NOW,
    }
}

/// The model ids of an outlook's point series, in order.
pub fn point_models(outlook: &StormOutlook) -> Vec<String> {
    outlook.points.iter().map(|p| p.model.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MM_PER_IN: f32 = 25.4;

    fn total(values: &[Option<f32>]) -> f32 {
        values.iter().flatten().sum()
    }

    fn grid_min(grid: &PressureGrid) -> f32 {
        grid.pressure_hpa
            .iter()
            .flatten()
            .flatten()
            .fold(f32::INFINITY, |a, b| a.min(*b))
    }

    fn ascending_by(times: &[UnixTime], step: i64) -> bool {
        times.windows(2).all(|w| w[1].0 - w[0].0 == step)
    }

    fn check_shapes(outlook: &StormOutlook) {
        assert_eq!(point_models(outlook), DETERMINISTIC_MODELS);
        for p in &outlook.points {
            assert_eq!(p.times.len(), STORM_HOURS, "{}", p.model);
            assert!(ascending_by(&p.times, HOUR), "{}", p.model);
            for v in [
                &p.precipitation_mm,
                &p.snowfall_cm,
                &p.gust_kmh,
                &p.pressure_hpa,
            ] {
                assert_eq!(v.len(), p.times.len(), "{}", p.model);
            }
            assert_eq!(p.run.as_ref().map(|r| r.init), Some(STORM_RUN_INIT));
        }
        let members: Vec<(&str, usize)> = outlook
            .ensembles
            .iter()
            .map(|e| (e.model.as_str(), e.member_count()))
            .collect();
        assert_eq!(
            members,
            [("ecmwf_ifs025", ECMWF_MEMBERS), ("gfs025", GFS_MEMBERS)]
        );
        for e in &outlook.ensembles {
            assert!(ascending_by(&e.times, HOUR));
            assert_eq!(e.snowfall_cm.len(), e.member_count());
            for m in e.precipitation_mm.iter().chain(&e.snowfall_cm) {
                assert_eq!(m.len(), e.times.len());
            }
        }
        assert_eq!(outlook.grids.len(), 4);
        for g in &outlook.grids {
            assert_eq!((g.lats.len(), g.lons.len()), (7, 7));
            assert_eq!(g.times.len(), GRID_STEPS);
            assert!(ascending_by(&g.times, GRID_STEP_SECS));
            assert!(g.pressure_hpa.iter().all(|step| step.len() == 49));
        }
        assert_eq!(outlook.fetched_at, STORM_NOW);
    }

    #[test]
    fn noreaster_shapes_and_determinism() {
        let outlook = noreaster();
        check_shapes(&outlook);
        assert_eq!(outlook, noreaster());
    }

    #[test]
    fn dry_week_shapes_and_values() {
        let outlook = dry_week();
        check_shapes(&outlook);
        assert_eq!(outlook, dry_week());
        for p in &outlook.points {
            assert!(p.precipitation_mm.iter().flatten().all(|mm| *mm <= 0.2));
            assert!(total(&p.precipitation_mm) < 2.0);
            assert_eq!(total(&p.snowfall_cm), 0.0);
        }
        for g in &outlook.grids {
            assert!(grid_min(g) > 1_008.0, "{}: {}", g.model, grid_min(g));
        }
    }

    #[test]
    fn noreaster_models_disagree_as_described() {
        let outlook = noreaster();
        let snow_in: Vec<f32> = outlook
            .points
            .iter()
            .map(|p| total(&p.snowfall_cm) * 10.0 / MM_PER_IN)
            .collect();
        assert!((snow_in[0] - 14.0).abs() < 0.3, "{snow_in:?}");
        assert!((snow_in[1] - 3.0).abs() < 0.2, "{snow_in:?}");
        assert!(snow_in[2] > snow_in[1] && snow_in[2] < snow_in[0]);
        assert!(snow_in[3] > snow_in[1] && snow_in[3] < snow_in[0]);
        let lows: Vec<f32> = outlook.grids.iter().map(grid_min).collect();
        assert!(lows[0] < 981.0 && lows[0] >= 978.0, "{lows:?}");
        assert!(lows[1] < 993.0 && lows[1] > 989.0, "{lows:?}");
        assert!(lows[0] < lows[3] && lows[3] < lows[2] && lows[2] < lows[1]);
        let place_min: Vec<f32> = outlook
            .points
            .iter()
            .map(|p| {
                p.pressure_hpa
                    .iter()
                    .flatten()
                    .fold(f32::INFINITY, |a, b| a.min(*b))
            })
            .collect();
        assert!(
            place_min[0] < 982.0 && place_min[1] > 995.0,
            "{place_min:?}"
        );
        let ecmwf_peak_gust = outlook.points[0]
            .gust_kmh
            .iter()
            .flatten()
            .fold(0.0_f32, |a, b| a.max(*b));
        assert!(ecmwf_peak_gust > 70.0, "{ecmwf_peak_gust}");
    }

    #[test]
    fn ensembles_spread_around_their_control() {
        let outlook = noreaster();
        for (e, (lo, hi)) in outlook.ensembles.iter().zip([(13.0, 62.0), (6.0, 37.0)]) {
            let totals: Vec<f32> = e.precipitation_mm.iter().map(|m| total(m)).collect();
            let control = outlook
                .points
                .iter()
                .find(|p| {
                    crate::storm::ModelRole::of(&p.model) == crate::storm::ModelRole::of(&e.model)
                })
                .map(|p| total(&p.precipitation_mm));
            assert!((totals[0] - control.unwrap_or_default()).abs() < 1.0);
            assert!(totals.iter().all(|t| (lo..=hi).contains(t)), "{totals:?}");
            let distinct = totals
                .windows(2)
                .filter(|w| (w[0] - w[1]).abs() > 0.5)
                .count();
            assert!(distinct > e.member_count() / 2);
        }
    }

    #[test]
    fn the_lows_move_northeast_and_cross_the_antimeridian() {
        let low = SyntheticLow {
            latitude: 50.0,
            longitude: 179.0,
            at: STORM_PEAK,
            central_hpa: 975.0,
            radius_km: 350.0,
            speed_kmh: 100.0,
            bearing_deg: 90.0,
            lifetime_h: 30.0,
        };
        let (lat, lon) = low.centre(UnixTime(STORM_PEAK.0 + 6 * HOUR));
        assert!(
            (lat - 50.0).abs() < 0.01 && lon < -170.0 && lon > -179.0,
            "{lon}"
        );
        let grid = gaussian_grid("x", None, 50.0, 180.0, &[STORM_PEAK], &[low]);
        let min = grid_min(&grid);
        assert!(min < 980.0, "{min}");
        let (lat, lon) = noreaster_storms()[0]
            .low
            .centre(UnixTime(STORM_PEAK.0 - 12 * HOUR));
        assert!(lat < 40.2 && lon < -73.2);
    }

    #[test]
    fn past_runs_precede_the_current_run() {
        let runs = noreaster_past_runs();
        assert_eq!(runs.len(), 8);
        for model in ["ecmwf_ifs025", "gfs_seamless"] {
            let inits: Vec<i64> = runs
                .iter()
                .filter(|r| r.model == model)
                .map(|r| r.run.init.0)
                .collect();
            assert!(inits.windows(2).all(|w| w[1] - w[0] == 6 * HOUR));
            assert_eq!(inits.last().copied(), Some(STORM_RUN_INIT.0 - 6 * HOUR));
        }
    }

    #[cfg(feature = "serde")]
    #[test]
    fn noreaster_round_trips_through_json_losslessly() {
        let outlook = noreaster();
        let json = serde_json::to_string(&outlook).unwrap();
        let back: StormOutlook = serde_json::from_str(&json).unwrap();
        assert_eq!(back, outlook);
        assert_eq!(serde_json::to_string(&back).unwrap(), json);
        let past = noreaster_past_runs();
        let json = serde_json::to_string(&past).unwrap();
        assert_eq!(serde_json::from_str::<Vec<PastRun>>(&json).unwrap(), past);
    }
}
