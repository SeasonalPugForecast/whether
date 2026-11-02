//! The fetch service: keeps every location's snapshot fresh and publishes the
//! application state. `Worker` holds the logic; `spawn_service` runs it on a thread
//! and `refresh_once` runs one pass for displays that print and exit.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::thread::JoinHandle;
use std::time::Duration;

use whether_config::{ConfigError, Issue, LinuxConfig, LocationConfig, Severity};
use whether_core::model::{Alert, ProviderId, ResolvedLocation};
use whether_core::requirements::DataRequirements;
use whether_core::time::UnixTime;
use whether_display::{
    AppState, Command, LoadedConfig, LocationState, SearchState, StatePublisher, now,
};
use whether_providers::{
    Attempt, Coordinates, FetchContext, FetchError, GeocodeQuery, HistoryContext, HttpRequest,
};

use crate::cache::{Cache, CachedSnapshot, SCHEMA};
use crate::fetch::fetch_location;
use crate::history::{
    CachedHistory, RETRY_AFTER_S, build_history_chain, fetch_history, history_due, history_key,
    keep_history,
};
use crate::locate::{Geocoder, resolve_location};
use crate::providers::{Chain, build_chain};
use crate::scheduler::{Jitter, Scheduler};
use crate::transport::{OwnedResponse, Transport, TransportError, user_agent};
use crate::usage::UsageStore;
use crate::watch::ConfigWatch;

/// Environment lookups shared with the service thread.
pub type Env = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;
/// Reloads the configuration (`ReloadConfig`, and after location edits).
pub type Reload = Box<dyn Fn() -> Result<LinuxConfig, ConfigError> + Send>;

/// How the service runs.
pub struct ServiceOptions {
    /// Never touch the network; show the cache only.
    pub offline: bool,
    /// Environment lookups for secrets.
    pub env: Env,
    /// The One Call ledger; without it, metered providers are not used.
    pub usage: Option<UsageStore>,
    /// What to fetch.
    pub requirements: DataRequirements,
    /// The file `AddLocation`/`RemoveLocation` edit.
    pub edit_path: Option<PathBuf>,
    /// How to reload the configuration.
    pub reload: Option<Reload>,
    /// Radar imagery for the `radar` widget, fetched on its own thread.
    pub radar: Option<RadarOptions>,
}

/// What the radar thread needs.
pub struct RadarOptions {
    /// Whether a presentation shows the radar, until the configuration is published.
    pub wanted: bool,
    /// Map zoom (`[radar] zoom`), likewise.
    pub zoom: u8,
    /// Its own transport, so imagery never waits behind forecasts.
    pub transport: Box<dyn Transport>,
}

impl std::fmt::Debug for RadarOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RadarOptions")
            .field("zoom", &self.zoom)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for ServiceOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceOptions")
            .field("offline", &self.offline)
            .field("edit_path", &self.edit_path)
            .finish_non_exhaustive()
    }
}

/// A transport that never connects (offline mode and cache-only resolution).
#[derive(Debug)]
struct Offline;

impl Transport for Offline {
    fn send(&self, _: &HttpRequest) -> Result<OwnedResponse, TransportError> {
        Err(TransportError::Failed("offline".into()))
    }
}

struct Slot {
    chain: Chain,
    resolved: Option<ResolvedLocation>,
    /// The history as cached, once read.
    history: Option<CachedHistory>,
    /// Whether the cache was read.
    history_loaded: bool,
    /// When the last history fetch failed, to wait before the next.
    history_failed_at: Option<UnixTime>,
}

/// The per-location fetch logic, independent of threads.
pub struct Worker {
    cfg: Arc<LinuxConfig>,
    publisher: StatePublisher,
    transport: Box<dyn Transport>,
    cache: Cache,
    opts: ServiceOptions,
    slots: Vec<Slot>,
    scheduler: Scheduler,
    jitter: Jitter,
}

impl std::fmt::Debug for Worker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Worker")
            .field("locations", &self.slots.len())
            .finish_non_exhaustive()
    }
}

/// "OpenWeather: unauthorized: …" for each failed attempt.
fn notes(attempts: &[Attempt]) -> Vec<String> {
    attempts
        .iter()
        .filter_map(|a| {
            a.error
                .as_ref()
                .map(|e| format!("{}: {e}", a.provider.display_name()))
        })
        .collect()
}

impl Worker {
    /// A worker for `cfg`, publishing initial (empty) location states.
    pub fn new(
        cfg: Arc<LinuxConfig>,
        publisher: StatePublisher,
        transport: Box<dyn Transport>,
        cache: Cache,
        opts: ServiceOptions,
    ) -> Self {
        let jitter = Jitter::new(u64::from(std::process::id()));
        let mut worker = Self {
            cfg,
            publisher,
            transport,
            cache,
            opts,
            slots: Vec::new(),
            scheduler: Scheduler::new(0, 0, &[]),
            jitter,
        };
        worker.rebuild();
        worker
    }

    /// Rebuilds slots, scheduler and the published locations from `self.cfg`, keeping
    /// existing snapshots of locations that kept their name.
    fn rebuild(&mut self) {
        let env = Arc::clone(&self.opts.env);
        let cfg = Arc::clone(&self.cfg);
        self.slots = cfg
            .common
            .location
            .iter()
            .map(|loc| Slot {
                chain: build_chain(&cfg.common.providers, loc, &*env),
                resolved: None,
                history: None,
                history_loaded: false,
                history_failed_at: None,
            })
            .collect();
        let req = self.opts.requirements;
        let metered = self.slots.iter().filter(|s| s.chain.is_metered()).count();
        let min_intervals: Vec<u32> = self
            .slots
            .iter()
            .map(|slot| {
                let paced = slot
                    .chain
                    .primary
                    .iter()
                    .filter_map(|p| {
                        let budget = p.budget()?;
                        let count = u32::try_from(metered).unwrap_or(u32::MAX);
                        Some(budget.paced_refresh_secs(p.metered_cost_per_fetch(&req), count))
                    })
                    .max()
                    .unwrap_or(0);
                let provider_min = slot
                    .chain
                    .primary
                    .first()
                    .map_or(0, |p| p.min_refresh_secs());
                paced.max(provider_min)
            })
            .collect();
        self.scheduler = Scheduler::new(
            self.slots.len(),
            cfg.common.general.effective_refresh().secs(),
            &min_intervals,
        );
        let issues = cfg.issues.clone();
        let loaded = LoadedConfig(Arc::clone(&cfg));
        self.publisher.update(|state| {
            state.config = Some(loaded);
            let old = std::mem::take(&mut state.locations);
            state.locations = cfg
                .common
                .location
                .iter()
                .map(|loc| {
                    let name = loc.name.clone().unwrap_or_default();
                    match old.iter().find(|o| o.name == name) {
                        Some(o) => LocationState {
                            config: loc.clone(),
                            ..o.clone()
                        },
                        None => LocationState::new(loc.clone()),
                    }
                })
                .collect();
            state.config_issues = issues;
        });
    }

