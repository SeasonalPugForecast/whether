//! Open-Meteo storm outlook requests: parsing the recorded New York responses
//! and the hand-made edge cases.

use whether_core::storm::{
    DETERMINISTIC_MODELS, ENSEMBLE_MODELS, EnsembleSeries, GRID_SIZE, ModelRun, PointSeries,
    PressureGrid,
};
use whether_core::time::{UnixTime, UtcOffset};
use whether_providers::open_meteo::storm::{
    ENSEMBLE_DOMAINS, RUN_DOMAINS, axes, ensemble_request, grid_request, parse_ensembles,
    parse_grids, parse_points, parse_run_meta, parse_single_run, parse_update_interval,
    point_request, single_run_request,
};
use whether_providers::{Coordinates, ProviderError};

const NEW_YORK: Coordinates = Coordinates {
    latitude: 40.7143,
    longitude: -74.005_97,
};

const POINT: &str = include_str!("fixtures/open_meteo/storm_point_10001.json");
const ENSEMBLE: &str = include_str!("fixtures/open_meteo/storm_ensemble_10001.json");
const SINGLE_RUN: &str =
    include_str!("fixtures/open_meteo/storm_single_run_ecmwf_ifs025_10001.json");
const GRID: &str = include_str!("fixtures/open_meteo/storm_grid_10001.json");
const METAS: [(&str, &str); 6] = [
    (
        "ecmwf_ifs025",
        include_str!("fixtures/open_meteo/storm_meta_ecmwf_ifs025.json"),
    ),
    (
        "ncep_gfs025",
        include_str!("fixtures/open_meteo/storm_meta_ncep_gfs025.json"),
    ),
    (
        "cmc_gem_gdps_15km",
        include_str!("fixtures/open_meteo/storm_meta_cmc_gem_gdps_15km.json"),
    ),
    (
        "dwd_icon",
        include_str!("fixtures/open_meteo/storm_meta_dwd_icon.json"),
    ),
    (
        "ecmwf_ifs025_ensemble",
        include_str!("fixtures/open_meteo/storm_meta_ecmwf_ifs025_ensemble.json"),
    ),
    (
        "ncep_gefs025",
        include_str!("fixtures/open_meteo/storm_meta_ncep_gefs025.json"),
    ),
];
const ERROR: &str = include_str!("fixtures/open_meteo/storm_error.synthetic.json");
const MISSING_MODEL: &str =
    include_str!("fixtures/open_meteo/storm_point_missing_model.synthetic.json");
const GRID_NULLS: &str = include_str!("fixtures/open_meteo/storm_grid_nulls.synthetic.json");

/// The run the single-run recording asked for: 2026-11-13 00:00 UTC.
const EARLIER_RUN: UnixTime = UnixTime(1_794_528_000);
/// Shortly after the recordings were made (2026-11-14 22:50 UTC).
const RECORDED: UnixTime = UnixTime(1_794_700_000);
const DAY: i64 = 86_400;

fn assert_steps(times: &[UnixTime], step: i64, what: &str) {
    assert!(
        times.windows(2).all(|w| w[1].0 - w[0].0 == step),
        "{what}: times are not {step} s apart"
    );
}

fn assert_range(values: impl IntoIterator<Item = Option<f32>>, lo: f32, hi: f32, what: &str) {
    for v in values.into_iter().flatten() {
        assert!((lo..=hi).contains(&v), "{what}: {v} outside {lo}..={hi}");
    }
}

fn assert_sane_point(s: &PointSeries) {
    let n = s.times.len();
    for (name, column) in [
        ("precipitation", &s.precipitation_mm),
        ("snowfall", &s.snowfall_cm),
        ("gusts", &s.gust_kmh),
        ("pressure", &s.pressure_hpa),
    ] {
        assert_eq!(column.len(), n, "{} {name}", s.model);
    }
    assert_steps(&s.times, 3_600, &s.model);
    assert_range(s.precipitation_mm.iter().copied(), 0.0, 100.0, &s.model);
    assert_range(s.snowfall_cm.iter().copied(), 0.0, 100.0, &s.model);
    assert_range(s.gust_kmh.iter().copied(), 0.0, 250.0, &s.model);
    assert_range(s.pressure_hpa.iter().copied(), 950.0, 1050.0, &s.model);
    assert!(
        s.pressure_hpa.iter().filter(|p| p.is_some()).count() > n / 2,
        "{}: mostly empty",
        s.model
    );
}

