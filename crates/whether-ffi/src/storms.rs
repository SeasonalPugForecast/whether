//! The storm outlook for the app: several models side by side at one place,
//! fetched from Open-Meteo with Swift doing the HTTP.
//!
//! A refresh first reads each model's `meta.json` (small static files). Only
//! when some model has a newer run than the cached outlook are the point, the
//! ensemble and the grid requested. Requests go out one after another, like
//! every other fetch here.
//!
//! The "Storm ahead" check is lighter: it asks only for the models' hours at
//! the place, and not again while its outlook is younger than three hours.

use std::collections::BTreeMap;
use std::sync::Arc;

use whether_core::model::{ProviderId, ResolvedLocation};
use whether_core::storm::analysis::window_totals;
use whether_core::storm::{DETERMINISTIC_MODELS, ENSEMBLE_MODELS, ModelRun, PastRun, StormOutlook};
use whether_core::strings::Strings;
use whether_core::time::{UnixTime, UtcOffset};
use whether_core::view::build_storms;
use whether_providers::open_meteo::storm::{
    FORECAST_DAYS, ensemble_domain, ensemble_request, grid_request, meta_request, parse_ensembles,
    parse_grids, parse_points, parse_run_meta, parse_single_run, parse_update_interval,
    point_request, run_domain, single_run_request,
};
use whether_providers::{HttpRequest, HttpResponse, ProviderError, check_status};

use crate::fetch::{AttemptInfo, FetchHost, host_request, name, outgoing};
use crate::history::to_json;
use crate::settings::{Place, Settings};
use crate::{WhetherError, from_json};

/// How long an outlook is trusted when the run times can't be read: one
/// model update interval.
const TRUSTED_WITHOUT_META_SECS: i64 = 6 * 3_600;

/// The most earlier runs one [`fetch_past_runs`] asks for.
const MAX_PAST_RUNS: u32 = 6;

/// The distance in degrees beyond which a cached outlook is for another place.
const SAME_PLACE_DEG: f64 = 0.01;

/// A storm outlook, fetched or rebuilt from the cache.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct StormsOutcome {
    /// The outlook as JSON: cache it per place and pass it back as
    /// `cached_json`, and to `storms_view`.
    pub outlook_json: String,
    /// The storms view (`StormsVm`) as JSON, built at `now`.
    pub view_json: String,
    /// The data was fetched now; `false` when the cached outlook was current
    /// and only the run times were read.
    pub refreshed: bool,
    /// Every request's outcome: the run times, then the data.
    pub attempts: Vec<AttemptInfo>,
}

/// The `meta.json` domains of the models the outlook uses, each once.
fn domains() -> Vec<&'static str> {
    let mut domains: Vec<&'static str> = Vec::new();
    let all = DETERMINISTIC_MODELS
        .iter()
        .filter_map(|m| run_domain(m))
        .chain(ENSEMBLE_MODELS.iter().filter_map(|m| ensemble_domain(m)));
    for domain in all {
        if !domains.contains(&domain) {
            domains.push(domain);
        }
    }
    domains
}

/// Sends `request`, checks its status and parses its body with `parse`,
/// recording the outcome under `endpoint`.
async fn exchange<T>(
    host: &Arc<dyn FetchHost>,
    request: &HttpRequest,
    endpoint: &str,
    attempts: &mut Vec<AttemptInfo>,
    parse: impl FnOnce(&[u8]) -> Result<T, ProviderError>,
) -> Option<T> {
    let result = match host.send(host_request(request)).await {
        Ok(resp) => check_status(&HttpResponse::new(resp.status, &resp.body))
            .and_then(|()| parse(&resp.body))
            .map_err(|e| e.to_string()),
        Err(e) => Err(e.to_string()),
    };
    attempts.push(AttemptInfo {
        provider: name(&ProviderId::OpenMeteo),
        endpoint: endpoint.to_string(),
        error: result.as_ref().err().cloned(),
    });
    result.ok()
}

/// The latest run of every domain whose `meta.json` answered.
async fn latest_runs(
    host: &Arc<dyn FetchHost>,
    attempts: &mut Vec<AttemptInfo>,
) -> BTreeMap<&'static str, ModelRun> {
    let mut runs = BTreeMap::new();
    for domain in domains() {
        let endpoint = format!("meta {domain}");
        if let Some(run) = exchange(host, &meta_request(domain), &endpoint, attempts, |b| {
            parse_run_meta(domain, b)
        })
        .await
        {
            runs.insert(domain, run);
        }
    }
    runs
}

/// Whether `cached` can stand for `place` now: it is for the same place, it
/// holds every model's point, grid and ensemble from a run at least as new as
/// the latest in `runs`, and, when some run times could not be read, it is
/// younger than [`TRUSTED_WITHOUT_META_SECS`].
///
/// A model missing from the cache makes it stale, so an outlook that came
/// back partly is fetched again next time.
fn is_current(
    cached: &StormOutlook,
    place: &Place,
    runs: &BTreeMap<&'static str, ModelRun>,
    now: i64,
) -> bool {
    if !same_place(cached, place) {
        return false;
    }
    if runs.len() < domains().len() && now - cached.fetched_at.0 >= TRUSTED_WITHOUT_META_SECS {
        return false;
    }
    // Without the latest run, being there is all that can be checked.
    let fresh = |run: Option<Option<&ModelRun>>, latest: Option<&ModelRun>| match latest {
        Some(latest) => run.flatten().is_some_and(|r| r.init >= latest.init),
        None => run.is_some(),
    };
    let deterministic = DETERMINISTIC_MODELS.iter().all(|m| {
        let latest = run_domain(m).and_then(|d| runs.get(d));
        let point = cached.points.iter().find(|p| p.model == *m);
        let grid = cached.grids.iter().find(|g| g.model == *m);
        fresh(point.map(|p| p.run.as_ref()), latest) && fresh(grid.map(|g| g.run.as_ref()), latest)
    });
    let ensembles = ENSEMBLE_MODELS.iter().all(|m| {
        let latest = ensemble_domain(m).and_then(|d| runs.get(d));
        let ensemble = cached.ensembles.iter().find(|e| e.model == *m);
        fresh(ensemble.map(|e| e.run.as_ref()), latest)
    });
    deterministic && ensembles
}