    fn refresh_s(&self) -> i64 {
        i64::from(self.cfg.common.general.effective_refresh().secs())
    }

    fn geocoder<'a>(&'a self, transport: &'a dyn Transport) -> Geocoder<'a> {
        Geocoder {
            providers: &self.cfg.common.providers,
            env: &*self.opts.env,
            transport,
            language: &self.cfg.common.general.language,
        }
    }

    /// Resolves location `i`, from the geocode cache only when `offline`.
    fn resolve(
        &mut self,
        i: usize,
        now: UnixTime,
        offline: bool,
    ) -> Result<ResolvedLocation, String> {
        if let Some(r) = self.slots[i].resolved.clone() {
            return Ok(r);
        }
        let loc = self.cfg.common.location[i].clone();
        let transport: &dyn Transport = if offline { &Offline } else { &*self.transport };
        let resolved = resolve_location(&loc, &self.geocoder(transport), &self.cache, now)
            .map_err(|e| e.to_string())?;
        self.slots[i].resolved = Some(resolved.clone());
        let published = resolved.clone();
        self.publisher
            .update(|s| s.locations[i].resolved = Some(published));
        Ok(resolved)
    }

    fn coordinates(r: &ResolvedLocation) -> Coordinates {
        Coordinates {
            latitude: r.latitude,
            longitude: r.longitude,
        }
    }

    /// Every cached snapshot of location `i`, one per provider that has one.
    fn cached_all(&self, i: usize, r: &ResolvedLocation) -> Vec<CachedSnapshot> {
        let mut ids: Vec<ProviderId> = self.slots[i].chain.primary.iter().map(|p| p.id()).collect();
        if !ids.contains(&ProviderId::OpenMeteo) {
            ids.push(ProviderId::OpenMeteo);
        }
        ids.into_iter()
            .filter_map(|id| self.cache.load(&Cache::key(id, Self::coordinates(r))))
            .collect()
    }

    /// The newest cached snapshot of location `i` across its providers.
    fn cached(&self, i: usize, r: &ResolvedLocation) -> Option<CachedSnapshot> {
        self.cached_all(i, r).into_iter().max_by_key(|c| c.saved_at)
    }

    /// Alerts with ids from every cached snapshot of location `i`, so a fetch reuses
    /// their details instead of paying for them again. A fallback to a
    /// provider without alerts does not lose them.
    fn known_alerts(&self, i: usize, r: &ResolvedLocation) -> Vec<Alert> {
        if !self.opts.requirements.alerts {
            return Vec::new();
        }
        self.cached_all(i, r)
            .into_iter()
            .flat_map(|c| c.snapshot.alerts)
            .filter(|a| a.id.is_some())
            .collect()
    }

    /// Publishes cached snapshots (marked stale when older than the refresh interval)
    /// and schedules fresh ones for later. Never touches the network.
    pub fn load_cache(&mut self, now: UnixTime) {
        for i in 0..self.slots.len() {
            let Ok(r) = self.resolve(i, now, true) else {
                continue;
            };
            let Some(cached) = self.cached(i, &r) else {
                continue;
            };
            let stale = now.0 - cached.saved_at.0 > self.refresh_s();
            if !stale {
                self.scheduler
                    .on_success(i, cached.saved_at, cached.not_before, 0.0);
            }
            let next_due = self.scheduler.next_due(i);
            let mut snapshot = cached.snapshot;
            snapshot.location.name.clone_from(&r.name);
            self.publisher.update(|s| {
                let l = &mut s.locations[i];
                l.snapshot = Some(Arc::new(snapshot));
                l.status.last_success = Some(cached.saved_at);
                l.status.provider = Some(cached.provider);
                l.status.stale = stale;
                l.status.next_due = next_due;
            });
        }
    }

    /// Marks snapshots older than the refresh interval as stale.
    fn update_staleness(&self, now: UnixTime) {
        let refresh = self.refresh_s();
        let current = self.publisher.handle().current();
        let changed: Vec<usize> = current
            .locations
            .iter()
            .enumerate()
            .filter(|(_, l)| {
                let stale = l.status.last_success.is_some_and(|t| now.0 - t.0 > refresh);
                stale != l.status.stale
            })
            .map(|(i, _)| i)
            .collect();
        if !changed.is_empty() {
            self.publisher.update(|s| {
                for i in changed {
                    let l = &mut s.locations[i];
                    l.status.stale = !l.status.stale;
                }
            });
        }
    }

    /// Fetches every due location (none when offline).
    pub fn fetch_due(&mut self, now: UnixTime) {
        if self.opts.offline {
            return;
        }
        for i in self.scheduler.due(now) {
            self.fetch_one(i, now);
        }
    }

    /// Caches and publishes a successful fetch of location `i`.
    fn store_success(
        &mut self,
        i: usize,
        now: UnixTime,
        resolved: &ResolvedLocation,
        mut snapshot: whether_core::model::WeatherSnapshot,
        last_modified: Option<String>,
        all_notes: Vec<String>,
    ) {
        snapshot.location.name.clone_from(&resolved.name);
        // DST changes within the forecast (16 days plus a margin) keep local times
        // right across them.
        if let Some(tz) = resolved.tz_name.as_deref() {
            let until = UnixTime(snapshot.fetched_at.0 + 17 * 86_400);
            snapshot.offset_changes = crate::locate::offset_changes(tz, snapshot.fetched_at, until);
        }
        let provider = snapshot.provider;
        let cached = CachedSnapshot {
            schema: SCHEMA,
            saved_at: now,
            provider,
            last_modified,
            not_before: None,
            snapshot: snapshot.clone(),
        };
        if let Err(e) = self
            .cache
            .store(&Cache::key(provider, Self::coordinates(resolved)), &cached)
        {
            tracing::warn!("cache not saved: {e}");
        }
        self.scheduler
            .on_success(i, now, None, self.jitter.sample());
        let next_due = self.scheduler.next_due(i);
        self.publisher.update(|s| {
            let l = &mut s.locations[i];
            l.snapshot = Some(Arc::new(snapshot));
            l.status.in_flight = false;
            l.status.last_success = Some(now);
            l.status.stale = false;
            l.status.error = None;
            l.status.notes = all_notes;
            l.status.provider = Some(provider);
            l.status.next_due = next_due;
        });
    }

