//! Building each location's provider chain from the configuration.

use std::sync::atomic::{AtomicBool, Ordering};

use whether_config::secret_resolve::SecretError;
use whether_config::types::{
    DEFAULT_PIRATE_WEATHER_MONTHLY_LIMIT, MetNorwayToml, MetProduct as MetProductToml,
    OpenMeteoToml, OpenWeatherToml, OwmApi as OwmApiToml, PIRATE_WEATHER_KEY_ENV,
    PirateWeatherToml,
};
use whether_config::{LocationConfig, NwsCurrent, ProvidersConfig, Secret};
use whether_core::model::ProviderId;
use whether_providers::Provider;
use whether_providers::met_norway::{MetNorway, MetNorwaySettings, MetProduct};
use whether_providers::nws::{self, Nws};
use whether_providers::open_meteo::{OpenMeteo, OpenMeteoSettings};
use whether_providers::openweather::{
    DEFAULT_BASE_URL, DEFAULT_DAILY_CALL_LIMIT, OpenWeather, OpenWeatherSettings, OwmApi,
};
use whether_providers::pirate_weather::{PirateWeather, PirateWeatherSettings};

/// The providers for one location: the fallback chain and the supplements.
pub struct Chain {
    /// In fallback order; the first that succeeds is the primary.
    pub primary: Vec<Box<dyn Provider>>,
    /// Fill what the primary lacks.
    pub supplements: Vec<Box<dyn Provider>>,
}

impl std::fmt::Debug for Chain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ids = |v: &[Box<dyn Provider>]| v.iter().map(|p| p.id()).collect::<Vec<_>>();
        f.debug_struct("Chain")
            .field("primary", &ids(&self.primary))
            .field("supplements", &ids(&self.supplements))
            .finish()
    }
}

impl Chain {
    /// Whether any provider sends metered requests (so a call ledger is needed).
    pub fn is_metered(&self) -> bool {
        self.primary
            .iter()
            .chain(&self.supplements)
            .any(|p| p.budget().is_some())
    }
}

/// Logs a message once per process, as a warning or (when `quiet`) as info.
fn log_once(flag: &AtomicBool, quiet: bool, message: &str) {
    if !flag.swap(true, Ordering::Relaxed) {
        if quiet {
            tracing::info!("{message}");
        } else {
            tracing::warn!("{message}");
        }
    }
}

/// Whether a key that cannot be resolved is an ordinary setup rather than a
/// mistake: an unset environment variable just means no key (Open-Meteo needs
/// none), while a missing file, a failing command or an empty value is worth a
/// warning.
fn unset_is_ordinary(error: &SecretError) -> bool {
    matches!(error, SecretError::EnvUnset(_))
}

static OPENWEATHER_SKIPPED: AtomicBool = AtomicBool::new(false);
static MET_SKIPPED: AtomicBool = AtomicBool::new(false);
static PIRATE_SKIPPED: AtomicBool = AtomicBool::new(false);

/// Open-Meteo from `[providers.open-meteo]`. Its `models` applies when it
/// forecasts; as a supplement it uses its best match, which fills what the chosen
/// model lacks (the NOAA blend has no UV or pressure).
fn open_meteo(
    toml: &OpenMeteoToml,
    env: &dyn Fn(&str) -> Option<String>,
    as_supplement: bool,
) -> OpenMeteo {
    let defaults = OpenMeteoSettings::default();
    let api_key = toml
        .api_key
        .as_ref()
        .and_then(|secret| match secret.resolve(env) {
            Ok(key) => Some(key),
            Err(e) => {
                tracing::warn!("Open-Meteo API key not used: {e}");
                None
            }
        });
    OpenMeteo::new(OpenMeteoSettings {
        base_url: toml.base_url.clone().unwrap_or(defaults.base_url),
        air_quality_url: toml
            .air_quality_url
            .clone()
            .unwrap_or(defaults.air_quality_url),
        api_key,
        models: if as_supplement {
            Vec::new()
        } else {
            toml.models.clone().map(|m| m.0).unwrap_or_default()
        },
    })
}

