//! `whether history`: a place's history, kept or fetched, and the
//! ACIS stations near it to choose from.

use std::fmt::Write as _;
use std::process::ExitCode;

use anyhow::{Context, anyhow};
use whether_config::LinuxConfig;
use whether_core::model::{ProviderId, ResolvedLocation};
use whether_core::presentation::{PresentationConfig, WidgetKind};
use whether_core::requirements::{HistoryRequirements, MAX_HISTORY_DAYS};
use whether_core::strings::Strings;
use whether_core::time::UnixTime;
use whether_core::units::Meters;
use whether_core::view::{Formatter, HistoryVm, LocationView, StatusInfo, ViewInput, build_view};
use whether_providers::Coordinates;
use whether_providers::acis::NearbyStation;
use whether_runtime::history::{
    HistoryError, HistoryFor, HistorySource, acis_stations, history_for,
};
use whether_runtime::{
    Cache, Geocoder, Transport, UreqTransport, paths, resolve_location, user_agent,
};

use crate::commands::run::location_index;
use crate::common::{env, load_config};
use whether::cli::{Cli, HistoryArgs};

/// Columns of the days: day, date, high, low, against normal.
const DAY: usize = 10;
const DATE: usize = 8;
const TEMP: usize = 6;
const DEPARTURE: usize = 8;

/// What `whether history` asks for: `days` past days, today's climate and
/// records, the month so far, and whatever the displays need, so the kept
/// history serves them too.
fn requirements(cfg: &LinuxConfig, days: u8) -> HistoryRequirements {
    HistoryRequirements {
        past_days: days,
        climate_days: 1,
        records: true,
        month_to_date: true,
    }
    .union(cfg.history_requirements())
}

/// The presentation of the history: the configured units and language, the
/// history widget alone, `days` days (else the configured number).
fn presentation(cfg: &LinuxConfig, days: Option<u8>) -> anyhow::Result<PresentationConfig> {
    let mut p = cfg.presentation_for("history")?;
    p.widgets = vec![WidgetKind::History];
    p.history_days = days.unwrap_or(p.history_days).clamp(1, MAX_HISTORY_DAYS);
    Ok(p)
}

/// The place's history as the view shows it.
fn view(
    place: &ResolvedLocation,
    found: &HistoryFor,
    presentation: &PresentationConfig,
    strings: &Strings,
    now: UnixTime,
) -> (Option<HistoryVm>, String) {
    let status = StatusInfo::default();
    let vm = build_view(&ViewInput {
        locations: &[LocationView {
            name: &place.name,
            snapshot: None,
            status: &status,
            history: Some(&found.cached.history),
        }],
        active: 0,
        presentation,
        strings,
        now,
    });
    (vm.history, vm.attribution)
}

/// The history as text: today's normals and records, the days, the month so
/// far, the credit and the attribution.
fn text(name: &str, h: &HistoryVm, attribution: &str) -> String {
    let mut out = format!("{name}\n");
    if let Some(today) = &h.today {
        let mut line = today.normal_text.clone();
        if let (Some(high), Some(low)) = (&today.record_high, &today.record_low) {
            let _ = write!(line, "  ·  Records {high} · {low}");
        }
        let _ = writeln!(out, "{line}");
        if let Some(chance) = &today.precip_chance {
            let _ = writeln!(out, "{chance}");
        }
    }
    if let Some(delta) = &h.yesterday_delta {
        let _ = writeln!(out, "{delta}");
    }
    if !h.days.is_empty() {
        let _ = writeln!(
            out,
            "\n{:<DAY$}{:<DATE$}{:>TEMP$}{:>TEMP$}{:>DEPARTURE$}  Precip",
            "", "", "High", "Low", "Normal"
        );
    }
    for d in &h.days {
        let _ = writeln!(
            out,
            "{:<DAY$}{:<DATE$}{:>TEMP$}{:>TEMP$}{:>DEPARTURE$}  {}",
            d.label, d.date_short, d.high.text, d.low.text, d.departure.text, d.precipitation.text
        );
    }
    if let Some(mtd) = &h.month_to_date {
        let _ = writeln!(out, "\n{}", mtd.text);
    }
    if !h.credit.is_empty() {
        let _ = writeln!(out, "{}", h.credit);
    }
    let _ = writeln!(out, "{attribution}");
    out
}

