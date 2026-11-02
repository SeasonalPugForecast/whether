//! History in the runtime: the source chain from `[history]`, the
//! cache (each place's history and its normals, which never expire), when a
//! fetch is due, and the fetch itself.

use std::fs;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use whether_config::{CommonConfig, Secret};
use whether_core::history::{ClimateQuery, History, Normals};
use whether_core::model::{ProviderId, ResolvedLocation};
use whether_core::requirements::HistoryRequirements;
use whether_core::time::{LocalDate, UnixTime, UtcOffset};
use whether_providers::acis::{Acis, AcisSettings, NearbyStation};
use whether_providers::open_meteo::history::{OpenMeteoHistory, OpenMeteoHistorySettings};
use whether_providers::{
    Attempt, Coordinates, FetchError, HistoryContext, HistoryProvider, HistorySession,
    ProviderError,
};

use crate::atomic::write_atomic;
use crate::cache::Cache;
use crate::transport::{Transport, user_agent};

/// The history cache format version.
/// 2: records reach back to the past days.
pub const HISTORY_SCHEMA: u32 = 2;

pub use whether_providers::history::DAY_START_MINUTES;

/// A failed history fetch is not retried before this.
pub const RETRY_AFTER_S: i64 = 3_600;

/// A stored history.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CachedHistory {
    /// Format version ([`HISTORY_SCHEMA`]).
    pub schema: u32,
    /// When it was stored.
    pub saved_at: UnixTime,
    /// The place's local date it was fetched on.
    pub today: LocalDate,
    /// What it was fetched for.
    pub past_days: u8,
    /// Climate dates fetched.
    pub climate_days: u8,
    /// Whether records were asked for.
    pub records: bool,
    /// Whether month-to-date days were asked for.
    pub month_to_date: bool,
    /// The data.
    pub history: History,
}

impl CachedHistory {
    /// What it was fetched for.
    pub fn requirements(&self) -> HistoryRequirements {
        HistoryRequirements {
            past_days: self.past_days,
            climate_days: self.climate_days,
            records: self.records,
            month_to_date: self.month_to_date,
        }
    }
}

/// Normals kept for a place.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct CachedNormals {
    schema: u32,
    normals: Normals,
}

/// The cache key of a place's history: its coordinates to 3 decimals.
pub fn history_key(c: Coordinates) -> String {
    format!("{:.3}-{:.3}", c.latitude, c.longitude)
}

impl Cache {
    fn history_path(&self, key: &str) -> PathBuf {
        self.dir().join("history").join(format!("{key}.json"))
    }

    fn normals_path(&self, key: &str) -> PathBuf {
        self.dir().join("normals").join(format!("{key}.json"))
    }

    fn load_json<T: for<'de> Deserialize<'de>>(path: &PathBuf) -> Option<T> {
        let text = fs::read_to_string(path).ok()?;
        match serde_json::from_str(&text) {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "deleting corrupt cache file");
                let _ = fs::remove_file(path);
                None
            }
        }
    }

    /// The stored history of a place.
    pub fn load_history(&self, key: &str) -> Option<CachedHistory> {
        Self::load_json::<CachedHistory>(&self.history_path(key))
            .filter(|c| c.schema == HISTORY_SCHEMA)
    }

    /// Stores a place's history atomically.
    pub fn store_history(&self, key: &str, value: &CachedHistory) -> io::Result<()> {
        let json = serde_json::to_vec(value).map_err(io::Error::other)?;
        write_atomic(&self.history_path(key), &json)
    }

    /// The normals kept for a place.
    pub fn load_normals(&self, key: &str) -> Option<Normals> {
        Self::load_json::<CachedNormals>(&self.normals_path(key))
            .filter(|c| c.schema == HISTORY_SCHEMA)
            .map(|c| c.normals)
    }

    /// Keeps a place's normals atomically; they never expire.
    pub fn store_normals(&self, key: &str, normals: &Normals) -> io::Result<()> {
        let value = CachedNormals {
            schema: HISTORY_SCHEMA,
            normals: normals.clone(),
        };
        let json = serde_json::to_vec(&value).map_err(io::Error::other)?;
        write_atomic(&self.normals_path(key), &json)
    }
}

