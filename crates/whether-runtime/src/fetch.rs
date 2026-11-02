//! One location's fetch: drives `FetchSession` over a `Transport`, with the call
//! ledger locked for the whole fetch when a provider is metered.

use whether_core::model::{Alert, WeatherSnapshot};
use whether_core::requirements::DataRequirements;
use whether_core::time::UnixTime;
use whether_providers::{
    Attempt, Coordinates, FetchContext, FetchError, FetchSession, Provider, redact_url,
};

use crate::providers::Chain;
use crate::transport::Transport;
use crate::usage::{UsageGuard, UsageStore};

/// The outcome of one fetch.
#[derive(Debug)]
pub struct Fetched {
    /// The snapshot with every attempt, or why there is none.
    pub result: Result<(WeatherSnapshot, Vec<Attempt>), FetchError>,
    /// `Last-Modified` of the last successful response, for conditional requests.
    pub last_modified: Option<String>,
    /// Why metered providers were unavailable, if the ledger could not be used.
    pub ledger_error: Option<String>,
}

/// Locks and loads the ledger when the chain needs one.
fn open_ledger(
    chain: &Chain,
    store: Option<&UsageStore>,
) -> (
    Option<(UsageGuard, whether_providers::UsageLedger)>,
    Option<String>,
) {
    if !chain.is_metered() {
        return (None, None);
    }
    let Some(store) = store else {
        return (None, Some("no call ledger is configured".into()));
    };
    match store
        .lock()
        .and_then(|guard| guard.load().map(|ledger| (guard, ledger)))
    {
        Ok(pair) => (Some(pair), None),
        Err(e) => {
            tracing::warn!("metered providers disabled: {e}");
            (None, Some(e.to_string()))
        }
    }
}