#[test]
fn the_point_has_every_model() {
    let request = point_request(&NEW_YORK, &[]);
    let f = parse_points(POINT.as_bytes(), &[]).unwrap();
    assert!(request.url.contains("&forecast_days=10&"));
    assert_eq!(f.location.utc_offset, UtcOffset(-18_000));
    assert_eq!(f.location.tz_name.as_deref(), Some("America/New_York"));
    let models: Vec<&str> = f.series.iter().map(|s| s.model.as_str()).collect();
    assert_eq!(models, DETERMINISTIC_MODELS);
    for s in &f.series {
        assert_eq!(s.times.len(), 240, "{}", s.model);
        assert_eq!(s.run, None);
        assert_sane_point(s);
    }
    assert!(
        f.series.windows(2).all(|w| w[0].times == w[1].times),
        "the models share the hours"
    );
    // ICON's global run ends before the tenth day.
    let icon = &f.series[3];
    assert_eq!(icon.pressure_hpa.last(), Some(&None));
}

#[test]
fn a_missing_model_is_left_out() {
    // The recorded point response cut to six hours, without GEM's columns, as
    // when Open-Meteo has no data for one of the models.
    let f = parse_points(MISSING_MODEL.as_bytes(), &DETERMINISTIC_MODELS).unwrap();
    let models: Vec<&str> = f.series.iter().map(|s| s.model.as_str()).collect();
    assert_eq!(models, ["ecmwf_ifs025", "gfs_seamless", "icon_seamless"]);
    assert!(f.series.iter().all(|s| s.times.len() == 6));
}

fn assert_sane_ensemble(e: &EnsembleSeries, members: usize) {
    assert_eq!(e.member_count(), members, "{}", e.model);
    assert_eq!(e.snowfall_cm.len(), members, "{}", e.model);
    assert_steps(&e.times, 3_600, &e.model);
    for m in e.precipitation_mm.iter().chain(&e.snowfall_cm) {
        assert_eq!(m.len(), e.times.len(), "{}", e.model);
        assert_range(m.iter().copied(), 0.0, 100.0, &e.model);
    }
    // Members differ; a parse that repeated the control would not.
    assert!(
        e.precipitation_mm[1..]
            .iter()
            .any(|m| *m != e.precipitation_mm[0])
    );
}

#[test]
fn the_ensembles_have_their_members() {
    assert!(
        ensemble_request(&NEW_YORK, &[], 10)
            .url
            .contains("&forecast_days=10&")
    );
    let e = parse_ensembles(ENSEMBLE.as_bytes(), &[]).unwrap();
    let models: Vec<&str> = e.iter().map(|s| s.model.as_str()).collect();
    assert_eq!(models, ENSEMBLE_MODELS);
    assert_eq!(e[0].times.len(), 240);
    assert_sane_ensemble(&e[0], 51);
    assert_sane_ensemble(&e[1], 31);
}

#[test]
fn every_run_file_parses() {
    let domains: Vec<&str> = RUN_DOMAINS
        .iter()
        .chain(&ENSEMBLE_DOMAINS)
        .map(|(_, d)| *d)
        .collect();
    assert_eq!(domains, METAS.map(|(d, _)| d));
    for (domain, body) in METAS {
        let run = parse_run_meta(domain, body.as_bytes()).unwrap();
        assert_eq!(run.model, domain);
        assert!(run.init <= run.available, "{domain}");
        assert!(
            (RECORDED.0 - 3 * DAY..RECORDED.0).contains(&run.init.0),
            "{domain}: the latest run is days old"
        );
        assert_eq!(
            run.init.0 % 21_600,
            0,
            "{domain}: runs start at 0, 6, 12 or 18 UTC"
        );
        let interval = parse_update_interval(body.as_bytes()).unwrap();
        assert!([21_600, 43_200].contains(&interval), "{domain}: {interval}");
    }
}

