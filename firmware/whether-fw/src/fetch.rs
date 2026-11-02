//! The fetcher task: keeps every location's snapshot current over HTTPS, on the
//! shared schedule (jitter, backoff, a floor for manual refreshes), and keeps
//! metered providers inside their daily budget with a ledger in flash.

use alloc::boxed::Box;
use alloc::string::ToString;
use alloc::vec::Vec;

use embassy_futures::select::{Either, select};
use embassy_net::Stack;
use embassy_time::{Instant, Timer};
use log::{error, info, warn};
use whether_core::model::{Alert, ProviderId, WeatherSnapshot};
use whether_core::presentation::{DailyIcon, PresentationConfig};
use whether_core::requirements::DataRequirements;
use whether_core::time::{UnixTime, UtcOffset};
use whether_providers::schedule::{Jitter, Scheduler};
use whether_providers::{
    Attempt, Coordinates, FetchContext, FetchError, FetchSession, Provider, Reservation,
    UsageLedger, redact_url,
};

use crate::ledger::{BLOCK, LedgerStore};
use crate::net::clock;
use crate::net::http::EspTransport;
use crate::state::{self, COMMANDS, FwCommand, LinkState};

/// The hosts whose pinned roots are checked at boot (certs/README.md).
const HOSTS: [&str; 4] = [
    "api.open-meteo.com",
    "air-quality-api.open-meteo.com",
    "api.openweathermap.org",
    "api.met.no",
];

/// The one metered provider and its stored reservation.
struct Budget {
    store: LedgerStore,
    provider: ProviderId,
    stored: Option<Reservation>,
    /// The exact count: kept across a deep sleep, or built once the clock is
    /// known from the stored reservation (`fw_sleep::ledger_for`).
    ledger: Option<UsageLedger>,
}

impl Budget {
    /// The ledger for a fetch at `now`, if metered calls may be made: only once a
    /// response has set the clock, because the counts follow UTC days.
    fn ledger(&mut self) -> Option<UsageLedger> {
        clock::now()?;
        Some(
            self.ledger
                .get_or_insert_with(|| fw_sleep::ledger_for(self.provider, None, self.stored))
                .clone(),
        )
    }

    /// Stores a new reservation if `ledger` has gone beyond the stored one.
    /// Returns `false` when it could not be stored: the call must not be sent.
    async fn reserve(&mut self, ledger: &UsageLedger) -> bool {
        let Some(next) = ledger.reservation_after_spend(self.provider, self.stored, BLOCK) else {
            return true;
        };
        match self.store.save(self.provider, next).await {
            Ok(()) => {
                info!(
                    "{:?}: reserved calls up to {} for day {}",
                    self.provider, next.reserved, next.day
                );
                self.stored = Some(next);
                true
            }
            Err(e) => {
                error!("call ledger not saved: {e:?}");
                false
            }
        }
    }
}

/// The fetch loop's state, shared by the always-on task and the deep-sleep
/// cycle: the providers, the budget and the schedule.
pub struct Cycle {
    cfg: &'static fw_config::FwRuntimeConfig,
    chain: Vec<Box<dyn Provider>>,
    supplements: Vec<Box<dyn Provider>>,
    req: DataRequirements,
    budget: Option<Budget>,
    scheduler: Scheduler,
    jitter: Jitter,
}

impl core::fmt::Debug for Cycle {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Cycle")
            .field("scheduler", &self.scheduler)
            .finish_non_exhaustive()
    }
}

