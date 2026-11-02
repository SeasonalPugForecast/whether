//! Semantic validation after merging: what serde cannot check.

use alloc::collections::BTreeSet;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use whether_core::model::ProviderId;
use whether_core::strings::{Strings, Text};
use whether_core::time::parse_utc_offset;

use crate::secret::Secret;
use crate::types::{
    CommonConfig, LocationConfig, MIN_REFRESH, ONECALL_FREE_DAILY_CALLS, OwmApi,
    PIRATE_WEATHER_FREE_MONTHLY_CALLS, PIRATE_WEATHER_MODELS, PirateWeatherToml, PresentationToml,
    ProvidersConfig,
};

/// How serious an [`Issue`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// The configuration cannot be used.
    Error,
    /// The configuration works, perhaps not as intended.
    Warning,
}

/// One problem found by validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Issue {
    /// Error or warning.
    pub severity: Severity,
    /// Dotted path of the offending key, e.g. `location[0].latitude`.
    pub path: String,
    /// What is wrong and how to fix it.
    pub message: String,
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let level = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        write!(f, "{level}: {}: {}", self.path, self.message)
    }
}

/// Whether any issue is an error.
pub fn has_errors(issues: &[Issue]) -> bool {
    issues.iter().any(|i| i.severity == Severity::Error)
}

/// Collects issues with their paths.
#[derive(Default)]
struct Issues(Vec<Issue>);

impl Issues {
    fn error(&mut self, path: impl Into<String>, message: impl Into<String>) {
        self.push(Severity::Error, path.into(), message.into());
    }
    fn warning(&mut self, path: impl Into<String>, message: impl Into<String>) {
        self.push(Severity::Warning, path.into(), message.into());
    }
    fn push(&mut self, severity: Severity, path: String, message: String) {
        self.0.push(Issue {
            severity,
            path,
            message,
        });
    }
}

impl CommonConfig {
    /// Every semantic problem, errors and warnings, in document order.
    pub fn validate(&self) -> Vec<Issue> {
        let mut issues = Issues::default();
        if self.version != 1 {
            issues.error(
                "version",
                format!("unsupported version {}; only 1 exists", self.version),
            );
        }
        for (list, ids) in [
            ("order", &self.history.order),
            ("supplement", &self.history.supplement),
        ] {
            for (n, id) in ids.iter().enumerate() {
                if !matches!(id, ProviderId::Acis | ProviderId::OpenMeteo) {
                    issues.error(
                        format!("history.{list}[{n}]"),
                        format!(
                            "`{}` gives no history; use \"acis\" or \"open-meteo\"",
                            id.id()
                        ),
                    );
                }
            }
        }
        for (key, id) in [
            ("history.acis.station", &self.history.acis.station),
            (
                "history.acis.records_station",
                &self.history.acis.records_station,
            ),
        ] {
            if let Some(id) = id
                && (id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric()))
            {
                issues.error(
                    key,
                    format!("`{id}` is not an ACIS station id such as \"LGA\" or \"LGAthr\""),
                );
            }
        }
        if self.general.refresh < MIN_REFRESH {
            issues.warning(
                "general.refresh",
                format!(
                    "{} is below the minimum; {MIN_REFRESH} is used",
                    self.general.refresh
                ),
            );
        }
        if self.general.coordinate_precision > 4 {
            issues.error(
                "general.coordinate_precision",
                "at most 4 decimals (about 11 m)",
            );
        }
        validate_locations(&self.location, &self.providers, &mut issues);
        validate_providers(&self.providers, &mut issues);
        issues.0.extend(validate_presentation(
            &self.presentation,
            "presentation",
            &self.providers,
        ));
        for key in self.strings.keys() {
            if Text::from_key(key).is_none() {
                issues.warning(format!("strings.{key}"), unknown_string_message(key));
            }
        }
        issues.0
    }
}

