//! The `line` (status bars, waybar) and `json` (scripts) display plugins.

pub mod config;
pub mod render;
pub mod template;

use std::io::Write;

use whether_config::LinuxConfig;
use whether_display::{
    DisplayContext, DisplayError, DisplayFactory, DisplayInfo, DisplayPlugin, RunMode, now,
};

pub use config::{IconStyle, JsonConfig, LineConfig, LineOutput};
pub use render::NoData;

/// Prints `text` on its own line; a closed stdout (the bar went away) ends the run.
fn emit(text: &str) -> std::io::Result<()> {
    let mut out = std::io::stdout().lock();
    writeln!(out, "{text}")?;
    out.flush()
}

/// Shows the output once, or on every change in watch mode.
fn run_loop(
    ctx: &DisplayContext,
    location: Option<&str>,
    render: impl Fn(&whether_display::AppState, usize) -> Result<String, DisplayError>,
) -> Result<(), DisplayError> {
    let changes = ctx.state.subscribe();
    let mut last = None;
    loop {
        let state = ctx.state.current();
        let i = render::location_index(&state, location, ctx.initial_location);
        match render::require_data(&state, i) {
            Ok(()) => {
                let text = render(&state, i)?;
                if last.as_ref() != Some(&text) {
                    if emit(&text).is_err() {
                        return Ok(());
                    }
                    last = Some(text);
                }
            }
            Err(no_data) if ctx.mode != RunMode::Watch => return Err(no_data.into()),
            Err(_) => {}
        }
        if ctx.mode != RunMode::Watch || changes.recv().is_err() {
            return Ok(());
        }
    }
}

/// The `line` plugin.
#[derive(Debug)]
pub struct LinePlugin {
    format: render::LineFormat,
}

impl DisplayPlugin for LinePlugin {
    fn info(&self) -> DisplayInfo {
        DisplayInfo {
            name: "line",
            description: LINE_DESCRIPTION,
            interactive: false,
            needs_tty: false,
        }
    }

    fn run(self: Box<Self>, ctx: DisplayContext) -> Result<(), DisplayError> {
        let location = self.format.cfg.location.clone();
        let settings = ctx.settings();
        run_loop(&ctx, location.as_deref(), |state, i| {
            let (presentation, strings) = settings.current();
            Ok(self.format.render(state, i, &presentation, &strings, now()))
        })
    }
}

/// The `json` plugin.
#[derive(Debug)]
pub struct JsonPlugin {
    cfg: JsonConfig,
}

impl DisplayPlugin for JsonPlugin {
    fn info(&self) -> DisplayInfo {
        DisplayInfo {
            name: "json",
            description: JSON_DESCRIPTION,
            interactive: false,
            needs_tty: false,
        }
    }

    fn run(self: Box<Self>, ctx: DisplayContext) -> Result<(), DisplayError> {
        // Watch mode prints one document per line.
        let compact = self.cfg.compact || ctx.mode == RunMode::Watch;
        let settings = ctx.settings();
        run_loop(&ctx, self.cfg.location.as_deref(), |state, i| {
            let (presentation, strings) = settings.current();
            Ok(render::render_json(
                state,
                i,
                self.cfg.snapshot,
                compact,
                &presentation,
                &strings,
                now(),
            )?)
        })
    }
}

const LINE_DESCRIPTION: &str = "one line of text, or waybar JSON, for status bars";
const JSON_DESCRIPTION: &str = "the view model or raw snapshot as JSON, for scripts";

fn create_line(
    section: Option<&toml::Table>,
    _: &LinuxConfig,
) -> Result<Box<dyn DisplayPlugin>, DisplayError> {
    let cfg: LineConfig = config::parse(section)?;
    Ok(Box::new(LinePlugin {
        format: render::LineFormat::new(cfg)?,
    }))
}

fn create_json(
    section: Option<&toml::Table>,
    _: &LinuxConfig,
) -> Result<Box<dyn DisplayPlugin>, DisplayError> {
    Ok(Box::new(JsonPlugin {
        cfg: config::parse(section)?,
    }))
}

/// The registry entry for `line`.
pub fn line_factory() -> DisplayFactory {
    DisplayFactory {
        name: "line",
        description: LINE_DESCRIPTION,
        interactive: false,
        create: create_line,
        validate: config::validate_line,
        schema: || schemars::schema_for!(LineConfig),
    }
}

/// The registry entry for `json`.
pub fn json_factory() -> DisplayFactory {
    DisplayFactory {
        name: "json",
        description: JSON_DESCRIPTION,
        interactive: false,
        create: create_json,
        validate: config::validate_json,
        schema: || schemars::schema_for!(JsonConfig),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::mpsc::channel;
    use whether_core::presentation::PresentationConfig;
    use whether_core::strings::Strings;
    use whether_display::{AppState, StatePublisher};

    fn context(mode: RunMode) -> (StatePublisher, DisplayContext) {
        let (publisher, state) = StatePublisher::new(AppState::default());
        let (commands, _) = channel();
        let ctx = DisplayContext {
            state,
            commands,
            display: "line".into(),
            presentation: Arc::new(PresentationConfig::default()),
            strings: Arc::new(Strings::default()),
            initial_location: 0,
            mode,
        };
        (publisher, ctx)
    }

    #[test]
    fn once_without_data_is_an_exit_code() {
        let (_publisher, ctx) = context(RunMode::Once);
        let plugin = (line_factory().create)(None, &dummy_config()).unwrap();
        let err = plugin.run(ctx).unwrap_err();
        assert_eq!(err.downcast_ref::<NoData>(), Some(&NoData::Empty));
    }

    #[test]
    fn watch_ends_when_the_publisher_goes_away() {
        let (publisher, ctx) = context(RunMode::Watch);
        drop(publisher);
        let plugin = (json_factory().create)(None, &dummy_config()).unwrap();
        plugin.run(ctx).unwrap();
    }

    #[test]
    fn factories_validate_and_describe() {
        let bad: toml::Table = "format = \"{nope}\"".parse().unwrap();
        assert!((line_factory().create)(Some(&bad), &dummy_config()).is_err());
        assert_eq!((line_factory().validate)(&bad).len(), 1);
        let schema = (line_factory().schema)();
        assert!(
            schema
                .get("properties")
                .is_some_and(|p| p.get("format").is_some())
        );
        assert_eq!(line_factory().run_mode(true), RunMode::Watch);
        assert_eq!(json_factory().name, "json");
    }

    fn dummy_config() -> LinuxConfig {
        LinuxConfig {
            common: whether_config::CommonConfig::default(),
            display: whether_config::DisplaySection::default(),
            runtime: whether_config::RuntimeConfig::default(),
            radar: whether_config::RadarConfig::default(),
            sources: vec![],
            issues: vec![],
            measurement_locale: None,
            location_override: None,
        }
    }
}
