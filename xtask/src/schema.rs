//! `cargo xtask schema [--check]`: writes `docs/config/whether.schema.json`, the
//! plugin protocol schema `sdk/schema/protocol-v1.schema.json` and the golden
//! protocol examples in `sdk/schema/examples/`.

use std::fs;
use std::path::PathBuf;

use whether_config::schema::{compose, core_schema, to_pretty_json};

/// The schema file referenced by the example configuration's `#:schema` line.
fn schema_path() -> PathBuf {
    crate::workspace_root().join("docs/config/whether.schema.json")
}

/// The composed schema with every display.
fn render() -> String {
    let displays = [whether_line::line_factory(), whether_line::json_factory()];
    let mut plugins: Vec<(&str, schemars::Schema)> =
        vec![("tui", schemars::schema_for!(whether_tui::TuiConfig))];
    plugins.push((
        "gui",
        schemars::schema_for!(whether_gui::desktop::config::GuiConfig),
    ));
    plugins.extend(displays.iter().map(|f| (f.name, (f.schema)())));
    plugins.push(("exec", whether_plugin_proto::exec::schema()));
    to_pretty_json(&compose(core_schema(), &plugins))
}

/// Every generated file: the config schema, the protocol schema and examples.
fn outputs() -> Vec<(PathBuf, String)> {
    let sdk = crate::workspace_root().join("sdk/schema");
    let mut out = vec![
        (schema_path(), render()),
        (
            sdk.join("protocol-v1.schema.json"),
            to_pretty_json(&whether_plugin_proto::schema()),
        ),
    ];
    for (name, text) in whether_plugin_proto::examples::examples() {
        out.push((sdk.join("examples").join(name), text));
    }
    out
}

pub(crate) fn run(args: &[&str]) -> Result<(), String> {
    let outputs = outputs();
    match args {
        [] => {
            for (path, text) in &outputs {
                if let Some(dir) = path.parent() {
                    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
                }
                fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))?;
                println!("wrote {} ({} bytes)", path.display(), text.len());
            }
            Ok(())
        }
        ["--check"] => {
            let stale: Vec<String> = outputs
                .iter()
                .filter(|(path, text)| fs::read_to_string(path).unwrap_or_default() != *text)
                .map(|(path, _)| path.display().to_string())
                .collect();
            if stale.is_empty() {
                println!("generated schemas and examples are up to date");
                Ok(())
            } else {
                Err(format!(
                    "stale: {}; run `cargo xtask schema`",
                    stale.join(", ")
                ))
            }
        }
        _ => Err("usage: cargo xtask schema [--check]".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_valid_json() {
        let value: serde_json::Value = serde_json::from_str(&render()).unwrap();
        assert_eq!(
            value.pointer("/$schema").and_then(|v| v.as_str()),
            Some("https://json-schema.org/draft/2020-12/schema")
        );
        assert!(
            value
                .pointer("/$defs/DisplayTables/properties/default")
                .is_some()
        );
        for (display, key) in [
            ("tui", "layout"),
            ("gui", "cycle_locations"),
            ("line", "format"),
            ("json", "compact"),
        ] {
            let pointer = format!("/$defs/DisplayTables/properties/{display}/properties/{key}");
            assert!(value.pointer(&pointer).is_some(), "{pointer}");
        }
    }

    #[test]
    fn generated_files_are_current() {
        assert_eq!(run(&["--check"]), Ok(()));
    }

    #[test]
    fn rejects_unknown_flags() {
        assert!(run(&["--frob"]).is_err());
    }
}
