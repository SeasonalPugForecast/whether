//! The in-process display plugin contract.

use std::fmt;
use std::sync::Arc;
use std::sync::mpsc::Sender;

use whether_config::{Issue, LinuxConfig};
use whether_core::presentation::PresentationConfig;
use whether_core::strings::Strings;

use crate::bus::StateHandle;
use crate::state::Command;

/// Errors a plugin returns. Libraries avoid `anyhow`; the app converts with `?`.
pub type DisplayError = Box<dyn std::error::Error + Send + Sync>;

/// What `whether displays` lists about a plugin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisplayInfo {
    /// Name used on the command line and in `[display.<name>]`.
    pub name: &'static str,
    /// One line for `whether displays`.
    pub description: &'static str,
    /// Runs until the user quits (TUI, GUI) rather than printing and exiting.
    pub interactive: bool,
    /// Needs a terminal on stdin and stdout.
    pub needs_tty: bool,
}

/// How a plugin runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunMode {
    /// An event loop until the user quits; the service runs alongside.
    Interactive,
    /// Show the current state once and return. The app has already loaded the cache
    /// and fetched if it was stale, so no service runs.
    Once,
    /// Show the state on every change until killed (`--watch`).
    Watch,
}

/// What a running plugin gets.
pub struct DisplayContext {
    /// Application state and change notifications.
    pub state: StateHandle,
    /// Requests to the service.
    pub commands: Sender<Command>,
    /// The display's name for `[display.<name>.presentation]` (`exec` for external
    /// plugins), used to re-derive the presentation after a reload.
    pub display: String,
    /// `[presentation]` merged with `[display.<name>.presentation]`, resolved, at
    /// start-up. [`Self::settings`] follows reloads.
    pub presentation: Arc<PresentationConfig>,
    /// Text, with `[strings]` overrides, at start-up.
    pub strings: Arc<Strings>,
    /// The location to show first.
    pub initial_location: usize,
    /// How to run.
    pub mode: RunMode,
}

impl fmt::Debug for DisplayContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DisplayContext")
            .field("initial_location", &self.initial_location)
            .field("mode", &self.mode)
            .finish_non_exhaustive()
    }
}

impl DisplayContext {
    /// The presentation and strings in effect, following config reloads; cheap to
    /// clone into threads.
    pub fn settings(&self) -> LiveSettings {
        LiveSettings {
            state: self.state.clone(),
            display: self.display.clone(),
            presentation: Arc::clone(&self.presentation),
            strings: Arc::clone(&self.strings),
        }
    }
}

/// The current presentation and strings: the start-up ones until a reload publishes
/// a new configuration, then that configuration's.
#[derive(Clone, Debug)]
pub struct LiveSettings {
    state: StateHandle,
    display: String,
    presentation: Arc<PresentationConfig>,
    strings: Arc<Strings>,
}

impl LiveSettings {
    /// The presentation and strings now. A reloaded configuration whose
    /// presentation does not resolve keeps the start-up one.
    pub fn current(&self) -> (Arc<PresentationConfig>, Arc<Strings>) {
        let state = self.state.current();
        match &state.config {
            Some(cfg) if state.config_epoch > 0 => (
                cfg.0
                    .presentation_for(&self.display)
                    .map_or_else(|_| Arc::clone(&self.presentation), Arc::new),
                Arc::new(
                    Strings::localized(&cfg.0.common.general.language, &cfg.0.common.strings).0,
                ),
            ),
            _ => (Arc::clone(&self.presentation), Arc::clone(&self.strings)),
        }
    }

    /// The reload count the current values come from.
    pub fn config_epoch(&self) -> u64 {
        self.state.current().config_epoch
    }
}

/// A display.
pub trait DisplayPlugin {
    /// Name, description and needs.
    fn info(&self) -> DisplayInfo;
    /// Runs on the main thread until done.
    fn run(self: Box<Self>, ctx: DisplayContext) -> Result<(), DisplayError>;
}

/// Builds a plugin from its `[display.<name>]` table and the loaded configuration.
pub type CreateFn = fn(
    section: Option<&toml::Table>,
    cfg: &LinuxConfig,
) -> Result<Box<dyn DisplayPlugin>, DisplayError>;

/// How the app finds, validates and creates a plugin. The app's registry is an
/// explicit list of these, one per compiled-in feature.
#[derive(Clone, Copy)]
pub struct DisplayFactory {
    /// The plugin's name.
    pub name: &'static str,
    /// One line for `whether displays`.
    pub description: &'static str,
    /// Whether it is interactive (selects the default [`RunMode`]).
    pub interactive: bool,
    /// Builds the plugin from its `[display.<name>]` table.
    pub create: CreateFn,
    /// Checks its `[display.<name>]` table (`whether config check`).
    pub validate: fn(section: &toml::Table) -> Vec<Issue>,
    /// The JSON Schema of its table, composed into the config schema.
    pub schema: fn() -> schemars::Schema,
}

impl fmt::Debug for DisplayFactory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DisplayFactory")
            .field("name", &self.name)
            .field("interactive", &self.interactive)
            .finish_non_exhaustive()
    }
}

impl DisplayFactory {
    /// The default run mode: interactive plugins run interactively, others once
    /// (`--watch` selects [`RunMode::Watch`]).
    pub fn run_mode(&self, watch: bool) -> RunMode {
        match (self.interactive, watch) {
            (true, _) => RunMode::Interactive,
            (false, true) => RunMode::Watch,
            (false, false) => RunMode::Once,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Echo;

    impl DisplayPlugin for Echo {
        fn info(&self) -> DisplayInfo {
            DisplayInfo {
                name: "echo",
                description: "test",
                interactive: false,
                needs_tty: false,
            }
        }
        fn run(self: Box<Self>, ctx: DisplayContext) -> Result<(), DisplayError> {
            ctx.commands.send(Command::Quit)?;
            Ok(())
        }
    }

    fn factory(interactive: bool) -> DisplayFactory {
        DisplayFactory {
            name: "echo",
            description: "test",
            interactive,
            create: |_, _| Ok(Box::new(Echo)),
            validate: |_| Vec::new(),
            schema: || schemars::json_schema!({ "type": "object" }),
        }
    }

    #[test]
    fn run_modes() {
        assert_eq!(factory(true).run_mode(false), RunMode::Interactive);
        assert_eq!(factory(true).run_mode(true), RunMode::Interactive);
        assert_eq!(factory(false).run_mode(false), RunMode::Once);
        assert_eq!(factory(false).run_mode(true), RunMode::Watch);
    }

    #[test]
    fn plugins_run_with_a_context() {
        let (_publisher, state) = crate::bus::StatePublisher::new(crate::AppState::default());
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = DisplayContext {
            state,
            commands: tx,
            display: "echo".into(),
            presentation: Arc::new(PresentationConfig::default()),
            strings: Arc::new(Strings::default()),
            initial_location: 0,
            mode: RunMode::Once,
        };
        let plugin: Box<dyn DisplayPlugin> = Box::new(Echo);
        assert_eq!(plugin.info().name, "echo");
        plugin.run(ctx).unwrap();
        assert_eq!(rx.recv().unwrap(), Command::Quit);
        assert!(format!("{:?}", factory(false)).contains("echo"));
    }
}