/// The years a station covers: "1940–2026", "since 2006".
fn period(s: &NearbyStation) -> String {
    match (s.first, s.last) {
        (Some(first), Some(last)) => format!("{}–{}", first.year, last.year),
        _ => String::new(),
    }
}

/// What to know before choosing a station.
fn remark(s: &NearbyStation) -> &'static str {
    if !s.usable {
        "stopped reporting"
    } else if !s.long_record {
        "no normals (starts after 1991)"
    } else if !s.reported_yesterday {
        "reports late"
    } else {
        ""
    }
}

/// The stations near a place, best first, the one in use starred: the
/// configured one, else the first usable. Stations that stopped reporting are
/// only counted.
fn stations_table(
    name: &str,
    list: &[NearbyStation],
    configured: Option<&str>,
    f: &Formatter<'_>,
) -> String {
    let used = configured.map_or_else(
        || list.iter().find(|s| s.usable).map(|s| s.id.as_str()),
        Some,
    );
    let mut out = format!("ACIS stations near {name} with temperatures, best first:\n");
    let usable: Vec<&NearbyStation> = list.iter().filter(|s| s.usable).collect();
    let width = usable
        .iter()
        .map(|s| s.name.chars().count())
        .max()
        .unwrap_or(0);
    for s in &usable {
        let star = if Some(s.id.as_str()) == used {
            '*'
        } else {
            ' '
        };
        #[allow(clippy::cast_precision_loss)] // under 60 km
        let distance = f.distance(Some(Meters(s.distance_m as f32))).text;
        let line = format!(
            "{star} {:<8} {:<width$}  {distance:>6}  {:<9}  {}",
            s.id,
            s.name,
            period(s),
            remark(s)
        );
        let _ = writeln!(out, "{}", line.trim_end());
    }
    let stopped = list.len() - usable.len();
    if stopped > 0 {
        let _ = writeln!(
            out,
            "  and {stopped} more that stopped reporting (--json lists them)"
        );
    }
    match configured {
        Some(id) if !list.iter().any(|s| s.id == id) => {
            let _ = writeln!(out, "* {id} (configured) is not among them");
        }
        Some(_) => {}
        None => {
            let _ = writeln!(
                out,
                "* is used; choose another with [history.acis] station = \"<id>\""
            );
        }
    }
    out
}

/// The stations as JSON, every one.
fn stations_json(list: &[NearbyStation]) -> serde_json::Value {
    let date = |d: Option<whether_core::time::LocalDate>| {
        d.map(|d| format!("{:04}-{:02}-{:02}", d.year, d.month, d.day))
    };
    list.iter()
        .map(|s| {
            serde_json::json!({
                "id": s.id,
                "name": s.name,
                "distance_m": s.distance_m,
                "first": date(s.first),
                "last": date(s.last),
                "long_record": s.long_record,
                "reported_yesterday": s.reported_yesterday,
                "usable": s.usable,
            })
        })
        .collect()
}

/// The place's offset: its forecast's when one is kept (a place given by
/// coordinates alone resolves to UTC), else the configured one.
pub(crate) fn with_forecast_offset(
    cfg: &LinuxConfig,
    cache: &Cache,
    mut place: ResolvedLocation,
) -> ResolvedLocation {
    let loc = Coordinates {
        latitude: place.latitude,
        longitude: place.longitude,
    };
    let kept = cfg
        .common
        .providers
        .order
        .iter()
        .copied()
        .chain([ProviderId::OpenMeteo])
        .filter_map(|id| cache.load(&Cache::key(id, loc)))
        .max_by_key(|c| c.saved_at);
    if let Some(kept) = kept {
        place.utc_offset = kept.snapshot.location.utc_offset;
    }
    place
}

