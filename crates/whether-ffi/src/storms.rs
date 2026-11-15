//! The storm outlook for the app: several models side by side at one place,
//! fetched from Open-Meteo with Swift doing the HTTP.
//!
//! A refresh first reads each model's `meta.json` (small static files). Only
//! when some model has a newer run than the cached outlook are the point, the
//! ensemble and the grid requested. Requests go out one after another, like
//! every other fetch here.
//!
//! Every refresh also reads the National Hurricane Center's list of active
//! cyclones (one small file, the same for every place). For each cyclone
//! that comes within [`HURRICANE_RANGE_KM`] of the place, its guidance and
//! official tracks (the a-deck) and its observed track (the b-deck) are
//! read, once per advisory: a cyclone whose advisory is already in the
//! cached outlook keeps its tracks.
//!
//! The "Storm ahead" check is lighter: it asks only for the models' hours at
//! the place, plus the active cyclones and the a-deck of each near one, and
//! not again while its outlook is younger than three hours.

use std::collections::BTreeMap;
use std::sync::Arc;

use whether_core::model::{ProviderId, ResolvedLocation};
use whether_core::storm::analysis::{distance_km, window_totals};
use whether_core::storm::hurricane::DRAWN_AIDS;
use whether_core::storm::{
    DETERMINISTIC_MODELS, ENSEMBLE_MODELS, Hurricane, ModelRun, PastRun, StormOutlook, TrackFix,
};
use whether_core::strings::Strings;
use whether_core::time::{UnixTime, UtcOffset};
use whether_core::view::build_storms;
use whether_providers::nhc::{
    ActiveStorm, ForecastTracks, adeck_request, bdeck_request, current_storms_request, parse_adeck,
    parse_bdeck, parse_current_storms,
};
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

/// How far from the place a cyclone is followed, in km: its position or a
/// point of its official forecast must be this close for its tracks to be
/// read and drawn.
pub(crate) const HURRICANE_RANGE_KM: f64 = 3_000.0;

/// The provider the hurricane requests are recorded under.
const NHC: &str = "nhc";

/// A storm outlook, fetched or rebuilt from the cache.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct StormsOutcome {
    /// The outlook as JSON: cache it per place and pass it back as
    /// `cached_json`, and to `storms_view`.
    pub outlook_json: String,
    /// The storms view (`StormsVm`) as JSON, built at `now`.
    pub view_json: String,
    /// The models' data was fetched now; `false` when the cached outlook was
    /// current and only the run times were read. The hurricanes are read on
    /// every refresh, so `outlook_json` can change either way.
    pub refreshed: bool,
    /// Every request's outcome: the run times, then the data, then the
    /// active storms and each near cyclone's decks.
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

/// Sends an Open-Meteo `request`, checks its status and parses its body
/// with `parse`, recording the outcome under `endpoint`.
async fn exchange<T>(
    host: &Arc<dyn FetchHost>,
    request: &HttpRequest,
    endpoint: &str,
    attempts: &mut Vec<AttemptInfo>,
    parse: impl FnOnce(&[u8]) -> Result<T, ProviderError>,
) -> Option<T> {
    let provider = name(&ProviderId::OpenMeteo);
    exchange_with(
        host,
        Ok(request.clone()),
        &provider,
        endpoint,
        attempts,
        parse,
    )
    .await
}

/// Like [`exchange`], for `provider`'s `request`, which may have failed to
/// build (and is then recorded as failed without being sent).
async fn exchange_with<T>(
    host: &Arc<dyn FetchHost>,
    request: Result<HttpRequest, ProviderError>,
    provider: &str,
    endpoint: &str,
    attempts: &mut Vec<AttemptInfo>,
    parse: impl FnOnce(&[u8]) -> Result<T, ProviderError>,
) -> Option<T> {
    let result = match request {
        Ok(request) => match host.send(host_request(&request)).await {
            Ok(resp) => check_status(&HttpResponse::new(resp.status, &resp.body))
                .and_then(|()| parse(&resp.body))
                .map_err(|e| e.to_string()),
            Err(e) => Err(e.to_string()),
        },
        Err(e) => Err(e.to_string()),
    };
    attempts.push(AttemptInfo {
        provider: provider.to_string(),
        endpoint: endpoint.to_string(),
        error: result.as_ref().err().cloned(),
    });
    result.ok()
}