    /// Publishes location `i`'s cached history, and fetches it when due (once a
    /// local day). A failure waits an hour and never touches the
    /// forecast.
    fn history_one(&mut self, i: usize, now: UnixTime, resolved: &ResolvedLocation) {
        let req = self.cfg.history_requirements();
        if req.is_empty() {
            return;
        }
        let loc = Self::coordinates(resolved);
        let key = history_key(loc);
        if !self.slots[i].history_loaded {
            self.slots[i].history_loaded = true;
            self.slots[i].history = self.cache.load_history(&key);
            if let Some(cached) = &self.slots[i].history {
                let history = Arc::new(cached.history.clone());
                self.publisher
                    .update(|s| s.locations[i].history = Some(history));
            }
        }
        let offset = resolved.utc_offset;
        let waiting = self.slots[i]
            .history_failed_at
            .is_some_and(|t| now.0 - t.0 < RETRY_AFTER_S);
        if self.opts.offline
            || waiting
            || !history_due(self.slots[i].history.as_ref(), req, now, offset)
        {
            return;
        }
        let today = now.local_date(offset);
        let ctx = HistoryContext {
            now,
            today,
            utc_offset: offset,
            user_agent: user_agent(),
            normals: self.cache.load_normals(&key),
        };
        let chain = build_history_chain(&self.cfg.common, &*self.opts.env);
        match fetch_history(&chain, loc, req, ctx, &*self.transport) {
            Ok((history, attempts)) => {
                for note in notes(&attempts) {
                    tracing::info!("history: {note}");
                }
                let cached =
                    keep_history(&self.cache, &key, history, req, now, today, &resolved.name);
                let shared = Arc::new(cached.history.clone());
                self.slots[i].history = Some(cached);
                self.slots[i].history_failed_at = None;
                self.publisher
                    .update(|s| s.locations[i].history = Some(shared));
            }
            Err(e) => {
                tracing::warn!("history for {}: {e}", resolved.name);
                self.slots[i].history_failed_at = Some(now);
            }
        }
    }

    /// Fetches location `i` now and publishes the outcome, then its history
    /// when due.
    pub fn fetch_one(&mut self, i: usize, now: UnixTime) {
        let resolved = self.fetch_forecast(i, now);
        if let Some(mut resolved) = resolved {
            // The forecast knows the place's offset; a place configured by
            // coordinates alone resolves to UTC, which would shift its days.
            if let Some(kept) = self.cached(i, &resolved) {
                resolved.utc_offset = kept.snapshot.location.utc_offset;
            }
            self.history_one(i, now, &resolved);
        }
    }

    /// Fetches location `i`'s forecast and publishes the outcome; the place, when
    /// it resolved.
    fn fetch_forecast(&mut self, i: usize, now: UnixTime) -> Option<ResolvedLocation> {
        self.publisher.update(|s| {
            s.locations[i].status.in_flight = true;
            s.locations[i].status.last_attempt = Some(now);
        });
        let resolved = match self.resolve(i, now, false) {
            Ok(r) => r,
            Err(error) => {
                self.scheduler.on_failure(i, now, self.jitter.sample());
                let next_due = self.scheduler.next_due(i);
                self.publisher.update(|s| {
                    let st = &mut s.locations[i].status;
                    st.in_flight = false;
                    st.error = Some(error);
                    st.next_due = next_due;
                });
                return None;
            }
        };
        let ctx = FetchContext {
            now,
            user_agent: user_agent(),
            utc_offset_hint: Some(resolved.utc_offset),
            if_modified_since: None,
            language: self.cfg.common.general.language.clone(),
        };
        let fetched = fetch_location(
            &self.slots[i].chain,
            Self::coordinates(&resolved),
            self.opts.requirements,
            ctx,
            &*self.transport,
            self.opts.usage.as_ref(),
            self.known_alerts(i, &resolved),
        );
        let mut extra = Vec::new();
        if let Some(e) = &fetched.ledger_error {
            extra.push(format!("One Call: {e}"));
        }
        match fetched.result {
            Ok((snapshot, attempts)) => {
                let mut all_notes = notes(&attempts);
                all_notes.extend(extra);
                self.store_success(
                    i,
                    now,
                    &resolved,
                    snapshot,
                    fetched.last_modified,
                    all_notes,
                );
            }
            Err(FetchError::NotModified) => {
                self.scheduler.on_not_modified(i, now, None);
                let next_due = self.scheduler.next_due(i);
                self.publisher.update(|s| {
                    let st = &mut s.locations[i].status;
                    st.in_flight = false;
                    st.last_success = Some(now);
                    st.stale = false;
                    st.next_due = next_due;
                });
            }
            Err(error) => {
                let mut all_notes = match &error {
                    FetchError::AllProvidersFailed(attempts) => notes(attempts),
                    _ => Vec::new(),
                };
                all_notes.extend(extra);
                let message = match &error {
                    FetchError::AllProvidersFailed(_) => "every provider failed".to_string(),
                    other => other.to_string(),
                };
                self.scheduler.on_failure(i, now, self.jitter.sample());
                let next_due = self.scheduler.next_due(i);
                self.publisher.update(|s| {
                    let st = &mut s.locations[i].status;
                    st.in_flight = false;
                    st.error = Some(message);
                    st.notes = all_notes;
                    st.next_due = next_due;
                });
            }
        }
        Some(resolved)
    }

    fn search(&self, query: &str) {
        let query = query.trim().to_string();
        let q = query.clone();
        self.publisher
            .update(|s| s.search = Some(SearchState::Searching { query: q }));
        let transport: &dyn Transport = if self.opts.offline {
            &Offline
        } else {
            &*self.transport
        };
        let result = self.geocoder(transport).search(
            &GeocodeQuery::Name {
                text: &query,
                country: None,
            },
            8,
        );
        self.publisher.update(|s| {
            s.search = Some(match result {
                Ok(results) => SearchState::Results { query, results },
                Err(e) => SearchState::Failed {
                    query,
                    error: e.to_string(),
                },
            });
        });
    }

    fn report(&self, message: String) {
        self.report_at("location", message);
    }

    fn report_at(&self, path: &str, message: String) {
        self.publisher.update(|s| {
            s.config_issues.push(Issue {
                severity: Severity::Error,
                path: path.into(),
                message,
            });
        });
    }

