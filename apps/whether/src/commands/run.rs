//! Running a display: selection, validation, the fetch service and exit codes.

use std::io::IsTerminal;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::mpsc::{Sender, channel};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{Context, anyhow};
use whether_config::LinuxConfig;
use whether_core::requirements::DataRequirements;
use whether_core::strings::Strings;
use whether_display::{
    Command, DisplayContext, DisplayFactory, DisplayPlugin, RunMode, StateHandle, StatePublisher,
};
use whether_plugin_proto::{exec, socket};
use whether_runtime::service::{Env, Reload};
use whether_runtime::{
    Cache, ServiceOptions, UreqTransport, UsageStore, Worker, initial_state, paths, refresh_once,
    spawn_service, user_agent,
};

use crate::common::{edit_path, load_config, load_with, overrides};
use crate::logging;
use crate::registry::{find, registry};
use whether::cli::Cli;

/// A chosen display: compiled in, or an external plugin.
#[derive(Debug)]
pub(crate) enum Selected {
    /// A built-in display.
    Builtin(DisplayFactory),
    /// `exec:<name>`, by name.
    Exec(String),
}

/// The display to run: the command line, then `WHETHER_DISPLAY`/`display.default`
/// (the loader merges both), then `tui` on a terminal and `line` elsewhere. Only a
/// display named on the command line must exist; otherwise `line` stands in.
/// `exec:<name>` selects an external plugin.
pub(crate) fn select(
    requested: Option<&str>,
    configured: Option<&str>,
    tty: bool,
) -> Result<Selected, String> {
    if let Some(name) = requested.or(configured).filter(|n| n.starts_with("exec:")) {
        return if whether_config::is_exec_name(name) {
            Ok(Selected::Exec(name["exec:".len()..].to_string()))
        } else {
            Err(format!(
                "`{name}`: external plugin names use letters, digits, `-` and `_`"
            ))
        };
    }
    select_builtin(requested, configured, tty).map(Selected::Builtin)
}

fn select_builtin(
    requested: Option<&str>,
    configured: Option<&str>,
    tty: bool,
) -> Result<DisplayFactory, String> {
    let available = || {
        registry()
            .iter()
            .map(|f| f.name)
            .collect::<Vec<_>>()
            .join(", ")
    };
    if let Some(name) = requested {
        return find(name).ok_or_else(|| {
            format!(
                "display `{name}` is not compiled in (available: {})",
                available()
            )
        });
    }
    let name = configured.unwrap_or(if tty { "tui" } else { "line" });
    find(name)
        .or_else(|| {
            tracing::warn!("display `{name}` is not compiled in; using `line`");
            find("line")
        })
        .ok_or_else(|| format!("no display is compiled in (available: {})", available()))
}

/// The location index for `--location` (name or 1-based index) or `WHETHER_LOCATION`.
pub(crate) fn location_index(cfg: &LinuxConfig, wanted: Option<&str>) -> usize {
    let Some(wanted) = wanted.or(cfg.location_override.as_deref()) else {
        return 0;
    };
    let names = cfg
        .common
        .location
        .iter()
        .map(|l| l.name.as_deref().unwrap_or(""));
    if let Some(i) = names.clone().position(|n| n.eq_ignore_ascii_case(wanted)) {
        return i;
    }
    match wanted.parse::<usize>() {
        Ok(n) if (1..=cfg.common.location.len()).contains(&n) => n - 1,
        _ => {
            tracing::warn!("no location `{wanted}`; showing the first");
            0
        }
    }
}

