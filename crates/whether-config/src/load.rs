//! Loading the Linux configuration: discovery, layering, overrides and validation.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use toml::{Table, Value};
use whether_core::presentation::PresentationConfig;

use crate::error::ConfigError;
use crate::linux::{config_layers, measurement_locale};
use crate::merge::{deep_merge, set_dotted};
use crate::resolve::resolve_presentation;
use crate::types::{
    CommonConfig, General, HistoryConfig, LocationConfig, PresentationToml, ProvidersConfig,
    UnitsOverride,
};
use crate::validate::{Issue, has_errors, validate_presentation};

/// One configuration file: the common sections plus `[display]` and `[runtime]`.
/// The fields repeat [`CommonConfig`]'s because serde cannot combine `flatten` with
/// `deny_unknown_fields`.
#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ConfigFile {
    version: u32,
    general: General,
    units: UnitsOverride,
    location: Vec<LocationConfig>,
    providers: ProvidersConfig,
    history: HistoryConfig,
    presentation: PresentationToml,
    strings: BTreeMap<String, String>,
    display: Table,
    runtime: RuntimeConfig,
    radar: RadarConfig,
}

impl Default for ConfigFile {
    fn default() -> Self {
        let CommonConfig {
            version,
            general,
            units,
            location,
            providers,
            history,
            presentation,
            strings,
        } = CommonConfig::default();
        Self {
            version,
            general,
            units,
            location,
            providers,
            history,
            presentation,
            strings,
            display: Table::new(),
            runtime: RuntimeConfig::default(),
            radar: RadarConfig::default(),
        }
    }
}

/// `[runtime]`: Linux-only paths and logging.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RuntimeConfig {
    /// Cache directory (default `$XDG_CACHE_HOME/whether`).
    pub cache_dir: Option<PathBuf>,
    /// Log file for interactive displays (default `$XDG_STATE_HOME/whether/whether.log`).
    pub log_file: Option<PathBuf>,
    /// `error`, `warn`, `info`, `debug` or `trace`.
    pub log_level: Option<String>,
}

/// `[radar]`: the radar imagery shown by the `radar` widget (Linux).
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RadarConfig {
    /// Map zoom, 6 (several states) to 10 (a county); 8 is about 360 km across.
    pub zoom: u8,
}

impl Default for RadarConfig {
    fn default() -> Self {
        Self { zoom: 8 }
    }
}

/// `[display]`: the default display and each display plugin's own table.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DisplaySection {
    /// The display used when none is named on the command line.
    pub default: Option<String>,
    /// External plugins (`exec:<name>`) that run alongside the main display.
    pub sidecars: Vec<String>,
    /// `[display.<name>]` tables by plugin name, validated by the plugins. External
    /// plugins live under `exec`, one table per name.
    pub plugins: BTreeMap<String, Table>,
}

/// Whether `name` is `exec:<name>` with a name of letters, digits, `-` and `_`.
pub fn is_exec_name(name: &str) -> bool {
    name.strip_prefix("exec:").is_some_and(|n| {
        !n.is_empty()
            && n.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    })
}

impl DisplaySection {
    fn from_table(table: Table) -> Result<Self, ConfigError> {
        let mut section = Self::default();
        for (key, value) in table {
            match (key.as_str(), value) {
                ("default", Value::String(name)) => section.default = Some(name),
                ("sidecars", Value::Array(items)) => {
                    for item in items {
                        match item {
                            Value::String(name) if is_exec_name(&name) => {
                                section.sidecars.push(name);
                            }
                            other => {
                                return Err(ConfigError::Display(format!(
                                    "display.sidecars entries must be \"exec:<name>\", not {other}"
                                )));
                            }
                        }
                    }
                }
                ("sidecars", other) => {
                    return Err(ConfigError::Display(format!(
                        "display.sidecars must be an array, not {}",
                        other.type_str()
                    )));
                }
                ("default", other) => {
                    return Err(ConfigError::Display(format!(
                        "display.default must be a display name, not {}",
                        other.type_str()
                    )));
                }
                (_, Value::Table(t)) => {
                    section.plugins.insert(key, t);
                }
                (_, other) => {
                    return Err(ConfigError::Display(format!(
                        "display.{key} must be a table, not {}",
                        other.type_str()
                    )));
                }
            }
        }
        Ok(section)
    }