fn validate_locations(
    locations: &[LocationConfig],
    providers: &ProvidersConfig,
    issues: &mut Issues,
) {
    if locations.is_empty() {
        issues.error(
            "location",
            "no [[location]] configured; find one with `whether locate <name> --add`",
        );
    }
    let mut names = BTreeSet::new();
    for (i, loc) in locations.iter().enumerate() {
        let path = |key: &str| format!("location[{i}].{key}");
        match loc.name.as_deref().map(str::trim) {
            None | Some("") => issues.error(path("name"), "every location needs a name"),
            Some(name) => {
                if !names.insert(name) {
                    issues.error(path("name"), format!("duplicate location name `{name}`"));
                }
            }
        }
        let has_coords = loc.latitude.is_some() || loc.longitude.is_some();
        let kinds = [has_coords, loc.zip.is_some(), loc.query.is_some()];
        match kinds.iter().filter(|k| **k).count() {
            0 => issues.error(
                path("name"),
                "set latitude + longitude, zip + country, or query",
            ),
            1 => {}
            _ => issues.error(
                path("name"),
                "use only one of latitude + longitude, zip or query",
            ),
        }
        match (loc.latitude, loc.longitude) {
            (Some(lat), Some(lon)) => {
                if !(-90.0..=90.0).contains(&lat) {
                    issues.error(path("latitude"), format!("{lat} is outside -90..=90"));
                }
                if !(-180.0..=180.0).contains(&lon) {
                    issues.error(path("longitude"), format!("{lon} is outside -180..=180"));
                }
            }
            (Some(_), None) => issues.error(path("longitude"), "latitude needs a longitude"),
            (None, Some(_)) => issues.error(path("latitude"), "longitude needs a latitude"),
            (None, None) => {}
        }
        if loc.zip.is_some() {
            let valid = loc
                .country
                .as_deref()
                .is_some_and(|c| c.len() == 2 && c.bytes().all(|b| b.is_ascii_alphabetic()));
            if !valid {
                issues.error(
                    path("country"),
                    "a zip needs a two-letter country code such as \"US\"",
                );
            }
        }
        if let Some(offset) = &loc.utc_offset
            && parse_utc_offset(offset).is_none()
        {
            issues.error(
                path("utc_offset"),
                format!("`{offset}` is not an offset such as \"-04:00\""),
            );
        }
        if let Some(id) = loc.provider {
            check_configured(id, providers, &path("provider"), issues);
        }
        if outside_the_us(loc) {
            let name = loc.name.as_deref().unwrap_or("this place");
            if loc.provider == Some(ProviderId::Nws) {
                issues.error(
                    path("provider"),
                    format!("the National Weather Service forecasts only the US, and {name} is outside it"),
                );
            } else if providers.order.first() == Some(&ProviderId::Nws) && loc.provider.is_none() {
                issues.warning(
                    path("name"),
                    format!("{name} is outside the US, so the next provider in providers.order forecasts it"),
                );
            }
        }
        if matches!(
            loc.units,
            Some(whether_core::units::UnitSystem::Auto | whether_core::units::UnitSystem::Custom)
        ) {
            issues.error(
                path("units"),
                "use \"metric\" or \"imperial\" for a location",
            );
        }
        if let Some(p) = &loc.presentation {
            issues
                .0
                .extend(validate_presentation(p, &path("presentation"), providers));
        }
    }
}

/// Whether `loc` is known to lie outside the area the NWS forecasts: its
/// coordinates, or its postal code's country (US territories count as the US).
fn outside_the_us(loc: &LocationConfig) -> bool {
    if let (Some(lat), Some(lon)) = (loc.latitude, loc.longitude) {
        return !whether_core::regions::covers_us(lat, lon);
    }
    loc.country.as_deref().is_some_and(|c| {
        !["US", "PR", "GU", "VI", "AS", "MP"]
            .iter()
            .any(|us| c.eq_ignore_ascii_case(us))
    })
}