/// `whether history` for a resolved place over `transport`; the text or JSON
/// to print, or why there is none.
pub(crate) fn report(
    cli: &Cli,
    args: &HistoryArgs,
    cfg: &LinuxConfig,
    place: &ResolvedLocation,
    cache: &Cache,
    transport: &dyn Transport,
    now: UnixTime,
) -> anyhow::Result<String> {
    let strings = Strings::localized(&cfg.common.general.language, &cfg.common.strings).0;
    let p = presentation(cfg, args.days)?;
    if args.station {
        let list = acis_stations(&cfg.common, place, now, transport)
            .map_err(|e| anyhow!("ACIS stations: {e}"))?;
        return Ok(if args.json {
            format!("{:#}\n", stations_json(&list))
        } else {
            let f = Formatter::new(&p, &strings);
            stations_table(
                &place.name,
                &list,
                cfg.common.history.acis.station.as_deref(),
                &f,
            )
        });
    }
    let src = HistorySource {
        cfg: &cfg.common,
        env: &env,
        cache,
        transport,
        offline: cli.offline,
    };
    let found = history_for(&src, place, requirements(cfg, p.history_days), now)?;
    for a in &found.attempts {
        if let Some(e) = &a.error {
            tracing::info!("{}: {e}", a.provider.display_name());
        }
    }
    let (vm, attribution) = view(place, &found, &p, &strings, now);
    Ok(if args.json {
        let doc = serde_json::json!({
            "location": place.name,
            "fetched": found.fetched,
            "view": vm,
            "attribution": attribution,
            "history": found.cached.history,
        });
        format!("{doc:#}\n")
    } else {
        vm.map_or_else(
            || format!("{}\nNo history for this place.\n", place.name),
            |h| text(&place.name, &h, &attribution),
        )
    })
}