    /// `[display.<name>]` without its `presentation` table, which the loader handles:
    /// what a display plugin validates and deserializes.
    pub fn plugin_table(&self, name: &str) -> Option<Table> {
        self.plugins.get(name).map(|t| {
            let mut t = t.clone();
            t.remove("presentation");
            t
        })
    }

    /// The parsed `[display.<name>.presentation]` overrides.
    pub fn presentation(&self, name: &str) -> Result<PresentationToml, ConfigError> {
        match self.plugins.get(name).and_then(|t| t.get("presentation")) {
            None => Ok(PresentationToml::default()),
            Some(value) => value.clone().try_into().map_err(|e: toml::de::Error| {
                ConfigError::Display(format!(
                    "display.{name}.presentation: {}",
                    e.message().trim()
                ))
            }),
        }
    }
}

/// How to load: an explicit file, the environment and command-line overrides.
pub struct LoadOptions<'a> {
    /// `--config`: replaces the system and user files.
    pub explicit_path: Option<&'a Path>,
    /// Environment lookups (injected so tests need no `set_var`).
    pub env: &'a dyn Fn(&str) -> Option<String>,
    /// Dotted keys and values from command-line flags, applied last.
    pub cli_overrides: Vec<(&'a str, Value)>,
}

impl fmt::Debug for LoadOptions<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LoadOptions")
            .field("explicit_path", &self.explicit_path)
            .field("cli_overrides", &self.cli_overrides)
            .finish_non_exhaustive()
    }
}

/// Environment variables that override configuration keys.
const ENV_OVERRIDES: &[(&str, &str)] = &[
    ("WHETHER_DISPLAY", "display.default"),
    ("WHETHER_UNITS", "general.units"),
    ("WHETHER_LOG", "runtime.log_level"),
];

/// The loaded Linux configuration.
#[derive(Clone, Debug)]
pub struct LinuxConfig {
    /// The sections shared with the firmware.
    pub common: CommonConfig,
    /// `[display]`.
    pub display: DisplaySection,
    /// `[runtime]`.
    pub runtime: RuntimeConfig,
    /// `[radar]`.
    pub radar: RadarConfig,
    /// The files read, lowest precedence first.
    pub sources: Vec<PathBuf>,
    /// Validation warnings.
    pub issues: Vec<Issue>,
    /// The locale that decides `units = "auto"`.
    pub measurement_locale: Option<String>,
    /// `WHETHER_LOCATION`: the location name to show first.
    pub location_override: Option<String>,
}

impl LinuxConfig {
    /// The history every display's presentation needs, for any location
    ///; empty when none shows history.
    pub fn history_requirements(&self) -> whether_core::requirements::HistoryRequirements {
        use whether_core::requirements::HistoryRequirements;
        core::iter::once("")
            .chain(self.display.plugins.keys().map(String::as_str))
            .filter_map(|name| self.presentation_for(name).ok())
            .flat_map(|p| {
                let mut each = vec![HistoryRequirements::from_presentation(&p)];
                each.extend(
                    self.common.location.iter().map(|l| {
                        HistoryRequirements::from_presentation(&crate::for_location(&p, l))
                    }),
                );
                each
            })
            .fold(HistoryRequirements::default(), HistoryRequirements::union)
    }

    /// Whether any presentation (the shared one or a display's) shows `kind`.
    pub fn shows_anywhere(&self, kind: whether_core::presentation::WidgetKind) -> bool {
        core::iter::once("")
            .chain(self.display.plugins.keys().map(String::as_str))
            .filter_map(|name| self.presentation_for(name).ok())
            .any(|p| {
                p.shows(kind)
                    || self
                        .common
                        .location
                        .iter()
                        .any(|l| crate::for_location(&p, l).shows(kind))
            })
    }