fn validate_providers(providers: &ProvidersConfig, issues: &mut Issues) {
    if providers.order.is_empty() {
        issues.error(
            "providers.order",
            "list at least one provider, e.g. [\"open-meteo\"]",
        );
    }
    for (list, ids) in [
        ("order", &providers.order),
        ("supplement", &providers.supplement),
    ] {
        for (i, id) in ids.iter().enumerate() {
            check_configured(*id, providers, &format!("providers.{list}[{i}]"), issues);
        }
    }
    if let Some(openweather) = &providers.openweather {
        if openweather.api == Some(OwmApi::OneCall3) {
            issues.error(
                "providers.openweather.api",
                "One Call 3.0 is closed to new subscriptions and not supported; use \"onecall-4\" or \"free\"",
            );
        }
        if let Some(limit) = openweather.daily_call_limit
            && !(1..=ONECALL_FREE_DAILY_CALLS).contains(&limit)
        {
            issues.error(
                    "providers.openweather.daily_call_limit",
                    format!(
                        "{limit} is outside 1..={ONECALL_FREE_DAILY_CALLS}, the calls One Call by Call includes free each day"
                    ),
                );
        }
    }
    if let Some(pirate) = &providers.pirate_weather {
        check_pirate_weather(pirate, issues);
    }
    let secrets = [
        (
            "providers.openweather.api_key",
            providers
                .openweather
                .as_ref()
                .and_then(|o| o.api_key.as_ref()),
        ),
        (
            "providers.open-meteo.api_key",
            providers.open_meteo.api_key.as_ref(),
        ),
        (
            "providers.pirate-weather.api_key",
            providers
                .pirate_weather
                .as_ref()
                .and_then(|p| p.api_key.as_ref()),
        ),
    ];
    for (path, secret) in secrets {
        if let Some(secret) = secret {
            check_secret(secret, path, issues);
        }
    }
    if let Some(models) = &providers.open_meteo.models {
        check_models(models.ids(), issues);
    }
    if providers.order.first() == Some(&ProviderId::Nws)
        && !providers.supplement.contains(&ProviderId::OpenMeteo)
    {
        issues.warning(
            "providers.supplement",
            "the NWS forecast has no UV, air quality, moon or nowcast; add \"open-meteo\" to fill them",
        );
    }
}

/// `[providers.pirate-weather]`: its budget and the models it may leave out.
fn check_pirate_weather(pirate: &PirateWeatherToml, issues: &mut Issues) {
    const PATH: &str = "providers.pirate-weather";
    if let Some(limit) = pirate.monthly_call_limit {
        if limit == 0 {
            issues.error(
                format!("{PATH}.monthly_call_limit"),
                "0 would never call; use at least 1 (the free key allows 10,000 a month)",
            );
        } else if limit > PIRATE_WEATHER_FREE_MONTHLY_CALLS {
            issues.warning(
                format!("{PATH}.monthly_call_limit"),
                format!(
                    "{limit} is above the {PIRATE_WEATHER_FREE_MONTHLY_CALLS} calls a month a free key allows; fine on a paid plan"
                ),
            );
        }
    }
    let models = pirate.exclude_models.as_deref().unwrap_or_default();
    for (i, id) in models.iter().enumerate() {
        let path = format!("{PATH}.exclude_models[{i}]");
        if models[..i].contains(id) {
            issues.error(path, format!("`{id}` is listed twice"));
        } else if !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            || id.is_empty()
        {
            issues.error(
                path,
                format!("`{id}` is not a model id (letters, digits, _ and -)"),
            );
        } else if !PIRATE_WEATHER_MODELS.contains(&id.as_str()) {
            issues.warning(
                path,
                format!("`{id}` is not a model whether knows; it is sent as is"),
            );
        }
    }
}

/// The most models one Open-Meteo request carries.
const MAX_MODELS: usize = 4;

