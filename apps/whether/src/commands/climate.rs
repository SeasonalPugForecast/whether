//! `whether climate`: how a trip's dates were over chosen years, at a
//! configured location or any place found by name (the vacation planner).

use std::fmt::Write as _;
use std::process::ExitCode;

use anyhow::{Context, anyhow, bail};
use whether_config::LinuxConfig;
use whether_core::history::{ClimateQuery, YearRange};
use whether_core::model::ResolvedLocation;
use whether_core::strings::Strings;
use whether_core::time::{LocalDate, UnixTime};
use whether_core::view::{PlanVm, plan_vm};
use whether_providers::GeocodeQuery;
use whether_runtime::history::{HistoryError, HistorySource, climate_for};
use whether_runtime::locate::resolved_from;
use whether_runtime::{
    Cache, Geocoder, Transport, UreqTransport, paths, resolve_location, user_agent,
};

use crate::commands::history::with_forecast_offset;
use crate::common::{env, load_config};
use whether::cli::{Cli, ClimateArgs};

/// Years looked back over by default.
const DEFAULT_YEARS: i32 = 30;

/// `MM-DD` (the next such date from `today`) or `YYYY-MM-DD`.
fn parse_date(text: &str, today: LocalDate) -> anyhow::Result<LocalDate> {
    let parts: Vec<&str> = text.trim().split('-').collect();
    let number = |s: &str| -> anyhow::Result<i32> {
        s.parse()
            .with_context(|| format!("`{text}` is not a date (MM-DD or YYYY-MM-DD)"))
    };
    let (year, month, day) = match parts.as_slice() {
        [m, d] => (None, number(m)?, number(d)?),
        [y, m, d] => (Some(number(y)?), number(m)?, number(d)?),
        _ => bail!("`{text}` is not a date (MM-DD or YYYY-MM-DD)"),
    };
    let (month, day) = (
        u8::try_from(month).context("no such month")?,
        u8::try_from(day).context("no such day")?,
    );
    let year = year.unwrap_or_else(|| {
        if (month, day) >= (today.month, today.day) {
            today.year
        } else {
            today.year + 1
        }
    });
    Ok(LocalDate { year, month, day })
}

/// `FIRST-LAST`, or the last 30 years before this one.
fn parse_years(text: Option<&str>, today: LocalDate) -> anyhow::Result<YearRange> {
    let Some(text) = text else {
        return Ok(YearRange {
            first: today.year - DEFAULT_YEARS,
            last: today.year - 1,
        });
    };
    let (first, last) = text
        .split_once('-')
        .with_context(|| format!("`{text}` is not a range of years (e.g. 1995-2024)"))?;
    let year = |s: &str| -> anyhow::Result<i32> {
        s.trim()
            .parse()
            .with_context(|| format!("`{text}` is not a range of years (e.g. 1995-2024)"))
    };
    Ok(YearRange {
        first: year(first)?,
        last: year(last)?,
    })
}

/// The query: `--to` defaults to `--from`, and an `MM-DD` before `--from`
/// falls in the next year.
pub(crate) fn query(args: &ClimateArgs, today: LocalDate) -> anyhow::Result<ClimateQuery> {
    let from = parse_date(&args.from, today)?;
    let to = match args.to.as_deref() {
        None => from,
        Some(text) if text.matches('-').count() == 1 => {
            let to = parse_date(text, from)?;
            if (to.month, to.day) < (from.month, from.day) {
                LocalDate {
                    year: from.year + 1,
                    ..to
                }
            } else {
                LocalDate {
                    year: from.year,
                    ..to
                }
            }
        }
        Some(text) => parse_date(text, from)?,
    };
    let years = parse_years(args.years.as_deref(), today)?;
    ClimateQuery::new(from, to, years).map_err(|e| anyhow!("{e}"))
}

/// The plan as text.
fn text(name: &str, plan: &PlanVm) -> String {
    let mut out = format!("{name}: {}\n\n", plan.title);
    let width = |pick: fn(&whether_core::view::PlanDayVm) -> Option<&String>| {
        plan.days
            .iter()
            .filter_map(pick)
            .map(|s| s.chars().count())
            .max()
            .unwrap_or(0)
    };
    let hottest = width(|d| d.hottest.as_ref()).max("Hottest".len());
    let _ = writeln!(
        out,
        "{:<11}{:>6}{:>6}{:>7}{:>10}  {:<hottest$}  Coldest",
        "", "High", "Low", "Wet", "Precip", "Hottest"
    );
    for d in &plan.days {
        let line = format!(
            "{:<4}{:<7}{:>6}{:>6}{:>7}{:>10}  {:<hottest$}  {}",
            d.label,
            d.date_short,
            d.high.text,
            d.low.text,
            d.precip_chance.text,
            d.precipitation.text,
            d.hottest.as_deref().unwrap_or(""),
            d.coldest.as_deref().unwrap_or(""),
        );
        let _ = writeln!(out, "{}", line.trim_end());
    }
    let summary: Vec<&str> = [plan.average.as_deref(), plan.wet_days.as_deref()]
        .into_iter()
        .flatten()
        .collect();
    if !summary.is_empty() {
        let _ = writeln!(out, "\n{}", summary.join(" · "));
    }
    let _ = writeln!(out, "{}", plan.credit);
    let _ = writeln!(out, "{}", plan.attribution);
    out
}

