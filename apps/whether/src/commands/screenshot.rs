//! `whether gui --screenshot`: render one frame headlessly, for docs and bug reports.

use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context, anyhow, bail};
use whether_core::strings::Strings;
use whether_display::{AppState, LocationState, StatePublisher, now, view_for};
use whether_gui::desktop::config::{GuiConfig, GuiMode};
use whether_gui::desktop::screenshot::{Which, render_png};
use whether_gui::model::to_ui;
use whether_runtime::service::Env;
use whether_runtime::{
    Cache, CachedSnapshot, ServiceOptions, UreqTransport, Worker, initial_state, paths,
    refresh_once, user_agent,
};

use crate::common::load_config;
use whether::cli::{Cli, GuiArgs};

/// `"1920x1080"` as (width, height).
pub(crate) fn parse_size(text: &str) -> Option<(u32, u32)> {
    let (w, h) = text.split_once(['x', 'X'])?;
    let size = (w.trim().parse().ok()?, h.trim().parse().ok()?);
    (size.0 >= 100 && size.1 >= 100).then_some(size)
}

/// Renders the state (from `--fixture`, or the cache without network) to the PNG.
pub(crate) fn run(cli: &Cli, args: &GuiArgs) -> anyhow::Result<ExitCode> {
    let out = args
        .screenshot
        .clone()
        .context("--screenshot needs a path")?;
    let cfg = load_config(cli, &[]).map_err(|e| anyhow!("{e}"))?;
    let gui: GuiConfig = match cfg.display.plugin_table("gui") {
        Some(t) => toml::Value::Table(t).try_into()?,
        None => GuiConfig::default(),
    };
    let compact = args.compact || gui.mode == GuiMode::Compact;
    let size = match &args.size {
        Some(text) => {
            parse_size(text).with_context(|| format!("`{text}` is not WxH, e.g. 1920x1080"))?
        }
        None if compact => (320, 240),
        None => (gui.size[0], gui.size[1]),
    };
    let state = match &args.fixture {
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("reading {}", path.display()))?;
            let cached: CachedSnapshot = serde_json::from_str(&text)
                .with_context(|| format!("{} is not a cache file", path.display()))?;
            let mut location =
                LocationState::new(cfg.common.location.first().cloned().unwrap_or_default());
            location.snapshot = Some(Arc::new(cached.snapshot));
            location.status.last_success = Some(cached.saved_at);
            location.status.provider = Some(cached.provider);
            AppState {
                locations: vec![location],
                ..AppState::default()
            }
        }
        None => cached_state(&cfg)?,
    };
    let presentation = cfg.presentation_for("gui")?;
    let strings = Strings::localized(&cfg.common.general.language, &cfg.common.strings).0;
    let vm = view_for(&state, 0, &presentation, &strings, now());
    let which = if compact {
        Which::Compact(0)
    } else {
        Which::Desktop
    };
    let opts = gui.window_options(args.fullscreen || gui.kiosk);
    render_png(which, &to_ui(&vm), &opts, size, &out)?;
    println!("wrote {}", out.display());
    if state.locations.iter().all(|l| l.snapshot.is_none()) {
        bail!("no cached data to show; run `whether line` once or pass --fixture");
    }
    Ok(ExitCode::SUCCESS)
}

/// The state with every location's cached snapshot, without touching the network.
fn cached_state(cfg: &whether_config::LinuxConfig) -> anyhow::Result<AppState> {
    let env: Env = Arc::new(|name| std::env::var(name).ok());
    let cache_dir = cfg
        .runtime
        .cache_dir
        .clone()
        .or_else(|| paths::cache_dir(&*env))
        .context("set HOME")?;
    let cfg = Arc::new(cfg.clone());
    let (publisher, handle) = StatePublisher::new(initial_state(&cfg));
    let options = ServiceOptions {
        offline: true,
        env,
        usage: None,
        requirements: whether_core::requirements::DataRequirements::full(),
        edit_path: None,
        reload: None,
        radar: None,
    };
    let mut worker = Worker::new(
        cfg,
        publisher,
        Box::new(UreqTransport::new(&user_agent())),
        Cache::at(cache_dir),
        options,
    );
    refresh_once(&mut worker);
    Ok((*handle.current()).clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes() {
        assert_eq!(parse_size("1920x1080"), Some((1920, 1080)));
        assert_eq!(parse_size("320X240"), Some((320, 240)));
        assert_eq!(parse_size("10x10"), None);
        assert_eq!(parse_size("wide"), None);
    }
}