fn check_models(ids: &[String], issues: &mut Issues) {
    const PATH: &str = "providers.open-meteo.models";
    if ids.len() > MAX_MODELS {
        issues.error(
            PATH,
            format!(
                "{} models listed; at most {MAX_MODELS} fit one request",
                ids.len()
            ),
        );
    }
    for (i, id) in ids.iter().enumerate() {
        if ids[..i].contains(id) {
            issues.error(format!("{PATH}[{i}]"), format!("`{id}` is listed twice"));
        } else if whether_core::models::lookup(id).is_none() {
            issues.warning(
                format!("{PATH}[{i}]"),
                format!(
                    "`{id}` is not a model whether knows; it is sent as is (`whether providers models` lists the known ones)"
                ),
            );
        }
    }
}

/// Whether `id` has the settings it needs; `open-meteo` always has.
fn check_configured(id: ProviderId, providers: &ProvidersConfig, path: &str, issues: &mut Issues) {
    match id {
        // The key defaults to PIRATE_WEATHER_API_KEY, so the section is optional.
        ProviderId::OpenMeteo | ProviderId::Nws | ProviderId::PirateWeather => {}
        ProviderId::Acis => issues.error(
            path,
            "acis gives history (normals, records, past days), never a forecast",
        ),
        ProviderId::OpenWeather => {
            if providers
                .openweather
                .as_ref()
                .is_none_or(|o| o.api_key.is_none())
            {
                issues.error(
                    path,
                    "openweather needs [providers.openweather] api_key = { env = \"OPENWEATHER_API_KEY\" }",
                );
            }
        }
        ProviderId::MetNorway => {
            let contact = providers
                .met_norway
                .as_ref()
                .and_then(|m| m.contact.as_deref());
            if contact.is_none_or(|c| c.trim().is_empty()) {
                issues.error(
                    path,
                    "met-norway needs [providers.met-norway] contact (an email or URL, required by its terms)",
                );
            }
        }
    }
}

fn check_secret(secret: &Secret, path: &str, issues: &mut Issues) {
    match secret {
        Secret::Literal(value) if value.is_empty() => issues.error(path, "the key is empty"),
        Secret::Env { env } if env.trim().is_empty() => {
            issues.error(path, "env needs a variable name");
        }
        Secret::File { file } if file.trim().is_empty() => issues.error(path, "file needs a path"),
        Secret::Command { command } if command.first().is_none_or(|p| p.trim().is_empty()) => {
            issues.error(
                path,
                "command needs a program, e.g. [\"pass\", \"show\", \"owm\"]",
            );
        }
        _ => {}
    }
}

/// Validates a `[presentation]` table found at `prefix` (also used for
/// `[display.<name>.presentation]`).
pub fn validate_presentation(
    p: &PresentationToml,
    prefix: &str,
    providers: &ProvidersConfig,
) -> Vec<Issue> {
    let mut issues = Issues::default();
    if let Some(hours) = p.hourly_hours
        && !(1..=48).contains(&hours)
    {
        issues.error(
            format!("{prefix}.hourly_hours"),
            format!("{hours} is outside 1..=48"),
        );
    }
    if let Some(hours) = p.precip_hours
        && !(1..=48).contains(&hours)
    {
        issues.error(
            format!("{prefix}.precip_hours"),
            format!("{hours} is outside 1..=48"),
        );
    }
    if let Some(days) = p.history_days
        && days > 31
    {
        issues.error(
            format!("{prefix}.history_days"),
            format!("{days} is outside 0..=31"),
        );
    }
    if let Some(hours) = p.nowcast_hours
        && !(1..=8).contains(&hours)
    {
        issues.error(
            format!("{prefix}.nowcast_hours"),
            format!("{hours} is outside 1..=8"),
        );
    }
    if let Some(days) = p.daily_days {
        if !(1..=16).contains(&days) {
            issues.error(
                format!("{prefix}.daily_days"),
                format!("{days} is outside 1..=16"),
            );
        } else if days > 5 && free_openweather_without_extra_days(providers) {
            issues.warning(
                format!("{prefix}.daily_days"),
                "OpenWeather's free API has 5 days; add \"open-meteo\" to providers.supplement for more",
            );
        }
    }
    if let Some(threshold) = p.precip_threshold
        && threshold > 100
    {
        issues.error(
            format!("{prefix}.precip_threshold"),
            "a percentage: 0..=100",
        );
    }
    issues.0
}