/// `whether climate` for a resolved place over `transport`: the text or JSON
/// to print.
pub(crate) fn report(
    cli: &Cli,
    args: &ClimateArgs,
    cfg: &LinuxConfig,
    place: &ResolvedLocation,
    cache: &Cache,
    transport: &dyn Transport,
    now: UnixTime,
) -> anyhow::Result<String> {
    let q = query(args, now.local_date(place.utc_offset))?;
    let src = HistorySource {
        cfg: &cfg.common,
        env: &env,
        cache,
        transport,
        offline: cli.offline,
    };
    let found = climate_for(&src, place, q, now)?;
    for a in &found.attempts {
        if let Some(e) = &a.error {
            tracing::info!("{}: {e}", a.provider.display_name());
        }
    }
    let presentation = cfg.presentation_for("climate")?;
    let strings = Strings::localized(&cfg.common.general.language, &cfg.common.strings).0;
    let plan = plan_vm(&presentation, &strings, &found.history, &q);
    Ok(if args.json {
        let doc = serde_json::json!({
            "location": place.name,
            "query": q,
            "fetched": found.fetched,
            "view": plan,
            "history": found.history,
        });
        format!("{doc:#}\n")
    } else {
        text(&place.name, &plan)
    })
}

/// The index of the configured location named `wanted` (or numbered), if any.
fn configured(cfg: &LinuxConfig, wanted: &str) -> Option<usize> {
    let locations = &cfg.common.location;
    locations
        .iter()
        .position(|l| {
            l.name
                .as_deref()
                .is_some_and(|n| n.eq_ignore_ascii_case(wanted))
        })
        .or_else(|| {
            wanted
                .parse::<usize>()
                .ok()
                .filter(|n| (1..=locations.len()).contains(n))
                .map(|n| n - 1)
        })
}

