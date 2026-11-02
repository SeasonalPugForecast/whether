//! `whether daemon`: the fetch service without a display, serving the plugin
//! protocol on `$XDG_RUNTIME_DIR/whether/whether.sock` to displays started with
//! `--attach` (and to any protocol client). Sidecars run here too.

use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context, anyhow};
use whether_core::strings::Strings;
use whether_display::{DisplayContext, RunMode};
use whether_plugin_proto::socket;
use whether_runtime::service::Env;
use whether_runtime::{paths, spawn_service};

use crate::commands::run::{join, start_sidecars, worker};
use crate::common::load_config;
use crate::logging;
use whether::cli::Cli;

pub(crate) fn run(cli: &Cli) -> anyhow::Result<ExitCode> {
    let cfg = match load_config(cli, &[]) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("{e}");
            return Ok(ExitCode::from(2));
        }
    };
    let env: Env = Arc::new(|name| std::env::var(name).ok());
    // Logs go to stderr (the journal under systemd) unless runtime.log_file says otherwise.
    let level = logging::level(cli.verbose, cfg.runtime.log_level.as_deref());
    logging::init(&level, cfg.runtime.log_file.as_deref())?;
    let path = paths::socket_path(&*env)
        .context("XDG_RUNTIME_DIR is not set, so there is nowhere private for the socket")?;
    let listener = match socket::listen(&path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("whether: {e}");
            return Ok(ExitCode::from(1));
        }
    };
    let state_dir = paths::state_dir(&*env);
    let cfg = Arc::new(cfg);
    let (worker, state) = worker(cli, &[], &cfg, env, state_dir)?;
    let service = spawn_service(worker).context("starting the fetch service")?;
    let strings = Arc::new(Strings::localized(&cfg.common.general.language, &cfg.common.strings).0);
    // Clients get the view in the external plugins' presentation; attached
    // displays rebuild their own from the snapshots.
    let presentation = Arc::new(cfg.presentation_for("exec").map_err(|e| anyhow!(e))?);
    let sidecars = start_sidecars(&cfg, &state, &service.commands, &strings, 0)?;
    tracing::info!("serving on {}", path.display());
    socket::accept_loop(&listener, || DisplayContext {
        state: state.clone(),
        commands: service.commands.clone(),
        display: "exec".into(),
        presentation: Arc::clone(&presentation),
        strings: Arc::clone(&strings),
        initial_location: 0,
        mode: RunMode::Watch,
    });
    // Only an accept error gets here.
    service.shutdown();
    join(sidecars);
    let _ = std::fs::remove_file(&path);
    Ok(ExitCode::from(1))
}