impl Cycle {
    /// Builds the providers and the schedule. `resume` is the state a deep sleep
    /// kept: its exact ledger and schedule carry on; without it the budget
    /// resumes from the reservation in flash.
    pub async fn new(
        cfg: &'static fw_config::FwRuntimeConfig,
        store: Option<LedgerStore>,
        resume: Option<&fw_sleep::RtcState>,
        seed: u64,
    ) -> Self {
        let chain = crate::providers::build(&cfg.providers);
        let supplements = crate::providers::build(&cfg.supplements);
        // Daylight day icons would ask for every day's hours, more than the
        // device's buffers hold; it fetches as before, and days its hourly
        // data does not cover show the provider's icon.
        let req = DataRequirements::from_presentation(&PresentationConfig {
            daily_icon: DailyIcon::Provider,
            ..cfg.presentation.clone()
        });
        let budget = budget(&chain, store, resume).await;
        // A metered provider's interval stretches so a whole day fits its budget.
        let locations = u32::try_from(cfg.locations.len()).unwrap_or(u32::MAX);
        let paced = chain
            .iter()
            .filter_map(|p| {
                let budget = p.budget()?;
                Some(budget.paced_refresh_secs(p.metered_cost_per_fetch(&req), locations))
            })
            .max()
            .unwrap_or(0);
        if paced > cfg.refresh_s {
            info!("refresh paced to {paced} s to stay within the daily call limit");
        }
        let scheduler = resume
            .and_then(|s| s.scheduler.clone())
            .filter(|s| s.len() == cfg.locations.len())
            .unwrap_or_else(|| {
                Scheduler::new(
                    cfg.locations.len(),
                    cfg.refresh_s,
                    &alloc::vec![paced; cfg.locations.len()],
                )
            });
        Self {
            cfg,
            chain,
            supplements,
            req,
            budget,
            scheduler,
            jitter: Jitter::new(seed),
        }
    }

    /// Fetches every location that is due now.
    pub async fn fetch_due(&mut self, transport: &mut EspTransport) {
        for i in self.scheduler.due(now()) {
            let ok = fetch(
                transport,
                self.cfg,
                &self.chain,
                &self.supplements,
                self.req,
                &mut self.budget,
                i,
            )
            .await;
            let at = now();
            if ok {
                self.scheduler.on_success(i, at, None, self.jitter.sample());
            } else {
                self.scheduler.on_failure(i, at, self.jitter.sample());
                info!(
                    "{}: next attempt in {} s",
                    self.cfg.locations[i].name,
                    self.scheduler.next_due(i).map_or(0, |t| t.0 - at.0)
                );
            }
        }
    }

    /// Records that every due location failed without a request (no network).
    pub fn fail_due(&mut self) {
        let at = now();
        for i in self.scheduler.due(at) {
            self.scheduler.on_failure(i, at, self.jitter.sample());
        }
    }

    /// The schedule.
    pub fn scheduler(&self) -> &Scheduler {
        &self.scheduler
    }

    /// The exact ledger and the stored reservation, to keep across a sleep.
    pub fn budget_state(&self) -> (Option<UsageLedger>, Option<Reservation>) {
        self.budget
            .as_ref()
            .map_or((None, None), |b| (b.ledger.clone(), b.stored))
    }
}

/// The schedule's clock: wall time once known, else seconds since boot. Times
/// scheduled before the clock is set fall due early once it is, which only
/// brings a fetch forward.
pub fn now() -> UnixTime {
    clock::now().unwrap_or_else(|| UnixTime(i64::try_from(Instant::now().as_secs()).unwrap_or(0)))
}

/// Everything the always-on fetcher needs, moved into its task.
pub struct Fetcher {
    /// The network stack.
    pub stack: Stack<'static>,
    /// HTTPS.
    pub transport: EspTransport,
    /// Providers, budget and schedule.
    pub cycle: Cycle,
}

impl core::fmt::Debug for Fetcher {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Fetcher").finish_non_exhaustive()
    }
}

