//! Loading the configuration the same way for every command.

use std::path::{Path, PathBuf};

use whether_config::linux::user_config_file;
use whether_config::{ConfigError, LinuxConfig, LoadOptions, load};

use whether::cli::{Cli, Units};

/// The process environment.
pub(crate) fn env(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// Dotted-key overrides from command-line flags.
pub(crate) fn overrides(cli: &Cli) -> Vec<(String, toml::Value)> {
    let mut out = Vec::new();
    if let Some(units) = cli.units {
        let value = match units {
            Units::Metric => "metric",
            Units::Imperial => "imperial",
        };
        out.push((
            "general.units".to_string(),
            toml::Value::String(value.into()),
        ));
    }
    out
}

/// Loads with `--config`, the environment and `extra` overrides on top of the flags.
pub(crate) fn load_config(
    cli: &Cli,
    extra: &[(String, toml::Value)],
) -> Result<LinuxConfig, ConfigError> {
    load_with(
        cli.config.as_deref(),
        &[overrides(cli), extra.to_vec()].concat(),
    )
}

/// Loads from an optional explicit file and overrides (also used to reload).
pub(crate) fn load_with(
    explicit: Option<&Path>,
    overrides: &[(String, toml::Value)],
) -> Result<LinuxConfig, ConfigError> {
    let cli_overrides = overrides
        .iter()
        .map(|(k, v)| (k.as_str(), v.clone()))
        .collect();
    load(&LoadOptions {
        explicit_path: explicit,
        env: &env,
        cli_overrides,
    })
}

/// The file `config init` and location edits write: `--config`, else the user file.
pub(crate) fn edit_path(cli: &Cli) -> Option<PathBuf> {
    cli.config.clone().or_else(|| user_config_file(&env))
}