#[test]
fn an_earlier_run_covers_its_window() {
    let request = single_run_request(
        &NEW_YORK,
        "ecmwf_ifs025",
        EARLIER_RUN,
        EARLIER_RUN,
        UnixTime(EARLIER_RUN.0 + 10 * DAY),
    )
    .unwrap();
    assert!(request.url.contains("&run=2026-11-13T00%3A00&"));
    assert!(request.url.contains("&forecast_days=10&"));
    let run = ModelRun {
        model: "ecmwf_ifs025".into(),
        init: EARLIER_RUN,
        available: UnixTime(EARLIER_RUN.0 + 7 * 3_600),
    };
    let s = parse_single_run(SINGLE_RUN.as_bytes(), "ecmwf_ifs025", run.clone()).unwrap();
    assert_eq!(s.run, Some(run));
    assert_eq!(s.model, "ecmwf_ifs025");
    // The run's hours start at its initialisation.
    assert_eq!(s.times.first(), Some(&EARLIER_RUN));
    assert_eq!(s.times.len(), 240);
    assert_sane_point(&s);
}

fn assert_sane_grid(g: &PressureGrid, steps: usize) {
    let (lats, lons) = axes(&NEW_YORK);
    assert_eq!(g.lats, lats, "{}", g.model);
    assert_eq!(g.lons, lons, "{}", g.model);
    assert_eq!((g.lats.len(), g.lons.len()), (GRID_SIZE, GRID_SIZE));
    assert_eq!(g.times.len(), steps, "{}", g.model);
    assert_eq!(g.pressure_hpa.len(), steps, "{}", g.model);
    assert_steps(&g.times, 21_600, &g.model);
    for step in &g.pressure_hpa {
        assert_eq!(step.len(), GRID_SIZE * GRID_SIZE, "{}", g.model);
        assert_range(step.iter().copied(), 950.0, 1050.0, &g.model);
    }
}

#[test]
fn the_grid_has_every_model_at_every_point() {
    assert!(
        grid_request(&NEW_YORK, &[])
            .url
            .contains("&temporal_resolution=hourly_6&")
    );
    let grids = parse_grids(GRID.as_bytes(), &NEW_YORK, &[]).unwrap();
    let models: Vec<&str> = grids.iter().map(|g| g.model.as_str()).collect();
    assert_eq!(models, DETERMINISTIC_MODELS);
    for g in &grids {
        assert_sane_grid(g, 40);
        assert!(g.pressure_hpa[0].iter().all(Option::is_some), "{}", g.model);
    }
    // ICON's last steps are past its range.
    assert!(grids[3].pressure_hpa[39].iter().all(Option::is_none));
}

#[test]
fn grid_points_without_data_hold_nothing() {
    // The recorded grid cut to two steps: ICON is null at point 10 and GEM's
    // column is missing at point 11, as for a point outside a model's area.
    let grids = parse_grids(GRID_NULLS.as_bytes(), &NEW_YORK, &[]).unwrap();
    assert_eq!(grids.len(), 4);
    for g in &grids {
        assert_sane_grid(g, 2);
    }
    let (gem, icon) = (&grids[2], &grids[3]);
    assert_eq!(icon.value(0, 1, 3), None);
    assert!(icon.value(0, 1, 4).is_some());
    assert_eq!(gem.value(1, 1, 4), None);
    assert!(gem.value(1, 1, 3).is_some());
}

#[test]
fn error_bodies_are_rejections_with_their_reason() {
    // The shape of the 400 the Ensemble API returned for an unknown model.
    let reason = "Invalid value: Cannot initialize MultiDomains from invalid String value bogus";
    let expected = Err(ProviderError::Rejected(reason.into()));
    assert_eq!(parse_points(ERROR.as_bytes(), &[]).map(|_| ()), expected);
    assert_eq!(parse_ensembles(ERROR.as_bytes(), &[]).map(|_| ()), expected);
    assert_eq!(
        parse_grids(ERROR.as_bytes(), &NEW_YORK, &[]).map(|_| ()),
        expected
    );
    assert_eq!(
        parse_run_meta("bogus", ERROR.as_bytes()).map(|_| ()),
        expected
    );
}