    fn edit(
        &mut self,
        change: impl FnOnce(&std::path::Path) -> Result<(), whether_config::edit::EditError>,
    ) {
        let Some(path) = self.opts.edit_path.clone() else {
            self.report("no configuration file to edit".into());
            return;
        };
        match change(&path) {
            Ok(()) => self.reload(),
            Err(e) => self.report(e.to_string()),
        }
    }

    /// Writes `values` into the edit target and reloads; when the result does not
    /// load, the previous file is restored and the issues stay published.
    fn set_config(&mut self, values: &[(String, toml::Value)]) {
        let Some(path) = self.opts.edit_path.clone() else {
            self.report_at("config", "no configuration file to edit".into());
            return;
        };
        let previous = match whether_config::edit::set_values(&path, values) {
            Ok(previous) => previous,
            Err(e) => return self.report_at("config", e.to_string()),
        };
        let epoch = self.publisher.handle().current().config_epoch;
        self.reload();
        if self.publisher.handle().current().config_epoch == epoch {
            tracing::warn!(
                "the new settings did not load; restoring {}",
                path.display()
            );
            if let Err(e) = whether_config::edit::replace(&path, &previous) {
                self.report_at("config", e.to_string());
            }
        }
    }

    fn reload(&mut self) {
        let Some(reload) = &self.opts.reload else {
            return;
        };
        match reload() {
            Ok(cfg) => {
                self.cfg = Arc::new(cfg);
                self.rebuild();
                self.publisher.update(|s| s.config_epoch += 1);
                self.load_cache(now());
            }
            Err(e) => {
                let issues = match e {
                    ConfigError::Invalid(issues) => issues,
                    other => vec![Issue {
                        severity: Severity::Error,
                        path: "config".into(),
                        message: other.to_string(),
                    }],
                };
                self.publisher.update(|s| s.config_issues = issues);
            }
        }
    }

    /// The files whose changes reload the configuration: the loaded layers and the
    /// file location edits go to. Empty when the service cannot reload.
    pub fn watched_paths(&self) -> Vec<PathBuf> {
        if self.opts.reload.is_none() {
            return Vec::new();
        }
        let mut paths = self.cfg.sources.clone();
        paths.extend(self.opts.edit_path.clone());
        paths
    }

    /// Handles one command; returns `false` on `Quit`.
    pub fn handle(&mut self, command: Command, now: UnixTime) -> bool {
        match command {
            Command::Refresh { location } => {
                if !self.scheduler.request_refresh(location, now) {
                    tracing::debug!("manual refresh ignored (60 s floor)");
                }
            }
            Command::Search { query } => self.search(&query),
            Command::AddLocation(result) => {
                let loc = LocationConfig {
                    name: Some(result.name.clone()),
                    latitude: Some(result.latitude),
                    longitude: Some(result.longitude),
                    timezone: result.timezone.clone(),
                    ..Default::default()
                };
                self.edit(|path| whether_config::edit::add_location(path, &loc));
            }
            Command::RemoveLocation(i) => {
                if let Some(name) = self.cfg.common.location.get(i).and_then(|l| l.name.clone()) {
                    self.edit(|path| whether_config::edit::remove_location(path, &name));
                }
            }
            Command::ReloadConfig => self.reload(),
            Command::SetConfig(values) => self.set_config(&values),
            Command::Quit => return false,
        }
        true
    }

    /// How long to wait for a command before the next location is due.
    fn wait(&self, now: UnixTime) -> Option<Duration> {
        if self.opts.offline {
            return None;
        }
        self.scheduler
            .next_wakeup(now)
            .map(|t| Duration::from_secs(u64::try_from(t.0 - now.0).unwrap_or(0)))
    }
}

/// One pass for displays that print and exit: publish the cache, then fetch the
/// locations whose cache is missing or stale (unless offline).
pub fn refresh_once(worker: &mut Worker) {
    let now = now();
    worker.load_cache(now);
    worker.fetch_due(now);
}

/// The running service.
#[derive(Debug)]
pub struct ServiceHandle {
    /// Commands for the service.
    pub commands: Sender<Command>,
    join: JoinHandle<()>,
    /// Dropped on shutdown, which ends the radar thread.
    _radar_stop: Option<Sender<()>>,
}

impl ServiceHandle {
    /// Sends `Quit` and waits for the thread.
    pub fn shutdown(self) {
        let _ = self.commands.send(Command::Quit);
        if self.join.join().is_err() {
            tracing::error!("the service thread panicked");
        }
    }
}