    /// The presentation for `display`: `[presentation]` with
    /// `[display.<display>.presentation]` on top, resolved.
    pub fn presentation_for(&self, display: &str) -> Result<PresentationConfig, ConfigError> {
        let over = self.display.presentation(display)?;
        let merged = self.common.presentation.overlay(&over);
        Ok(resolve_presentation(
            &self.common.general,
            &self.common.units,
            &merged,
            self.measurement_locale.as_deref(),
        ))
    }
}

/// Reads `path`; `Ok(None)` when it does not exist and `optional` is set.
fn read(path: &Path, optional: bool) -> Result<Option<String>, ConfigError> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if optional && e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(ConfigError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Loads, layers, overrides and validates the configuration. Validation errors
/// abort; warnings are kept in [`LinuxConfig::issues`].
pub fn load(opts: &LoadOptions<'_>) -> Result<LinuxConfig, ConfigError> {
    let candidates = match opts.explicit_path {
        Some(path) => vec![(path.to_path_buf(), false)],
        None => config_layers(opts.env)
            .into_iter()
            .map(|p| (p, true))
            .collect(),
    };
    let mut merged = Table::new();
    let mut sources = Vec::new();
    for (path, optional) in candidates {
        let Some(text) = read(&path, optional)? else {
            continue;
        };
        // Deserialize each file on its own so errors point at its lines.
        if let Err(e) = toml::from_str::<ConfigFile>(&text) {
            return Err(ConfigError::parse(path, &text, &e));
        }
        let table: Table = text
            .parse()
            .map_err(|e| ConfigError::parse(path.clone(), &text, &e))?;
        deep_merge(&mut merged, table);
        sources.push(path);
    }
    finish(merged, sources, opts)
}

/// Loads one configuration document, without the environment or other files (for
/// tests and previews).
pub fn parse(text: &str) -> Result<LinuxConfig, ConfigError> {
    let path = PathBuf::from("<text>");
    if let Err(e) = toml::from_str::<ConfigFile>(text) {
        return Err(ConfigError::parse(path, text, &e));
    }
    let table: Table = text
        .parse()
        .map_err(|e| ConfigError::parse(path.clone(), text, &e))?;
    let opts = LoadOptions {
        explicit_path: None,
        env: &|_| None,
        cli_overrides: Vec::new(),
    };
    finish(table, vec![path], &opts)
}

/// Applies the overrides to the merged files and validates the result.
fn finish(
    mut merged: Table,
    sources: Vec<PathBuf>,
    opts: &LoadOptions<'_>,
) -> Result<LinuxConfig, ConfigError> {
    for (var, key) in ENV_OVERRIDES {
        if let Some(value) = (opts.env)(var).filter(|v| !v.is_empty()) {
            set_dotted(&mut merged, key, Value::String(value)).map_err(ConfigError::Override)?;
        }
    }
    for (key, value) in &opts.cli_overrides {
        set_dotted(&mut merged, key, value.clone()).map_err(ConfigError::Override)?;
    }
    let file: ConfigFile = Value::Table(merged)
        .try_into()
        .map_err(|e: toml::de::Error| ConfigError::Override(e.message().trim().to_string()))?;

    let ConfigFile {
        version,
        general,
        units,
        location,
        providers,
        history,
        presentation,
        strings,
        display,
        runtime,
        radar,
    } = file;
    let common = CommonConfig {
        version,
        general,
        units,
        location,
        providers,
        history,
        presentation,
        strings,
    };
    let display = DisplaySection::from_table(display)?;
    let mut issues = common.validate();
    if !(6..=10).contains(&radar.zoom) {
        issues.push(Issue {
            severity: crate::validate::Severity::Error,
            path: "radar.zoom".into(),
            message: format!("{} is outside 6..=10", radar.zoom),
        });
    }
    for name in display.plugins.keys() {
        let over = display.presentation(name)?;
        let prefix = format!("display.{name}.presentation");
        issues.extend(validate_presentation(&over, &prefix, &common.providers));
    }
    if has_errors(&issues) {
        return Err(ConfigError::Invalid(issues));
    }
    Ok(LinuxConfig {
        common,
        display,
        runtime,
        radar,
        sources,
        issues,
        measurement_locale: measurement_locale(opts.env),
        location_override: (opts.env)("WHETHER_LOCATION").filter(|v| !v.is_empty()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use whether_core::units::{UnitPrefs, UnitSystem};

    use crate::duration::DurationStr;

    const LOCATION: &str = "[[location]]\nname = \"New York\"\nzip = \"10001\"\ncountry = \"US\"\n";

    #[test]
    fn history_is_needed_only_where_shown() {
        let none = crate::parse(LOCATION).unwrap();
        assert!(none.history_requirements().is_empty());
        let card = crate::parse(&format!(
            "{LOCATION}[presentation]\ndetails = [\"records\"]\n"
        ))
        .unwrap();
        let r = card.history_requirements();
        assert_eq!(
            (r.past_days, r.climate_days, r.records),
            (0, 7, true),
            "with the daily normals"
        );
        let tui = crate::parse(&format!(
            "{LOCATION}[display.tui.presentation]\nwidgets = [\"history\"]\nhistory_days = 10\n"
        ))
        .unwrap();
        let r = tui.history_requirements();
        assert_eq!(
            (r.past_days, r.month_to_date),
            (10, true),
            "one display is enough"
        );
    }

    /// A temporary XDG tree: `sys/whether/config.toml` and `home/.config/whether/config.toml`.
    struct Tree {
        dir: tempfile::TempDir,
        vars: Vec<(String, String)>,
    }

    impl Tree {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().to_str().unwrap().to_string();
            let vars = vec![
                ("HOME".to_string(), format!("{root}/home")),
                ("XDG_CONFIG_DIRS".to_string(), format!("{root}/sys")),
            ];
            Self { dir, vars }
        }

        fn write(&self, rel: &str, text: &str) -> PathBuf {
            let path = self.dir.path().join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, text).unwrap();
            path
        }

        fn system(&self, text: &str) -> PathBuf {
            self.write("sys/whether/config.toml", text)
        }

        fn user(&self, text: &str) -> PathBuf {
            self.write("home/.config/whether/config.toml", text)
        }

        fn set(&mut self, var: &str, value: &str) {
            self.vars.push((var.to_string(), value.to_string()));
        }

        fn load_with(
            &self,
            explicit: Option<&Path>,
            cli: Vec<(&str, Value)>,
        ) -> Result<LinuxConfig, ConfigError> {
            let env = |k: &str| {
                self.vars
                    .iter()
                    .rev()
                    .find(|(name, _)| name == k)
                    .map(|(_, v)| v.clone())
            };
            load(&LoadOptions {
                explicit_path: explicit,
                env: &env,
                cli_overrides: cli,
            })
        }

        fn load(&self) -> Result<LinuxConfig, ConfigError> {
            self.load_with(None, vec![])
        }
    }

    #[test]
    fn system_user_and_cli_precedence() {
        let tree = Tree::new();
        let sys = tree.system(&format!(
            "{LOCATION}[general]\nrefresh = \"30m\"\nlanguage = \"de\"\ntime_format = \"12h\""
        ));
        let user = tree.user("[general]\nrefresh = \"20m\"\nlanguage = \"fr\"");
        let c = tree
            .load_with(None, vec![("general.language", Value::String("es".into()))])
            .unwrap();
        assert_eq!(c.sources, vec![sys, user]);
        assert_eq!(
            c.common.general.refresh,
            DurationStr(1200),
            "user over system"
        );
        assert_eq!(c.common.general.language, "es", "CLI over user");
        assert_eq!(
            c.common.general.time_format,
            whether_core::presentation::TimeFormat::H12,
            "system kept"
        );
        assert_eq!(c.common.location.len(), 1);
    }

    #[test]
    fn explicit_path_replaces_both_layers() {
        let tree = Tree::new();
        tree.system("[general]\nlanguage = \"de\"");
        tree.user("[general]\nlanguage = \"fr\"");
        let explicit = tree.write("other.toml", LOCATION);
        let c = tree.load_with(Some(&explicit), vec![]).unwrap();
        assert_eq!(c.sources, vec![explicit]);
        assert_eq!(c.common.general.language, "en");
        let missing = tree.dir.path().join("missing.toml");
        assert!(matches!(
            tree.load_with(Some(&missing), vec![]),
            Err(ConfigError::Io { .. })
        ));
    }

    #[test]
    fn arrays_replace_across_layers() {
        let tree = Tree::new();
        tree.system(&format!(
            "{LOCATION}{}",
            LOCATION.replace("New York", "Brooklyn")
        ));
        tree.user("[[location]]\nname = \"Berlin\"\nlatitude = 52.52\nlongitude = 13.41");
        let c = tree.load().unwrap();
        let names: Vec<_> = c
            .common
            .location
            .iter()
            .map(|l| l.name.clone().unwrap())
            .collect();
        assert_eq!(names, vec!["Berlin"]);
    }

    #[test]
    fn unknown_keys_report_file_and_line() {
        let tree = Tree::new();
        let user = tree.user(&format!("{LOCATION}[general]\nrefersh = \"15m\"\n"));
        let err = tree.load().unwrap_err();
        let shown = err.to_string();
        assert!(
            shown.starts_with(&format!("{}:6:1: unknown field `refersh`", user.display())),
            "{shown}"
        );
        assert!(shown.contains("\n  refersh = \"15m\"\n  ^"), "{shown}");
    }

    #[test]
    fn type_errors_report_file_and_line() {
        let tree = Tree::new();
        tree.system("[general]\nrefresh = 15\n");
        let err = tree.load().unwrap_err();
        assert!(
            matches!(
                &err,
                ConfigError::Parse {
                    position: Some((2, _)),
                    ..
                }
            ),
            "{err}"
        );
        assert!(err.to_string().contains("duration"), "{err}");
    }

    #[test]
    fn env_overrides() {
        let mut tree = Tree::new();
        tree.user(LOCATION);
        tree.set("WHETHER_UNITS", "imperial");
        tree.set("WHETHER_DISPLAY", "gui");
        tree.set("WHETHER_LOG", "debug");
        tree.set("WHETHER_LOCATION", "New York");
        let c = tree.load().unwrap();
        assert_eq!(c.common.general.units, UnitSystem::Imperial);
        assert_eq!(c.display.default.as_deref(), Some("gui"));
        assert_eq!(c.runtime.log_level.as_deref(), Some("debug"));
        assert_eq!(c.location_override.as_deref(), Some("New York"));
        tree.set("WHETHER_UNITS", "british");
        let err = tree.load().unwrap_err();
        assert!(
            matches!(err, ConfigError::Override(ref m) if m.contains("british")),
            "{err}"
        );
    }

    #[test]
    fn auto_units_follow_the_injected_locale() {
        let mut tree = Tree::new();
        tree.user(LOCATION);
        tree.set("LANG", "en_US.UTF-8");
        assert_eq!(
            tree.load().unwrap().presentation_for("tui").unwrap().units,
            UnitPrefs::IMPERIAL
        );
        tree.set("LC_ALL", "de_DE.UTF-8");
        assert_eq!(
            tree.load().unwrap().presentation_for("tui").unwrap().units,
            UnitPrefs::METRIC
        );
    }

    #[test]
    fn display_presentation_overrides() {
        let tree = Tree::new();
        tree.user(&format!(
            "{LOCATION}[presentation]\nhourly_hours = 24\ndaily_days = 5\n\
             [display]\ndefault = \"tui\"\n[display.tui]\nlayout = \"wide\"\n\
             [display.tui.presentation]\nhourly_hours = 48\n"
        ));
        let c = tree.load().unwrap();
        let tui = c.presentation_for("tui").unwrap();
        let gui = c.presentation_for("gui").unwrap();
        assert_eq!((tui.hourly_hours, tui.daily_days), (48, 5));
        assert_eq!((gui.hourly_hours, gui.daily_days), (24, 5));
        assert_eq!(c.display.default.as_deref(), Some("tui"));
        assert_eq!(
            c.display.plugins["tui"]
                .get("layout")
                .and_then(Value::as_str),
            Some("wide")
        );
    }

    #[test]
    fn sidecars_name_external_plugins() {
        let tree = Tree::new();
        tree.user(&format!(
            "{LOCATION}[display]\ndefault = \"exec:wall\"\nsidecars = [\"exec:notify\", \"exec:led_2\"]\n[display.exec.notify]\ncommand = [\"whether-notify\"]\n"
        ));
        let c = tree.load().unwrap();
        assert_eq!(c.display.sidecars, ["exec:notify", "exec:led_2"]);
        assert_eq!(c.display.default.as_deref(), Some("exec:wall"));
        assert!(
            c.display
                .plugin_table("exec")
                .unwrap()
                .contains_key("notify")
        );
        for bad in [
            "sidecars = [\"line\"]",
            "sidecars = [\"exec:\"]",
            "sidecars = \"exec:x\"",
            "sidecars = [\"exec:a b\"]",
        ] {
            let tree = Tree::new();
            tree.user(&format!("{LOCATION}[display]\n{bad}\n"));
            assert!(tree.load().is_err(), "{bad}");
        }
    }

    #[test]
    fn plugin_tables_leave_out_presentation() {
        let tree = Tree::new();
        tree.user(&format!(
            "{LOCATION}[display.line]\nformat = \"{{temp}}\"\n[display.line.presentation]\nhourly_hours = 48\n"
        ));
        let c = tree.load().unwrap();
        let table = c.display.plugin_table("line").unwrap();
        assert!(table.get("presentation").is_none() && table.get("format").is_some());
        assert_eq!(c.presentation_for("line").unwrap().hourly_hours, 48);
        assert!(c.display.plugin_table("tui").is_none());
    }

    #[test]
    fn display_tables_are_checked() {
        let tree = Tree::new();
        tree.user(&format!("{LOCATION}[display]\ntui = 3\n"));
        assert!(matches!(tree.load(), Err(ConfigError::Display(_))));
        tree.user(&format!(
            "{LOCATION}[display.tui.presentation]\nhourly = 3\n"
        ));
        assert!(matches!(tree.load(), Err(ConfigError::Display(m)) if m.contains("unknown field")));
        tree.user(&format!(
            "{LOCATION}[display.tui.presentation]\nhourly_hours = 0\n"
        ));
        let Err(ConfigError::Invalid(issues)) = tree.load() else {
            panic!()
        };
        assert_eq!(issues[0].path, "display.tui.presentation.hourly_hours");
    }

    #[test]
    fn validation_errors_abort_and_warnings_stay() {
        let tree = Tree::new();
        let err = tree.load().unwrap_err();
        assert!(err.to_string().contains("whether locate"), "{err}");
        tree.user(&format!("{LOCATION}[general]\nrefresh = \"1m\"\n"));
        let c = tree.load().unwrap();
        assert_eq!(c.issues.len(), 1);
        assert_eq!(c.issues[0].path, "general.refresh");
    }

    #[test]
    fn cli_override_paths_are_checked() {
        let tree = Tree::new();
        tree.user(LOCATION);
        let err = tree
            .load_with(None, vec![("version.x", Value::Integer(1))])
            .unwrap_err();
        assert!(matches!(err, ConfigError::Override(_)));
    }

    #[test]
    fn parsed_text_and_widgets_shown_anywhere() {
        use whether_core::presentation::WidgetKind;
        let place = "[[location]]\nname = \"New York\"\nlatitude = 40.7\nlongitude = -74.0\n";
        let cfg = parse(&format!(
            "{place}[display.gui.presentation]\nwidgets = [\"current\", \"radar\"]\n"
        ))
        .unwrap();
        assert!(cfg.shows_anywhere(WidgetKind::Radar), "the GUI's override");
        assert!(!cfg.shows_anywhere(WidgetKind::Moon));
        assert_eq!(
            cfg.display
                .presentation("gui")
                .unwrap()
                .widgets
                .map(|w| w.len()),
            Some(2)
        );
        assert!(
            parse(&format!("{place}[radar]\nzoom = 3\n")).is_err(),
            "validated"
        );
        assert!(parse("general = 1").is_err());
    }
}
