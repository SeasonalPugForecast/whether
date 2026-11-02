//! `whether locate`: search places, print a TOML snippet, optionally add it.

use std::fmt::Write as _;
use std::process::ExitCode;

use anyhow::{Context, anyhow, bail};
use whether_config::{LocationConfig, ProvidersConfig};
use whether_providers::{GeocodeQuery, GeocodeResult};
use whether_runtime::{Geocoder, UreqTransport, user_agent};

use crate::common::{edit_path, env, load_config};
use whether::cli::{Cli, LocateArgs};

/// The `[[location]]` entry for a result: a zip with its country when searching by
/// zip, else coordinates and the time zone.
pub(crate) fn location_for(result: &GeocodeResult, args: &LocateArgs) -> LocationConfig {
    let name = args.name.clone().unwrap_or_else(|| result.name.clone());
    if args.zip {
        LocationConfig {
            name: Some(name),
            zip: Some(args.query.clone()),
            country: args.country.clone().map(|c| c.to_uppercase()),
            ..Default::default()
        }
    } else {
        LocationConfig {
            name: Some(name),
            latitude: Some(result.latitude),
            longitude: Some(result.longitude),
            timezone: result.timezone.clone(),
            ..Default::default()
        }
    }
}

/// The TOML snippet for `loc`, keys in the documented order.
pub(crate) fn snippet(loc: &LocationConfig) -> anyhow::Result<String> {
    #[derive(serde::Serialize)]
    struct Snippet<'a> {
        location: [&'a LocationConfig; 1],
    }
    Ok(toml::to_string(&Snippet { location: [loc] })?)
}

/// The results table.
pub(crate) fn table(results: &[GeocodeResult]) -> String {
    let mut out = String::new();
    for (i, r) in results.iter().enumerate() {
        let _ = writeln!(
            out,
            "{:>2}  {:<40} {:>9.4} {:>10.4}  {:<20} {}",
            i + 1,
            r.display_name(),
            r.latitude,
            r.longitude,
            r.timezone.as_deref().unwrap_or("-"),
            r.source.display_name()
        );
    }
    out
}

/// `whether locate`.
pub(crate) fn run(cli: &Cli, args: &LocateArgs) -> anyhow::Result<ExitCode> {
    if cli.offline {
        bail!("locate needs the network");
    }
    // Searching must work before any location is configured.
    let (providers, language) = match load_config(cli, &[]) {
        Ok(cfg) => (cfg.common.providers, cfg.common.general.language),
        Err(_) => (ProvidersConfig::default(), "en".to_string()),
    };
    let query = if args.zip {
        let country = args
            .country
            .as_deref()
            .context("--zip needs --country, e.g. --country US")?;
        GeocodeQuery::Zip {
            zip: &args.query,
            country,
        }
    } else {
        GeocodeQuery::Name {
            text: &args.query,
            country: args.country.as_deref(),
        }
    };
    let transport = UreqTransport::new(&user_agent());
    let geocoder = Geocoder {
        providers: &providers,
        env: &env,
        transport: &transport,
        language: &language,
    };
    let results = geocoder
        .search(&query, args.count.max(1))
        .map_err(|e| anyhow!(e))?;
    print!("{}", table(&results));
    let first = location_for(&results[0], args);
    println!("\n{}", snippet(&first)?.trim_end());
    if args.add {
        let path = edit_path(cli)
            .context("cannot find the configuration file; set HOME or use --config")?;
        whether_config::edit::add_location(&path, &first)?;
        println!("\nadded to {}", path.display());
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use whether_core::model::ProviderId;

    fn result() -> GeocodeResult {
        GeocodeResult {
            name: "New York".into(),
            latitude: 40.7143,
            longitude: -74.005_97,
            country_code: Some("US".into()),
            admin1: Some("New York".into()),
            postcode: Some("10001".into()),
            timezone: Some("America/New_York".into()),
            elevation: None,
            population: None,
            source: ProviderId::OpenMeteo,
        }
    }

    fn args(zip: bool) -> LocateArgs {
        LocateArgs {
            query: "10001".into(),
            zip,
            country: Some("us".into()),
            count: 5,
            add: false,
            name: Some("Home".into()),
        }
    }

    #[test]
    fn zip_snippets_keep_the_zip() {
        let text = snippet(&location_for(&result(), &args(true))).unwrap();
        assert_eq!(
            text,
            "[[location]]\nname = \"Home\"\nzip = \"10001\"\ncountry = \"US\"\n"
        );
    }

    #[test]
    fn name_snippets_use_coordinates() {
        let text = snippet(&location_for(&result(), &args(false))).unwrap();
        assert!(
            text.contains("latitude = 40.7143") && text.contains("timezone = \"America/New_York\""),
            "{text}"
        );
    }

    #[test]
    fn tables_list_results() {
        let t = table(&[result()]);
        assert!(t.starts_with(" 1  New York, New York, US"), "{t}");
        assert!(t.contains("Open-Meteo"));
    }
}