/// Runs the always-on fetcher: fetch what is due, wait for the next due time or
/// a Refresh command, repeat.
#[embassy_executor::task]
pub async fn run(fetcher: Fetcher) {
    let Fetcher {
        stack,
        mut transport,
        mut cycle,
    } = fetcher;
    publish_link(stack);
    stack.wait_config_up().await;
    publish_link(stack);
    check_tls(&mut transport).await;
    loop {
        cycle.fetch_due(&mut transport).await;
        publish_link(stack);
        let at = now();
        let wait = cycle
            .scheduler
            .next_wakeup(at)
            .map_or(60, |t| (t.0 - at.0).clamp(1, 3600));
        let wait = u64::try_from(wait).unwrap_or(60);
        if let Either::Second(FwCommand::Refresh) =
            select(Timer::after_secs(wait), COMMANDS.receive()).await
            && !cycle.scheduler.request_refresh(None, now())
        {
            info!("refresh ignored: the last one was less than a minute ago");
        }
    }
}

/// The budget of the first metered provider in the chain, if any. After a deep
/// sleep it carries on from the kept state; otherwise it resumes from the
/// reservation in flash. Without the store, metered calls stay off.
async fn budget(
    chain: &[Box<dyn Provider>],
    store: Option<LedgerStore>,
    resume: Option<&fw_sleep::RtcState>,
) -> Option<Budget> {
    let provider = chain.iter().find(|p| p.budget().is_some())?.id();
    let Some(mut store) = store else {
        warn!("{provider:?}: no call ledger in flash, so its metered calls are off");
        return None;
    };
    if let Some(kept) = resume.filter(|s| s.ledger.is_some()) {
        return Some(Budget {
            store,
            provider,
            stored: kept.stored,
            ledger: kept.ledger.clone(),
        });
    }
    match store.load(provider).await {
        Ok(stored) => {
            if let Some(r) = stored {
                info!(
                    "{provider:?}: resuming with {} calls reserved on day {}",
                    r.reserved, r.day
                );
            }
            Some(Budget {
                store,
                provider,
                stored,
                ledger: None,
            })
        }
        Err(e) => {
            error!("{provider:?}: call ledger unreadable ({e:?}), so its metered calls are off");
            None
        }
    }
}

pub fn publish_link(stack: Stack<'static>) {
    let link = if stack.config_v4().is_some() {
        LinkState::Online
    } else if crate::net::wifi::link_up() {
        LinkState::Associated
    } else {
        LinkState::Down
    };
    if state::read(|s| s.link) != link {
        state::update(|s| s.link = link);
    }
}

/// Checks that a wrong root fails the handshake and that every API host's chain
/// still leads to its pinned root.
pub async fn check_tls(transport: &mut EspTransport) {
    match transport.wrong_root_is_refused("api.open-meteo.com").await {
        Ok(true) => info!("TLS self-check: a wrong root is refused"),
        Ok(false) => error!("TLS self-check FAILED: a wrong root was accepted"),
        Err(e) => warn!("TLS self-check did not run: {e}"),
    }
    for host in HOSTS {
        match transport.check_host(host).await {
            Ok(status) => info!("TLS check {host}: verified (HTTP {status})"),
            Err(e) => error!("TLS check {host}: {e}"),
        }
    }
}