/// Whether `position` (`(latitude, longitude)`) is within
/// [`HURRICANE_RANGE_KM`] of `place`.
fn within_range(place: &Place, (latitude, longitude): (f64, f64)) -> bool {
    distance_km(place.latitude, place.longitude, latitude, longitude) <= HURRICANE_RANGE_KM
}

/// Whether a cyclone at `position` with the official forecast `official`
/// comes within [`HURRICANE_RANGE_KM`] of `place`, now or on the forecast.
fn comes_near(place: &Place, position: (f64, f64), official: &[TrackFix]) -> bool {
    within_range(place, position) || official.iter().any(|f| within_range(place, f.position()))
}

/// Which tracks of a near cyclone a refresh reads.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Decks {
    /// The forecast tracks (a-deck) and the observed track (b-deck), for the
    /// Storms screen.
    All,
    /// Only the forecast tracks, which the cone needs, for the Storm ahead
    /// card.
    Forecast,
}

/// The active cyclones near `place`, with their tracks: the observed one
/// only with [`Decks::All`].
///
/// Reads NHC's active storms. A cyclone is near when its position, or a
/// point of its official forecast in `known` (the cached cyclones, from
/// any advisory), is within [`HURRICANE_RANGE_KM`]; only near ones get
/// their decks read, and only near ones (by the fresh forecast) are kept.
/// A cyclone whose advisory is in `known` keeps its tracks from there; a
/// track missing there (its deck failed or was not read) is read again.
/// A deck that fails leaves its tracks out, so a cyclone without its
/// a-deck has no cone. When the active storms can't be read, the near
/// cyclones in `known` are kept. Cyclones no longer active are dropped.
async fn hurricanes(
    host: &Arc<dyn FetchHost>,
    place: &Place,
    known: &[Hurricane],
    decks: Decks,
    attempts: &mut Vec<AttemptInfo>,
) -> Vec<Hurricane> {
    let Some(active) = exchange_with(
        host,
        Ok(current_storms_request()),
        NHC,
        "current storms",
        attempts,
        parse_current_storms,
    )
    .await
    else {
        return known
            .iter()
            .filter(|h| comes_near(place, (h.latitude, h.longitude), &h.official))
            .cloned()
            .collect();
    };
    let mut near = Vec::new();
    for storm in &active {
        if let Some(h) = hurricane(host, place, storm, known, decks, attempts).await {
            near.push(h);
        }
    }
    near
}