/// OpenWeather's free API is the primary and no supplement adds days 6 and later.
fn free_openweather_without_extra_days(providers: &ProvidersConfig) -> bool {
    let free = providers
        .openweather
        .as_ref()
        .is_none_or(|o| o.api.unwrap_or_default() == OwmApi::Free);
    providers.order.first() == Some(&ProviderId::OpenWeather)
        && free
        && !providers
            .supplement
            .iter()
            .any(|id| *id != ProviderId::OpenWeather)
}

fn unknown_string_message(key: &str) -> String {
    let close: Vec<&str> = Strings::keys()
        .map(|(k, _)| k)
        .filter(|k| edit_distance(k, key) <= 2 || (key.len() >= 4 && k.contains(key)))
        .take(3)
        .collect();
    if close.is_empty() {
        "unknown string key (see `whether config strings`)".to_string()
    } else {
        format!("unknown string key; did you mean {}?", close.join(", "))
    }
}

/// Levenshtein distance over bytes (string keys are ASCII).
fn edit_distance(a: &str, b: &str) -> usize {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (above + 1)
                .min(row[j] + 1)
                .min(diagonal + usize::from(ca != cb));
            diagonal = above;
        }
    }
    row[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::duration::DurationStr;
    use crate::types::{MetNorwayToml, OpenWeatherToml};
    use alloc::vec;

    fn place(name: &str) -> LocationConfig {
        LocationConfig {
            name: Some(name.to_string()),
            zip: Some("10001".to_string()),
            country: Some("US".to_string()),
            ..Default::default()
        }
    }

    fn valid() -> CommonConfig {
        CommonConfig {
            location: vec![place("New York")],
            ..Default::default()
        }
    }

    /// The `(severity, path)` pairs of `c`'s issues.
    fn found(c: &CommonConfig) -> Vec<(Severity, String)> {
        c.validate()
            .into_iter()
            .map(|i| (i.severity, i.path))
            .collect()
    }

    fn error(path: &str) -> (Severity, String) {
        (Severity::Error, path.to_string())
    }

    #[test]
    fn model_lists_are_checked() {
        let with = |ids: &[&str]| {
            let mut c = valid();
            c.providers.open_meteo.models = Some(crate::ModelList(
                ids.iter().map(ToString::to_string).collect(),
            ));
            found(&c)
        };
        assert_eq!(
            with(&["ncep_hrrr_conus", "ncep_nbm_conus", "ecmwf_ifs025"]),
            vec![]
        );
        assert_eq!(
            with(&[
                "gfs_seamless",
                "ecmwf_ifs025",
                "icon_seamless",
                "gem_seamless",
                "jma_seamless"
            ]),
            vec![error("providers.open-meteo.models")]
        );
        assert_eq!(
            with(&["knmi_seamless"]),
            vec![(
                Severity::Warning,
                "providers.open-meteo.models[0]".to_string()
            )]
        );
        assert_eq!(
            with(&["gfs_seamless", "gfs_seamless"]),
            vec![error("providers.open-meteo.models[1]")]
        );
    }

    #[test]
    fn defaults_with_a_location_are_valid() {
        assert_eq!(found(&valid()), vec![]);
    }

    #[test]
    fn version_must_be_one() {
        let c = CommonConfig {
            version: 2,
            ..valid()
        };
        assert_eq!(found(&c), vec![error("version")]);
    }

    #[test]
    fn a_location_is_required() {
        let issues = CommonConfig::default().validate();
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("whether locate"));
        assert!(has_errors(&issues));
    }

    #[test]
    fn names_are_present_and_unique() {
        let mut c = valid();
        c.location.push(place("New York"));
        c.location.push(LocationConfig {
            name: Some(" ".into()),
            ..place("x")
        });
        assert_eq!(
            found(&c),
            vec![error("location[1].name"), error("location[2].name")]
        );
    }

    #[test]
    fn exactly_one_kind_of_place() {
        let mut c = valid();
        c.location[0].query = Some("New York".into());
        c.location.push(LocationConfig {
            name: Some("Nowhere".into()),
            ..Default::default()
        });
        assert_eq!(
            found(&c),
            vec![error("location[0].name"), error("location[1].name")]
        );
    }

    #[test]
    fn coordinates_are_paired_and_in_range() {
        let coords = |lat, lon| LocationConfig {
            name: Some(alloc::format!("{lat:?}{lon:?}")),
            latitude: lat,
            longitude: lon,
            ..Default::default()
        };
        let c = CommonConfig {
            location: vec![
                coords(Some(40.7), Some(-74.0)),
                coords(Some(95.0), Some(181.0)),
                coords(Some(1.0), None),
                coords(None, Some(1.0)),
            ],
            ..Default::default()
        };
        assert_eq!(
            found(&c),
            vec![
                error("location[1].latitude"),
                error("location[1].longitude"),
                error("location[2].longitude"),
                error("location[3].latitude"),
            ]
        );
    }

    #[test]
    fn zip_needs_a_two_letter_country() {
        let mut c = valid();
        c.location[0].country = None;
        c.location.push(LocationConfig {
            country: Some("USA".into()),
            ..place("b")
        });
        assert_eq!(
            found(&c),
            vec![error("location[0].country"), error("location[1].country")]
        );
    }

    #[test]
    fn utc_offsets_must_parse() {
        let mut c = valid();
        c.location[0].utc_offset = Some("-04:00".into());
        assert_eq!(found(&c), vec![]);
        c.location[0].utc_offset = Some("EDT".into());
        assert_eq!(found(&c), vec![error("location[0].utc_offset")]);
    }

    #[test]
    fn refresh_and_precision_limits() {
        let mut c = valid();
        c.general.refresh = DurationStr(60);
        c.general.coordinate_precision = 5;
        assert_eq!(
            found(&c),
            vec![
                (Severity::Warning, "general.refresh".into()),
                error("general.coordinate_precision")
            ]
        );
    }

    #[test]
    fn nws_forecasts_us_places() {
        let mut c = valid();
        c.providers.openweather = None;
        c.providers.order = vec![ProviderId::Nws, ProviderId::OpenMeteo];
        c.providers.supplement = vec![ProviderId::OpenMeteo, ProviderId::Nws];
        c.location[0].provider = Some(ProviderId::Nws);
        assert_eq!(found(&c), vec![], "10001 is in the US");
        c.providers.supplement = vec![ProviderId::Nws];
        assert_eq!(
            found(&c),
            vec![(Severity::Warning, "providers.supplement".to_string())],
            "nothing fills UV and the nowcast"
        );
    }

    #[test]
    fn history_sources_give_history() {
        let mut c = valid();
        c.history.order = vec![ProviderId::Nws, ProviderId::Acis];
        assert_eq!(found(&c), vec![error("history.order[0]")]);
        // Places abroad need no warning: ACIS steps aside there (Open-Meteo).
        c.history.order = vec![ProviderId::Acis, ProviderId::OpenMeteo];
        c.location.push(LocationConfig {
            name: Some("Oslo".into()),
            latitude: Some(59.91),
            longitude: Some(10.75),
            ..Default::default()
        });
        assert_eq!(found(&c), vec![]);
    }

    #[test]
    fn acis_station_ids_are_plain() {
        let mut c = valid();
        c.history.acis.station = Some("LGA".into());
        c.history.acis.records_station = Some("LGAthr".into());
        assert_eq!(found(&c), vec![]);
        c.history.acis.station = Some("../LGA".into());
        assert_eq!(found(&c), vec![error("history.acis.station")]);
    }

    #[test]
    fn acis_never_forecasts() {
        let mut c = valid();
        c.providers.supplement = vec![ProviderId::OpenMeteo, ProviderId::Acis];
        assert_eq!(found(&c), vec![error("providers.supplement[1]")]);
    }

    #[test]
    fn nws_refuses_places_abroad() {
        let mut c = valid();
        c.providers.openweather = None;
        c.providers.order = vec![ProviderId::Nws, ProviderId::OpenMeteo];
        c.location.push(LocationConfig {
            name: Some("Oslo".into()),
            latitude: Some(59.91),
            longitude: Some(10.75),
            ..Default::default()
        });
        c.location.push(LocationConfig {
            name: Some("Toronto".into()),
            zip: Some("M5V".into()),
            country: Some("CA".into()),
            provider: Some(ProviderId::Nws),
            ..Default::default()
        });
        c.location.push(LocationConfig {
            name: Some("San Juan".into()),
            zip: Some("00901".into()),
            country: Some("PR".into()),
            provider: Some(ProviderId::Nws),
            ..Default::default()
        });
        assert_eq!(
            found(&c),
            vec![
                (Severity::Warning, "location[1].name".to_string()),
                error("location[2].provider"),
            ],
            "Oslo falls back; Toronto may not insist; Puerto Rico is covered"
        );
    }

    #[test]
    fn pirate_weather_needs_no_section_but_checks_the_one_given() {
        let mut c = valid();
        c.providers.order = vec![ProviderId::PirateWeather, ProviderId::OpenMeteo];
        c.providers.supplement = vec![ProviderId::OpenMeteo];
        c.location[0].provider = Some(ProviderId::PirateWeather);
        assert_eq!(
            found(&c),
            vec![],
            "the key defaults to the environment variable"
        );
        c.providers.pirate_weather = Some(PirateWeatherToml {
            api_key: Some(Secret::Literal(String::new())),
            exclude_models: Some(vec![
                "gfs".into(),
                "gfs".into(),
                "a,b".into(),
                "future_model".into(),
                "hrrr".into(),
            ]),
            monthly_call_limit: Some(0),
            ..PirateWeatherToml::default()
        });
        assert_eq!(
            found(&c),
            vec![
                error("providers.pirate-weather.monthly_call_limit"),
                error("providers.pirate-weather.exclude_models[1]"),
                error("providers.pirate-weather.exclude_models[2]"),
                (
                    Severity::Warning,
                    "providers.pirate-weather.exclude_models[3]".to_string()
                ),
                error("providers.pirate-weather.api_key"),
            ]
        );
        c.providers.pirate_weather = Some(PirateWeatherToml {
            monthly_call_limit: Some(PIRATE_WEATHER_FREE_MONTHLY_CALLS + 1),
            exclude_models: Some(PIRATE_WEATHER_MODELS.map(String::from).to_vec()),
            ..PirateWeatherToml::default()
        });
        assert_eq!(
            found(&c),
            vec![(
                Severity::Warning,
                "providers.pirate-weather.monthly_call_limit".to_string()
            )]
        );
    }

    #[test]
    fn providers_in_use_must_be_configured() {
        let mut c = valid();
        c.providers.openweather = Some(OpenWeatherToml::default());
        c.providers.supplement = vec![ProviderId::MetNorway];
        c.location[0].provider = Some(ProviderId::MetNorway);
        assert_eq!(
            found(&c),
            vec![
                error("location[0].provider"),
                error("providers.order[0]"),
                error("providers.supplement[0]"),
            ]
        );
        c.providers.openweather = None;
        c.providers.order = vec![ProviderId::OpenMeteo];
        c.providers.met_norway = Some(MetNorwayToml {
            contact: Some("me@example.com".into()),
            ..Default::default()
        });
        assert_eq!(found(&c), vec![]);
        c.providers.order.clear();
        assert_eq!(found(&c), vec![error("providers.order")]);
    }

    #[test]
    fn location_overrides_are_checked() {
        use whether_core::units::UnitSystem;
        let mut c = valid();
        c.location[0].units = Some(UnitSystem::Metric);
        c.location[0].presentation = Some(PresentationToml {
            hourly_hours: Some(24),
            ..PresentationToml::default()
        });
        assert_eq!(found(&c), vec![]);
        c.location[0].units = Some(UnitSystem::Auto);
        c.location[0].presentation = Some(PresentationToml {
            hourly_hours: Some(99),
            ..PresentationToml::default()
        });
        assert_eq!(
            found(&c),
            vec![
                error("location[0].units"),
                error("location[0].presentation.hourly_hours")
            ]
        );
    }

    #[test]
    fn empty_secrets_are_errors() {
        let mut c = valid();
        for secret in [
            Secret::Literal(String::new()),
            Secret::Env { env: String::new() },
            Secret::File { file: " ".into() },
            Secret::Command { command: vec![] },
        ] {
            c.providers.openweather.as_mut().unwrap().api_key = Some(secret);
            assert_eq!(found(&c), vec![error("providers.openweather.api_key")]);
        }
    }

    #[test]
    fn presentation_ranges() {
        let mut c = valid();
        c.presentation = PresentationToml {
            hourly_hours: Some(0),
            daily_days: Some(17),
            precip_threshold: Some(101),
            ..Default::default()
        };
        assert_eq!(
            found(&c),
            vec![
                error("presentation.hourly_hours"),
                error("presentation.daily_days"),
                error("presentation.precip_threshold"),
            ]
        );
    }

    #[test]
    fn more_than_five_days_needs_a_supplement_with_free_openweather() {
        let mut c = valid();
        c.presentation.daily_days = Some(7);
        assert_eq!(found(&c), vec![], "open-meteo supplements by default");
        c.providers.supplement.clear();
        assert_eq!(
            found(&c),
            vec![(Severity::Warning, "presentation.daily_days".into())]
        );
        c.providers.openweather.as_mut().unwrap().api = Some(OwmApi::OneCall4);
        assert_eq!(found(&c), vec![], "One Call 4.0 has 10 days");
    }

    #[test]
    fn unknown_strings_suggest_close_keys() {
        let mut c = valid();
        let (known, _) = Strings::keys().next().unwrap();
        c.strings.insert(known.to_string(), "x".into());
        let mut typo = known.to_string();
        typo.push('x');
        c.strings.insert(typo.clone(), "y".into());
        let issues = c.validate();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].path, alloc::format!("strings.{typo}"));
        assert!(issues[0].message.contains(known), "{}", issues[0].message);
    }

    #[test]
    fn edit_distances() {
        assert_eq!(edit_distance("today", "today"), 0);
        assert_eq!(edit_distance("today", "tody"), 1);
        assert_eq!(edit_distance("kitten", "sitting"), 3);
        assert_eq!(edit_distance("", "abc"), 3);
    }

    #[test]
    fn issues_display_with_their_path() {
        let issue = Issue {
            severity: Severity::Warning,
            path: "general.refresh".into(),
            message: "m".into(),
        };
        assert_eq!(alloc::format!("{issue}"), "warning: general.refresh: m");
    }

    #[test]
    fn one_call_settings() {
        let mut c = valid();
        let openweather = c.providers.openweather.as_mut().unwrap();
        openweather.api = Some(OwmApi::OneCall4);
        openweather.daily_call_limit = Some(1000);
        assert_eq!(found(&c), vec![]);
        for limit in [0, 1001, 2000] {
            c.providers.openweather.as_mut().unwrap().daily_call_limit = Some(limit);
            assert_eq!(
                found(&c),
                vec![error("providers.openweather.daily_call_limit")],
                "{limit}"
            );
        }
        let openweather = c.providers.openweather.as_mut().unwrap();
        openweather.daily_call_limit = None;
        openweather.api = Some(OwmApi::OneCall3);
        assert_eq!(found(&c), vec![error("providers.openweather.api")]);
    }
}