fn openweather(
    toml: &OpenWeatherToml,
    env: &dyn Fn(&str) -> Option<String>,
) -> Option<OpenWeather> {
    let secret = toml.api_key.as_ref()?;
    let api_key = match secret.resolve(env) {
        Ok(key) => key,
        Err(e) => {
            log_once(
                &OPENWEATHER_SKIPPED,
                unset_is_ordinary(&e),
                &format!("OpenWeather skipped ({e}); Open-Meteo serves instead"),
            );
            return None;
        }
    };
    let api = match toml.api.unwrap_or_default() {
        OwmApiToml::Free => OwmApi::Free,
        OwmApiToml::OneCall3 => OwmApi::OneCall3,
        OwmApiToml::OneCall4 => OwmApi::OneCall4,
    };
    Some(OpenWeather::new(OpenWeatherSettings {
        api_key,
        api,
        base_url: toml
            .base_url
            .clone()
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
        daily_call_limit: toml.daily_call_limit.unwrap_or(DEFAULT_DAILY_CALL_LIMIT),
    }))
}

/// MET Norway from `[providers.met-norway]`, when a contact is configured.
fn met_norway(toml: Option<&MetNorwayToml>) -> Option<MetNorway> {
    let toml = toml?;
    let contact = toml
        .contact
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())?;
    let mut settings = MetNorwaySettings::new(contact);
    settings.product = match toml.product.unwrap_or_default() {
        MetProductToml::Complete => MetProduct::Complete,
        MetProductToml::Compact => MetProduct::Compact,
    };
    if let Some(base) = &toml.base_url {
        settings.base_url.clone_from(base);
    }
    Some(MetNorway::new(settings))
}

/// Pirate Weather from `[providers.pirate-weather]`, whose section is optional: the
/// key defaults to `PIRATE_WEATHER_API_KEY`, and without one the provider is left
/// out so the next in the chain serves.
fn pirate_weather(
    toml: Option<&PirateWeatherToml>,
    env: &dyn Fn(&str) -> Option<String>,
) -> Option<PirateWeather> {
    let default_key = Secret::Env {
        env: PIRATE_WEATHER_KEY_ENV.to_string(),
    };
    let secret = toml
        .and_then(|t| t.api_key.as_ref())
        .unwrap_or(&default_key);
    let api_key = match secret.resolve(env) {
        Ok(key) => key,
        Err(e) => {
            log_once(
                &PIRATE_SKIPPED,
                unset_is_ordinary(&e),
                &format!("Pirate Weather skipped ({e}); the next provider serves instead"),
            );
            return None;
        }
    };
    let mut settings = PirateWeatherSettings::new(api_key);
    settings.monthly_call_limit = DEFAULT_PIRATE_WEATHER_MONTHLY_LIMIT;
    if let Some(toml) = toml {
        if let Some(base) = &toml.base_url {
            settings.base_url.clone_from(base);
        }
        settings.exclude_models = toml.exclude_models.clone().unwrap_or_default();
        settings.ai_models = toml.ai_models.unwrap_or_default();
        if let Some(limit) = toml.monthly_call_limit {
            settings.monthly_call_limit = limit;
        }
    }
    Some(PirateWeather::new(settings))
}