/// `whether climate`.
pub(crate) fn run(cli: &Cli, args: &ClimateArgs) -> anyhow::Result<ExitCode> {
    let cfg = match load_config(cli, &[]) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("{e}");
            return Ok(ExitCode::from(2));
        }
    };
    let transport = UreqTransport::new(&user_agent());
    let cache = Cache::at(
        cfg.runtime
            .cache_dir
            .clone()
            .or_else(|| paths::cache_dir(&env))
            .context("set HOME")?,
    );
    let geocoder = Geocoder {
        providers: &cfg.common.providers,
        env: &env,
        transport: &transport,
        language: &cfg.common.general.language,
    };
    let now = whether_display::now();
    let wanted = cli.location.as_deref();
    // Not a configured location: a place to search for.
    let place = if let Some(text) = wanted.filter(|w| configured(&cfg, w).is_none()) {
        let query = GeocodeQuery::Name {
            text,
            country: None,
        };
        let found = geocoder.search(&query, 1).map_err(|e| anyhow!(e))?;
        let first = found
            .first()
            .with_context(|| format!("no place `{text}`"))?;
        resolved_from(first, None, now)
    } else {
        let i = wanted.and_then(|w| configured(&cfg, w)).unwrap_or(0);
        let loc = cfg
            .common
            .location
            .get(i)
            .context("no location configured")?;
        let place = resolve_location(loc, &geocoder, &cache, now).map_err(|e| anyhow!(e))?;
        with_forecast_offset(&cfg, &cache, place)
    };
    match report(cli, args, &cfg, &place, &cache, &transport, now) {
        Ok(out) => {
            print!("{out}");
            Ok(ExitCode::SUCCESS)
        }
        Err(e) if matches!(e.downcast_ref(), Some(HistoryError::NothingCached)) => {
            eprintln!("whether: {e}");
            Ok(ExitCode::from(3))
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::testing::{NOV_2, cli, fake, new_york};

    fn on(year: i32, month: u8, day: u8) -> LocalDate {
        LocalDate { year, month, day }
    }

    fn args(from: &str, to: Option<&str>, years: Option<&str>) -> ClimateArgs {
        ClimateArgs {
            from: from.into(),
            to: to.map(Into::into),
            years: years.map(Into::into),
            json: false,
        }
    }

    #[test]
    fn dates_are_the_next_ones_and_years_the_last_thirty() {
        let today = on(2026, 10, 17);
        let q = query(&args("07-12", Some("07-19"), None), today).unwrap();
        assert_eq!((q.from, q.to), (on(2027, 7, 12), on(2027, 7, 19)));
        assert_eq!((q.years.first, q.years.last), (1996, 2025));
        // Later this year; across the new year; a single day; full dates.
        let q = query(&args("12-30", Some("01-02"), Some("1991-2020")), today).unwrap();
        assert_eq!((q.from, q.to), (on(2026, 12, 30), on(2027, 1, 2)));
        assert_eq!(q.years.first, 1991);
        let q = query(&args("2028-02-29", None, None), today).unwrap();
        assert_eq!((q.from, q.to), (on(2028, 2, 29), on(2028, 2, 29)));
        // Mistakes.
        for (from, to, years) in [
            ("July 12", None, None),
            ("02-30", None, None),
            ("07-01", Some("08-15"), None),
            ("07-12", None, Some("2024")),
            ("07-12", None, Some("2024-1995")),
        ] {
            assert!(
                query(&args(from, to, years), today).is_err(),
                "{from} {to:?} {years:?}"
            );
        }
    }

    fn config() -> LinuxConfig {
        whether_config::load::parse(
            "[general]\nunits = \"imperial\"\n\
             [[location]]\nname = \"New York\"\nlatitude = 40.7143\nlongitude = -74.00597\n",
        )
        .unwrap()
    }

    #[test]
    fn a_july_week_from_acis() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path());
        let net = fake(vec![
            (
                "StnMeta",
                include_str!(
                    "../../../../crates/whether-providers/tests/fixtures/acis/stnmeta_10001.json"
                ),
            ),
            (
                "StnData",
                include_str!(
                    "../../../../crates/whether-providers/tests/fixtures/acis/climate_NYC_0712_0719.json"
                ),
            ),
        ]);
        let week = args("07-12", Some("07-19"), Some("1995-2024"));
        let out = report(
            &cli(&["climate", "--from", "07-12"]),
            &week,
            &config(),
            &new_york(),
            &cache,
            &net,
            NOV_2,
        )
        .unwrap();
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "New York: Jul 12 – Jul 19, as in 1995–2024");
        assert!(
            lines[2].contains("High") && lines[2].ends_with("Coldest"),
            "{out}"
        );
        assert_eq!(
            lines[3],
            "Mon Jul 12    84°   69°    33%   0.19 in  94° in 2010   62° in 2002"
        );
        assert_eq!(
            lines.iter().filter(|l| l.contains(" Jul ")).count(),
            9,
            "{out}"
        );
        assert!(out.contains("On average 86° / 71° · Rain or snow on 34% of days"));
        assert!(out.contains("NY City Central Park, 4.9 mi · 1995–2024"));
        assert!(out.ends_with("History from NOAA's Regional Climate Centers (ACIS)\n"));
        assert_eq!(*net.sent.lock().unwrap(), 2);
        // Kept: offline, the same answer, as JSON.
        let json = ClimateArgs { json: true, ..week };
        let v: serde_json::Value = serde_json::from_str(
            &report(
                &cli(&["--offline", "climate", "--from", "07-12"]),
                &json,
                &config(),
                &new_york(),
                &cache,
                &net,
                NOV_2,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(v["fetched"], false);
        assert_eq!(v["query"]["years"]["first"], 1995);
        assert_eq!(v["view"]["days"].as_array().unwrap().len(), 8);
        assert_eq!(*net.sent.lock().unwrap(), 2);
        // Another week offline: nothing kept.
        let e = report(
            &cli(&["--offline", "climate", "--from", "07-12"]),
            &args("08-01", None, None),
            &config(),
            &new_york(),
            &cache,
            &net,
            NOV_2,
        )
        .unwrap_err();
        assert!(matches!(
            e.downcast_ref(),
            Some(HistoryError::NothingCached)
        ));
    }

    #[test]
    fn configured_names_and_numbers() {
        let cfg = config();
        assert_eq!(configured(&cfg, "new york"), Some(0));
        assert_eq!(configured(&cfg, "1"), Some(0));
        assert_eq!(configured(&cfg, "2"), None);
        assert_eq!(configured(&cfg, "Lisbon"), None);
    }
}