/// Whether `outlook` is for `place`: within [`SAME_PLACE_DEG`] in both
/// coordinates.
fn same_place(outlook: &StormOutlook, place: &Place) -> bool {
    (outlook.place.latitude - place.latitude).abs() <= SAME_PLACE_DEG
        && (outlook.place.longitude - place.longitude).abs() <= SAME_PLACE_DEG
}

/// The outlook's place: the app's name and coordinates. The offset is the
/// place's; Open-Meteo's (from `resolved`, the point response) only fills in
/// when the place has none. The zone name and elevation come from Open-Meteo.
fn location(place: &Place, resolved: Option<&ResolvedLocation>) -> ResolvedLocation {
    let utc_offset = place
        .utc_offset_seconds
        .map(UtcOffset)
        .or(resolved.map(|r| r.utc_offset))
        .unwrap_or(UtcOffset(0));
    ResolvedLocation {
        name: place.name.clone(),
        latitude: place.latitude,
        longitude: place.longitude,
        elevation_m: resolved.and_then(|r| r.elevation_m),
        utc_offset,
        tz_name: resolved.and_then(|r| r.tz_name.clone()),
        // The abbreviation names Open-Meteo's offset; keep it only if that
        // is the one used.
        tz_abbrev: resolved
            .filter(|r| r.utc_offset == utc_offset)
            .and_then(|r| r.tz_abbrev.clone()),
        ..ResolvedLocation::default()
    }
}

/// The storms view of `outlook` (plus `past`) as JSON, in `place`'s units
/// and under its current name.
fn view(
    mut outlook: StormOutlook,
    past: &[PastRun],
    place: &Place,
    settings: &Settings,
    now: i64,
) -> Result<String, WhetherError> {
    outlook.place.name.clone_from(&place.name);
    let presentation = settings.presentation(place);
    let strings = Strings::localized(&settings.language, &BTreeMap::default()).0;
    let vm = build_storms(&outlook, past, &presentation, &strings, UnixTime(now));
    to_json("storms view", &vm)
}