/// `storm` with its tracks when it comes near `place`, else `None`; see
/// [`hurricanes`].
async fn hurricane(
    host: &Arc<dyn FetchHost>,
    place: &Place,
    storm: &ActiveStorm,
    known: &[Hurricane],
    decks: Decks,
    attempts: &mut Vec<AttemptInfo>,
) -> Option<Hurricane> {
    let earlier = known.iter().find(|h| h.id == storm.id);
    let known_track = earlier.map_or(&[][..], |h| h.official.as_slice());
    if !comes_near(place, (storm.latitude, storm.longitude), known_track) {
        return None;
    }
    let advisory_time = storm
        .hurricane(ForecastTracks::default(), Vec::new())
        .advisory_time;
    let same = earlier.filter(|h| h.advisory_time == advisory_time);
    let forecast = match same.filter(|h| !h.official.is_empty()) {
        Some(h) => ForecastTracks {
            official: h.official.clone(),
            official_init: h.official_init,
            aids: h.aids.clone(),
        },
        None => exchange_with(
            host,
            adeck_request(&storm.id),
            NHC,
            &format!("adeck {}", storm.id),
            attempts,
            |b| parse_adeck(b, &DRAWN_AIDS),
        )
        .await
        .unwrap_or_default(),
    };
    let past = match same.filter(|h| !h.past.is_empty()) {
        Some(h) => h.past.clone(),
        None if decks == Decks::Forecast => Vec::new(),
        None => exchange_with(
            host,
            bdeck_request(&storm.id),
            NHC,
            &format!("bdeck {}", storm.id),
            attempts,
            parse_bdeck,
        )
        .await
        .unwrap_or_default(),
    };
    let h = storm.hurricane(forecast, past);
    comes_near(place, (h.latitude, h.longitude), &h.official).then_some(h)
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
/// A part without a run time is stale whenever the latest run is known, so
/// an outlook from [`storm_ahead`] is never current here.
///
/// A model missing from a part the cache has (its point, grid or ensemble)
/// is current when the outlook was fetched after the model's latest run
/// became available: that fetch got all there was, and Open-Meteo leaves
/// some models out of some requests. Without the latest run, or when the
/// whole part is missing (its request failed), a missing model makes the
/// cache stale, so the outlook is fetched again next time.
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
    // `run` is `None` when the model is missing from a part, `answered`
    // whether the part holds any model. Without the latest run, being there
    // is all that can be checked.
    let fresh = |run: Option<Option<&ModelRun>>, answered: bool, latest: Option<&ModelRun>| match (
        run, latest,
    ) {
        (Some(run), Some(latest)) => run.is_some_and(|r| r.init >= latest.init),
        (None, Some(latest)) => answered && cached.fetched_at >= latest.available,
        (run, None) => run.is_some(),
    };
    let deterministic = DETERMINISTIC_MODELS.iter().all(|m| {
        let latest = run_domain(m).and_then(|d| runs.get(d));
        let point = cached.points.iter().find(|p| p.model == *m);
        let grid = cached.grids.iter().find(|g| g.model == *m);
        fresh(
            point.map(|p| p.run.as_ref()),
            !cached.points.is_empty(),
            latest,
        ) && fresh(
            grid.map(|g| g.run.as_ref()),
            !cached.grids.is_empty(),
            latest,
        )
    });
    let ensembles = ENSEMBLE_MODELS.iter().all(|m| {
        let latest = ensemble_domain(m).and_then(|d| runs.get(d));
        let ensemble = cached.ensembles.iter().find(|e| e.model == *m);
        fresh(
            ensemble.map(|e| e.run.as_ref()),
            !cached.ensembles.is_empty(),
            latest,
        )
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
/// When every model's latest run is already in it (or, for a model a request
/// left out, it was fetched after that run was out), only the run times are
/// read and the view is rebuilt from it. Otherwise the models' hours at the
/// place, the ensembles and the pressure grid are fetched; a request that
/// fails leaves its part out (and shows in `attempts`), and only when all
/// three fail is it an error. If the run times can't be read, a cached
/// outlook younger than six hours is used, else the data is fetched.
///
/// Either way NHC's active storms are read, then the decks of each cyclone
/// that comes near and whose advisory is not in the cache yet. With
/// nothing active that is one small request; a current cache with known
/// advisories gets only its hurricanes replaced, never the models' data.
/// The hurricane requests never fail the outlook: without the active
/// storms the cached hurricanes are kept.
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
    let cached = cached_json.and_then(|json| {
        let outlook = from_json::<StormOutlook>("cached storm outlook", &json).ok()?;
        Some((json, outlook))
    });
    let known = match cached {
        Some((json, outlook)) if is_current(&outlook, &place, &runs, now) => {
            let hurricanes = hurricanes(
                &host,
                &place,
                &outlook.hurricanes,
                Decks::All,
                &mut attempts,
            )
            .await;
            let (outlook_json, outlook) = if hurricanes == outlook.hurricanes {
                (json, outlook)
            } else {
                let outlook = StormOutlook {
                    hurricanes,
                    ..outlook
                };
                (to_json("storm outlook", &outlook)?, outlook)
            };
            return Ok(StormsOutcome {
                view_json: view(outlook, &[], &place, &settings, now)?,
                outlook_json,
                refreshed: false,
                attempts,
            });
        }
        Some((_, outlook)) => outlook.hurricanes,
        None => Vec::new(),
    };

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
    let hurricanes = hurricanes(&host, &place, &known, Decks::All, &mut attempts).await;
    let outlook = StormOutlook {
        place: location(&place, resolved.as_ref()),
        // The zone's changes as the app computed them for the place, like the
        // forecast's; none when the place carries none.
        offset_changes: place.core_offset_changes(),
        points,
        ensembles,
        grids,
        past_runs: Vec::new(),
        hurricanes,
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
    /// The outlook the card was built from, as JSON. Keep it per place apart
    /// from the [`StormsOutcome::outlook_json`], so it never replaces the
    /// full outlook, and pass the newer of the two back as `cached_json`. A
    /// fetched one holds only the models' hours at the place, without run
    /// times, so [`fetch_storms`] fetches the full outlook when it is given
    /// one.
    pub outlook_json: String,
    /// The hours at the place were fetched now; `false` when the cached
    /// outlook was used.
    pub refreshed: bool,
    /// Every request's outcome: none when the cache was used, else the
    /// point request, the active storms and each near cyclone's a-deck.
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
/// `cached_json` is the newer of the outlooks kept for the place: the last
/// [`StormAheadOutcome::outlook_json`] or the last
/// [`StormsOutcome::outlook_json`], whichever was fetched later.
/// When it is for the place, holds the models' hours there and is younger than
/// [`STORM_AHEAD_MAX_AGE_SECS`], the card is built from it with no request.
/// Otherwise only the models' hours at the place are fetched (no run times,
/// ensembles or pressure grid), and the agreement comes from their totals.
/// It is an error when that request fails.
///
/// The card also warns of a hurricane whose forecast cone reaches the
/// place: a cached outlook carries its cyclones, and a fetch reads NHC's
/// active storms and the a-deck of each near cyclone whose advisory is not
/// in `cached_json` (no observed tracks), with the same failure rules as
/// [`fetch_storms`]. So a check costs one point request, one active storms
/// request and one a-deck per near cyclone, at most every three hours.
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
    let cached = cached_json.and_then(|json| {
        let outlook = from_json::<StormOutlook>("cached storm outlook", &json).ok()?;
        Some((json, outlook))
    });
    let young = |o: &StormOutlook| {
        same_place(o, &place)
            && !o.points.is_empty()
            && (0..STORM_AHEAD_MAX_AGE_SECS).contains(&(now - o.fetched_at.0))
    };
    let known = match cached {
        Some((json, outlook)) if young(&outlook) => {
            return Ok(StormAheadOutcome {
                view_json: ahead_view(outlook, &place, &settings, now)?,
                outlook_json: json,
                refreshed: false,
                attempts: Vec::new(),
            });
        }
        Some((_, outlook)) => outlook.hurricanes,
        None => Vec::new(),
    };

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
    let hurricanes = hurricanes(&host, &place, &known, Decks::Forecast, &mut attempts).await;
    let outlook = StormOutlook {
        place: location(&place, Some(&point.location)),
        offset_changes: place.core_offset_changes(),
        points,
        ensembles: Vec::new(),
        grids: Vec::new(),
        past_runs: Vec::new(),
        hurricanes,
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
    const NO_STORMS: &[u8] =
        include_bytes!("../../whether-providers/tests/fixtures/nhc/current_storms.json");
    const JOSEPHINE: &str = include_str!(
        "../../whether-providers/tests/fixtures/nhc/current_storms_josephine.synthetic.json"
    );
    const ADECK: &[u8] =
        include_bytes!("../../whether-providers/tests/fixtures/nhc/aal102026.dat.gz");
    const BDECK: &[u8] = include_bytes!("../../whether-providers/tests/fixtures/nhc/bal102026.dat");
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
        /// NHC's active storms; none when `None`, as in the live file.
        storms: Option<String>,
    }

    impl Recorded {
        fn asked(&self) -> Vec<String> {
            self.asked.lock().unwrap().clone()
        }

        fn body(&self, url: &str) -> Option<Vec<u8>> {
            let nhc: Option<&[u8]> = match url {
                "https://www.nhc.noaa.gov/CurrentStorms.json" => {
                    Some(self.storms.as_ref().map_or(NO_STORMS, |s| s.as_bytes()))
                }
                "https://ftp.nhc.noaa.gov/atcf/aid_public/aal102026.dat.gz" => Some(ADECK),
                "https://ftp.nhc.noaa.gov/atcf/btk/bal102026.dat" => Some(BDECK),
                _ => None,
            };
            if let Some(body) = nhc {
                return Some(body.to_vec());
            }
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

    const CURRENT_STORMS: &str = "www.nhc.noaa.gov/CurrentStorms.json";

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
            CURRENT_STORMS.to_string(),
        ]);
        assert_eq!(paths, expected);
        assert!(asked[6].contains("latitude=40.71&longitude=-74.01&models="));
        assert!(asked[7].contains("latitude=40.71&longitude=-74.01&models="));
        assert!(asked[7].contains("&forecast_days=10&"));
        assert!(asked[8].contains("&temporal_resolution=hourly_6&"));
        assert_eq!(outcome.attempts.len(), 10);
        assert!(outcome.attempts.iter().all(|a| a.error.is_none()));
        assert_eq!(outcome.attempts[0].endpoint, "meta ecmwf_ifs025");
        assert_eq!(outcome.attempts[0].provider, "open-meteo");
        // Nothing is active: one small request, and no hurricanes.
        assert_eq!(
            (
                outcome.attempts[9].provider.as_str(),
                outcome.attempts[9].endpoint.as_str()
            ),
            ("nhc", "current storms")
        );

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
        assert_eq!(o.hurricanes, []);

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
        let mut expected = meta_paths();
        expected.push(CURRENT_STORMS.to_string());
        assert_eq!(paths, expected);
        assert_eq!(again.outlook_json, first.outlook_json);
        assert_eq!(again.attempts.len(), 7);
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
        assert_eq!(net.asked().len(), 10);
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
        let grid = outcome
            .attempts
            .iter()
            .find(|a| a.endpoint == "grid")
            .unwrap();
        assert_eq!(grid.endpoint, "grid");
        assert_eq!(grid.error.as_deref(), Some("HTTP 503"));
        // An outlook without its grid is fetched again next time.
        let net = Arc::new(Recorded::default());
        assert!(
            fetch(&net, Some(outcome.outlook_json), NOW)
                .unwrap()
                .refreshed
        );
        assert_eq!(net.asked().len(), 10);
    }

    /// GEM's latest run in the recordings became available at this time.
    const GEM_AVAILABLE: i64 = 1_794_678_853;

    #[test]
    fn a_model_left_out_after_its_run_was_out_is_current() {
        let first = fetch(&Arc::new(Recorded::default()), None, NOW).unwrap();
        let without_gem_grid = |fetched_at: i64| {
            let mut o = outlook(&first);
            o.grids.retain(|g| g.model != "gem_seamless");
            o.fetched_at = UnixTime(fetched_at);
            serde_json::to_string(&o).unwrap()
        };
        // Fetched after GEM's run was out: that fetch got all there was.
        let net = Arc::new(Recorded::default());
        let again = fetch(&net, Some(without_gem_grid(NOW)), NOW + HOUR).unwrap();
        assert!(!again.refreshed);
        let asked = net.asked();
        let paths: Vec<&str> = asked.iter().map(|u| host_path(u)).collect();
        assert_eq!(paths[..6], meta_paths());
        assert_eq!(paths[6..], [CURRENT_STORMS]);
        // Fetched before it: the new run may bring the grid.
        let net = Arc::new(Recorded::default());
        let before = without_gem_grid(GEM_AVAILABLE - 1);
        assert!(fetch(&net, Some(before), NOW + HOUR).unwrap().refreshed);
        assert_eq!(net.asked().len(), 10);
        // Without GEM's run time, a missing grid stays stale.
        let net = Arc::new(Recorded {
            down: vec!["cmc_gem_gdps_15km/static/meta.json"],
            ..Recorded::default()
        });
        let blind = fetch(&net, Some(without_gem_grid(NOW)), NOW + HOUR).unwrap();
        assert!(blind.refreshed);
        // A check's point-only outlook, though fetched later, stays stale.
        let light = check(&Arc::new(Recorded::default()), None, NOW).unwrap();
        let net = Arc::new(Recorded::default());
        assert!(
            fetch(&net, Some(light.outlook_json), NOW + HOUR)
                .unwrap()
                .refreshed
        );
        assert_eq!(net.asked().len(), 10);
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
        assert_eq!(net.asked().len(), 7);
        let runs = soon.attempts.iter().filter(|a| a.provider == "open-meteo");
        assert!(runs.clone().count() == 6 && runs.clone().all(|a| a.error.is_some()));
        // Older than one update interval: the data is fetched, without runs.
        let net = blind();
        let late = fetch(&net, Some(first.outlook_json), NOW + 6 * HOUR).unwrap();
        assert!(late.refreshed);
        assert_eq!(net.asked().len(), 10);
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

    /// Josephine's advisory 5: 2026-10-22 21:00 UTC.
    const ADVISORY: i64 = 1_792_702_800;
    /// Her next advisory, six hours later.
    const NEXT_ADVISORY: i64 = ADVISORY + 6 * HOUR;
    const ADECK_PATH: &str = "ftp.nhc.noaa.gov/atcf/aid_public/aal102026.dat.gz";
    const BDECK_PATH: &str = "ftp.nhc.noaa.gov/atcf/btk/bal102026.dat";

    /// A made-up place on Josephine's official track, at its 36-hour
    /// position, over 900 km from her.
    fn on_the_track() -> Place {
        Place {
            name: "Test Point".into(),
            latitude: 43.5,
            longitude: -55.9,
            units: None,
            utc_offset_seconds: Some(-3 * 3_600),
            offset_changes: Vec::new(),
        }
    }

    /// The hand-made summary with Josephine, as served.
    fn josephine() -> String {
        JOSEPHINE.to_string()
    }

    /// The summary at her next advisory.
    fn josephine_next() -> String {
        JOSEPHINE.replace("2026-10-22T21:00:00.000Z", "2026-10-23T03:00:00.000Z")
    }

    /// `summary` with Josephine moved to the eastern Atlantic (15° N,
    /// 35° W), over 4,700 km from New York.
    fn far_away(summary: &str) -> String {
        summary
            .replace(r#""latitude": "39.5N""#, r#""latitude": "15.0N""#)
            .replace(r#""longitude": "66.8W""#, r#""longitude": "35.0W""#)
            .replace(r#""latitudeNumeric": 39.5"#, r#""latitudeNumeric": 15.0"#)
            .replace(
                r#""longitudeNumeric": -66.8"#,
                r#""longitudeNumeric": -35.0"#,
            )
    }

    fn with_storms(summary: String) -> Arc<Recorded> {
        Arc::new(Recorded {
            storms: Some(summary),
            ..Recorded::default()
        })
    }

    fn fetch_at(
        host: &Arc<Recorded>,
        place: Place,
        cached: Option<String>,
        now: i64,
    ) -> StormsOutcome {
        let shared: Arc<dyn FetchHost> = host.clone();
        pollster::block_on(fetch_storms(place, default_settings(), cached, now, shared)).unwrap()
    }

    /// The paths asked for after the run times.
    fn after_runs(net: &Recorded) -> Vec<String> {
        let asked = net.asked();
        let runs: Vec<String> = asked[..6]
            .iter()
            .map(|u| host_path(u).to_string())
            .collect();
        assert_eq!(runs, meta_paths());
        asked[6..]
            .iter()
            .map(|u| host_path(u).to_string())
            .collect()
    }

    fn nhc_attempts(outcome: &StormsOutcome) -> Vec<(&str, Option<&str>)> {
        outcome
            .attempts
            .iter()
            .filter(|a| a.provider == "nhc")
            .map(|a| (a.endpoint.as_str(), a.error.as_deref()))
            .collect()
    }

    #[test]
    fn a_near_hurricane_brings_its_tracks() {
        let net = with_storms(josephine());
        let outcome = fetch_at(&net, on_the_track(), None, NOW);
        assert!(outcome.refreshed);
        assert_eq!(
            after_runs(&net),
            [
                "api.open-meteo.com/v1/forecast",
                "ensemble-api.open-meteo.com/v1/ensemble",
                "api.open-meteo.com/v1/forecast",
                CURRENT_STORMS,
                ADECK_PATH,
                BDECK_PATH
            ]
        );
        assert_eq!(
            nhc_attempts(&outcome),
            [
                ("current storms", None),
                ("adeck al102026", None),
                ("bdeck al102026", None)
            ]
        );
        let o = outlook(&outcome);
        assert_eq!(o.hurricanes.len(), 1);
        let h = &o.hurricanes[0];
        assert_eq!((h.id.as_str(), h.name.as_str()), ("al102026", "Josephine"));
        assert_eq!(h.advisory_time, UnixTime(ADVISORY));
        assert!(!h.official.is_empty() && !h.aids.is_empty() && !h.past.is_empty());
        let vm: serde_json::Value = serde_json::from_str(&outcome.view_json).unwrap();
        assert_eq!(vm["hurricanes"][0]["id"], "al102026");
        assert_ne!(vm["hurricanes"][0]["cone"], serde_json::json!([]));
    }

    #[test]
    fn a_known_advisory_reads_no_decks() {
        let first = fetch_at(&with_storms(josephine()), on_the_track(), None, NOW);
        let net = with_storms(josephine());
        let cached = Some(first.outlook_json.clone());
        let again = fetch_at(&net, on_the_track(), cached, NOW + HOUR);
        assert!(!again.refreshed);
        assert_eq!(after_runs(&net), [CURRENT_STORMS]);
        assert_eq!(again.outlook_json, first.outlook_json);
    }

    #[test]
    fn a_new_advisory_reads_the_decks_and_keeps_the_models() {
        let first = fetch_at(&with_storms(josephine()), on_the_track(), None, NOW);
        let net = with_storms(josephine_next());
        let cached = Some(first.outlook_json.clone());
        let again = fetch_at(&net, on_the_track(), cached, NOW + HOUR);
        // The models' runs are current: none of their data is asked for.
        assert!(!again.refreshed);
        assert_eq!(after_runs(&net), [CURRENT_STORMS, ADECK_PATH, BDECK_PATH]);
        let (before, after) = (outlook(&first), outlook(&again));
        assert_eq!(after.hurricanes[0].advisory_time, UnixTime(NEXT_ADVISORY));
        assert_eq!(
            StormOutlook {
                hurricanes: before.hurricanes.clone(),
                ..after
            },
            before
        );
        assert_ne!(again.outlook_json, first.outlook_json);
    }

    #[test]
    fn a_failed_adeck_leaves_the_hurricane_without_a_cone() {
        let net = Arc::new(Recorded {
            storms: Some(josephine()),
            down: vec!["aid_public"],
            ..Recorded::default()
        });
        let outcome = fetch_at(&net, on_the_track(), None, NOW);
        assert_eq!(
            nhc_attempts(&outcome),
            [
                ("current storms", None),
                ("adeck al102026", Some("HTTP 503")),
                ("bdeck al102026", None)
            ]
        );
        let o = outlook(&outcome);
        let h = &o.hurricanes[0];
        assert!(h.official.is_empty() && h.aids.is_empty() && !h.past.is_empty());
        let vm: serde_json::Value = serde_json::from_str(&outcome.view_json).unwrap();
        assert_eq!(vm["hurricanes"][0]["cone"], serde_json::json!([]));
        // The missing a-deck is read again for the same advisory; the
        // b-deck is not.
        let net = with_storms(josephine());
        let again = fetch_at(&net, on_the_track(), Some(outcome.outlook_json), NOW + HOUR);
        assert_eq!(after_runs(&net), [CURRENT_STORMS, ADECK_PATH]);
        assert_eq!(outlook(&again).hurricanes[0].official.len(), 7);
    }

    #[test]
    fn only_near_hurricanes_are_read() {
        // Far from New York, with nothing known of her track: no decks.
        let net = with_storms(far_away(&josephine()));
        let outcome = fetch_at(&net, place(), None, NOW);
        assert_eq!(after_runs(&net)[3..], [CURRENT_STORMS]);
        assert_eq!(outlook(&outcome).hurricanes, []);
        // Where she really was, about 630 km out, she is near.
        let net = with_storms(josephine());
        let near = fetch_at(&net, place(), None, NOW);
        assert_eq!(
            after_runs(&net)[3..],
            [CURRENT_STORMS, ADECK_PATH, BDECK_PATH]
        );
        // A known forecast that comes near counts too, before the new
        // advisory's track is read.
        let net = with_storms(far_away(&josephine_next()));
        let again = fetch_at(&net, place(), Some(near.outlook_json), NOW + HOUR);
        assert_eq!(after_runs(&net), [CURRENT_STORMS, ADECK_PATH, BDECK_PATH]);
        let h = &outlook(&again).hurricanes[0];
        assert_eq!(
            (h.latitude, h.advisory_time),
            (15.0, UnixTime(NEXT_ADVISORY))
        );
    }

    #[test]
    fn without_the_active_storms_the_known_hurricanes_stay() {
        let first = fetch_at(&with_storms(josephine()), on_the_track(), None, NOW);
        let net = Arc::new(Recorded {
            down: vec!["CurrentStorms"],
            ..Recorded::default()
        });
        let cached = Some(first.outlook_json.clone());
        let again = fetch_at(&net, on_the_track(), cached, NOW + HOUR);
        assert_eq!(after_runs(&net), [CURRENT_STORMS]);
        assert_eq!(nhc_attempts(&again), [("current storms", Some("HTTP 503"))]);
        assert_eq!(again.outlook_json, first.outlook_json);
        // Once she is no longer active, she is gone; the models stay.
        let net = Arc::new(Recorded::default());
        let cached = Some(first.outlook_json.clone());
        let gone = fetch_at(&net, on_the_track(), cached, NOW + HOUR);
        assert!(!gone.refreshed);
        assert_eq!(after_runs(&net), [CURRENT_STORMS]);
        let o = outlook(&gone);
        assert_eq!(o.hurricanes, []);
        assert_eq!(o.points, outlook(&first).points);
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
    fn a_storm_check_asks_for_the_point_and_the_active_storms() {
        let net = Arc::new(Recorded::default());
        let outcome = check(&net, None, NOW).unwrap();
        assert!(outcome.refreshed);
        let asked = net.asked();
        assert_eq!(asked.len(), 2);
        assert_eq!(host_path(&asked[0]), "api.open-meteo.com/v1/forecast");
        assert!(asked[0].contains("latitude=40.71&longitude=-74.01&models="));
        assert!(!asked[0].contains("temporal_resolution"));
        assert_eq!(host_path(&asked[1]), CURRENT_STORMS);
        assert_eq!(outcome.attempts.len(), 2);
        assert_eq!(outcome.attempts[0].endpoint, "point");
        assert_eq!(outcome.attempts[0].error, None);
        assert_eq!(
            (
                outcome.attempts[1].provider.as_str(),
                outcome.attempts[1].endpoint.as_str()
            ),
            ("nhc", "current storms")
        );

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
        assert_eq!(whether_core::storm::analysis::all_events(&o), []);
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
        assert_eq!(net.asked().len(), 2);
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
        assert_eq!(net.asked().len(), 2);
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
            assert_eq!(net.asked().len(), 10);
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

    /// Half an hour after Josephine's advisory 5.
    const JOSEPHINE_NOW: i64 = ADVISORY + 1_800;

    fn check_at(
        host: &Arc<Recorded>,
        place: Place,
        cached: Option<String>,
        now: i64,
    ) -> StormAheadOutcome {
        let shared: Arc<dyn FetchHost> = host.clone();
        pollster::block_on(storm_ahead(place, default_settings(), cached, now, shared)).unwrap()
    }

    #[test]
    fn a_cone_reaching_the_place_shows_the_hurricane_card() {
        let net = with_storms(josephine());
        let outcome = check_at(&net, on_the_track(), None, JOSEPHINE_NOW);
        // No observed track: the card needs only the forecast.
        let asked = net.asked();
        let paths: Vec<&str> = asked.iter().map(|u| host_path(u)).collect();
        assert_eq!(
            paths,
            ["api.open-meteo.com/v1/forecast", CURRENT_STORMS, ADECK_PATH]
        );
        let endpoints: Vec<(&str, &str)> = outcome
            .attempts
            .iter()
            .map(|a| (a.provider.as_str(), a.endpoint.as_str()))
            .collect();
        assert_eq!(
            endpoints,
            [
                ("open-meteo", "point"),
                ("nhc", "current storms"),
                ("nhc", "adeck al102026")
            ]
        );
        let vm: serde_json::Value = serde_json::from_str(&outcome.view_json.unwrap()).unwrap();
        assert_eq!(vm["title"], "Subtropical Storm Josephine");
        assert_eq!(vm["line"], "The forecast cone reaches Test Point Fri");
        assert_eq!(vm["hurricane"], "al102026");
        let o: StormOutlook = serde_json::from_str(&outcome.outlook_json).unwrap();
        let h = &o.hurricanes[0];
        assert!(h.past.is_empty() && h.official.len() == 7);

        // A young check is reused with its hurricane, with no request.
        let net = with_storms(josephine());
        let cached = Some(outcome.outlook_json.clone());
        let again = check_at(&net, on_the_track(), cached, JOSEPHINE_NOW + HOUR);
        assert_eq!(net.asked(), Vec::<String>::new());
        assert!(again.view_json.unwrap().contains("al102026"));

        // An old one keeps the known advisory's tracks: no a-deck.
        let net = with_storms(josephine());
        let later = JOSEPHINE_NOW + STORM_AHEAD_MAX_AGE_SECS;
        let cached = Some(outcome.outlook_json.clone());
        let late = check_at(&net, on_the_track(), cached, later);
        assert_eq!(net.asked().len(), 2);
        assert!(late.view_json.unwrap().contains("al102026"));

        // The Storms screen reads only the missing b-deck.
        let net = with_storms(josephine());
        let full = fetch_at(&net, on_the_track(), Some(outcome.outlook_json), NOW);
        assert_eq!(after_runs(&net)[3..], [CURRENT_STORMS, BDECK_PATH]);
        assert_eq!(outlook(&full).hurricanes[0].official.len(), 7);
    }

    #[test]
    fn a_failed_adeck_leaves_the_card_to_the_models() {
        let net = Arc::new(Recorded {
            storms: Some(josephine()),
            down: vec!["aid_public"],
            ..Recorded::default()
        });
        let outcome = check_at(&net, on_the_track(), None, JOSEPHINE_NOW);
        let failed = outcome.attempts.last().unwrap();
        assert_eq!(
            (failed.endpoint.as_str(), failed.error.as_deref()),
            ("adeck al102026", Some("HTTP 503"))
        );
        // Without a cone the recorded dry week brings no card.
        assert_eq!(outcome.view_json, None);
        let o: StormOutlook = serde_json::from_str(&outcome.outlook_json).unwrap();
        assert_eq!(o.hurricanes.len(), 1);
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