/// The service loop. Config files are polled between commands, and a settled change
/// reloads; `watch` starts from the files as they were when the service
/// was spawned.
fn run(mut worker: Worker, mut watch: ConfigWatch, rx: &Receiver<Command>) {
    worker.load_cache(now());
    loop {
        let now = now();
        worker.update_staleness(now);
        worker.fetch_due(now);
        let now = whether_display::now();
        let wait = match (worker.wait(now), watch.is_empty()) {
            (wait, true) => wait,
            (Some(wait), false) => Some(wait.min(watch.next_poll(std::time::Instant::now()))),
            (None, false) => Some(watch.next_poll(std::time::Instant::now())),
        };
        let received = match wait {
            Some(wait) => rx.recv_timeout(wait),
            None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        match received {
            Ok(command) => {
                if !worker.handle(command, now) {
                    break;
                }
                // The service's own edits and reloads are not changes to report.
                watch.rebase(worker.watched_paths());
            }
            Err(RecvTimeoutError::Timeout) => {
                if watch.poll(std::time::Instant::now()) {
                    tracing::info!("configuration changed; reloading");
                    worker.handle(Command::ReloadConfig, now);
                    watch.rebase(worker.watched_paths());
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

/// Starts the service thread.
pub fn spawn_service(mut worker: Worker) -> std::io::Result<ServiceHandle> {
    let (commands, rx) = channel();
    let watch = ConfigWatch::new(worker.watched_paths());
    let radar_stop = match worker.opts.radar.take() {
        Some(opts) => {
            let (stop_tx, stop_rx) = channel::<()>();
            let writer = worker.publisher.writer();
            let cache = worker.cache.dir().join("radar");
            std::thread::Builder::new()
                .name("whether-radar".into())
                .spawn(move || {
                    crate::radar::run(
                        &writer,
                        &*opts.transport,
                        Some(&cache),
                        (opts.wanted, opts.zoom),
                        &stop_rx,
                    );
                })?;
            Some(stop_tx)
        }
        None => None,
    };
    let join = std::thread::Builder::new()
        .name("whether-service".into())
        .spawn(move || run(worker, watch, &rx))?;
    Ok(ServiceHandle {
        commands,
        join,
        _radar_stop: radar_stop,
    })
}

/// An empty state for `cfg`, before the worker publishes.
pub fn initial_state(cfg: &LinuxConfig) -> AppState {
    AppState {
        locations: cfg
            .common
            .location
            .iter()
            .cloned()
            .map(LocationState::new)
            .collect(),
        config_issues: cfg.issues.clone(),
        ..AppState::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Instant;
    use whether_config::{CommonConfig, DisplaySection, ProvidersConfig, RuntimeConfig};
    use whether_display::StateHandle;
    use whether_providers::redact_url;

    const FORECAST: &str =
        include_str!("../../whether-providers/tests/fixtures/open_meteo/forecast_full_10001.json");
    const AIR: &str =
        include_str!("../../whether-providers/tests/fixtures/open_meteo/air_quality_10001.json");

    /// Open-Meteo answers while `up` is set.
    struct Switch {
        up: Arc<AtomicBool>,
    }

    impl Transport for Switch {
        fn send(&self, req: &HttpRequest) -> Result<OwnedResponse, TransportError> {
            if !self.up.load(Ordering::SeqCst) {
                return Err(TransportError::Failed("down".into()));
            }
            if req.url.contains("air-quality") {
                Ok(OwnedResponse::new(200, AIR))
            } else if req.url.contains("api.open-meteo.com") {
                Ok(OwnedResponse::new(200, FORECAST))
            } else {
                Err(TransportError::Failed(format!(
                    "unexpected {}",
                    redact_url(&req.url)
                )))
            }
        }
    }

    fn config() -> LinuxConfig {
        LinuxConfig {
            common: CommonConfig {
                location: vec![LocationConfig {
                    name: Some("New York".into()),
                    latitude: Some(40.7143),
                    longitude: Some(-74.005_97),
                    timezone: Some("America/New_York".into()),
                    ..Default::default()
                }],
                providers: ProvidersConfig {
                    order: vec![ProviderId::OpenMeteo],
                    supplement: vec![],
                    ..Default::default()
                },
                ..Default::default()
            },
            display: DisplaySection::default(),
            runtime: RuntimeConfig::default(),
            radar: whether_config::RadarConfig::default(),
            sources: vec![],
            issues: vec![],
            measurement_locale: None,
            location_override: None,
        }
    }

    fn options(offline: bool) -> ServiceOptions {
        ServiceOptions {
            offline,
            env: Arc::new(|_| None),
            usage: None,
            requirements: DataRequirements::full(),
            edit_path: None,
            reload: None,
            radar: None,
        }
    }

    fn worker(cache: &Cache, up: &Arc<AtomicBool>, offline: bool) -> (Worker, StateHandle) {
        let cfg = config();
        let (publisher, handle) = StatePublisher::new(initial_state(&cfg));
        let transport = Box::new(Switch { up: Arc::clone(up) });
        (
            Worker::new(
                Arc::new(cfg),
                publisher,
                transport,
                cache.clone(),
                options(offline),
            ),
            handle,
        )
    }

    /// Waits until `pred` holds for the published state.
    fn wait_for(handle: &StateHandle, pred: impl Fn(&AppState) -> bool) -> Arc<AppState> {
        let deadline = Instant::now() + Duration::from_secs(5);
        let rx = handle.subscribe();
        loop {
            let state = handle.current();
            if pred(&state) {
                return state;
            }
            assert!(Instant::now() < deadline, "timed out; state: {state:?}");
            let _ = rx.recv_timeout(Duration::from_millis(50));
        }
    }

    fn seed_cache(cache: &Cache, age_s: i64) {
        let snapshot = whether_core::testdata::sample_snapshot();
        let key = Cache::key(
            ProviderId::OpenMeteo,
            Coordinates {
                latitude: 40.7143,
                longitude: -74.005_97,
            },
        );
        let cached = CachedSnapshot {
            schema: SCHEMA,
            saved_at: UnixTime(now().0 - age_s),
            provider: ProviderId::OpenMeteo,
            last_modified: None,
            not_before: None,
            snapshot,
        };
        cache.store(&key, &cached).unwrap();
    }

    #[test]
    fn cached_alerts_with_ids_are_known() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path());
        let alert = |id: Option<&str>| Alert {
            id: id.map(Into::into),
            title: "Wind Advisory".into(),
            description: String::new(),
            severity: whether_core::scales::Level::Moderate,
            start: UnixTime(0),
            end: UnixTime(i64::MAX),
            source: "NWS".into(),
            checked_at: None,
            url: None,
        };
        let mut snapshot = whether_core::testdata::sample_snapshot();
        snapshot.alerts = vec![alert(Some("a")), alert(None)];
        let cached = CachedSnapshot {
            schema: SCHEMA,
            saved_at: now(),
            provider: ProviderId::OpenMeteo,
            last_modified: None,
            not_before: None,
            snapshot,
        };
        let r = ResolvedLocation {
            latitude: 40.7143,
            longitude: -74.005_97,
            ..Default::default()
        };
        cache
            .store(
                &Cache::key(ProviderId::OpenMeteo, Worker::coordinates(&r)),
                &cached,
            )
            .unwrap();
        let up = Arc::new(AtomicBool::new(false));
        let (worker, _) = worker(&cache, &up, true);
        let known = worker.known_alerts(0, &r);
        assert_eq!(known.len(), 1);
        assert_eq!(known[0].id.as_deref(), Some("a"));

        let cfg = config();
        let (publisher, _) = StatePublisher::new(initial_state(&cfg));
        let no_alerts = Worker::new(
            Arc::new(cfg),
            publisher,
            Box::new(Switch { up }),
            cache.clone(),
            ServiceOptions {
                requirements: DataRequirements {
                    alerts: false,
                    ..DataRequirements::full()
                },
                ..options(true)
            },
        );
        assert_eq!(no_alerts.known_alerts(0, &r), vec![]);
    }

    #[test]
    fn startup_shows_a_stale_cache_then_fresh_data() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path());
        seed_cache(&cache, 3600);
        let up = Arc::new(AtomicBool::new(true));
        let (mut w, handle) = worker(&cache, &up, true);
        w.load_cache(now());
        let state = handle.current();
        assert!(state.locations[0].snapshot.is_some() && state.locations[0].status.stale);

        let (w, handle) = worker(&cache, &up, false);
        let service = spawn_service(w).unwrap();
        let state = wait_for(&handle, |s| {
            s.locations[0]
                .status
                .last_success
                .is_some_and(|t| now().0 - t.0 < 60)
        });
        let location = &state.locations[0];
        assert!(!location.status.stale && location.status.error.is_none());
        assert_eq!(location.status.provider, Some(ProviderId::OpenMeteo));
        assert_eq!(
            location.snapshot.as_ref().unwrap().location.name,
            "New York"
        );
        service.shutdown();
    }

    #[test]
    fn fresh_caches_are_not_refetched_once() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path());
        seed_cache(&cache, 60);
        let up = Arc::new(AtomicBool::new(false));
        let (mut w, handle) = worker(&cache, &up, false);
        refresh_once(&mut w);
        let location = &handle.current().locations[0];
        assert!(location.status.error.is_none(), "no fetch was attempted");
        assert!(!location.status.stale && location.snapshot.is_some());
    }

    #[test]
    fn failure_then_recovery_on_manual_refresh() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path());
        let up = Arc::new(AtomicBool::new(false));
        let (w, handle) = worker(&cache, &up, false);
        let service = spawn_service(w).unwrap();
        let state = wait_for(&handle, |s| s.locations[0].status.error.is_some());
        assert!(
            state.locations[0]
                .status
                .notes
                .iter()
                .any(|n| n.contains("Open-Meteo"))
        );
        up.store(true, Ordering::SeqCst);
        service
            .commands
            .send(Command::Refresh { location: None })
            .unwrap();
        let state = wait_for(&handle, |s| s.locations[0].snapshot.is_some());
        assert!(state.locations[0].status.error.is_none());
        service.shutdown();
    }

    #[test]
    fn quit_joins_quickly() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path());
        let up = Arc::new(AtomicBool::new(true));
        let (w, handle) = worker(&cache, &up, false);
        let service = spawn_service(w).unwrap();
        wait_for(&handle, |s| s.locations[0].snapshot.is_some());
        let started = Instant::now();
        service.shutdown();
        assert!(started.elapsed() < Duration::from_millis(100));
    }

    #[test]
    fn offline_never_fetches() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path());
        let up = Arc::new(AtomicBool::new(true));
        let (mut w, handle) = worker(&cache, &up, true);
        refresh_once(&mut w);
        let location = &handle.current().locations[0];
        assert!(location.snapshot.is_none() && location.status.last_attempt.is_none());
    }

    #[test]
    fn search_publishes_results_or_failure() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path());
        let up = Arc::new(AtomicBool::new(false));
        let (mut w, handle) = worker(&cache, &up, false);
        assert!(w.handle(
            Command::Search {
                query: "Brooklyn".into()
            },
            now()
        ));
        assert!(matches!(
            handle.current().search,
            Some(SearchState::Failed { .. })
        ));
        assert!(!w.handle(Command::Quit, now()));
    }

    /// a running service reloads a settled edit once, displays follow it,
    /// and an invalid edit keeps the old configuration and reports why.
    #[test]
    fn a_running_service_reloads_edited_config() {
        use whether_core::units::UnitPrefs;
        use whether_display::{DisplayContext, RunMode};
        const CONFIG: &str = "[general]\nunits = \"metric\"\n\n[[location]]\nname = \"New York\"\nlatitude = 40.7143\nlongitude = -74.00597\n";
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, CONFIG).unwrap();
        let load = {
            let path = path.clone();
            move || {
                let env = |_: &str| None;
                whether_config::load(&whether_config::LoadOptions {
                    explicit_path: Some(&path),
                    env: &env,
                    cli_overrides: vec![],
                })
            }
        };
        let cfg = load().unwrap();
        let presentation = Arc::new(cfg.presentation_for("tui").unwrap());
        let (publisher, handle) = StatePublisher::new(initial_state(&cfg));
        let opts = ServiceOptions {
            offline: true,
            env: Arc::new(|_| None),
            usage: None,
            requirements: DataRequirements::full(),
            edit_path: Some(path.clone()),
            reload: Some(Box::new(load)),
            radar: None,
        };
        let worker = Worker::new(
            Arc::new(cfg),
            publisher,
            Box::new(Offline),
            Cache::at(dir.path().join("cache")),
            opts,
        );
        let service = spawn_service(worker).unwrap();
        let ctx = DisplayContext {
            state: handle.clone(),
            commands: service.commands.clone(),
            display: "tui".into(),
            presentation: Arc::clone(&presentation),
            strings: Arc::new(whether_core::strings::Strings::default()),
            initial_location: 0,
            mode: RunMode::Interactive,
        };
        let settings = ctx.settings();
        assert_eq!(settings.current().0.units, UnitPrefs::METRIC);
        let until = |what: &str, done: &dyn Fn(&AppState) -> bool| {
            let deadline = Instant::now() + Duration::from_secs(10);
            while !done(&handle.current()) {
                assert!(Instant::now() < deadline, "timed out waiting for {what}");
                std::thread::sleep(Duration::from_millis(50));
            }
        };

        std::fs::write(&path, CONFIG.replace("metric", "imperial")).unwrap();
        until("the reload", &|s| s.config_epoch == 1);
        assert_eq!(
            settings.current().0.units,
            UnitPrefs::IMPERIAL,
            "displays follow"
        );
        assert_eq!(settings.config_epoch(), 1);

        std::fs::write(&path, "[general]\nunits = 7\n").unwrap();
        until("the issues", &|s| !s.config_issues.is_empty());
        assert_eq!(
            handle.current().config_epoch,
            1,
            "the old configuration stays"
        );
        assert_eq!(settings.current().0.units, UnitPrefs::IMPERIAL);
        service.shutdown();
    }

    /// settings are written with comments kept and reloaded; a value that
    /// does not load is undone and reported.
    #[test]
    fn set_config_edits_reloads_and_undoes_bad_values() {
        const CONFIG: &str = "# mine\n[general]\nunits = \"metric\"  # SI please\n\n[[location]]\nname = \"New York\"\nlatitude = 40.7143\nlongitude = -74.00597\n";
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, CONFIG).unwrap();
        let load = {
            let path = path.clone();
            move || {
                let env = |_: &str| None;
                whether_config::load(&whether_config::LoadOptions {
                    explicit_path: Some(&path),
                    env: &env,
                    cli_overrides: vec![],
                })
            }
        };
        let cfg = load().unwrap();
        let (publisher, handle) = StatePublisher::new(initial_state(&cfg));
        let opts = ServiceOptions {
            offline: true,
            env: Arc::new(|_| None),
            usage: None,
            requirements: DataRequirements::full(),
            edit_path: Some(path.clone()),
            reload: Some(Box::new(load)),
            radar: None,
        };
        let mut w = Worker::new(
            Arc::new(cfg),
            publisher,
            Box::new(Offline),
            Cache::at(dir.path().join("cache")),
            opts,
        );
        let set = |pairs: &[(&str, toml::Value)]| {
            Command::SetConfig(
                pairs
                    .iter()
                    .map(|(k, v)| ((*k).to_string(), v.clone()))
                    .collect(),
            )
        };
        assert!(w.handle(
            set(&[
                ("general.units", "imperial".into()),
                ("general.time_format", "12h".into()),
            ]),
            now()
        ));
        let state = handle.current();
        assert_eq!(state.config_epoch, 1);
        let cfg = &state.config.as_ref().unwrap().0;
        assert_eq!(
            cfg.common.general.time_format,
            whether_core::presentation::TimeFormat::H12
        );
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("# mine\n") && text.contains("units = \"imperial\"  # SI please"));

        w.handle(set(&[("general.units", "kilometric".into())]), now());
        let state = handle.current();
        assert_eq!(state.config_epoch, 1, "not loaded");
        assert!(!state.config_issues.is_empty(), "the reason is shown");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            text,
            "the file is restored"
        );
    }

    #[test]
    fn search_add_and_remove_edit_the_config_file() {
        use crate::testing::Replay;
        const GEOCODE: &str = include_str!(
            "../../whether-providers/tests/fixtures/open_meteo/geocode_springfield.json"
        );
        const CONFIG: &str = "# my config\n[[location]]\nname = \"New York\"  # home\nlatitude = 40.7143\nlongitude = -74.00597\n\n[providers]\norder = [\"open-meteo\"]\nsupplement = []\n";
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, CONFIG).unwrap();
        let load = {
            let path = path.clone();
            move || {
                let env = |_: &str| None;
                whether_config::load(&whether_config::LoadOptions {
                    explicit_path: Some(&path),
                    env: &env,
                    cli_overrides: vec![],
                })
            }
        };
        let cfg = load().unwrap();
        let (publisher, handle) = StatePublisher::new(initial_state(&cfg));
        let opts = ServiceOptions {
            offline: false,
            env: Arc::new(|_| None),
            usage: None,
            requirements: DataRequirements::full(),
            edit_path: Some(path.clone()),
            reload: Some(Box::new(load)),
            radar: None,
        };
        let transport = Box::new(Replay::new(vec![(
            "geocoding-api.open-meteo.com",
            200,
            GEOCODE,
        )]));
        let mut w = Worker::new(
            Arc::new(cfg),
            publisher,
            transport,
            Cache::at(dir.path().join("cache")),
            opts,
        );

        assert!(w.handle(
            Command::Search {
                query: "Springfield".into()
            },
            now()
        ));
        let Some(SearchState::Results { results, .. }) = handle.current().search.clone() else {
            panic!()
        };
        assert!(results.len() > 1);

        w.handle(Command::AddLocation(results[0].clone()), now());
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.starts_with("# my config\n") && text.contains("# home"),
            "comments kept: {text}"
        );
        assert!(text.contains("# added by whether locate"));
        let names: Vec<String> = handle
            .current()
            .locations
            .iter()
            .map(|l| l.name.clone())
            .collect();
        assert_eq!(names, vec!["New York".to_string(), results[0].name.clone()]);

        w.handle(Command::RemoveLocation(1), now());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            CONFIG,
            "removing restores the file"
        );
        assert_eq!(handle.current().locations.len(), 1);
    }

    // History in the service.

    macro_rules! acis {
        ($name:literal) => {
            include_str!(concat!(
                "../../whether-providers/tests/fixtures/acis/",
                $name,
                ".json"
            ))
        };
    }

    /// The forecast, ACIS and Open-Meteo history from fixtures; history requests
    /// are counted and fail while `history_up` is unset.
    struct WithHistory {
        history_up: AtomicBool,
        acis: std::sync::Mutex<usize>,
        archive: std::sync::Mutex<usize>,
    }

    impl WithHistory {
        fn new(history_up: bool) -> Self {
            Self {
                history_up: AtomicBool::new(history_up),
                acis: std::sync::Mutex::new(0),
                archive: std::sync::Mutex::new(0),
            }
        }
    }

    /// 30 years of a mild place, in `/v1/archive`'s shape.
    fn era5() -> String {
        use whether_core::time::{civil_from_days, days_from_civil};
        let (first, last) = (days_from_civil(1991, 1, 1), days_from_civil(2020, 12, 31));
        let days: Vec<i64> = (first..=last).collect();
        let time: Vec<i64> = days.iter().map(|d| d * 86_400 + 18_000).collect();
        let max: Vec<f64> = days
            .iter()
            .map(|d| {
                let (y, _, _) = civil_from_days(*d);
                15.0 + (d - days_from_civil(y, 1, 1)) as f64 / 50.0
            })
            .collect();
        serde_json::json!({
            "utc_offset_seconds": -18_000,
            "daily": {"time": time, "temperature_2m_max": max,
                      "temperature_2m_min": vec![5.0; days.len()],
                      "precipitation_sum": vec![0.0; days.len()]}
        })
        .to_string()
    }

    impl Transport for WithHistory {
        fn send(&self, req: &HttpRequest) -> Result<OwnedResponse, TransportError> {
            let history = req.url.contains("rcc-acis")
                || req.url.contains("archive-api")
                || req.url.contains("historical-forecast");
            if history && !self.history_up.load(Ordering::SeqCst) {
                return Err(TransportError::Failed("history down".into()));
            }
            if req.url.contains("rcc-acis") {
                *self.acis.lock().unwrap() += 1;
                let body = req.body.as_deref().unwrap_or("");
                let q: serde_json::Value = serde_json::from_str(body).unwrap();
                let text = match (q.get("bbox").is_some(), q["sid"].as_str(), body) {
                    (true, _, _) => acis!("stnmeta_10001"),
                    (_, Some("NYC"), b) if b.contains("normal") => acis!("stndata_NYC"),
                    (_, Some("NYC"), b) if b.contains("cnt_ge") => acis!("chance_NYC"),
                    (_, Some("NYC"), _) => acis!("records_NYC"),
                    (_, Some("NYCthr"), _) => acis!("records_NYCthr"),
                    _ => r#"{"error":"Unknown sId"}"#,
                };
                return Ok(OwnedResponse::new(200, text));
            }
            if req.url.contains("archive-api") {
                *self.archive.lock().unwrap() += 1;
                return Ok(OwnedResponse::new(200, era5()));
            }
            if req.url.contains("historical-forecast") {
                return Ok(OwnedResponse::new(
                    200,
                    include_str!(
                        "../../whether-providers/tests/fixtures/open_meteo/history_past_10001.json"
                    ),
                ));
            }
            if req.url.contains("air-quality") {
                Ok(OwnedResponse::new(200, AIR))
            } else {
                Ok(OwnedResponse::new(200, FORECAST))
            }
        }
    }

    fn history_config(order: Vec<ProviderId>) -> LinuxConfig {
        let mut cfg = config();
        cfg.common.presentation.widgets = Some(vec![
            whether_core::presentation::WidgetKind::Current,
            whether_core::presentation::WidgetKind::History,
        ]);
        cfg.common.history.order = order;
        cfg.common.history.supplement = vec![];
        cfg
    }

    fn history_worker(
        cache: &Cache,
        cfg: LinuxConfig,
        transport: Arc<WithHistory>,
    ) -> (Worker, StateHandle) {
        struct Shared(Arc<WithHistory>);
        impl Transport for Shared {
            fn send(&self, req: &HttpRequest) -> Result<OwnedResponse, TransportError> {
                self.0.send(req)
            }
        }
        let (publisher, handle) = StatePublisher::new(initial_state(&cfg));
        (
            Worker::new(
                Arc::new(cfg),
                publisher,
                Box::new(Shared(transport)),
                cache.clone(),
                options(false),
            ),
            handle,
        )
    }

    /// November 2026 (`day` 2 is when the fixtures were recorded) at
    /// `hour`:`minute` in New York (EST).
    fn local(day: u8, hour: i64, minute: i64) -> UnixTime {
        let midnight_utc = 1_793_577_600 + (i64::from(day) - 2) * 86_400;
        UnixTime(midnight_utc + 5 * 3_600 + hour * 3_600 + minute * 60)
    }

    #[test]
    fn history_is_fetched_once_a_local_day_and_kept() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path());
        let net = Arc::new(WithHistory::new(true));
        let (mut w, handle) = history_worker(
            &cache,
            history_config(vec![ProviderId::Acis]),
            Arc::clone(&net),
        );
        w.fetch_one(0, local(2, 10, 0));
        let calls = *net.acis.lock().unwrap();
        assert_eq!(
            calls, 5,
            "station list, days, chance, records, threaded records"
        );
        let state = handle.current();
        let history = state.locations[0].history.as_ref().unwrap();
        assert_eq!(history.station.as_ref().unwrap().id, "NYC");
        assert!(state.locations[0].snapshot.is_some());
        // A refresh later the same day asks nothing.
        w.fetch_one(0, local(2, 10, 15));
        assert_eq!(*net.acis.lock().unwrap(), calls);
        // Nor does a restart: the cache serves today's history.
        let (mut again, handle2) = history_worker(
            &cache,
            history_config(vec![ProviderId::Acis]),
            Arc::clone(&net),
        );
        again.fetch_one(0, local(2, 11, 0));
        assert_eq!(*net.acis.lock().unwrap(), calls);
        assert!(handle2.current().locations[0].history.is_some());
        // Just after midnight it waits; at a quarter past, the new day is fetched.
        again.fetch_one(0, local(3, 0, 10));
        assert_eq!(*net.acis.lock().unwrap(), calls);
        again.fetch_one(0, local(3, 0, 20));
        assert!(*net.acis.lock().unwrap() > calls, "the 3rd's fetch");
    }

    /// A place given by coordinates alone: its days follow the forecast's
    /// offset, not UTC, so at 21:00 EST it is still the 2nd.
    #[test]
    fn history_days_follow_the_forecast_offset() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path());
        let net = Arc::new(WithHistory::new(true));
        let mut cfg = history_config(vec![ProviderId::Acis]);
        cfg.common.location[0].timezone = None;
        let (mut w, _) = history_worker(&cache, cfg, Arc::clone(&net));
        w.fetch_one(0, local(2, 21, 0));
        let key = history_key(Coordinates {
            latitude: 40.7143,
            longitude: -74.005_97,
        });
        let kept = cache.load_history(&key).unwrap();
        assert_eq!(kept.today.day, 2);
        assert_eq!(
            kept.history.location.utc_offset,
            whether_core::time::UtcOffset(-5 * 3_600)
        );
    }

    #[test]
    fn normals_are_computed_once_then_kept() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path());
        let net = Arc::new(WithHistory::new(true));
        let (mut w, handle) = history_worker(
            &cache,
            history_config(vec![ProviderId::OpenMeteo]),
            Arc::clone(&net),
        );
        w.fetch_one(0, local(2, 10, 0));
        assert_eq!(*net.archive.lock().unwrap(), 1);
        let history = handle.current().locations[0].history.clone().unwrap();
        assert!(
            history
                .climate_on(whether_core::time::LocalDate {
                    year: 2026,
                    month: 11,
                    day: 2
                })
                .unwrap()
                .normal_max
                .is_some()
        );
        assert_eq!(history.normals, None, "kept apart, not published");
        w.fetch_one(0, local(3, 9, 0));
        assert_eq!(
            *net.archive.lock().unwrap(),
            1,
            "the next day uses the kept normals"
        );
        let history = handle.current().locations[0].history.clone().unwrap();
        assert!(
            history
                .climate_on(whether_core::time::LocalDate {
                    year: 2026,
                    month: 11,
                    day: 3
                })
                .unwrap()
                .normal_max
                .is_some()
        );
    }

    #[test]
    fn a_history_failure_leaves_the_forecast_alone_and_waits_an_hour() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path());
        let net = Arc::new(WithHistory::new(false));
        let (mut w, handle) = history_worker(
            &cache,
            history_config(vec![ProviderId::Acis]),
            Arc::clone(&net),
        );
        w.fetch_one(0, local(2, 10, 0));
        let state = handle.current();
        assert!(state.locations[0].snapshot.is_some());
        assert_eq!(state.locations[0].status.error, None);
        assert_eq!(state.locations[0].history, None);
        net.history_up.store(true, Ordering::SeqCst);
        w.fetch_one(0, local(2, 10, 30));
        assert_eq!(handle.current().locations[0].history, None, "waits an hour");
        w.fetch_one(0, local(2, 11, 5));
        assert!(handle.current().locations[0].history.is_some());
    }

    #[test]
    fn no_history_shown_no_history_fetched() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path());
        let net = Arc::new(WithHistory::new(true));
        let mut cfg = history_config(vec![ProviderId::Acis]);
        cfg.common.presentation.widgets = None;
        let (mut w, handle) = history_worker(&cache, cfg, Arc::clone(&net));
        w.fetch_one(0, local(2, 10, 0));
        assert_eq!(*net.acis.lock().unwrap(), 0);
        assert_eq!(handle.current().locations[0].history, None);
    }
}
