//! `whether providers`: what each provider needs and offers, and One Call usage.

use std::fmt::Write as _;

use whether::cli::Cli;
use whether_config::LinuxConfig;
use whether_config::types::OwmApi;
use whether_core::model::ProviderId;
use whether_core::models::{CATALOGUE, Region};
use whether_providers::Coordinates;
use whether_providers::Provider;
use whether_providers::met_norway::{MetNorway, MetNorwaySettings};
use whether_providers::nws::{self, Nws};
use whether_providers::open_meteo::OpenMeteo;
use whether_providers::open_meteo::models::for_location;
use whether_providers::openweather::{FREE_DAILY_CALLS, OpenWeather, OpenWeatherSettings};
use whether_providers::pirate_weather::{
    DEFAULT_MONTHLY_CALL_LIMIT, FREE_MONTHLY_CALLS, PirateWeather, PirateWeatherSettings,
};
use whether_providers::regions::in_region;
use whether_runtime::{Cache, UsageStore, known_coordinates, paths};

use crate::common::{self, env};

fn capabilities(p: &dyn Provider) -> String {
    p.capabilities()
        .iter_names()
        .map(|(name, _)| name.to_lowercase().replace('_', "-"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The static provider table.
pub(crate) fn table() -> String {
    let open_meteo = OpenMeteo::default();
    let openweather = OpenWeather::new(OpenWeatherSettings::onecall4("", FREE_DAILY_CALLS));
    let met = MetNorway::new(MetNorwaySettings::new(""));
    let nws = Nws::forecaster(nws::DEFAULT_BASE_URL);
    let pirate = PirateWeather::new(PirateWeatherSettings::new(""));
    let rows: [(&dyn Provider, &str, String); 5] = [
        (
            &openweather,
            "OPENWEATHER_API_KEY",
            format!(
                "free: 60/min, 1M/month; onecall-4: daily_call_limit, at most {FREE_DAILY_CALLS} per UTC day"
            ),
        ),
        (
            &open_meteo,
            "none",
            "10,000 calls/day, non-commercial".to_string(),
        ),
        (
            &met,
            "none; [providers.met-norway] contact goes in the User-Agent",
            "honours Expires and If-Modified-Since (terms of service)".to_string(),
        ),
        (
            &nws,
            "none; a User-Agent naming the app",
            "US only; forecasts in providers.order, alerts only as a supplement".to_string(),
        ),
        (
            &pirate,
            "PIRATE_WEATHER_API_KEY (sent in a header)",
            format!(
                "free: {FREE_MONTHLY_CALLS}/month; monthly_call_limit defaults to {DEFAULT_MONTHLY_CALL_LIMIT}; one call per fetch, refreshed at most every 15 min"
            ),
        ),
    ];
    let mut out = String::new();
    for (p, auth, limits) in rows {
        let attribution = p.attribution();
        let _ = writeln!(out, "{}", p.id().id());
        let _ = writeln!(out, "  auth:         {auth}");
        let _ = writeln!(out, "  capabilities: {}", capabilities(p));
        let _ = writeln!(out, "  limits:       {limits}");
        let _ = writeln!(
            out,
            "  attribution:  {} ({})",
            attribution.text, attribution.license
        );
    }
    out
}

/// The call counts of the metered providers that are configured: "One Call 4.0
/// today: N of LIMIT" and "Pirate Weather this month: N of LIMIT".
pub(crate) fn usage(cfg: &LinuxConfig) -> Option<String> {
    let providers = &cfg.common.providers;
    let one_call = providers
        .openweather
        .as_ref()
        .filter(|owm| owm.api == Some(OwmApi::OneCall4));
    let pirate_listed = providers
        .order
        .iter()
        .chain(&providers.supplement)
        .any(|p| *p == ProviderId::PirateWeather);
    if one_call.is_none() && !pirate_listed {
        return None;
    }
    let store = UsageStore::at(paths::state_dir(&env)?.join("usage.json"));
    let ledger = store.lock().and_then(|guard| guard.load());
    let mut lines = Vec::new();
    if let Some(owm) = one_call {
        let limit = owm
            .daily_call_limit
            .unwrap_or(whether_config::types::DEFAULT_DAILY_CALL_LIMIT)
            .min(FREE_DAILY_CALLS);
        lines.push(match &ledger {
            Ok(ledger) => format!(
                "One Call 4.0 today (UTC): {} of {limit} calls ({})",
                ledger.used(ProviderId::OpenWeather, whether_display::now()),
                store.path().display()
            ),
            Err(e) => format!("One Call 4.0 ledger unavailable: {e}"),
        });
    }
    if pirate_listed {
        let limit = providers
            .pirate_weather
            .as_ref()
            .and_then(|p| p.monthly_call_limit)
            .unwrap_or(whether_config::types::DEFAULT_PIRATE_WEATHER_MONTHLY_LIMIT);
        lines.push(match &ledger {
            Ok(ledger) => format!(
                "Pirate Weather this month (UTC): {} of {limit} calls ({})",
                ledger.used(ProviderId::PirateWeather, whether_display::now()),
                store.path().display()
            ),
            Err(e) => format!("Pirate Weather ledger unavailable: {e}"),
        });
    }
    Some(lines.join("\n"))
}

fn region_label(region: Region) -> &'static str {
    match region {
        Region::Global => "global",
        Region::Conus => "lower 48",
        Region::NorthAmerica => "Canada, northern US",
    }
}

fn horizon(hours: u16) -> String {
    if hours >= 72 {
        format!("{} days", hours / 24)
    } else {
        format!("{hours} h")
    }
}

/// The model catalogue, with `✓` for models that may have data at `place` and the
/// configured list as a request for `place` would carry it.
pub(crate) fn models_table(place: Option<(&str, Coordinates)>, configured: &[String]) -> String {
    let mut out = String::new();
    let _ = match &place {
        Some((name, _)) => writeln!(
            out,
            "Open-Meteo weather models ([providers.open-meteo] models; ✓ = has data for {name})"
        ),
        None => writeln!(
            out,
            "Open-Meteo weather models ([providers.open-meteo] models; no location with known coordinates)"
        ),
    };
    for m in CATALOGUE {
        let mark = match &place {
            Some((_, loc)) if in_region(m.region, loc) => "✓",
            _ => " ",
        };
        let _ = writeln!(
            out,
            "  {mark} {:<23} {:<32} {:<20} {}",
            m.id,
            m.name,
            region_label(m.region),
            horizon(m.hours)
        );
    }
    if configured.is_empty() {
        let _ = writeln!(out, "\nconfigured: none (Open-Meteo's best match)");
    } else {
        let _ = writeln!(out, "\nconfigured: {}", configured.join(" → "));
        if let Some((name, loc)) = &place {
            let sent = for_location(configured, loc);
            let sent = if sent.is_empty() {
                "none of them; Open-Meteo's best match".to_string()
            } else {
                sent.join(" → ")
            };
            let _ = writeln!(out, "for {name}: {sent}");
        }
    }
    out
}

/// `whether providers models` for the loaded configuration and its first location.
pub(crate) fn models(cli: &Cli) -> String {
    let Ok(cfg) = common::load_config(cli, &[]) else {
        return models_table(None, &[]);
    };
    let configured = cfg
        .common
        .providers
        .open_meteo
        .models
        .clone()
        .map(|m| m.0)
        .unwrap_or_default();
    let cache = cfg
        .runtime
        .cache_dir
        .clone()
        .or_else(|| paths::cache_dir(&env))
        .map(Cache::at);
    let place = cfg.common.location.first().and_then(|loc| {
        let (latitude, longitude) = known_coordinates(loc, cache.as_ref()?)?;
        Some((
            loc.name
                .clone()
                .unwrap_or_else(|| "the first location".into()),
            Coordinates {
                latitude,
                longitude,
            },
        ))
    });
    models_table(
        place.as_ref().map(|(name, loc)| (name.as_str(), *loc)),
        &configured,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_table() {
        insta::assert_snapshot!(super::table());
    }

    #[test]
    fn model_table_for_new_york() {
        let new_york = Coordinates {
            latitude: 40.7143,
            longitude: -74.0060,
        };
        let configured = ["ncep_hrrr_conus", "ncep_nbm_conus"].map(String::from);
        insta::assert_snapshot!(models_table(Some(("New York", new_york)), &configured));
    }

    #[test]
    fn model_table_without_a_place() {
        let table = models_table(None, &[]);
        assert!(!table.contains('✓'), "{table}");
        assert!(
            table.ends_with("configured: none (Open-Meteo's best match)\n"),
            "{table}"
        );
    }
}