/// Whether a place's history should be fetched at `now` (see
/// [`whether_providers::history_due`]).
pub fn history_due(
    cached: Option<&CachedHistory>,
    req: HistoryRequirements,
    now: UnixTime,
    offset: UtcOffset,
) -> bool {
    whether_providers::history_due(
        cached.map(|c| (c.today, c.requirements())),
        &req,
        now,
        offset,
    )
}

/// History sources in order, and the ones that fill gaps.
pub struct HistoryChain {
    /// Fallback order.
    pub primary: Vec<Box<dyn HistoryProvider>>,
    /// Gap fillers.
    pub supplements: Vec<Box<dyn HistoryProvider>>,
}

impl std::fmt::Debug for HistoryChain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ids =
            |list: &[Box<dyn HistoryProvider>]| list.iter().map(|p| p.id()).collect::<Vec<_>>();
        f.debug_struct("HistoryChain")
            .field("primary", &ids(&self.primary))
            .field("supplements", &ids(&self.supplements))
            .finish()
    }
}

fn history_provider(
    id: ProviderId,
    cfg: &CommonConfig,
    env: &dyn Fn(&str) -> Option<String>,
    as_supplement: bool,
) -> Option<Box<dyn HistoryProvider>> {
    match id {
        ProviderId::Acis => {
            let acis = &cfg.history.acis;
            Some(Box::new(Acis::new(AcisSettings {
                base_url: acis
                    .base_url
                    .clone()
                    .unwrap_or_else(|| whether_providers::acis::DEFAULT_BASE_URL.to_string()),
                station: acis.station.clone(),
                records_station: acis.records_station.clone(),
            })))
        }
        ProviderId::OpenMeteo => {
            let om = &cfg.providers.open_meteo;
            Some(Box::new(OpenMeteoHistory::new(OpenMeteoHistorySettings {
                api_key: om
                    .api_key
                    .as_ref()
                    .and_then(|s: &Secret| s.resolve(env).ok()),
                models: om.models.clone().map(|m| m.0).unwrap_or_default(),
                // A supplement fills days; normals come from the source ahead.
                normals: !as_supplement,
                ..OpenMeteoHistorySettings::default()
            })))
        }
        ProviderId::OpenWeather
        | ProviderId::MetNorway
        | ProviderId::Nws
        | ProviderId::PirateWeather => None,
    }
}

/// The history chain from `[history]`.
pub fn build_history_chain(
    cfg: &CommonConfig,
    env: &dyn Fn(&str) -> Option<String>,
) -> HistoryChain {
    let mut seen = Vec::new();
    let mut list = |ids: &[ProviderId], supplement: bool| -> Vec<Box<dyn HistoryProvider>> {
        ids.iter()
            .filter(|id| {
                let new = !seen.contains(&(**id, supplement));
                seen.push((**id, supplement));
                new
            })
            .filter_map(|id| history_provider(*id, cfg, env, supplement))
            .collect()
    };
    HistoryChain {
        primary: list(&cfg.history.order, false),
        supplements: list(&cfg.history.supplement, true),
    }
}

/// Runs one history fetch for `loc` over `transport`.
pub fn fetch_history(
    chain: &HistoryChain,
    loc: Coordinates,
    req: HistoryRequirements,
    ctx: HistoryContext,
    transport: &dyn Transport,
) -> Result<(History, Vec<Attempt>), FetchError> {
    let primary: Vec<&dyn HistoryProvider> = chain.primary.iter().map(AsRef::as_ref).collect();
    let supplements: Vec<&dyn HistoryProvider> =
        chain.supplements.iter().map(AsRef::as_ref).collect();
    let now = ctx.now;
    let mut session = HistorySession::new(&primary, &supplements, loc, req, ctx);
    while let Some(request) = session.next_request() {
        match transport.send(request) {
            Ok(resp) => session.on_response(&resp.as_response()),
            Err(e) => session.on_transport_error(&e.to_string()),
        }
    }
    session.finish(now)
}

