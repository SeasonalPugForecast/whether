//! JSON Schema (2020-12) for configuration files, for editor completion and checks.

use std::collections::BTreeMap;

use schemars::{JsonSchema, Schema};
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::load::{RadarConfig, RuntimeConfig};
use crate::types::{
    CommonConfig, General, HistoryConfig, LocationConfig, PresentationToml, ProvidersConfig,
    UnitsOverride,
};

/// A whole configuration file, for the schema only (it mirrors `load::ConfigFile`).
#[derive(Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
#[schemars(title = "whether configuration")]
struct ConfigFileSchema {
    version: u32,
    general: General,
    units: UnitsOverride,
    location: Vec<LocationConfig>,
    providers: ProvidersConfig,
    history: HistoryConfig,
    presentation: PresentationToml,
    strings: BTreeMap<String, String>,
    display: DisplayTables,
    runtime: RuntimeConfig,
    radar: RadarConfig,
}

impl Default for ConfigFileSchema {
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
            display: DisplayTables::default(),
            runtime: RuntimeConfig::default(),
            radar: RadarConfig::default(),
        }
    }
}

/// `[display]`: `default` plus one table per display plugin (added by [`compose`]).
#[derive(Default, Serialize, JsonSchema)]
#[serde(default)]
struct DisplayTables {
    /// The display used when none is named on the command line.
    #[serde(skip_serializing_if = "Option::is_none")]
    default: Option<String>,
}

/// An external plugin's name (see [`crate::is_exec_name`]).
const EXEC_PATTERN: &str = "^exec:[A-Za-z0-9_-]+$";

/// Where [`DisplayTables`] lives in the schema.
const DISPLAY_DEF: &str = "/$defs/DisplayTables";

/// The schema of the core sections, `[display]` (without plugin tables) and `[runtime]`.
pub fn core_schema() -> Schema {
    let mut schema = schemars::schema_for!(ConfigFileSchema);
    without_nulls(schema.ensure_object());
    // Unknown display tables belong to plugins that are not compiled in.
    if let Some(display) = schema
        .pointer_mut(DISPLAY_DEF)
        .and_then(Value::as_object_mut)
    {
        display.insert("additionalProperties".into(), json!({ "type": "object" }));
    }
    schema
}

/// `core` with `[display.<name>]` described by each plugin's schema, and
/// `display.default` limited to the plugin names. Plugin definitions move into the
/// root `$defs` as `<name>.<Definition>` so names cannot collide.
pub fn compose(mut core: Schema, plugins: &[(&str, Schema)]) -> Schema {
    let mut properties = Map::new();
    for (name, plugin) in plugins {
        let mut plugin = plugin.clone();
        without_nulls(plugin.ensure_object());
        let mut plugin = plugin.to_value();
        prefix_refs(&mut plugin, name);
        if let Some(object) = plugin.as_object_mut() {
            object.remove("$schema");
            if let Some(Value::Object(defs)) = object.remove("$defs") {
                let root_defs = core
                    .ensure_object()
                    .entry("$defs")
                    .or_insert_with(|| Value::Object(Map::new()));
                if let Some(root_defs) = root_defs.as_object_mut() {
                    for (def, value) in defs {
                        root_defs.insert(format!("{name}.{def}"), value);
                    }
                }
            }
        }
        // Every display accepts `presentation` overrides (handled by the loader).
        if let Some(props) = plugin.get_mut("properties").and_then(Value::as_object_mut) {
            props.insert(
                "presentation".into(),
                json!({ "$ref": "#/$defs/PresentationToml" }),
            );
        }
        properties.insert((*name).to_string(), plugin);
    }
    let names: Vec<&str> = plugins.iter().map(|(name, _)| *name).collect();
    properties.insert(
        "default".into(),
        json!({
            "description": "The display used when none is named on the command line: a built-in, or an external plugin as `exec:<name>`.",
            "anyOf": [
                { "type": "string", "enum": names },
                { "type": "string", "pattern": EXEC_PATTERN }
            ]
        }),
    );
    properties.insert(
        "sidecars".into(),
        json!({
            "description": "External plugins (`exec:<name>`, each with a [display.exec.<name>] table) that run alongside the main display.",
            "type": "array",
            "items": { "type": "string", "pattern": EXEC_PATTERN }
        }),
    );
    if let Some(display) = core.pointer_mut(DISPLAY_DEF).and_then(Value::as_object_mut) {
        display.insert("properties".into(), Value::Object(properties));
    }
    core
}

