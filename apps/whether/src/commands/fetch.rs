//! `whether fetch`: one fetch, printed as JSON (for debugging providers).

use std::process::ExitCode;

use anyhow::{Context, anyhow};
use whether_config::{LinuxConfig, ProvidersConfig};
use whether_core::model::{ProviderId, WeatherSnapshot};
use whether_core::requirements::DataRequirements;
use whether_providers::{Attempt, Coordinates, FetchContext};
use whether_runtime::{
    Cache, Geocoder, UreqTransport, UsageStore, build_chain, fetch_location, paths,
    resolve_location, user_agent,
};

use crate::commands::run::location_index;
use crate::common::{env, load_config};
use whether::cli::{Cli, FetchArgs};

/// Parses a provider id as written in the configuration.
pub(crate) fn parse_provider(id: &str) -> anyhow::Result<ProviderId> {
    [
        ProviderId::OpenWeather,
        ProviderId::OpenMeteo,
        ProviderId::MetNorway,
        ProviderId::PirateWeather,
    ]
    .into_iter()
    .find(|p| p.id() == id)
    .with_context(|| {
        format!("unknown provider `{id}` (openweather, open-meteo, met-norway, pirate-weather)")
    })
}

/// The printed document: the snapshot alone (`raw`) or with the attempts.
pub(crate) fn document(
    snapshot: &WeatherSnapshot,
    attempts: &[Attempt],
    raw: bool,
) -> anyhow::Result<String> {
    let value = if raw {
        serde_json::to_value(snapshot)?
    } else {
        let attempts: Vec<String> = attempts
            .iter()
            .map(|a| match &a.error {
                None => format!("{} {:?}: ok", a.provider.id(), a.endpoint),
                Some(e) => format!("{} {:?}: {e}", a.provider.id(), a.endpoint),
            })
            .collect();
        serde_json::json!({ "provider": snapshot.provider.id(), "attempts": attempts, "snapshot": snapshot })
    };
    Ok(serde_json::to_string_pretty(&value)?)
}

fn providers_for(cfg: &LinuxConfig, only: Option<ProviderId>) -> ProvidersConfig {
    let mut providers = cfg.common.providers.clone();
    if let Some(id) = only {
        providers.order = vec![id];
        providers.supplement = vec![];
    }
    providers
}

/// `whether fetch`.
pub(crate) fn run(cli: &Cli, args: &FetchArgs) -> anyhow::Result<ExitCode> {
    let cfg = match load_config(cli, &[]) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("{e}");
            return Ok(ExitCode::from(2));
        }
    };
    let only = args.provider.as_deref().map(parse_provider).transpose()?;
    let providers = providers_for(&cfg, only);
    let i = location_index(&cfg, cli.location.as_deref());
    let loc = cfg
        .common
        .location
        .get(i)
        .context("no location configured")?;
    let mut location = loc.clone();
    if only.is_some() {
        location.provider = None;
    }
    let transport = UreqTransport::new(&user_agent());
    let cache = Cache::at(
        cfg.runtime
            .cache_dir
            .clone()
            .or_else(|| paths::cache_dir(&env))
            .context("set HOME")?,
    );
    let geocoder = Geocoder {
        providers: &providers,
        env: &env,
        transport: &transport,
        language: &cfg.common.general.language,
    };
    let now = whether_display::now();
    let resolved = resolve_location(&location, &geocoder, &cache, now).map_err(|e| anyhow!(e))?;
    let chain = build_chain(&providers, &location, &env);
    let ctx = FetchContext {
        now,
        user_agent: user_agent(),
        utc_offset_hint: Some(resolved.utc_offset),
        if_modified_since: None,
        language: cfg.common.general.language.clone(),
    };
    let usage = paths::state_dir(&env).map(|d| UsageStore::at(d.join("usage.json")));
    let coordinates = Coordinates {
        latitude: resolved.latitude,
        longitude: resolved.longitude,
    };
    // Alerts the app already cached cost no call here either.
    let known_alerts = chain
        .primary
        .iter()
        .map(|p| p.id())
        .chain([ProviderId::OpenMeteo])
        .filter_map(|id| cache.load(&Cache::key(id, coordinates)))
        .flat_map(|c| c.snapshot.alerts)
        .filter(|a| a.id.is_some())
        .collect();
    let fetched = fetch_location(
        &chain,
        coordinates,
        DataRequirements::full(),
        ctx,
        &transport,
        usage.as_ref(),
        known_alerts,
    );
    if let Some(e) = fetched.ledger_error {
        eprintln!("One Call unavailable: {e}");
    }
    let (snapshot, attempts) = fetched.result.map_err(|e| anyhow!("{e}"))?;
    println!("{}", document(&snapshot, &attempts, args.raw)?);
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use whether_core::testdata;
    use whether_providers::{Endpoint, ProviderError};

    #[test]
    fn provider_ids() {
        assert_eq!(parse_provider("open-meteo").unwrap(), ProviderId::OpenMeteo);
        assert!(parse_provider("owm").is_err());
    }

    #[test]
    fn documents() {
        let snapshot = testdata::sample_snapshot();
        let attempts = [Attempt {
            provider: ProviderId::OpenWeather,
            endpoint: Endpoint::Current,
            error: Some(ProviderError::Transport("down".into())),
        }];
        let full: serde_json::Value =
            serde_json::from_str(&document(&snapshot, &attempts, false).unwrap()).unwrap();
        assert_eq!(full["attempts"][0], "openweather Current: network: down");
        let raw: serde_json::Value =
            serde_json::from_str(&document(&snapshot, &attempts, true).unwrap()).unwrap();
        assert!(raw.get("attempts").is_none() && raw.get("hourly").is_some());
    }
}