/// Stores a fresh history and the normals it computed (kept apart: they never
/// expire), named after the place; returns what was stored.
pub fn keep_history(
    cache: &Cache,
    key: &str,
    mut history: History,
    req: HistoryRequirements,
    now: UnixTime,
    today: LocalDate,
    name: &str,
) -> CachedHistory {
    if let Some(normals) = history.normals.take()
        && let Err(e) = cache.store_normals(key, &normals)
    {
        tracing::warn!("normals not saved: {e}");
    }
    name.clone_into(&mut history.location.name);
    let cached = CachedHistory {
        schema: HISTORY_SCHEMA,
        saved_at: now,
        today,
        past_days: req.past_days,
        climate_days: req.climate_days,
        records: req.records,
        month_to_date: req.month_to_date,
        history,
    };
    if let Err(e) = cache.store_history(key, &cached) {
        tracing::warn!("history not saved: {e}");
    }
    cached
}

/// What [`history_for`] needs besides the place.
pub struct HistorySource<'a> {
    /// The configuration (`[history]`, Open-Meteo's key and models).
    pub cfg: &'a CommonConfig,
    /// Environment lookups, for secrets.
    pub env: &'a dyn Fn(&str) -> Option<String>,
    /// Where history and normals are kept.
    pub cache: &'a Cache,
    /// The network.
    pub transport: &'a dyn Transport,
    /// Never fetch.
    pub offline: bool,
}

impl std::fmt::Debug for HistorySource<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HistorySource")
            .field("cache", &self.cache.dir())
            .field("offline", &self.offline)
            .finish_non_exhaustive()
    }
}

/// A history from [`history_for`].
#[derive(Debug)]
pub struct HistoryFor {
    /// The history, as kept.
    pub cached: CachedHistory,
    /// Whether it was fetched just now.
    pub fetched: bool,
    /// The fetch's attempts (empty from the cache).
    pub attempts: Vec<Attempt>,
}

/// Why [`history_for`] has no history.
#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    /// Offline, and nothing kept for the place.
    #[error("no history kept for this place yet (offline)")]
    NothingCached,
    /// Every source failed.
    #[error(transparent)]
    Fetch(#[from] FetchError),
}

/// The history of `place` for `req`: the kept one while it is current and
/// covers `req` (or whatever is kept, offline), else fetched and kept.
pub fn history_for(
    src: &HistorySource<'_>,
    place: &ResolvedLocation,
    req: HistoryRequirements,
    now: UnixTime,
) -> Result<HistoryFor, HistoryError> {
    let loc = Coordinates {
        latitude: place.latitude,
        longitude: place.longitude,
    };
    let key = history_key(loc);
    let kept = src.cache.load_history(&key);
    let offset = place.utc_offset;
    if src.offline || !history_due(kept.as_ref(), req, now, offset) {
        return kept
            .map(|cached| HistoryFor {
                cached,
                fetched: false,
                attempts: Vec::new(),
            })
            .ok_or(HistoryError::NothingCached);
    }
    let today = now.local_date(offset);
    let ctx = HistoryContext {
        now,
        today,
        utc_offset: offset,
        user_agent: user_agent(),
        normals: src.cache.load_normals(&key),
    };
    let chain = build_history_chain(src.cfg, src.env);
    let (history, attempts) = fetch_history(&chain, loc, req, ctx, src.transport)?;
    let cached = keep_history(src.cache, &key, history, req, now, today, &place.name);
    Ok(HistoryFor {
        cached,
        fetched: true,
        attempts,
    })
}

/// A climate query's kept answer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct CachedClimate {
    schema: u32,
    saved_at: UnixTime,
    query: ClimateQuery,
    history: History,
}

impl Cache {
    fn climate_path(&self, key: &str, q: &ClimateQuery) -> PathBuf {
        let md = |d: LocalDate| format!("{:02}{:02}", d.month, d.day);
        self.dir().join("climate").join(format!(
            "{key}-{}-{}-{}-{}.json",
            md(q.from),
            md(q.to),
            q.years.first,
            q.years.last
        ))
    }
}

/// A climate query's answer from [`climate_for`].
#[derive(Debug)]
pub struct ClimateFor {
    /// The climate of the query's dates (and the station or reanalysis).
    pub history: History,
    /// Whether it was fetched just now.
    pub fetched: bool,
    /// The fetch's attempts (empty when kept).
    pub attempts: Vec<Attempt>,
}

