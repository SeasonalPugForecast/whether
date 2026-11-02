//! `whether config init | check | show | path | schema | strings`.

use std::fmt::Write as _;
use std::path::Path;
use std::process::ExitCode;

use anyhow::{Context, bail};
use whether_config::linux::config_layers;
use whether_config::{ConfigError, Issue, LinuxConfig, Severity, example::EXAMPLE, schema};
use whether_core::strings::Strings;

use crate::common::{edit_path, env, load_config};
use crate::registry::{find, registry};
use whether::cli::Cli;

/// Writes the example to `path` unless it exists (or `force`).
pub(crate) fn init(path: &Path, force: bool) -> anyhow::Result<()> {
    if path.exists() && !force {
        bail!(
            "{} already exists; use --force to overwrite it",
            path.display()
        );
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    std::fs::write(path, EXAMPLE).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// `config init`.
pub(crate) fn run_init(cli: &Cli, force: bool) -> anyhow::Result<ExitCode> {
    let path = edit_path(cli)
        .context("cannot find the configuration directory; set HOME or use --config")?;
    init(&path, force)?;
    println!("wrote {}", path.display());
    Ok(ExitCode::SUCCESS)
}

/// Issues from each `[display.<name>]` table: the plugin's own checks, or a warning
/// for displays that are not compiled in; external plugins (`[display.exec]`) and
/// the names that refer to them.
pub(crate) fn display_issues(cfg: &LinuxConfig) -> Vec<Issue> {
    let mut issues = Vec::new();
    let exec = cfg.display.plugin_table("exec");
    if let Some(exec) = &exec {
        issues.extend(whether_plugin_proto::exec::validate(exec));
    }
    let referenced = cfg
        .display
        .sidecars
        .iter()
        .map(|s| ("display.sidecars", s))
        .chain(cfg.display.default.iter().map(|d| ("display.default", d)));
    for (path, name) in referenced {
        if let Some(plugin) = name.strip_prefix("exec:")
            && exec.as_ref().and_then(|t| t.get(plugin)).is_none()
        {
            issues.push(Issue {
                severity: Severity::Warning,
                path: path.into(),
                message: format!("`{name}` has no [display.exec.{plugin}] table"),
            });
        }
    }
    for name in cfg.display.plugins.keys().filter(|n| *n != "exec") {
        match find(name) {
            Some(factory) => issues.extend(
                cfg.display
                    .plugin_table(name)
                    .map(|t| (factory.validate)(&t))
                    .unwrap_or_default(),
            ),
            None => issues.push(Issue {
                severity: Severity::Warning,
                path: format!("display.{name}"),
                message: "this display is not compiled into this binary; the table is ignored"
                    .into(),
            }),
        }
    }
    issues
}

/// The text `config check` prints and its exit code.
pub(crate) fn check_report(result: Result<LinuxConfig, ConfigError>) -> (String, u8) {
    let mut out = String::new();
    let (issues, failed) = match result {
        Ok(cfg) => {
            let mut issues = cfg.issues.clone();
            issues.extend(display_issues(&cfg));
            for source in &cfg.sources {
                let _ = writeln!(out, "read {}", source.display());
            }
            let failed = whether_config::has_errors(&issues);
            (issues, failed)
        }
        Err(ConfigError::Invalid(issues)) => (issues, true),
        Err(other) => {
            let _ = writeln!(out, "error: {other}");
            return (out, 2);
        }
    };
    for issue in &issues {
        let _ = writeln!(out, "{issue}");
    }
    if failed {
        (out, 2)
    } else {
        let _ = writeln!(out, "ok: {} warning(s)", issues.len());
        (out, 0)
    }
}

/// `config check`.
pub(crate) fn run_check(cli: &Cli) -> ExitCode {
    let (text, code) = check_report(load_config(cli, &[]));
    print!("{text}");
    ExitCode::from(code)
}

/// The effective configuration as TOML (secret literals print as `***`).
pub(crate) fn show(cfg: &LinuxConfig, resolved: bool) -> anyhow::Result<String> {
    let mut table = match toml::Value::try_from(&cfg.common)? {
        toml::Value::Table(t) => t,
        _ => toml::Table::new(),
    };
    if resolved {
        let name = cfg.display.default.as_deref().unwrap_or("line");
        let presentation = cfg.presentation_for(name)?;
        table.insert("presentation".into(), toml::Value::try_from(&presentation)?);
    }
    let mut display = toml::Table::new();
    if let Some(default) = &cfg.display.default {
        display.insert("default".into(), toml::Value::String(default.clone()));
    }
    if !cfg.display.sidecars.is_empty() {
        let sidecars = cfg
            .display
            .sidecars
            .iter()
            .cloned()
            .map(toml::Value::String);
        display.insert("sidecars".into(), toml::Value::Array(sidecars.collect()));
    }
    for (name, plugin) in &cfg.display.plugins {
        display.insert(name.clone(), toml::Value::Table(plugin.clone()));
    }
    table.insert("display".into(), toml::Value::Table(display));
    table.insert("runtime".into(), toml::Value::try_from(&cfg.runtime)?);
    Ok(toml::to_string(&table)?)
}

/// `config show`.
pub(crate) fn run_show(cli: &Cli, resolved: bool) -> anyhow::Result<ExitCode> {
    match load_config(cli, &[]) {
        Ok(cfg) => {
            print!("{}", show(&cfg, resolved)?);
            Ok(ExitCode::SUCCESS)
        }
        Err(e) => {
            eprintln!("{e}");
            Ok(ExitCode::from(2))
        }
    }
}

/// `config path`.
pub(crate) fn paths(cli: &Cli) -> String {
    let mut out = String::new();
    let files = match &cli.config {
        Some(path) => vec![path.clone()],
        None => config_layers(&env),
    };
    for file in files {
        let state = if file.exists() { "found" } else { "missing" };
        let _ = writeln!(out, "{state:<8} {}", file.display());
    }
    out
}

/// `config schema`: the core schema with every compiled-in display's table.
pub(crate) fn schema_json() -> String {
    let mut plugins: Vec<(&str, schemars::Schema)> =
        registry().iter().map(|f| (f.name, (f.schema)())).collect();
    plugins.push(("exec", whether_plugin_proto::exec::schema()));
    schema::to_pretty_json(&schema::compose(schema::core_schema(), &plugins))
}

/// `config strings`.
pub(crate) fn strings() -> String {
    let mut out = String::from("[strings]\n");
    for (key, text) in Strings::keys() {
        let _ = writeln!(out, "# {key} = {}", toml::Value::String(text.into()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use whether_config::{CommonConfig, DisplaySection, RuntimeConfig, Secret};

    fn cfg() -> LinuxConfig {
        let mut common = CommonConfig::default();
        common.providers.openweather.as_mut().unwrap().api_key =
            Some(Secret::Literal("HUNTER2".into()));
        LinuxConfig {
            common,
            display: DisplaySection {
                default: Some("line".into()),
                sidecars: Vec::new(),
                plugins: std::collections::BTreeMap::default(),
            },
            runtime: RuntimeConfig::default(),
            radar: whether_config::RadarConfig::default(),
            sources: vec![],
            issues: vec![],
            measurement_locale: None,
            location_override: None,
        }
    }

    #[test]
    fn init_refuses_to_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("whether/config.toml");
        init(&path, false).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), EXAMPLE);
        assert!(
            init(&path, false)
                .unwrap_err()
                .to_string()
                .contains("--force")
        );
        init(&path, true).unwrap();
    }

    #[test]
    fn show_masks_secrets() {
        let text = show(&cfg(), false).unwrap();
        assert!(!text.contains("HUNTER2") && text.contains("***"), "{text}");
        assert!(text.contains("[display]") && text.contains("default = \"line\""));
        let resolved = show(&cfg(), true).unwrap();
        assert!(resolved.contains("[presentation.units]"), "{resolved}");
    }

    #[test]
    fn check_reports_and_exit_codes() {
        let (text, code) = check_report(Ok(cfg()));
        assert_eq!(code, 0, "{text}");
        let issue = Issue {
            severity: Severity::Error,
            path: "location".into(),
            message: "none".into(),
        };
        let (text, code) = check_report(Err(ConfigError::Invalid(vec![issue])));
        assert_eq!((text.as_str(), code), ("error: location: none\n", 2));
        let mut unknown = cfg();
        unknown
            .display
            .plugins
            .insert("nope".into(), toml::Table::new());
        assert!(
            check_report(Ok(unknown))
                .0
                .contains("warning: display.nope")
        );
    }

    #[test]
    fn schema_and_strings() {
        let schema: serde_json::Value = serde_json::from_str(&schema_json()).unwrap();
        assert!(
            schema
                .pointer("/$defs/DisplayTables/properties/default")
                .is_some()
        );
        assert!(strings().starts_with("[strings]\n# "));
    }
}