/// `whether history`.
pub(crate) fn run(cli: &Cli, args: &HistoryArgs) -> anyhow::Result<ExitCode> {
    let cfg = match load_config(cli, &[]) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("{e}");
            return Ok(ExitCode::from(2));
        }
    };
    let i = location_index(&cfg, cli.location.as_deref());
    let loc = cfg
        .common
        .location
        .get(i)
        .context("no location configured")?;
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
    let place = resolve_location(loc, &geocoder, &cache, now).map_err(|e| anyhow!(e))?;
    let place = with_forecast_offset(&cfg, &cache, place);
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
    use crate::commands::testing::{NOV_2, cli, fake, new_york as place};
    use whether_core::history::{DayNormal, Normals, YearRange};
    use whether_runtime::history::history_key;

    /// Imperial units, history from Open-Meteo alone.
    fn config() -> LinuxConfig {
        whether_config::load::parse(
            "[general]\nunits = \"imperial\"\n\
             [history]\norder = [\"open-meteo\"]\nsupplement = []\n\
             [[location]]\nname = \"New York\"\nlatitude = 40.7143\nlongitude = -74.00597\n",
        )
        .unwrap()
    }

    #[test]
    fn stations_best_first_with_the_one_in_use() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path());
        let net = fake(vec![(
            "StnMeta",
            include_str!(
                "../../../../crates/whether-providers/tests/fixtures/acis/stnmeta_10001.json"
            ),
        )]);
        let args = HistoryArgs {
            station: true,
            ..HistoryArgs::default()
        };
        let cfg = config();
        let out = report(
            &cli(&["history", "--station"]),
            &args,
            &cfg,
            &place(),
            &cache,
            &net,
            NOV_2,
        )
        .unwrap();
        let lines: Vec<&str> = out.lines().collect();
        assert!(
            lines[1].starts_with("* NYC      NY City Central Park"),
            "{out}"
        );
        assert!(
            lines[1].contains("4.9 mi") && lines[1].contains("1869–2026"),
            "{out}"
        );
        assert!(out.contains("  LGA      Laguardia Airport"), "{out}");
        assert!(out.contains("Harrison") && out.contains("no normals (starts after 1991)"));
        assert!(out.contains("more that stopped reporting"));
        assert!(out.ends_with("choose another with [history.acis] station = \"<id>\"\n"));
        // A configured station is the one starred.
        let mut pinned = config();
        pinned.common.history.acis.station = Some("LGA".into());
        let out = report(
            &cli(&["history", "--station"]),
            &args,
            &pinned,
            &place(),
            &cache,
            &net,
            NOV_2,
        )
        .unwrap();
        assert!(out.contains("* LGA") && out.contains("  NYC"), "{out}");
        // JSON lists every station, stopped ones too.
        let json = HistoryArgs { json: true, ..args };
        let v: serde_json::Value = serde_json::from_str(
            &report(
                &cli(&["history"]),
                &json,
                &cfg,
                &place(),
                &cache,
                &net,
                NOV_2,
            )
            .unwrap(),
        )
        .unwrap();
        let all = v.as_array().unwrap();
        assert_eq!(all[0]["id"], "NYC");
        assert!(all.iter().any(|s| s["usable"] == false));
    }

    #[test]
    fn the_history_fetched_then_kept() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path());
        // Normals already kept: only the past days are asked for.
        let key = history_key(Coordinates {
            latitude: 40.7143,
            longitude: -74.005_97,
        });
        let mut days = vec![DayNormal::default(); 366];
        // 2 November (day 306 of a leap year, from 0): 59/46 °F.
        days[306] = DayNormal {
            max: Some(whether_core::units::Celsius(15.0)),
            min: Some(whether_core::units::Celsius(7.778)),
            ..DayNormal::default()
        };
        let normals = Normals {
            period: YearRange::NORMALS_1991_2020,
            source: ProviderId::OpenMeteo,
            days,
        };
        cache.store_normals(&key, &normals).unwrap();
        let net = fake(vec![(
            "historical-forecast",
            include_str!(
                "../../../../crates/whether-providers/tests/fixtures/open_meteo/history_past_10001.json"
            ),
        )]);
        let cfg = config();
        let args = HistoryArgs {
            days: Some(3),
            ..HistoryArgs::default()
        };
        let out = report(
            &cli(&["history"]),
            &args,
            &cfg,
            &place(),
            &cache,
            &net,
            NOV_2,
        )
        .unwrap();
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "New York");
        assert!(out.contains("High   Low  Normal  Precip"), "{out}");
        let yesterday = lines.iter().find(|l| l.starts_with("Yesterday")).unwrap();
        // 1 November: 15.3 °C, dry.
        assert!(
            yesterday.contains("Nov 1") && yesterday.contains("60°"),
            "{out}"
        );
        assert!(yesterday.ends_with("0.00 in"), "{out}");
        assert_eq!(
            lines
                .iter()
                .filter(|l| l.contains(" Oct ") || l.contains(" Nov "))
                .count(),
            3,
            "three days: {out}"
        );
        assert!(out.contains("ERA5 reanalysis"), "{out}");
        assert!(out.ends_with("Weather data by Open-Meteo.com\n"), "{out}");
        assert_eq!(*net.sent.lock().unwrap(), 1);
        // Kept: asked again the same day, nothing is fetched; offline too.
        let json = HistoryArgs { json: true, ..args };
        let v: serde_json::Value = serde_json::from_str(
            &report(
                &cli(&["--offline", "history"]),
                &json,
                &cfg,
                &place(),
                &cache,
                &net,
                NOV_2,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(v["fetched"], false);
        assert_eq!(v["view"]["days"].as_array().unwrap().len(), 3);
        assert_eq!(v["history"]["source"], "open-meteo");
        assert_eq!(*net.sent.lock().unwrap(), 1);
    }

    #[test]
    fn offline_with_nothing_kept() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path());
        let net = fake(vec![]);
        let e = report(
            &cli(&["--offline", "history"]),
            &HistoryArgs::default(),
            &config(),
            &place(),
            &cache,
            &net,
            NOV_2,
        )
        .unwrap_err();
        assert!(matches!(
            e.downcast_ref(),
            Some(HistoryError::NothingCached)
        ));
        assert_eq!(*net.sent.lock().unwrap(), 0);
    }
}