/// Fetches location `i` and publishes the result. Returns whether it succeeded.
#[allow(
    clippy::large_stack_frames,
    reason = "the TLS handshake state (about 15 KB) is polled on the 101 KB main stack"
)]
async fn fetch(
    transport: &mut EspTransport,
    cfg: &fw_config::FwRuntimeConfig,
    chain: &[Box<dyn Provider>],
    supplements: &[Box<dyn Provider>],
    req: DataRequirements,
    budget: &mut Option<Budget>,
    i: usize,
) -> bool {
    let loc = &cfg.locations[i];
    let (offset, alerts): (Option<UtcOffset>, Vec<Alert>) = state::update(|s| {
        let l = &mut s.locations[i];
        l.status.refreshing = true;
        l.snapshot.as_ref().map_or((None, Vec::new()), |snap| {
            (Some(snap.location.utc_offset), snap.alerts.clone())
        })
    });
    let chain: Vec<&dyn Provider> = chain.iter().map(AsRef::as_ref).collect();
    let supplements: Vec<&dyn Provider> = supplements.iter().map(AsRef::as_ref).collect();
    let ctx = FetchContext {
        now: now(),
        user_agent: cfg.user_agent.clone(),
        utc_offset_hint: offset,
        if_modified_since: None,
        // Alert texts only; the device config carries no
        // `general.language` yet (STATUS follow-up).
        language: "en".to_string(),
    };
    let coordinates = Coordinates {
        latitude: loc.latitude,
        longitude: loc.longitude,
    };
    let mut session =
        FetchSession::new(&chain, &supplements, coordinates, req, ctx).with_known_alerts(alerts);
    if let Some(ledger) = budget.as_mut().and_then(Budget::ledger) {
        session = session.with_usage(ledger);
    }

    let started = Instant::now();
    let mut bytes = 0;
    while let Some(request) = session.next_request() {
        let request = request.clone();
        if request.metered {
            let reserved = match (budget.as_mut(), session.usage()) {
                (Some(b), Some(ledger)) => b.reserve(ledger).await,
                _ => false,
            };
            if !reserved {
                session.on_transport_error("call ledger not saved");
                continue;
            }
        }
        let peak = esp_alloc::HEAP.stats().max_usage;
        let result = transport.get(&request).await;
        let after = esp_alloc::HEAP.stats().max_usage;
        if after > peak {
            info!(
                "heap peak {after} (+{}) during GET {}",
                after - peak,
                redact_url(&request.url)
            );
        }
        match result {
            Ok(reply) => {
                bytes += reply.body.len();
                session.on_response(&reply.as_response());
            }
            Err(e) => {
                warn!("GET {} failed: {e}", redact_url(&request.url));
                session.on_transport_error(&e.to_string());
            }
        }
    }
    if let (Some(b), Some(ledger)) = (budget.as_mut(), session.usage()) {
        b.ledger = Some(ledger.clone());
    }
    let elapsed = started.elapsed().as_millis();
    publish(&loc.name, i, session.finish(now()), bytes, elapsed)
}

/// Publishes a fetch's outcome; a failure keeps the last good snapshot and marks
/// it stale. Returns whether the fetch succeeded.
fn publish(
    name: &str,
    i: usize,
    result: Result<(WeatherSnapshot, Vec<Attempt>), FetchError>,
    bytes: usize,
    elapsed: u64,
) -> bool {
    let ok = match result {
        Ok((snapshot, attempts)) => {
            let temperature = snapshot
                .current
                .as_ref()
                .and_then(|c| c.temperature)
                .map(|t| t.0);
            info!(
                "{}: {:?} °C from {:?} ({} B in {} ms)",
                name, temperature, snapshot.provider, bytes, elapsed
            );
            for a in attempts.iter().filter(|a| a.error.is_some()) {
                warn!("  {:?} {:?}: {:?}", a.provider, a.endpoint, a.error);
            }
            let fetched_at = snapshot.fetched_at;
            state::update(|s| {
                let l = &mut s.locations[i];
                l.snapshot = Some(snapshot);
                l.status.last_success = Some(fetched_at);
                l.status.error = None;
                l.status.stale = false;
                l.status.refreshing = false;
            });
            true
        }
        Err(e) => {
            let message = match &e {
                FetchError::AllProvidersFailed(attempts) => attempts
                    .last()
                    .and_then(|a| a.error.as_ref())
                    .map_or_else(|| e.to_string(), ToString::to_string),
                other => other.to_string(),
            };
            warn!("{name}: no data ({message}) after {elapsed} ms");
            state::update(|s| {
                let l = &mut s.locations[i];
                l.status.error = Some(message);
                l.status.stale = l.snapshot.is_some();
                l.status.refreshing = false;
            });
            false
        }
    };
    // After the previous snapshot is gone, so a leak shows as a falling number.
    let stats = esp_alloc::HEAP.stats();
    let (stack, stack_size) = crate::diag::stack_high_water();
    info!(
        "heap after the fetch: used {} of {}, peak {}; stack peak {stack} of {stack_size}",
        stats.current_usage, stats.size, stats.max_usage
    );
    ok
}