/// Exit code for a plugin error that means "no data", if it is one.
fn no_data_code(error: &(dyn std::error::Error + 'static)) -> Option<u8> {
    #[cfg(feature = "line")]
    if let Some(no_data) = error.downcast_ref::<whether_line::NoData>() {
        return u8::try_from(no_data.exit_code()).ok();
    }
    let _ = error;
    None
}

/// The service settings: offline mode, the call ledger, edits and reloads with the
/// same flags.
fn service_options(
    cli: &Cli,
    extra: &[(String, toml::Value)],
    env: Env,
    state_dir: Option<std::path::PathBuf>,
) -> ServiceOptions {
    let explicit = cli.config.clone();
    let reload_overrides = [overrides(cli), extra.to_vec()].concat();
    let reload: Reload = Box::new(move || load_with(explicit.as_deref(), &reload_overrides));
    ServiceOptions {
        offline: cli.offline,
        env,
        usage: state_dir.map(|d| UsageStore::at(d.join("usage.json"))),
        requirements: DataRequirements::full(),
        edit_path: edit_path(cli),
        reload: Some(reload),
        radar: None,
    }
}

/// A display plugin, the name its presentation overrides use, and its run mode.
type Created = (Box<dyn DisplayPlugin>, &'static str, RunMode);

/// The display plugin for `selected`; `None` after printing configuration errors.
fn create(selected: &Selected, cfg: &LinuxConfig, watch: bool) -> anyhow::Result<Option<Created>> {
    match selected {
        Selected::Builtin(factory) => {
            let section = cfg.display.plugin_table(factory.name);
            let issues: Vec<_> = section
                .as_ref()
                .map(|t| (factory.validate)(t))
                .unwrap_or_default();
            if whether_config::has_errors(&issues) {
                for issue in issues {
                    eprintln!("{issue}");
                }
                return Ok(None);
            }
            let plugin = (factory.create)(section.as_ref(), cfg).map_err(|e| anyhow!(e))?;
            Ok(Some((plugin, factory.name, factory.run_mode(watch))))
        }
        Selected::Exec(name) => {
            let exec = cfg.display.plugin_table("exec");
            match exec::lookup(exec.as_ref(), name) {
                Ok(c) => {
                    let mode = if watch { RunMode::Watch } else { RunMode::Once };
                    let plugin: Box<dyn DisplayPlugin> =
                        Box::new(exec::ExecDisplay::new(name, c, false));
                    Ok(Some((plugin, "exec", mode)))
                }
                Err(e) => {
                    eprintln!("whether: {e}");
                    Ok(None)
                }
            }
        }
    }
}

/// Starts each `display.sidecars` plugin on its own thread; a missing or invalid
/// table is reported and skipped.
pub(crate) fn start_sidecars(
    cfg: &LinuxConfig,
    state: &StateHandle,
    commands: &Sender<Command>,
    strings: &Arc<Strings>,
    initial_location: usize,
) -> anyhow::Result<Vec<JoinHandle<()>>> {
    if cfg.display.sidecars.is_empty() {
        return Ok(Vec::new());
    }
    let exec = cfg.display.plugin_table("exec");
    let presentation = Arc::new(cfg.presentation_for("exec")?);
    let mut threads = Vec::new();
    for sidecar in &cfg.display.sidecars {
        let name = sidecar.trim_start_matches("exec:").to_string();
        let plugin = match exec::lookup(exec.as_ref(), &name) {
            Ok(c) => exec::ExecDisplay::new(&name, c, true),
            Err(e) => {
                tracing::error!("sidecar skipped: {e}");
                continue;
            }
        };
        let ctx = DisplayContext {
            state: state.clone(),
            commands: commands.clone(),
            display: "exec".into(),
            presentation: Arc::clone(&presentation),
            strings: Arc::clone(strings),
            initial_location,
            mode: RunMode::Watch,
        };
        let thread = std::thread::Builder::new()
            .name(format!("sidecar-{name}"))
            .spawn(move || {
                if let Err(e) = Box::new(plugin).run(ctx) {
                    tracing::error!("sidecar exec:{name}: {e}");
                }
            })
            .context("starting a sidecar thread")?;
        threads.push(thread);
    }
    Ok(threads)
}

/// `whether [run | line | json | tui | gui]`.
pub(crate) fn run_display(
    cli: &Cli,
    requested: Option<&str>,
    extra: &[(String, toml::Value)],
) -> anyhow::Result<ExitCode> {
    let cfg = match load_config(cli, extra) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("{e}");
            return Ok(ExitCode::from(2));
        }
    };
    let env: Env = Arc::new(|name| std::env::var(name).ok());
    let requested = requested.or(cli.display.as_deref());
    let selected = match select(
        requested,
        cfg.display.default.as_deref(),
        std::io::stdout().is_terminal(),
    ) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("whether: {e}");
            return Ok(ExitCode::from(2));
        }
    };
    let interactive = matches!(&selected, Selected::Builtin(f) if f.interactive);
    let state_dir = paths::state_dir(&*env);
    let level = logging::level(cli.verbose, cfg.runtime.log_level.as_deref());
    let log_file = if interactive {
        cfg.runtime
            .log_file
            .clone()
            .or_else(|| state_dir.as_ref().map(|d| d.join("whether.log")))
    } else {
        None
    };
    logging::init(&level, log_file.as_deref())?;

    let Some((plugin, presentation_name, mode)) = create(&selected, &cfg, cli.watch)? else {
        return Ok(ExitCode::from(2));
    };
    let presentation = Arc::new(cfg.presentation_for(presentation_name)?);
    let strings = Arc::new(Strings::localized(&cfg.common.general.language, &cfg.common.strings).0);
    let initial_location = location_index(&cfg, cli.location.as_deref());
    let cfg = Arc::new(cfg);
    let context = |state: &StateHandle, commands| DisplayContext {
        state: state.clone(),
        commands,
        display: presentation_name.to_string(),
        presentation: Arc::clone(&presentation),
        strings: Arc::clone(&strings),
        initial_location,
        mode,
    };
    if cli.attach {
        // Another process fetches; this one only shows.
        let path = paths::socket_path(&*env)
            .context("XDG_RUNTIME_DIR is not set, so there is no daemon socket")?;
        let attached = match socket::attach(&path, Duration::from_secs(5)) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("whether: {e}");
                return Ok(ExitCode::from(1));
            }
        };
        // Sidecars run where the fetch service runs: the daemon owns them, so an
        // attached display starting its own would duplicate them (and a one-shot
        // run would restart them on every invocation).
        if !cfg.display.sidecars.is_empty() {
            tracing::debug!("sidecars run in the daemon, not in attached displays");
        }
        let result = plugin.run(context(&attached.state, attached.commands.clone()));
        let _ = attached.commands.send(Command::Quit);
        let _ = attached.reader.join();
        return exit_code(result);
    }
    let (mut worker, state) = worker(cli, extra, &cfg, env, state_dir)?;
    let result = if mode == RunMode::Once {
        refresh_once(&mut worker);
        let (commands, _ignored) = channel();
        plugin.run(context(&state, commands))
    } else {
        let service = spawn_service(worker).context("starting the fetch service")?;
        let sidecars = start_sidecars(&cfg, &state, &service.commands, &strings, initial_location)?;
        let result = plugin.run(context(&state, service.commands.clone()));
        // Stopping the service drops the state publisher, which stops the sidecars.
        service.shutdown();
        join(sidecars);
        result
    };
    exit_code(result)
}