fn provider(
    id: ProviderId,
    cfg: &ProvidersConfig,
    env: &dyn Fn(&str) -> Option<String>,
    as_supplement: bool,
) -> Option<Box<dyn Provider>> {
    match id {
        ProviderId::OpenMeteo => Some(Box::new(open_meteo(&cfg.open_meteo, env, as_supplement))),
        ProviderId::Nws => {
            let base = cfg.nws.base_url.as_deref().unwrap_or(nws::DEFAULT_BASE_URL);
            // In the chain it forecasts; as a supplement it gives alerts only.
            Some(Box::new(if as_supplement {
                Nws::new(base)
            } else {
                Nws::forecaster(base).observing(cfg.nws.current != Some(NwsCurrent::Forecast))
            }))
        }
        ProviderId::OpenWeather => cfg
            .openweather
            .as_ref()
            .and_then(|o| openweather(o, env))
            .map(|p| Box::new(p) as Box<dyn Provider>),
        // History only; validation keeps it out of the forecast lists.
        ProviderId::Acis => None,
        ProviderId::PirateWeather => pirate_weather(cfg.pirate_weather.as_ref(), env)
            .map(|p| Box::new(p) as Box<dyn Provider>),
        ProviderId::MetNorway => {
            let met = met_norway(cfg.met_norway.as_ref());
            if met.is_none() {
                log_once(
                    &MET_SKIPPED,
                    false,
                    "MET Norway skipped: [providers.met-norway] contact is not set (its terms require one)",
                );
            }
            met.map(|p| Box::new(p) as Box<dyn Provider>)
        }
    }
}