/// How `place`'s `query` dates were over its years (the vacation planner):
/// kept for good once the years are over (else for the day), else fetched from
/// `[history] order` and kept. The query's dates may be those of any year; the
/// answer is shown for them.
pub fn climate_for(
    src: &HistorySource<'_>,
    place: &ResolvedLocation,
    query: ClimateQuery,
    now: UnixTime,
) -> Result<ClimateFor, HistoryError> {
    let loc = Coordinates {
        latitude: place.latitude,
        longitude: place.longitude,
    };
    let path = src.cache.climate_path(&history_key(loc), &query);
    let today = now.local_date(place.utc_offset);
    let kept = Cache::load_json::<CachedClimate>(&path)
        .filter(|c| c.schema == HISTORY_SCHEMA && c.query == query)
        .filter(|c| {
            src.offline
                || query.years.last < today.year
                || c.saved_at.local_date(place.utc_offset) == today
        });
    if let Some(kept) = kept {
        return Ok(ClimateFor {
            history: kept.history,
            fetched: false,
            attempts: Vec::new(),
        });
    }
    if src.offline {
        return Err(HistoryError::NothingCached);
    }
    let ctx = HistoryContext {
        now,
        today,
        utc_offset: place.utc_offset,
        user_agent: user_agent(),
        normals: None,
    };
    let chain = build_history_chain(src.cfg, src.env);
    let primary: Vec<&dyn HistoryProvider> = chain.primary.iter().map(AsRef::as_ref).collect();
    let mut session = HistorySession::climate(&primary, loc, query, ctx);
    while let Some(request) = session.next_request() {
        match src.transport.send(request) {
            Ok(resp) => session.on_response(&resp.as_response()),
            Err(e) => session.on_transport_error(&e.to_string()),
        }
    }
    let (mut history, attempts) = session.finish(now)?;
    place.name.clone_into(&mut history.location.name);
    let kept = CachedClimate {
        schema: HISTORY_SCHEMA,
        saved_at: now,
        query,
        history,
    };
    match serde_json::to_vec(&kept) {
        Ok(json) => {
            if let Err(e) = write_atomic(&path, &json) {
                tracing::warn!("climate not saved: {e}");
            }
        }
        Err(e) => tracing::warn!("climate not saved: {e}"),
    }
    Ok(ClimateFor {
        history: kept.history,
        fetched: true,
        attempts,
    })
}