/// TOML has no null: removes `null` from `type` lists and `anyOf` alternatives (from
/// `Option` fields), and null entries from defaults.
fn without_nulls(object: &mut Map<String, Value>) {
    if let Some(Value::Array(types)) = object.get_mut("type") {
        types.retain(|t| t != "null");
        if types.len() == 1 {
            let only = types.remove(0);
            object.insert("type".into(), only);
        }
    }
    let null = json!({ "type": "null" });
    if let Some(Value::Array(alternatives)) = object.get_mut("anyOf") {
        alternatives.retain(|a| *a != null);
        if alternatives.len() == 1 {
            let only = alternatives.remove(0);
            object.remove("anyOf");
            if let Value::Object(inner) = only {
                for (key, value) in inner {
                    object.entry(key).or_insert(value);
                }
            }
        }
    }
    match object.get_mut("default") {
        Some(Value::Null) => {
            object.remove("default");
        }
        Some(default) => drop_null_entries(default),
        None => {}
    }
    for value in object.values_mut() {
        match value {
            Value::Object(inner) => without_nulls(inner),
            Value::Array(items) => {
                for item in items.iter_mut().filter_map(Value::as_object_mut) {
                    without_nulls(item);
                }
            }
            _ => {}
        }
    }
}

fn drop_null_entries(value: &mut Value) {
    match value {
        Value::Object(object) => {
            object.retain(|_, v| !v.is_null());
            object.values_mut().for_each(drop_null_entries);
        }
        Value::Array(items) => items.iter_mut().for_each(drop_null_entries),
        _ => {}
    }
}

/// Rewrites `#/$defs/X` references to `#/$defs/<name>.X`.
fn prefix_refs(value: &mut Value, name: &str) {
    match value {
        Value::Object(object) => {
            for (key, inner) in object.iter_mut() {
                match inner {
                    Value::String(target) if key == "$ref" => {
                        if let Some(def) = target.strip_prefix("#/$defs/") {
                            *target = format!("#/$defs/{name}.{def}");
                        }
                    }
                    _ => prefix_refs(inner, name),
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|item| prefix_refs(item, name)),
        _ => {}
    }
}

/// Pretty JSON with sorted keys and a trailing newline, for files and snapshots.
pub fn to_pretty_json(schema: &Schema) -> String {
    // serde_json's Map is a BTreeMap (no preserve_order), so keys come out sorted.
    let mut text = serde_json::to_string_pretty(schema.as_value()).unwrap_or_default();
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_schema_snapshot() {
        insta::assert_snapshot!(to_pretty_json(&core_schema()));
    }

    #[test]
    fn core_sections_reject_unknown_keys() {
        let schema = core_schema();
        assert_eq!(schema.pointer("/additionalProperties"), Some(&json!(false)));
        assert_eq!(
            schema.pointer("/$defs/General/additionalProperties"),
            Some(&json!(false))
        );
        assert_eq!(
            schema.pointer("/$defs/DisplayTables/additionalProperties/type"),
            Some(&json!("object"))
        );
        assert_eq!(
            schema
                .pointer("/$defs/General/properties/refresh/pattern")
                .and_then(Value::as_str),
            Some("^(\\d+h)?(\\d+m)?(\\d+s)?$")
        );
        assert!(schema.pointer("/$defs/Secret/oneOf").is_some());
    }

    #[test]
    fn no_nulls_for_toml() {
        let text = to_pretty_json(&core_schema());
        assert!(!text.contains("null"), "{text}");
        let schema = core_schema();
        let openweather = schema
            .pointer("/$defs/ProvidersConfig/properties/openweather")
            .unwrap();
        assert_eq!(
            openweather.pointer("/$ref"),
            Some(&json!("#/$defs/OpenWeatherToml"))
        );
        assert_eq!(openweather.pointer("/default/api"), Some(&json!("free")));
        assert!(openweather.pointer("/default/base_url").is_none());
    }

    #[test]
    fn compose_adds_plugin_tables() {
        #[derive(JsonSchema)]
        #[allow(dead_code)]
        struct Tui {
            layout: Layout,
        }
        #[derive(JsonSchema)]
        #[allow(dead_code)]
        enum Layout {
            Wide,
            Compact,
        }
        let tui = schemars::schema_for!(Tui);
        let gui = schemars::json_schema!({ "type": "object" });
        let schema = compose(core_schema(), &[("tui", tui), ("gui", gui)]);
        let display = schema.pointer("/$defs/DisplayTables/properties").unwrap();
        assert_eq!(
            display.pointer("/default/anyOf/0/enum"),
            Some(&json!(["tui", "gui"]))
        );
        assert_eq!(
            display.pointer("/sidecars/items/pattern"),
            Some(&json!(EXEC_PATTERN))
        );
        assert_eq!(
            display.pointer("/tui/properties/layout/$ref"),
            Some(&json!("#/$defs/tui.Layout"))
        );
        assert!(
            display.pointer("/tui/$schema").is_none() && display.pointer("/tui/$defs").is_none()
        );
        assert!(schema.pointer("/$defs/tui.Layout").is_some());
        assert_eq!(display.pointer("/gui"), Some(&json!({ "type": "object" })));
        assert_eq!(
            display.pointer("/tui/properties/presentation/$ref"),
            Some(&json!("#/$defs/PresentationToml"))
        );
    }

    #[test]
    fn pretty_json_is_stable() {
        assert_eq!(
            to_pretty_json(&core_schema()),
            to_pretty_json(&core_schema())
        );
        assert!(to_pretty_json(&core_schema()).ends_with("}\n"));
    }
}