/// Waits for sidecar threads.
pub(crate) fn join(sidecars: Vec<JoinHandle<()>>) {
    for sidecar in sidecars {
        if sidecar.join().is_err() {
            tracing::error!("a sidecar thread panicked");
        }
    }
}

/// The fetch worker for `cfg` and the state bus it publishes to.
pub(crate) fn worker(
    cli: &Cli,
    extra: &[(String, toml::Value)],
    cfg: &Arc<LinuxConfig>,
    env: Env,
    state_dir: Option<std::path::PathBuf>,
) -> anyhow::Result<(Worker, StateHandle)> {
    let cache_dir = cfg
        .runtime
        .cache_dir
        .clone()
        .or_else(|| paths::cache_dir(&*env))
        .context("cannot find a cache directory; set HOME or runtime.cache_dir")?;
    let mut options = service_options(cli, extra, env, state_dir);
    // The radar thread idles until a presentation shows the radar (settings and
    // reloads can turn it on).
    if !cli.offline {
        options.radar = Some(whether_runtime::service::RadarOptions {
            wanted: cfg.shows_anywhere(whether_core::presentation::WidgetKind::Radar),
            zoom: cfg.radar.zoom,
            transport: Box::new(UreqTransport::new(&user_agent())),
        });
    }
    let (publisher, state) = StatePublisher::new(initial_state(cfg));
    let transport = Box::new(UreqTransport::new(&user_agent()));
    let worker = Worker::new(
        Arc::clone(cfg),
        publisher,
        transport,
        Cache::at(cache_dir),
        options,
    );
    Ok((worker, state))
}

/// The process exit code for a display's result.
fn exit_code(result: Result<(), whether_display::DisplayError>) -> anyhow::Result<ExitCode> {
    match result {
        Ok(()) => Ok(ExitCode::SUCCESS),
        Err(e) => match no_data_code(&*e) {
            Some(code) => {
                eprintln!("whether: {e}");
                Ok(ExitCode::from(code))
            }
            None => Err(anyhow!(e)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use whether_config::{CommonConfig, DisplaySection, LocationConfig, RuntimeConfig};

    fn cfg(names: &[&str]) -> LinuxConfig {
        LinuxConfig {
            common: CommonConfig {
                location: names
                    .iter()
                    .map(|n| LocationConfig {
                        name: Some((*n).into()),
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            },
            display: DisplaySection::default(),
            runtime: RuntimeConfig::default(),
            radar: whether_config::RadarConfig::default(),
            sources: vec![],
            issues: vec![],
            measurement_locale: None,
            location_override: None,
        }
    }

    #[test]
    fn locations_by_name_or_index() {
        let c = cfg(&["Home", "Cabin"]);
        assert_eq!(location_index(&c, None), 0);
        assert_eq!(location_index(&c, Some("cabin")), 1);
        assert_eq!(location_index(&c, Some("2")), 1);
        assert_eq!(location_index(&c, Some("9")), 0);
        let mut overridden = cfg(&["Home", "Cabin"]);
        overridden.location_override = Some("Cabin".into());
        assert_eq!(location_index(&overridden, None), 1);
    }

    fn name(selected: Result<Selected, String>) -> String {
        match selected.unwrap() {
            Selected::Builtin(f) => f.name.to_string(),
            Selected::Exec(name) => format!("exec:{name}"),
        }
    }

    #[cfg(feature = "line")]
    #[test]
    fn selection_order() {
        assert_eq!(name(select(Some("json"), Some("line"), true)), "json");
        assert!(
            select(Some("nope"), None, false)
                .unwrap_err()
                .contains("line, json")
        );
        assert_eq!(name(select(None, Some("json"), true)), "json");
        assert_eq!(name(select(None, None, false)), "line");
        assert_eq!(
            name(select(None, Some("gtk"), true)),
            "line",
            "a configured display that is not compiled in falls back to line"
        );
    }

    #[test]
    fn external_plugins_are_selected_by_name() {
        assert_eq!(name(select(None, Some("exec:notify"), true)), "exec:notify");
        assert_eq!(
            name(select(Some("exec:wall"), Some("tui"), true)),
            "exec:wall"
        );
        assert!(select(Some("exec:no good"), None, true).is_err());
    }
}