/// The chain for `loc`: its `provider` override first, then `providers.order`, each
/// once; providers whose key cannot be resolved are left out. Supplements follow
/// `providers.supplement`.
pub fn build_chain(
    cfg: &ProvidersConfig,
    loc: &LocationConfig,
    env: &dyn Fn(&str) -> Option<String>,
) -> Chain {
    let mut ids: Vec<ProviderId> = loc.provider.into_iter().collect();
    for id in &cfg.order {
        if !ids.contains(id) {
            ids.push(*id);
        }
    }
    let primary = ids
        .into_iter()
        .filter_map(|id| provider(id, cfg, env, false))
        .collect();
    let mut supplement_ids: Vec<ProviderId> = Vec::new();
    for id in &cfg.supplement {
        if !supplement_ids.contains(id) {
            supplement_ids.push(*id);
        }
    }
    let supplements = supplement_ids
        .into_iter()
        .filter_map(|id| provider(id, cfg, env, true))
        .collect();
    Chain {
        primary,
        supplements,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn met_norway_needs_a_contact() {
        assert!(met_norway(None).is_none());
        let blank = MetNorwayToml {
            contact: Some("  ".into()),
            ..MetNorwayToml::default()
        };
        assert!(met_norway(Some(&blank)).is_none());
        let set = MetNorwayToml {
            contact: Some("me@example.org".into()),
            product: Some(MetProductToml::Compact),
            ..MetNorwayToml::default()
        };
        let p = met_norway(Some(&set)).unwrap();
        assert_eq!(p.id(), ProviderId::MetNorway);
    }

    #[test]
    fn only_an_unset_key_variable_is_ordinary() {
        assert!(unset_is_ordinary(&SecretError::EnvUnset(
            "OPENWEATHER_API_KEY".into()
        )));
        assert!(!unset_is_ordinary(&SecretError::Empty(
            "OPENWEATHER_API_KEY".into()
        )));
        assert!(!unset_is_ordinary(&SecretError::NoSource));
    }

    use super::*;
    use whether_config::{ModelList, Secret};
    use whether_providers::Capabilities;

    fn ids(v: &[Box<dyn Provider>]) -> Vec<ProviderId> {
        v.iter().map(|p| p.id()).collect()
    }

    fn with_key(k: &str) -> Option<String> {
        (k == "OPENWEATHER_API_KEY").then(|| "KEY".to_string())
    }

    #[test]
    fn default_order_with_a_key() {
        let chain = build_chain(
            &ProvidersConfig::default(),
            &LocationConfig::default(),
            &with_key,
        );
        assert_eq!(
            ids(&chain.primary),
            vec![ProviderId::OpenWeather, ProviderId::OpenMeteo]
        );
        assert_eq!(ids(&chain.supplements), vec![ProviderId::OpenMeteo]);
        assert!(!chain.is_metered(), "the free API is not metered");
    }

    #[test]
    fn a_chosen_model_forecasts_and_the_best_match_fills_gaps() {
        let mut cfg = ProvidersConfig {
            order: vec![ProviderId::OpenMeteo, ProviderId::OpenWeather],
            supplement: vec![ProviderId::OpenMeteo, ProviderId::Nws],
            ..ProvidersConfig::default()
        };
        cfg.open_meteo.models = Some(ModelList(vec![
            "ncep_hrrr_conus".into(),
            "ncep_nbm_conus".into(),
        ]));
        let chain = build_chain(&cfg, &LocationConfig::default(), &with_key);
        assert_eq!(
            ids(&chain.primary),
            vec![ProviderId::OpenMeteo, ProviderId::OpenWeather]
        );
        assert_eq!(
            chain.primary[0].variant(),
            Some("ncep_hrrr_conus,ncep_nbm_conus")
        );
        assert_eq!(
            ids(&chain.supplements),
            vec![ProviderId::OpenMeteo, ProviderId::Nws]
        );
        assert_eq!(chain.supplements[0].variant(), None, "best match");
    }

    #[test]
    fn nws_forecasts_in_the_chain_and_alerts_as_a_supplement() {
        let cfg = ProvidersConfig {
            order: vec![ProviderId::Nws, ProviderId::OpenMeteo],
            supplement: vec![ProviderId::OpenMeteo, ProviderId::Nws],
            ..ProvidersConfig::default()
        };
        let chain = build_chain(&cfg, &LocationConfig::default(), &with_key);
        let caps = |p: &dyn Provider| p.capabilities();
        assert!(caps(&*chain.primary[0]).contains(Capabilities::HOURLY));
        assert_eq!(caps(&*chain.supplements[1]), Capabilities::ALERTS);
    }

    #[test]
    fn nws_current_conditions_follow_the_setting() {
        const POINT: &[u8] =
            include_bytes!("../../whether-providers/tests/fixtures/nws/points_10001.json");
        let asks_stations = |current: Option<NwsCurrent>| {
            let mut cfg = ProvidersConfig {
                order: vec![ProviderId::Nws],
                ..ProvidersConfig::default()
            };
            cfg.nws.current = current;
            let chain = build_chain(&cfg, &LocationConfig::default(), &with_key);
            let ctx = whether_providers::FetchContext {
                now: whether_core::time::UnixTime(0),
                user_agent: "test".into(),
                utc_offset_hint: None,
                if_modified_since: None,
                language: "en".into(),
            };
            let follow = chain.primary[0]
                .follow_up(
                    whether_providers::Endpoint::Point,
                    &whether_providers::HttpResponse::new(200, POINT),
                    &ctx,
                )
                .unwrap();
            follow
                .iter()
                .any(|r| r.endpoint == whether_providers::Endpoint::Stations)
        };
        assert!(asks_stations(None), "observed by default");
        assert!(asks_stations(Some(NwsCurrent::Observed)));
        assert!(!asks_stations(Some(NwsCurrent::Forecast)));
    }

    #[test]
    fn missing_key_leaves_openweather_out() {
        let chain = build_chain(
            &ProvidersConfig::default(),
            &LocationConfig::default(),
            &|_| None,
        );
        assert_eq!(ids(&chain.primary), vec![ProviderId::OpenMeteo]);
    }

    #[test]
    fn location_override_comes_first_once() {
        let cfg = ProvidersConfig {
            order: vec![ProviderId::OpenWeather, ProviderId::OpenMeteo],
            ..Default::default()
        };
        let loc = LocationConfig {
            provider: Some(ProviderId::OpenMeteo),
            ..Default::default()
        };
        let chain = build_chain(&cfg, &loc, &with_key);
        assert_eq!(
            ids(&chain.primary),
            vec![ProviderId::OpenMeteo, ProviderId::OpenWeather]
        );
    }

    #[test]
    fn one_call_is_metered_with_the_configured_limit() {
        let mut cfg = ProvidersConfig::default();
        let owm = cfg.openweather.as_mut().unwrap();
        owm.api = Some(OwmApiToml::OneCall4);
        owm.daily_call_limit = Some(700);
        let chain = build_chain(&cfg, &LocationConfig::default(), &with_key);
        assert!(chain.is_metered());
        assert_eq!(
            chain.primary[0].budget(),
            Some(whether_providers::Budget::daily(700))
        );
    }

    fn pirate_key(k: &str) -> Option<String> {
        (k == "PIRATE_WEATHER_API_KEY").then(|| "PWKEY".to_string())
    }

    #[test]
    fn pirate_weather_works_from_the_environment_alone() {
        let cfg = ProvidersConfig {
            order: vec![ProviderId::PirateWeather, ProviderId::OpenMeteo],
            ..ProvidersConfig::default()
        };
        assert!(cfg.pirate_weather.is_none());
        let chain = build_chain(&cfg, &LocationConfig::default(), &pirate_key);
        assert_eq!(
            ids(&chain.primary),
            vec![ProviderId::PirateWeather, ProviderId::OpenMeteo]
        );
        assert!(chain.is_metered());
        assert_eq!(
            chain.primary[0].budget(),
            Some(whether_providers::Budget::monthly(9_000))
        );
        // Without the variable it is left out and the next provider serves.
        let chain = build_chain(&cfg, &LocationConfig::default(), &|_| None);
        assert_eq!(ids(&chain.primary), vec![ProviderId::OpenMeteo]);
        assert!(!chain.is_metered());
    }

    #[test]
    fn pirate_weather_takes_its_settings_from_the_section() {
        let section = PirateWeatherToml {
            api_key: Some(Secret::Env {
                env: "MY_PIRATE_KEY".into(),
            }),
            exclude_models: Some(vec!["gfs".into()]),
            ai_models: Some(true),
            monthly_call_limit: Some(1_200),
            base_url: Some("http://localhost:9/".into()),
        };
        let env = |k: &str| (k == "MY_PIRATE_KEY").then(|| "CUSTOM".to_string());
        let p = pirate_weather(Some(&section), &env).unwrap();
        assert_eq!(p.budget(), Some(whether_providers::Budget::monthly(1_200)));
        let ctx = whether_providers::FetchContext {
            now: whether_core::time::UnixTime(0),
            user_agent: "t".into(),
            utc_offset_hint: None,
            if_modified_since: None,
            language: "en".into(),
        };
        let plan = p
            .plan(
                &whether_providers::Coordinates {
                    latitude: 1.0,
                    longitude: 2.0,
                },
                &whether_core::requirements::DataRequirements::full(),
                &ctx,
            )
            .unwrap();
        assert_eq!(
            plan[0].url,
            "http://localhost:9/forecast/key/1,2?units=si&version=2&exclude=summary,gfs&include=aimodels"
        );
        assert!(plan[0].headers.contains(&("apikey", "CUSTOM".to_string())));
        // The variable the section names is the only one read.
        assert!(pirate_weather(Some(&section), &pirate_key).is_none());
    }

    /// The config crate checks `exclude_models` against its own copy of the list.
    #[test]
    fn the_config_knows_every_pirate_weather_model() {
        assert_eq!(
            whether_config::types::PIRATE_WEATHER_MODELS,
            whether_providers::pirate_weather::MODELS
        );
        assert_eq!(
            whether_config::types::DEFAULT_PIRATE_WEATHER_MONTHLY_LIMIT,
            whether_providers::pirate_weather::DEFAULT_MONTHLY_CALL_LIMIT
        );
    }

    #[test]
    fn met_norway_is_skipped_and_literal_keys_work() {
        let mut cfg = ProvidersConfig {
            order: vec![ProviderId::MetNorway, ProviderId::OpenWeather],
            supplement: vec![],
            ..Default::default()
        };
        cfg.openweather.as_mut().unwrap().api_key = Some(Secret::Literal("K".into()));
        let chain = build_chain(&cfg, &LocationConfig::default(), &|_| None);
        assert_eq!(ids(&chain.primary), vec![ProviderId::OpenWeather]);
        assert!(chain.supplements.is_empty());
        assert!(format!("{chain:?}").contains("OpenWeather"));
    }
}