fn failed(attempts: &[AttemptInfo]) -> WhetherError {
    WhetherError::NoStorms {
        summary: attempts
            .iter()
            .map(|a| {
                format!(
                    "{} {}: {}",
                    a.provider,
                    a.endpoint,
                    a.error.as_deref().unwrap_or("ok")
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

/// The storm outlook for `place` at Unix time `now`, through `host`.
///
/// `cached_json` is the last [`StormsOutcome::outlook_json`] for the place.
/// When every model's latest run is already in it, only the run times are
/// read and the view is rebuilt from it. Otherwise the models' hours at the
/// place, the ensembles and the pressure grid are fetched; a request that
/// fails leaves its part out (and shows in `attempts`), and only when all
/// three fail is it an error. If the run times can't be read, a cached
/// outlook younger than six hours is used, else the data is fetched.
#[uniffi::export]
#[allow(clippy::needless_pass_by_value)] // UniFFI passes arguments by value
pub async fn fetch_storms(
    place: Place,
    settings: Settings,
    cached_json: Option<String>,
    now: i64,
    host: Arc<dyn FetchHost>,
) -> Result<StormsOutcome, WhetherError> {
    let mut attempts = Vec::new();
    let runs = latest_runs(&host, &mut attempts).await;
    // A cache that doesn't read is no cache: the outlook is fetched again.
    let cached = cached_json
        .as_deref()
        .and_then(|json| from_json::<StormOutlook>("cached storm outlook", json).ok());
    if let (Some(json), Some(outlook)) = (cached_json, cached)
        && is_current(&outlook, &place, &runs, now)
    {
        return Ok(StormsOutcome {
            view_json: view(outlook, &[], &place, &settings, now)?,
            outlook_json: json,
            refreshed: false,
            attempts,
        });
    }

    let loc = outgoing(&place);
    let point = exchange(
        &host,
        &point_request(&loc, &DETERMINISTIC_MODELS),
        "point",
        &mut attempts,
        |b| parse_points(b, &DETERMINISTIC_MODELS),
    )
    .await;
    let ensembles = exchange(
        &host,
        &ensemble_request(&loc, &ENSEMBLE_MODELS, FORECAST_DAYS),
        "ensemble",
        &mut attempts,
        |b| parse_ensembles(b, &ENSEMBLE_MODELS),
    )
    .await;
    let grids = exchange(
        &host,
        &grid_request(&loc, &DETERMINISTIC_MODELS),
        "grid",
        &mut attempts,
        |b| parse_grids(b, &loc, &DETERMINISTIC_MODELS),
    )
    .await;
    if point.is_none() && ensembles.is_none() && grids.is_none() {
        return Err(failed(&attempts));
    }

    let run_of = |domain: Option<&str>| domain.and_then(|d| runs.get(d)).cloned();
    let (resolved, mut points) = point.map_or((None, Vec::new()), |p| (Some(p.location), p.series));
    for p in &mut points {
        p.run = run_of(run_domain(&p.model));
    }
    let mut ensembles = ensembles.unwrap_or_default();
    for e in &mut ensembles {
        e.run = run_of(ensemble_domain(&e.model));
    }
    let mut grids = grids.unwrap_or_default();
    for g in &mut grids {
        g.run = run_of(run_domain(&g.model));
    }
    let outlook = StormOutlook {
        place: location(&place, resolved.as_ref()),
        // The zone's changes as the app computed them for the place, like the
        // forecast's; none when the place carries none.
        offset_changes: place.core_offset_changes(),
        points,
        ensembles,
        grids,
        past_runs: Vec::new(),
        fetched_at: UnixTime(now),
    };
    Ok(StormsOutcome {
        outlook_json: to_json("storm outlook", &outlook)?,
        view_json: view(outlook, &[], &place, &settings, now)?,
        refreshed: true,
        attempts,
    })
}

/// How long [`storm_ahead`] trusts a cached outlook, in seconds: 3 hours, so
/// the check costs at most one Open-Meteo call per place every 3 hours.
pub const STORM_AHEAD_MAX_AGE_SECS: i64 = 3 * 3_600;

/// Whether a significant storm is coming, checked or rebuilt from the cache.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct StormAheadOutcome {
    /// The "Storm ahead" card (`StormAheadVm`) as JSON, built at `now`;
    /// `None` when nothing coming is significant.
    pub view_json: Option<String>,
    /// The outlook the card was built from, as JSON: cache it per place and
    /// pass it back as `cached_json`. A fetched one holds only the models'
    /// hours at the place, without run times, so [`fetch_storms`] fetches
    /// the full outlook when it is given one.
    pub outlook_json: String,
    /// The hours at the place were fetched now; `false` when the cached
    /// outlook was used.
    pub refreshed: bool,
    /// Every request's outcome: none when the cache was used, else the one
    /// point request.
    pub attempts: Vec<AttemptInfo>,
}

/// The "Storm ahead" card of `outlook` as JSON, in `place`'s units and under
/// its current name; `None` when nothing coming is significant.
fn ahead_view(
    mut outlook: StormOutlook,
    place: &Place,
    settings: &Settings,
    now: i64,
) -> Result<Option<String>, WhetherError> {
    outlook.place.name.clone_from(&place.name);
    let presentation = settings.presentation(place);
    let strings = Strings::localized(&settings.language, &BTreeMap::default()).0;
    whether_core::view::storm_ahead(&outlook, &presentation, &strings, UnixTime(now))
        .map(|vm| to_json("storm ahead", &vm))
        .transpose()
}

/// Whether a significant storm is coming to `place`, at Unix time `now`,
/// through `host`.
///
/// `cached_json` is the last outlook kept for the place: a
/// [`StormAheadOutcome::outlook_json`] or a [`StormsOutcome::outlook_json`].
/// When it is for the place, holds the models' hours there and is younger than
/// [`STORM_AHEAD_MAX_AGE_SECS`], the card is built from it with no request.
/// Otherwise only the models' hours at the place are fetched (no run times,
/// ensembles or pressure grid), and the agreement comes from their totals.
/// It is an error when that request fails.
#[uniffi::export]
#[allow(clippy::needless_pass_by_value)] // UniFFI passes arguments by value
pub async fn storm_ahead(
    place: Place,
    settings: Settings,
    cached_json: Option<String>,
    now: i64,
    host: Arc<dyn FetchHost>,
) -> Result<StormAheadOutcome, WhetherError> {
    // A cache that doesn't read is no cache: the hours are fetched again.
    let cached = cached_json
        .as_deref()
        .and_then(|json| from_json::<StormOutlook>("cached storm outlook", json).ok());
    if let (Some(json), Some(outlook)) = (cached_json, cached)
        && same_place(&outlook, &place)
        && !outlook.points.is_empty()
        && (0..STORM_AHEAD_MAX_AGE_SECS).contains(&(now - outlook.fetched_at.0))
    {
        return Ok(StormAheadOutcome {
            view_json: ahead_view(outlook, &place, &settings, now)?,
            outlook_json: json,
            refreshed: false,
            attempts: Vec::new(),
        });
    }

    let mut attempts = Vec::new();
    let loc = outgoing(&place);
    let Some(point) = exchange(
        &host,
        &point_request(&loc, &DETERMINISTIC_MODELS),
        "point",
        &mut attempts,
        |b| parse_points(b, &DETERMINISTIC_MODELS),
    )
    .await
    else {
        return Err(failed(&attempts));
    };
    let mut points = point.series;
    // No run times were read: fetch_storms takes this outlook as out of date.
    for p in &mut points {
        p.run = None;
    }
    let outlook = StormOutlook {
        place: location(&place, Some(&point.location)),
        offset_changes: place.core_offset_changes(),
        points,
        ensembles: Vec::new(),
        grids: Vec::new(),
        past_runs: Vec::new(),
        fetched_at: UnixTime(now),
    };
    Ok(StormAheadOutcome {
        outlook_json: to_json("storm outlook", &outlook)?,
        view_json: ahead_view(outlook, &place, &settings, now)?,
        refreshed: true,
        attempts,
    })
}

/// Seconds between `domain`'s runs when its `meta.json` doesn't say: GEM
/// runs twice a day, the others four times.
fn default_interval(domain: &str) -> i64 {
    if domain.starts_with("cmc_gem") {
        12 * 3_600
    } else {
        6 * 3_600
    }
}

/// The totals of `model`'s earlier runs over the window
/// `window_start..window_end`, for the run-to-run bars: up to `runs` (at most
/// six) runs before the one initialised at `latest_init`, newest first, as a
/// JSON list of `PastRun`.
///
/// Runs are spaced by the model's update interval from its `meta.json` (six
/// hours, twelve for GEM, when it can't be read). A run the Single Runs API
/// doesn't have, or one that doesn't reach the window, is left out. Past
/// runs never change: keep each under [`past_run_key`].
#[uniffi::export]
#[allow(clippy::needless_pass_by_value)] // UniFFI passes arguments by value
pub async fn fetch_past_runs(
    place: Place,
    model: String,
    window_start: i64,
    window_end: i64,
    latest_init: i64,
    runs: u32,
    host: Arc<dyn FetchHost>,
) -> Result<String, WhetherError> {
    let domain = run_domain(&model).ok_or_else(|| WhetherError::NoStorms {
        summary: format!("no runs are known for {model}"),
    })?;
    // The outcomes are not returned: a run that fails is simply not shown.
    let mut attempts = Vec::new();
    let interval = exchange(
        &host,
        &meta_request(domain),
        "meta",
        &mut attempts,
        parse_update_interval,
    )
    .await
    .map_or_else(|| default_interval(domain), i64::from);
    let loc = outgoing(&place);
    let (start, end) = (UnixTime(window_start), UnixTime(window_end));
    let mut past = Vec::new();
    for k in 1..=i64::from(runs.min(MAX_PAST_RUNS)) {
        let init = UnixTime(latest_init - k * interval);
        // A run whose hours don't cover the window has nothing to compare.
        let Ok(request) = single_run_request(&loc, &model, init, start, end) else {
            continue;
        };
        let run = ModelRun {
            model: domain.to_string(),
            init,
            available: init,
        };
        let Some(series) = exchange(&host, &request, "single run", &mut attempts, |b| {
            parse_single_run(b, &model, run.clone())
        })
        .await
        else {
            continue;
        };
        let (window_total_mm, window_snow_cm) = window_totals(&series, start, end);
        past.push(PastRun {
            model: model.clone(),
            run,
            window_start: start,
            window_end: end,
            window_total_mm,
            window_snow_cm,
        });
    }
    to_json("past runs", &past)
}

/// The key the app keeps an earlier run's totals under: the model, the run's
/// start and the window, which together fix the totals for good.
#[uniffi::export]
#[allow(clippy::needless_pass_by_value)] // UniFFI passes arguments by value
pub fn past_run_key(model: String, run_init: i64, window_start: i64, window_end: i64) -> String {
    format!("{model}@{run_init}:{window_start}-{window_end}")
}

/// The storms view (`StormsVm` as JSON) of a cached outlook at `now`, in
/// `place`'s units. `past_runs_json` is a JSON list of `PastRun` for the
/// run-to-run bars: what [`fetch_past_runs`] returned, or the kept runs of
/// several calls joined into one list.
#[uniffi::export]
#[allow(clippy::needless_pass_by_value)] // UniFFI passes arguments by value
pub fn storms_view(
    place: Place,
    outlook_json: String,
    past_runs_json: Option<String>,
    settings: Settings,
    now: i64,
) -> Result<String, WhetherError> {
    let outlook: StormOutlook = from_json("storm outlook", &outlook_json)?;
    let past: Vec<PastRun> = past_runs_json
        .as_deref()
        .map(|json| from_json("past runs", json))
        .transpose()?
        .unwrap_or_default();
    view(outlook, &past, &place, &settings, now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetch::{HostError, HostRequest, HostResponse};
    use crate::settings::{OffsetChange, default_settings};
    use std::sync::Mutex;

    const POINT: &[u8] =
        include_bytes!("../../whether-providers/tests/fixtures/open_meteo/storm_point_10001.json");
    const ENSEMBLE: &[u8] = include_bytes!(
        "../../whether-providers/tests/fixtures/open_meteo/storm_ensemble_10001.json"
    );
    const GRID: &[u8] =
        include_bytes!("../../whether-providers/tests/fixtures/open_meteo/storm_grid_10001.json");
    const SINGLE_RUN: &[u8] = include_bytes!(
        "../../whether-providers/tests/fixtures/open_meteo/storm_single_run_ecmwf_ifs025_10001.json"
    );
    const ERROR: &[u8] = include_bytes!(
        "../../whether-providers/tests/fixtures/open_meteo/storm_error.synthetic.json"
    );
    const METAS: [(&str, &[u8]); 6] = [
        (
            "ecmwf_ifs025",
            include_bytes!(
                "../../whether-providers/tests/fixtures/open_meteo/storm_meta_ecmwf_ifs025.json"
            ),
        ),
        (
            "ncep_gfs025",
            include_bytes!(
                "../../whether-providers/tests/fixtures/open_meteo/storm_meta_ncep_gfs025.json"
            ),
        ),
        (
            "cmc_gem_gdps_15km",
            include_bytes!(
                "../../whether-providers/tests/fixtures/open_meteo/storm_meta_cmc_gem_gdps_15km.json"
            ),
        ),
        (
            "dwd_icon",
            include_bytes!(
                "../../whether-providers/tests/fixtures/open_meteo/storm_meta_dwd_icon.json"
            ),
        ),
        (
            "ecmwf_ifs025_ensemble",
            include_bytes!(
                "../../whether-providers/tests/fixtures/open_meteo/storm_meta_ecmwf_ifs025_ensemble.json"
            ),
        ),
        (
            "ncep_gefs025",
            include_bytes!(
                "../../whether-providers/tests/fixtures/open_meteo/storm_meta_ncep_gefs025.json"
            ),
        ),
    ];

    /// Shortly after the recordings were made (2026-11-14 22:50 UTC).
    const NOW: i64 = 1_794_700_000;
    /// The latest ECMWF run in the recordings: 2026-11-14 12:00 UTC.
    const ECMWF_INIT: i64 = 1_794_657_600;
    const HOUR: i64 = 3_600;

    /// Answers the storm requests from the recorded New York responses.
    #[derive(Default)]
    struct Recorded {
        asked: Mutex<Vec<String>>,
        /// `meta.json` bodies that replace the recorded ones, by domain.
        meta: Vec<(&'static str, String)>,
        /// URLs containing one of these get a 503.
        down: Vec<&'static str>,
        /// `run=` values answered with an Open-Meteo error body.
        rejected: Vec<&'static str>,
    }

    impl Recorded {
        fn asked(&self) -> Vec<String> {
            self.asked.lock().unwrap().clone()
        }

        fn body(&self, url: &str) -> Option<Vec<u8>> {
            if let Some(rest) = url.strip_prefix("https://api.open-meteo.com/data/") {
                let domain = rest.strip_suffix("/static/meta.json")?;
                if let Some((_, body)) = self.meta.iter().find(|(d, _)| *d == domain) {
                    return Some(body.as_bytes().to_vec());
                }
                return METAS
                    .iter()
                    .find(|(d, _)| *d == domain)
                    .map(|(_, b)| b.to_vec());
            }
            let body: &[u8] = if url.starts_with("https://ensemble-api.open-meteo.com/v1/ensemble?")
            {
                ENSEMBLE
            } else if url.starts_with("https://single-runs-api.open-meteo.com/v1/forecast?") {
                if self
                    .rejected
                    .iter()
                    .any(|r| url.contains(&format!("&run={r}&")))
                {
                    ERROR
                } else {
                    SINGLE_RUN
                }
            } else if url.starts_with("https://api.open-meteo.com/v1/forecast?") {
                if url.contains("&temporal_resolution=hourly_6") {
                    GRID
                } else {
                    POINT
                }
            } else {
                return None;
            };
            Some(body.to_vec())
        }
    }

    #[async_trait::async_trait]
    impl FetchHost for Recorded {
        async fn send(&self, request: HostRequest) -> Result<HostResponse, HostError> {
            self.asked.lock().unwrap().push(request.url.clone());
            let reply = |status: u16, body: Vec<u8>| HostResponse {
                status,
                body,
                date: None,
                expires: None,
                last_modified: None,
            };
            if self.down.iter().any(|d| request.url.contains(d)) {
                return Ok(reply(503, Vec::new()));
            }
            let body = self.body(&request.url).ok_or_else(|| HostError::Failed {
                message: "no route".into(),
            })?;
            // Open-Meteo answers a rejected request with a 400.
            let status = if body.as_slice() == ERROR { 400 } else { 200 };
            Ok(reply(status, body))
        }

        fn save_usage(&self, _: String) -> Result<(), HostError> {
            panic!("storm requests are not metered")
        }
    }

    fn place() -> Place {
        Place {
            name: "New York".into(),
            latitude: 40.7143,
            longitude: -74.005_97,
            units: None,
            utc_offset_seconds: None,
            offset_changes: vec![OffsetChange {
                at: 1_805_180_400,
                offset_seconds: -4 * 3_600,
            }],
        }
    }

    fn fetch(
        host: &Arc<Recorded>,
        cached: Option<String>,
        now: i64,
    ) -> Result<StormsOutcome, WhetherError> {
        let shared: Arc<dyn FetchHost> = host.clone();
        pollster::block_on(fetch_storms(
            place(),
            default_settings(),
            cached,
            now,
            shared,
        ))
    }

    /// `url` without its scheme and query.
    fn host_path(url: &str) -> &str {
        let url = url.strip_prefix("https://").unwrap_or(url);
        url.split('?').next().unwrap_or(url)
    }

    fn meta_paths() -> Vec<String> {
        METAS
            .iter()
            .map(|(d, _)| format!("api.open-meteo.com/data/{d}/static/meta.json"))
            .collect()
    }

    fn outlook(outcome: &StormsOutcome) -> StormOutlook {
        serde_json::from_str(&outcome.outlook_json).unwrap()
    }

    #[test]
    fn a_fresh_fetch_reads_the_runs_then_the_data() {
        let net = Arc::new(Recorded::default());
        let outcome = fetch(&net, None, NOW).unwrap();
        assert!(outcome.refreshed);
        let asked = net.asked();
        let paths: Vec<&str> = asked.iter().map(|u| host_path(u)).collect();
        let mut expected = meta_paths();
        expected.extend([
            "api.open-meteo.com/v1/forecast".to_string(),
            "ensemble-api.open-meteo.com/v1/ensemble".to_string(),
            "api.open-meteo.com/v1/forecast".to_string(),
        ]);
        assert_eq!(paths, expected);
        assert!(asked[6].contains("latitude=40.71&longitude=-74.01&models="));
        assert!(asked[7].contains("latitude=40.71&longitude=-74.01&models="));
        assert!(asked[7].contains("&forecast_days=10&"));
        assert!(asked[8].contains("&temporal_resolution=hourly_6&"));
        assert_eq!(outcome.attempts.len(), 9);
        assert!(outcome.attempts.iter().all(|a| a.error.is_none()));
        assert_eq!(outcome.attempts[0].endpoint, "meta ecmwf_ifs025");
        assert_eq!(outcome.attempts[0].provider, "open-meteo");

        let o = outlook(&outcome);
        assert_eq!(
            (o.points.len(), o.ensembles.len(), o.grids.len()),
            (4, 2, 4)
        );
        assert_eq!(o.fetched_at, UnixTime(NOW));
        let ecmwf = o.points[0].run.as_ref().unwrap();
        assert_eq!(
            (ecmwf.model.as_str(), ecmwf.init),
            ("ecmwf_ifs025", UnixTime(ECMWF_INIT))
        );
        assert_eq!(o.grids[2].run.as_ref().unwrap().model, "cmc_gem_gdps_15km");
        assert_eq!(o.ensembles[1].run.as_ref().unwrap().model, "ncep_gefs025");
        // The place as the app has it; the offset from Open-Meteo, as the
        // place has none.
        assert_eq!(o.place.name, "New York");
        assert_eq!((o.place.latitude, o.place.longitude), (40.7143, -74.005_97));
        assert_eq!(o.place.utc_offset, UtcOffset(-18_000));
        assert_eq!(o.place.tz_name.as_deref(), Some("America/New_York"));
        assert_eq!(o.offset_changes, place().core_offset_changes());

        let vm: serde_json::Value = serde_json::from_str(&outcome.view_json).unwrap();
        assert_eq!(vm["place"], "New York");
        assert_eq!(vm["models"].as_array().unwrap().len(), 4);
    }

    #[test]
    fn a_current_cache_needs_only_the_run_times() {
        let first = fetch(&Arc::new(Recorded::default()), None, NOW).unwrap();
        let net = Arc::new(Recorded::default());
        let later = NOW + HOUR;
        let again = fetch(&net, Some(first.outlook_json.clone()), later).unwrap();
        assert!(!again.refreshed);
        let asked = net.asked();
        let paths: Vec<&str> = asked.iter().map(|u| host_path(u)).collect();
        assert_eq!(paths, meta_paths());
        assert_eq!(again.outlook_json, first.outlook_json);
        assert_eq!(again.attempts.len(), 6);
        assert_eq!(
            again.view_json,
            storms_view(place(), first.outlook_json, None, default_settings(), later).unwrap()
        );
    }

    #[test]
    fn a_newer_run_fetches_the_data_again() {
        let first = fetch(&Arc::new(Recorded::default()), None, NOW).unwrap();
        let net = Arc::new(Recorded {
            meta: vec![(
                "ncep_gefs025",
                r#"{"last_run_initialisation_time":1794679200,
                    "last_run_availability_time":1794699000,"update_interval_seconds":21600}"#
                    .into(),
            )],
            ..Recorded::default()
        });
        let again = fetch(&net, Some(first.outlook_json), NOW).unwrap();
        assert!(again.refreshed);
        assert_eq!(net.asked().len(), 9);
        let o = outlook(&again);
        assert_eq!(
            o.ensembles[1].run.as_ref().unwrap().init,
            UnixTime(1_794_679_200)
        );
        // A cache for another place, or one that doesn't read, is not used.
        let moved = Place {
            latitude: 42.36,
            ..place()
        };
        let net = Arc::new(Recorded::default());
        let shared: Arc<dyn FetchHost> = net.clone();
        let cached = Some(again.outlook_json);
        let out = pollster::block_on(fetch_storms(moved, default_settings(), cached, NOW, shared));
        assert!(out.unwrap().refreshed);
        assert!(
            fetch(&Arc::new(Recorded::default()), Some("{".into()), NOW)
                .unwrap()
                .refreshed
        );
    }

    #[test]
    fn a_failed_grid_leaves_the_rest() {
        let net = Arc::new(Recorded {
            down: vec!["temporal_resolution=hourly_6"],
            ..Recorded::default()
        });
        let outcome = fetch(&net, None, NOW).unwrap();
        let o = outlook(&outcome);
        assert_eq!(
            (o.points.len(), o.ensembles.len(), o.grids.len()),
            (4, 2, 0)
        );
        let grid = outcome.attempts.last().unwrap();
        assert_eq!(grid.endpoint, "grid");
        assert_eq!(grid.error.as_deref(), Some("HTTP 503"));
        // An outlook without its grid is fetched again next time.
        let net = Arc::new(Recorded::default());
        assert!(
            fetch(&net, Some(outcome.outlook_json), NOW)
                .unwrap()
                .refreshed
        );
        assert_eq!(net.asked().len(), 9);
    }

    #[test]
    fn nothing_fetched_is_an_error() {
        let net = Arc::new(Recorded {
            down: vec!["/v1/"],
            ..Recorded::default()
        });
        match fetch(&net, None, NOW) {
            Err(WhetherError::NoStorms { summary }) => {
                assert!(summary.contains("open-meteo point: HTTP 503"), "{summary}");
                assert!(
                    summary.contains("open-meteo meta dwd_icon: ok"),
                    "{summary}"
                );
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn without_run_times_a_young_cache_is_used() {
        let first = fetch(&Arc::new(Recorded::default()), None, NOW).unwrap();
        let blind = || {
            Arc::new(Recorded {
                down: vec!["meta.json"],
                ..Recorded::default()
            })
        };
        let net = blind();
        let soon = fetch(&net, Some(first.outlook_json.clone()), NOW + 5 * HOUR).unwrap();
        assert!(!soon.refreshed);
        assert_eq!(net.asked().len(), 6);
        assert!(soon.attempts.iter().all(|a| a.error.is_some()));
        // Older than one update interval: the data is fetched, without runs.
        let net = blind();
        let late = fetch(&net, Some(first.outlook_json), NOW + 6 * HOUR).unwrap();
        assert!(late.refreshed);
        assert_eq!(net.asked().len(), 9);
        let o = outlook(&late);
        assert!(o.points.iter().all(|p| p.run.is_none()));
        // Once the run times read again, that outlook is out of date.
        let net = Arc::new(Recorded::default());
        assert!(
            fetch(&net, Some(late.outlook_json), NOW + 6 * HOUR)
                .unwrap()
                .refreshed
        );
    }

    #[test]
    fn the_place_keeps_its_own_offset() {
        let resolved = ResolvedLocation {
            latitude: 40.75,
            longitude: -74.0,
            utc_offset: UtcOffset(-18_000),
            tz_name: Some("America/New_York".into()),
            tz_abbrev: Some("EST".into()),
            ..ResolvedLocation::default()
        };
        let own = Place {
            utc_offset_seconds: Some(-14_400),
            ..place()
        };
        let l = location(&own, Some(&resolved));
        assert_eq!((l.latitude, l.longitude), (40.7143, -74.005_97));
        assert_eq!(l.utc_offset, UtcOffset(-14_400));
        assert_eq!(
            (l.tz_name.as_deref(), l.tz_abbrev),
            (Some("America/New_York"), None)
        );
        let l = location(&place(), Some(&resolved));
        assert_eq!(
            (l.utc_offset, l.tz_abbrev.as_deref()),
            (UtcOffset(-18_000), Some("EST"))
        );
        assert_eq!(location(&place(), None).utc_offset, UtcOffset(0));
    }

    fn past_runs(net: &Arc<Recorded>, model: &str, start: i64, runs: u32) -> Vec<PastRun> {
        let shared: Arc<dyn FetchHost> = net.clone();
        let json = pollster::block_on(fetch_past_runs(
            place(),
            model.into(),
            start,
            start + 2 * 86_400,
            ECMWF_INIT,
            runs,
            shared,
        ))
        .unwrap();
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn earlier_runs_step_back_by_the_update_interval() {
        let net = Arc::new(Recorded {
            rejected: vec!["2026-11-14T00%3A00"],
            ..Recorded::default()
        });
        // From 2026-11-15 00:00 UTC, two days.
        let start = 1_795_132_800;
        let past = past_runs(&net, "ecmwf_ifs025", start, 3);
        let asked = net.asked();
        assert_eq!(
            host_path(&asked[0]),
            "api.open-meteo.com/data/ecmwf_ifs025/static/meta.json"
        );
        let runs: Vec<&str> = asked[1..]
            .iter()
            .map(|u| {
                assert_eq!(host_path(u), "single-runs-api.open-meteo.com/v1/forecast");
                assert!(u.contains("latitude=40.71&longitude=-74.01&models=ecmwf_ifs025&"));
                u.split("&run=").nth(1).unwrap().split('&').next().unwrap()
            })
            .collect();
        assert_eq!(
            runs,
            [
                "2026-11-14T06%3A00",
                "2026-11-14T00%3A00",
                "2026-11-13T18%3A00"
            ]
        );
        // The rejected run is left out.
        let inits: Vec<i64> = past.iter().map(|p| p.run.init.0).collect();
        assert_eq!(inits, [ECMWF_INIT - 6 * HOUR, ECMWF_INIT - 18 * HOUR]);
        for p in &past {
            assert_eq!(p.model, "ecmwf_ifs025");
            assert_eq!(p.run.model, "ecmwf_ifs025");
            assert_eq!(
                (p.window_start.0, p.window_end.0),
                (start, start + 2 * 86_400)
            );
            assert!(p.window_total_mm.is_some());
        }
        // At most six, and GEM steps back twelve hours when its file is down.
        let net = Arc::new(Recorded {
            down: vec!["meta.json"],
            ..Recorded::default()
        });
        past_runs(&net, "gem_seamless", start, 10);
        let asked = net.asked();
        assert_eq!(asked.len(), 7);
        assert!(
            asked[1].contains("&run=2026-11-14T00%3A00&"),
            "{}",
            asked[1]
        );
        assert!(
            asked[2].contains("&run=2026-11-13T12%3A00&"),
            "{}",
            asked[2]
        );
        // A model without runs is an error.
        let shared: Arc<dyn FetchHost> = net;
        let unknown = pollster::block_on(fetch_past_runs(
            place(),
            "bogus".into(),
            start,
            start + 1,
            ECMWF_INIT,
            1,
            shared,
        ));
        assert!(matches!(unknown, Err(WhetherError::NoStorms { .. })));
    }

    #[test]
    fn past_run_keys_name_the_run_and_window() {
        assert_eq!(
            past_run_key("ecmwf_ifs025".into(), 1_794_528_000, 10, 20),
            "ecmwf_ifs025@1794528000:10-20"
        );
        assert_ne!(
            past_run_key("ecmwf_ifs025".into(), 1, 10, 20),
            past_run_key("ecmwf_ifs025".into(), 1, 10, 21)
        );
    }

    #[test]
    fn the_view_round_trips_with_earlier_runs() {
        let outcome = fetch(&Arc::new(Recorded::default()), None, NOW).unwrap();
        let view = |past: Option<String>| {
            storms_view(
                place(),
                outcome.outlook_json.clone(),
                past,
                default_settings(),
                NOW,
            )
        };
        assert_eq!(view(None).unwrap(), outcome.view_json);
        assert_eq!(view(Some("[]".into())).unwrap(), outcome.view_json);
        let past = past_runs(
            &Arc::new(Recorded::default()),
            "ecmwf_ifs025",
            1_795_132_800,
            2,
        );
        let with_past = view(Some(serde_json::to_string(&past).unwrap())).unwrap();
        let vm: serde_json::Value = serde_json::from_str(&with_past).unwrap();
        assert_eq!(vm["place"], "New York");
        // A renamed place shows its new name before the next refresh.
        let home = Place {
            name: "Home".into(),
            ..place()
        };
        let renamed = storms_view(
            home,
            outcome.outlook_json.clone(),
            None,
            default_settings(),
            NOW,
        )
        .unwrap();
        let vm: serde_json::Value = serde_json::from_str(&renamed).unwrap();
        assert_eq!(vm["place"], "Home");
        assert!(matches!(
            view(Some("{".into())),
            Err(WhetherError::InvalidJson { .. })
        ));
        assert!(matches!(
            storms_view(place(), "[]".into(), None, default_settings(), NOW),
            Err(WhetherError::InvalidJson { .. })
        ));
    }

    fn check(
        host: &Arc<Recorded>,
        cached: Option<String>,
        now: i64,
    ) -> Result<StormAheadOutcome, WhetherError> {
        let shared: Arc<dyn FetchHost> = host.clone();
        pollster::block_on(storm_ahead(
            place(),
            default_settings(),
            cached,
            now,
            shared,
        ))
    }

    #[test]
    fn a_storm_check_asks_only_for_the_point() {
        let net = Arc::new(Recorded::default());
        let outcome = check(&net, None, NOW).unwrap();
        assert!(outcome.refreshed);
        let asked = net.asked();
        assert_eq!(asked.len(), 1);
        assert_eq!(host_path(&asked[0]), "api.open-meteo.com/v1/forecast");
        assert!(asked[0].contains("latitude=40.71&longitude=-74.01&models="));
        assert!(!asked[0].contains("temporal_resolution"));
        assert_eq!(outcome.attempts.len(), 1);
        assert_eq!(outcome.attempts[0].endpoint, "point");
        assert_eq!(outcome.attempts[0].error, None);

        let o: StormOutlook = serde_json::from_str(&outcome.outlook_json).unwrap();
        assert_eq!(
            (o.points.len(), o.ensembles.len(), o.grids.len()),
            (4, 0, 0)
        );
        assert!(o.points.iter().all(|p| p.run.is_none()));
        assert_eq!(o.fetched_at, UnixTime(NOW));
        assert_eq!(o.place.utc_offset, UtcOffset(-18_000));
        assert_eq!(o.offset_changes, place().core_offset_changes());
        // The recorded ten days in New York are dry (under 1 mm from any
        // model, gusts under 50 km/h): no event, so no card.
        assert_eq!(outcome.view_json, None);
        assert_eq!(whether_core::storm::analysis::find_events(&o), []);
    }

    #[test]
    fn a_young_storm_check_is_reused() {
        let first = check(&Arc::new(Recorded::default()), None, NOW).unwrap();
        let net = Arc::new(Recorded::default());
        let young = NOW + STORM_AHEAD_MAX_AGE_SECS - 1;
        let again = check(&net, Some(first.outlook_json.clone()), young).unwrap();
        assert_eq!(net.asked(), Vec::<String>::new());
        assert!(!again.refreshed);
        assert_eq!(again.attempts, []);
        assert_eq!(again.outlook_json, first.outlook_json);

        // Three hours old: the point is fetched again.
        let net = Arc::new(Recorded::default());
        let old = NOW + STORM_AHEAD_MAX_AGE_SECS;
        let late = check(&net, Some(first.outlook_json.clone()), old).unwrap();
        assert!(late.refreshed);
        assert_eq!(net.asked().len(), 1);
        let o: StormOutlook = serde_json::from_str(&late.outlook_json).unwrap();
        assert_eq!(o.fetched_at, UnixTime(old));

        // Another place, a cache that doesn't read or one from the future
        // is not used.
        let moved = Place {
            latitude: 42.36,
            ..place()
        };
        let net = Arc::new(Recorded::default());
        let shared: Arc<dyn FetchHost> = net.clone();
        let cached = Some(first.outlook_json.clone());
        let out = pollster::block_on(storm_ahead(moved, default_settings(), cached, NOW, shared));
        assert!(out.unwrap().refreshed);
        assert!(
            check(&Arc::new(Recorded::default()), Some("{".into()), NOW)
                .unwrap()
                .refreshed
        );
        assert!(
            check(
                &Arc::new(Recorded::default()),
                Some(first.outlook_json),
                NOW - 1
            )
            .unwrap()
            .refreshed
        );
    }

    #[test]
    fn a_full_outlook_serves_the_storm_check() {
        let full = fetch(&Arc::new(Recorded::default()), None, NOW).unwrap();
        let net = Arc::new(Recorded::default());
        let outcome = check(&net, Some(full.outlook_json.clone()), NOW + HOUR).unwrap();
        assert_eq!(net.asked(), Vec::<String>::new());
        assert!(!outcome.refreshed);
        assert_eq!(outcome.outlook_json, full.outlook_json);
        assert_eq!(outcome.view_json, None);
        // An outlook whose point failed has nothing to check.
        let pointless = fetch(
            &Arc::new(Recorded {
                down: vec!["api.open-meteo.com/v1/forecast?latitude"],
                ..Recorded::default()
            }),
            None,
            NOW,
        )
        .unwrap();
        assert_eq!(outlook(&pointless).points, []);
        let net = Arc::new(Recorded::default());
        assert!(
            check(&net, Some(pointless.outlook_json), NOW)
                .unwrap()
                .refreshed
        );
        assert_eq!(net.asked().len(), 1);
    }

    #[test]
    fn the_storms_screen_fetches_over_a_storm_check() {
        let light = check(&Arc::new(Recorded::default()), None, NOW).unwrap();
        // With the run times, and without them while the check is young.
        for down in [vec![], vec!["meta.json"]] {
            let net = Arc::new(Recorded {
                down,
                ..Recorded::default()
            });
            let full = fetch(&net, Some(light.outlook_json.clone()), NOW + HOUR).unwrap();
            assert!(full.refreshed);
            assert_eq!(net.asked().len(), 9);
            let o = outlook(&full);
            assert_eq!((o.ensembles.len(), o.grids.len()), (2, 4));
        }
    }

    #[test]
    fn a_failed_storm_check_is_an_error() {
        let net = Arc::new(Recorded {
            down: vec!["/v1/"],
            ..Recorded::default()
        });
        match check(&net, None, NOW) {
            Err(WhetherError::NoStorms { summary }) => {
                assert_eq!(summary, "open-meteo point: HTTP 503");
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(net.asked().len(), 1);
    }

    #[test]
    fn a_coming_storm_shows_its_card() {
        use whether_core::storm::testdata::{STORM_NOW, noreaster};
        let json = serde_json::to_string(&noreaster()).unwrap();
        let net = Arc::new(Recorded::default());
        let outcome = check(&net, Some(json), STORM_NOW.0).unwrap();
        assert_eq!(net.asked().len(), 0);
        let vm: serde_json::Value = serde_json::from_str(&outcome.view_json.unwrap()).unwrap();
        assert_eq!(vm["title"], "Storm ahead · Sat night – Sun night");
        assert_eq!(vm["line"], "Up to 35 cm of snow · models disagree");
        assert_eq!(vm["level"], "poor");
        assert_eq!(vm["agreement"], "Low agreement");
    }
}