/// The ACIS stations near `place`, best first (US places only).
pub fn acis_stations(
    cfg: &CommonConfig,
    place: &ResolvedLocation,
    now: UnixTime,
    transport: &dyn Transport,
) -> Result<Vec<NearbyStation>, ProviderError> {
    let loc = Coordinates {
        latitude: place.latitude,
        longitude: place.longitude,
    };
    let ctx = HistoryContext {
        now,
        today: now.local_date(place.utc_offset),
        utc_offset: place.utc_offset,
        user_agent: user_agent(),
        normals: None,
    };
    let acis = Acis::new(AcisSettings {
        base_url: cfg
            .history
            .acis
            .base_url
            .clone()
            .unwrap_or_else(|| whether_providers::acis::DEFAULT_BASE_URL.to_string()),
        ..AcisSettings::default()
    });
    let request = acis.stations_request(&loc, &ctx)?;
    let resp = transport
        .send(&request)
        .map_err(|e| ProviderError::Transport(e.to_string()))?;
    Acis::nearby_stations(&resp.as_response(), &loc, &ctx)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cached(day: u8) -> CachedHistory {
        CachedHistory {
            schema: HISTORY_SCHEMA,
            saved_at: UnixTime(0),
            today: LocalDate {
                year: 2026,
                month: 10,
                day,
            },
            past_days: 7,
            climate_days: 7,
            records: true,
            month_to_date: true,
            history: whether_core::testdata::history_10001(),
        }
    }

    fn req() -> HistoryRequirements {
        HistoryRequirements {
            past_days: 7,
            climate_days: 7,
            records: true,
            month_to_date: true,
        }
    }

    const EDT: UtcOffset = UtcOffset(-4 * 3600);
    const EST: UtcOffset = UtcOffset(-5 * 3600);

    #[test]
    fn once_a_local_day_after_a_quarter_past_midnight() {
        // 2026-10-17 04:10 UTC is 00:10 EDT: yesterday's history waits.
        let at_0010 = UnixTime(1_792_195_200 + 4 * 3600 + 600);
        assert!(!history_due(Some(&cached(16)), req(), at_0010, EDT));
        let at_0020 = UnixTime(at_0010.0 + 600);
        assert!(history_due(Some(&cached(16)), req(), at_0020, EDT));
        assert!(
            !history_due(Some(&cached(17)), req(), at_0020, EDT),
            "done today"
        );
        assert!(history_due(None, req(), at_0010, EDT), "nothing cached");
        let more = HistoryRequirements {
            past_days: 14,
            ..req()
        };
        assert!(
            history_due(Some(&cached(17)), more, at_0020, EDT),
            "needs more days"
        );
    }

    #[test]
    fn the_local_day_follows_the_offset_across_dst() {
        // 2026-11-01 04:20 UTC: 00:20 EDT on the 1st, but 23:20 EST on the 31st
        // once the clocks have gone back.
        let t = UnixTime(1_793_505_600 + 1_200);
        let oct_31 = CachedHistory {
            today: LocalDate {
                year: 2026,
                month: 10,
                day: 31,
            },
            ..cached(17)
        };
        assert!(history_due(Some(&oct_31), req(), t, EDT));
        assert!(!history_due(Some(&oct_31), req(), t, EST));
    }

    #[test]
    fn the_chain_follows_the_config() {
        let cfg = CommonConfig::default();
        let chain = build_history_chain(&cfg, &|_| None);
        let ids = |l: &[Box<dyn HistoryProvider>]| l.iter().map(|p| p.id()).collect::<Vec<_>>();
        assert_eq!(
            ids(&chain.primary),
            [ProviderId::Acis, ProviderId::OpenMeteo]
        );
        assert_eq!(ids(&chain.supplements), [ProviderId::OpenMeteo]);
    }

    #[test]
    fn history_and_normals_round_trip_through_the_cache() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path());
        let key = history_key(Coordinates {
            latitude: 40.7143,
            longitude: -74.005_97,
        });
        assert_eq!(key, "40.714--74.006");
        assert_eq!(cache.load_history(&key), None);
        cache.store_history(&key, &cached(17)).unwrap();
        assert_eq!(cache.load_history(&key), Some(cached(17)));
        let normals = Normals {
            period: whether_core::history::YearRange::NORMALS_1991_2020,
            source: ProviderId::OpenMeteo,
            days: vec![whether_core::history::DayNormal::default(); 366],
        };
        cache.store_normals(&key, &normals).unwrap();
        assert_eq!(cache.load_normals(&key), Some(normals));
        fs::write(dir.path().join("history").join(format!("{key}.json")), "{").unwrap();
        assert_eq!(cache.load_history(&key), None, "corrupt files are dropped");
    }

    fn new_york() -> ResolvedLocation {
        ResolvedLocation {
            name: "New York".into(),
            latitude: 40.7143,
            longitude: -74.005_97,
            utc_offset: EST,
            ..ResolvedLocation::default()
        }
    }

    /// 2026-11-02 10:00 EST, the day the past-days fixture was recorded.
    const NOV_2: UnixTime = UnixTime(1_793_631_600);

    #[test]
    fn kept_until_the_next_day_and_offline() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path());
        let mut cfg = CommonConfig::default();
        cfg.history.order = vec![ProviderId::OpenMeteo];
        cfg.history.supplement = vec![];
        // Normals already kept: only the past days are asked for.
        let key = history_key(Coordinates {
            latitude: 40.7143,
            longitude: -74.005_97,
        });
        let normals = Normals {
            period: whether_core::history::YearRange::NORMALS_1991_2020,
            source: ProviderId::OpenMeteo,
            days: vec![whether_core::history::DayNormal::default(); 366],
        };
        cache.store_normals(&key, &normals).unwrap();
        let transport = crate::testing::Replay::new(vec![(
            "historical-forecast",
            200,
            include_str!(
                "../../whether-providers/tests/fixtures/open_meteo/history_past_10001.json"
            ),
        )]);
        let src = |offline| HistorySource {
            cfg: &cfg,
            env: &|_| None,
            cache: &cache,
            transport: &transport,
            offline,
        };
        assert!(matches!(
            history_for(&src(true), &new_york(), req(), NOV_2),
            Err(HistoryError::NothingCached)
        ));
        let first = history_for(&src(false), &new_york(), req(), NOV_2).unwrap();
        assert!(first.fetched);
        assert_eq!(first.cached.history.location.name, "New York");
        assert_eq!(first.cached.today.day, 2);
        assert_eq!(transport.seen().len(), 1);
        // The same day: kept, nothing asked.
        let again =
            history_for(&src(false), &new_york(), req(), UnixTime(NOV_2.0 + 3_600)).unwrap();
        assert!(!again.fetched);
        assert_eq!(again.cached, first.cached);
        assert_eq!(transport.seen().len(), 1);
        // The next day offline: still the kept one.
        let tomorrow = UnixTime(NOV_2.0 + 86_400);
        assert!(
            !history_for(&src(true), &new_york(), req(), tomorrow)
                .unwrap()
                .fetched
        );
        assert_eq!(transport.seen().len(), 1);
    }

    #[test]
    fn stations_near_a_place() {
        let transport = crate::testing::Replay::new(vec![(
            "StnMeta",
            200,
            include_str!("../../whether-providers/tests/fixtures/acis/stnmeta_10001.json"),
        )]);
        let list = acis_stations(&CommonConfig::default(), &new_york(), NOV_2, &transport).unwrap();
        assert_eq!(list[0].id, "NYC");
        assert!(list.iter().any(|s| s.id == "LGA"));
        assert!(transport.seen()[0].starts_with("https://data.rcc-acis.org/StnMeta"));
        let down = crate::testing::Replay::new(vec![]);
        assert!(matches!(
            acis_stations(&CommonConfig::default(), &new_york(), NOV_2, &down),
            Err(ProviderError::Transport(_))
        ));
    }

    #[test]
    fn climate_answers_are_kept_once_the_years_are_over() {
        use whether_core::history::YearRange;
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path());
        let mut cfg = CommonConfig::default();
        cfg.history.order = vec![ProviderId::OpenMeteo];
        let transport = crate::testing::Replay::new(vec![(
            "archive-api",
            200,
            include_str!(
                "../../whether-providers/tests/fixtures/open_meteo/climate_10001_0712_0719.json"
            ),
        )]);
        let src = |offline| HistorySource {
            cfg: &cfg,
            env: &|_| None,
            cache: &cache,
            transport: &transport,
            offline,
        };
        let on = |year, month, day| LocalDate { year, month, day };
        let years = YearRange {
            first: 1995,
            last: 2024,
        };
        let july = ClimateQuery::new(on(2027, 7, 12), on(2027, 7, 19), years).unwrap();
        assert!(matches!(
            climate_for(&src(true), &new_york(), july, NOV_2),
            Err(HistoryError::NothingCached)
        ));
        let first = climate_for(&src(false), &new_york(), july, NOV_2).unwrap();
        assert!(first.fetched);
        assert_eq!(first.history.climate.len(), 8);
        assert_eq!(first.history.location.name, "New York");
        // A year later, still kept: 1995–2024 is over.
        let later = UnixTime(NOV_2.0 + 365 * 86_400);
        let again = climate_for(&src(false), &new_york(), july, later).unwrap();
        assert!(!again.fetched);
        assert_eq!(again.history, first.history);
        assert_eq!(transport.seen().len(), 1);
        // Years up to this one are kept for the day only.
        let recent = ClimateQuery::new(
            on(2027, 7, 12),
            on(2027, 7, 19),
            YearRange {
                first: 1997,
                last: 2026,
            },
        )
        .unwrap();
        assert!(
            climate_for(&src(false), &new_york(), recent, NOV_2)
                .unwrap()
                .fetched
        );
        assert!(
            !climate_for(&src(false), &new_york(), recent, NOV_2)
                .unwrap()
                .fetched
        );
        let tomorrow = UnixTime(NOV_2.0 + 86_400);
        assert!(
            climate_for(&src(false), &new_york(), recent, tomorrow)
                .unwrap()
                .fetched
        );
        assert!(
            !climate_for(&src(true), &new_york(), recent, later)
                .unwrap()
                .fetched
        );
        assert_eq!(transport.seen().len(), 3);
    }
}