/// Fetches `loc` through `chain`. Metered requests are sent only after the updated
/// ledger has been saved; if saving fails, the request is not sent. `known_alerts`
/// (from the cached snapshots) are reused instead of requesting their details again.
pub fn fetch_location(
    chain: &Chain,
    loc: Coordinates,
    req: DataRequirements,
    ctx: FetchContext,
    transport: &dyn Transport,
    usage: Option<&UsageStore>,
    known_alerts: Vec<Alert>,
) -> Fetched {
    let primary: Vec<&dyn Provider> = chain.primary.iter().map(AsRef::as_ref).collect();
    let supplements: Vec<&dyn Provider> = chain.supplements.iter().map(AsRef::as_ref).collect();
    let fetched_at = ctx.now;
    let (ledger, ledger_error) = open_ledger(chain, usage);
    let mut session =
        FetchSession::new(&primary, &supplements, loc, req, ctx).with_known_alerts(known_alerts);
    let guard = match ledger {
        Some((guard, ledger)) => {
            session = session.with_usage(ledger);
            Some(guard)
        }
        None => None,
    };
    let mut last_modified = None;
    while let Some(request) = session.next_request() {
        let request = request.clone();
        if request.metered {
            let saved = match (&guard, session.usage()) {
                (Some(guard), Some(ledger)) => guard.save(ledger).map_err(|e| e.to_string()),
                _ => Err("no call ledger".to_string()),
            };
            if let Err(e) = saved {
                tracing::warn!("not sending a metered request: {e}");
                session.on_transport_error(&format!("call ledger not saved: {e}"));
                continue;
            }
        }
        tracing::debug!(url = %redact_url(&request.url), metered = request.metered, "GET");
        match transport.send(&request) {
            Ok(resp) => {
                tracing::debug!(status = resp.status, bytes = resp.body.len(), "response");
                if resp.status == 200 && resp.last_modified.is_some() {
                    last_modified.clone_from(&resp.last_modified);
                }
                session.on_response(&resp.as_response());
            }
            Err(e) => {
                tracing::debug!(error = %e, "transport error");
                session.on_transport_error(&e.to_string());
            }
        }
    }
    drop(guard);
    Fetched {
        result: session.finish(UnixTime(fetched_at.0)),
        last_modified,
        ledger_error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use whether_config::types::OwmApi as OwmApiToml;
    use whether_config::{LocationConfig, ProvidersConfig};
    use whether_core::model::ProviderId;
    use whether_providers::{ProviderError, QuotaError};

    use crate::providers::build_chain;
    use crate::testing::Replay;

    const LOC: Coordinates = Coordinates {
        latitude: 40.7143,
        longitude: -74.005_97,
    };
    /// 2026-10-05 12:30 EDT.
    const NOW: UnixTime = UnixTime(1_791_217_800);

    macro_rules! fixture {
        ($path:literal) => {
            include_str!(concat!("../../whether-providers/tests/fixtures/", $path))
        };
    }

    fn open_meteo_routes() -> Vec<(&'static str, u16, &'static str)> {
        vec![
            (
                "air-quality-api.open-meteo.com",
                200,
                fixture!("open_meteo/air_quality_10001.json"),
            ),
            (
                "api.open-meteo.com",
                200,
                fixture!("open_meteo/forecast_full_10001.json"),
            ),
        ]
    }

    fn ctx() -> FetchContext {
        FetchContext {
            now: NOW,
            user_agent: "test".into(),
            utc_offset_hint: None,
            if_modified_since: None,
            language: "en".into(),
        }
    }

    fn key(k: &str) -> Option<String> {
        (k == "OPENWEATHER_API_KEY").then(|| "KEY".to_string())
    }

    #[test]
    fn openweather_401_falls_back_to_open_meteo() {
        let mut routes = vec![(
            "openweathermap.org",
            401,
            fixture!("openweather/error_401_invalid_key.json"),
        )];
        routes.extend(open_meteo_routes());
        let transport = Replay::new(routes);
        let chain = build_chain(
            &ProvidersConfig::default(),
            &LocationConfig::default(),
            &key,
        );
        let fetched = fetch_location(
            &chain,
            LOC,
            DataRequirements::full(),
            ctx(),
            &transport,
            None,
            Vec::new(),
        );
        let (snapshot, attempts) = fetched.result.unwrap();
        assert_eq!(snapshot.provider, ProviderId::OpenMeteo);
        assert!(matches!(
            attempts[0].error,
            Some(ProviderError::Unauthorized(_))
        ));
        assert!(
            transport
                .seen()
                .iter()
                .all(|u| !u.contains("KEY") || u.contains("openweathermap.org"))
        );
    }

    fn one_call_chain() -> Chain {
        one_call_chain_limited(4)
    }

    fn one_call_chain_limited(limit: u32) -> Chain {
        let mut cfg = ProvidersConfig::default();
        let owm = cfg.openweather.as_mut().unwrap();
        owm.api = Some(OwmApiToml::OneCall4);
        owm.daily_call_limit = Some(limit);
        build_chain(&cfg, &LocationConfig::default(), &key)
    }

    fn one_call_routes() -> Vec<(&'static str, u16, &'static str)> {
        let mut routes = vec![
            (
                "/onecall/current",
                200,
                fixture!("openweather/onecall4_current_10001.synthetic.json"),
            ),
            (
                "/timeline/1h",
                200,
                fixture!("openweather/onecall4_1h_p0_10001.synthetic.json"),
            ),
            (
                "/timeline/1day",
                200,
                fixture!("openweather/onecall4_1day_10001.synthetic.json"),
            ),
            (
                "/timeline/15min",
                200,
                fixture!("openweather/onecall4_15min_10001.synthetic.json"),
            ),
            (
                "air_pollution",
                200,
                fixture!("openweather/air_pollution_10001.json"),
            ),
        ];
        routes.extend(open_meteo_routes());
        routes
    }

    #[test]
    fn one_call_spends_from_the_persisted_ledger() {
        let dir = tempfile::tempdir().unwrap();
        let store = UsageStore::at(dir.path().join("usage.json"));
        let transport = Replay::new(one_call_routes());
        let chain = one_call_chain();
        for _ in 0..3 {
            let fetched = fetch_location(
                &chain,
                LOC,
                DataRequirements::full(),
                ctx(),
                &transport,
                Some(&store),
                Vec::new(),
            );
            assert!(fetched.result.is_ok());
        }
        let sent = transport
            .seen()
            .iter()
            .filter(|u| u.contains("/data/4.0/"))
            .count();
        assert_eq!(sent, 4, "the limit of 4 holds across fetches");
        let ledger = store.lock().unwrap().load().unwrap();
        assert_eq!(ledger.used(ProviderId::OpenWeather, NOW), 4);
    }

    #[test]
    fn a_corrupt_ledger_disables_one_call() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("usage.json");
        std::fs::write(&path, "garbage").unwrap();
        let transport = Replay::new(one_call_routes());
        let fetched = fetch_location(
            &one_call_chain(),
            LOC,
            DataRequirements::full(),
            ctx(),
            &transport,
            Some(&UsageStore::at(&path)),
            Vec::new(),
        );
        assert!(fetched.ledger_error.unwrap().contains("corrupt"));
        assert!(transport.seen().iter().all(|u| !u.contains("/data/4.0/")));
        let (snapshot, attempts) = fetched.result.unwrap();
        assert_eq!(snapshot.provider, ProviderId::OpenMeteo);
        assert_eq!(
            attempts[0].error,
            Some(ProviderError::Quota(QuotaError::NoLedger))
        );
    }

    #[test]
    fn no_store_means_no_one_call() {
        let transport = Replay::new(one_call_routes());
        let fetched = fetch_location(
            &one_call_chain(),
            LOC,
            DataRequirements::full(),
            ctx(),
            &transport,
            None,
            Vec::new(),
        );
        assert!(fetched.ledger_error.is_some());
        assert!(transport.seen().iter().all(|u| !u.contains("/data/4.0/")));
        assert!(fetched.result.is_ok());
    }

    /// Records each request's URL and headers, and answers with one forecast.
    type Sent = Vec<(String, Vec<(&'static str, String)>)>;
    struct Spy(std::sync::Mutex<Sent>);

    impl crate::transport::Transport for Spy {
        fn send(
            &self,
            req: &whether_providers::HttpRequest,
        ) -> Result<crate::transport::OwnedResponse, crate::transport::TransportError> {
            self.0
                .lock()
                .unwrap()
                .push((req.url.clone(), req.headers.clone()));
            Ok(crate::transport::OwnedResponse::new(
                200,
                fixture!("pirate_weather/forecast_10001.json"),
            ))
        }
    }

    #[test]
    fn pirate_weather_sends_its_key_in_a_header_and_spends_the_month() {
        let dir = tempfile::tempdir().unwrap();
        let store = UsageStore::at(dir.path().join("usage.json"));
        let cfg = ProvidersConfig {
            order: vec![ProviderId::PirateWeather],
            supplement: vec![],
            ..ProvidersConfig::default()
        };
        let chain = build_chain(&cfg, &LocationConfig::default(), &|k| {
            (k == "PIRATE_WEATHER_API_KEY").then(|| "PWKEY".to_string())
        });
        let transport = Spy(std::sync::Mutex::new(Vec::new()));
        // The recording's own time, so its blocks parse as current.
        let ctx = FetchContext {
            now: UnixTime(1_793_637_720),
            ..ctx()
        };
        let fetched = fetch_location(
            &chain,
            LOC,
            DataRequirements::full(),
            ctx.clone(),
            &transport,
            Some(&store),
            Vec::new(),
        );
        let (snapshot, _) = fetched.result.unwrap();
        assert_eq!(snapshot.provider, ProviderId::PirateWeather);
        assert_eq!(snapshot.daily.len(), 8);
        let seen = transport.0.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert!(!seen[0].0.contains("PWKEY"), "{}", seen[0].0);
        assert!(seen[0].1.contains(&("apikey", "PWKEY".to_string())));
        let ledger = store.lock().unwrap().load().unwrap();
        assert_eq!(ledger.used(ProviderId::PirateWeather, ctx.now), 1);
        // A month later the count starts again.
        let later = UnixTime(ctx.now.0 + 40 * 86_400);
        assert_eq!(ledger.used(ProviderId::PirateWeather, later), 0);
    }

    #[test]
    fn all_failing_reports_every_attempt() {
        let transport = Replay::new(vec![]);
        let chain = build_chain(
            &ProvidersConfig::default(),
            &LocationConfig::default(),
            &|_| None,
        );
        let fetched = fetch_location(
            &chain,
            LOC,
            DataRequirements::full(),
            ctx(),
            &transport,
            None,
            Vec::new(),
        );
        let Err(FetchError::AllProvidersFailed(attempts)) = fetched.result else {
            panic!()
        };
        assert!(matches!(
            attempts[0].error,
            Some(ProviderError::Transport(_))
        ));
    }

    #[test]
    fn known_alerts_are_not_fetched_again() {
        let dir = tempfile::tempdir().unwrap();
        let store = UsageStore::at(dir.path().join("usage.json"));
        let mut routes = vec![
            (
                "/onecall/alert/",
                200,
                fixture!("openweather/onecall4_alert_10001.synthetic.json"),
            ),
            (
                "/onecall/current",
                200,
                fixture!("openweather/onecall4_current_alerts_10001.synthetic.json"),
            ),
        ];
        routes.extend(one_call_routes());
        let transport = Replay::new(routes);
        let chain = one_call_chain_limited(100);
        let fetch = |known| {
            fetch_location(
                &chain,
                LOC,
                DataRequirements::full(),
                ctx(),
                &transport,
                Some(&store),
                known,
            )
            .result
            .unwrap()
            .0
        };
        let first = fetch(Vec::new());
        assert_eq!(first.alerts.len(), 1);
        let alert_calls = || {
            transport
                .seen()
                .iter()
                .filter(|u| u.contains("/onecall/alert/"))
                .count()
        };
        assert_eq!(alert_calls(), 1);
        let second = fetch(first.alerts.clone());
        assert_eq!(second.alerts, first.alerts);
        assert_eq!(alert_calls(), 1, "the known alert was reused");
    }
}
