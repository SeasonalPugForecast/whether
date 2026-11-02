//! The `[display.line]` and `[display.json]` tables.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use whether_config::{Issue, Severity};

use crate::template::Template;

/// Output format of the line display.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum LineOutput {
    /// The formatted text.
    #[default]
    Plain,
    /// One waybar JSON object per line (`text`, `tooltip`, `class`, `alt`).
    Waybar,
}

/// Which glyphs `{icon}` uses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum IconStyle {
    /// Emoji.
    #[default]
    #[serde(rename = "emoji")]
    Emoji,
    /// Nerd Font Weather Icons (needs a patched font).
    #[serde(rename = "nerd-font")]
    NerdFont,
    /// No icon.
    #[serde(rename = "none")]
    None,
}

/// `[display.line]`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct LineConfig {
    /// The text template.
    pub format: String,
    /// The tooltip template (waybar).
    pub tooltip: Option<String>,
    /// Plain text or waybar JSON.
    pub output: LineOutput,
    /// Glyphs for `{icon}`.
    pub icons: IconStyle,
    /// The location to show by name (default: the first).
    pub location: Option<String>,
}

impl Default for LineConfig {
    fn default() -> Self {
        Self {
            format: "{icon} {temp} {condition}".into(),
            tooltip: Some("{location}: {summary}".into()),
            output: LineOutput::Plain,
            icons: IconStyle::Emoji,
            location: None,
        }
    }
}

/// `[display.json]` (the CLI sets `snapshot` and `compact` from its flags).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct JsonConfig {
    /// Print the raw `WeatherSnapshot` instead of the `ViewModel`.
    pub snapshot: bool,
    /// One line instead of pretty JSON.
    pub compact: bool,
    /// The location to show by name (default: the first).
    pub location: Option<String>,
}

/// Parses a display table, or its defaults when absent.
pub(crate) fn parse<T: Default + for<'de> Deserialize<'de>>(
    section: Option<&toml::Table>,
) -> Result<T, String> {
    match section {
        None => Ok(T::default()),
        Some(table) => toml::Value::Table(table.clone())
            .try_into()
            .map_err(|e: toml::de::Error| e.message().trim().to_string()),
    }
}

fn error(path: &str, message: String) -> Issue {
    Issue {
        severity: Severity::Error,
        path: path.into(),
        message,
    }
}

/// Issues in `[display.line]`, including template errors.
pub fn validate_line(section: &toml::Table) -> Vec<Issue> {
    let cfg: LineConfig = match parse(Some(section)) {
        Ok(cfg) => cfg,
        Err(e) => return vec![error("display.line", e)],
    };
    let mut issues = Vec::new();
    if let Err(e) = Template::parse(&cfg.format) {
        issues.push(error("display.line.format", e.to_string()));
    }
    if let Some(Err(e)) = cfg.tooltip.as_deref().map(Template::parse) {
        issues.push(error("display.line.tooltip", e.to_string()));
    }
    issues
}

/// Issues in `[display.json]`.
pub fn validate_json(section: &toml::Table) -> Vec<Issue> {
    match parse::<JsonConfig>(Some(section)) {
        Ok(_) => Vec::new(),
        Err(e) => vec![error("display.json", e)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(text: &str) -> toml::Table {
        text.parse().unwrap()
    }

    #[test]
    fn example_section_is_the_default() {
        let example: toml::Table = whether_config::example::EXAMPLE.parse().unwrap();
        let line = example["display"]["line"].as_table().unwrap();
        assert_eq!(
            parse::<LineConfig>(Some(line)).unwrap(),
            LineConfig::default()
        );
        assert_eq!(validate_line(line), vec![]);
    }

    #[test]
    fn validation() {
        assert_eq!(
            validate_line(&table("output = \"waybar\"\nicons = \"nerd-font\"")),
            vec![]
        );
        let bad = validate_line(&table("format = \"{tmp}\"\ntooltip = \"{x\""));
        let paths: Vec<_> = bad.iter().map(|i| i.path.as_str()).collect();
        assert_eq!(paths, vec!["display.line.format", "display.line.tooltip"]);
        assert_eq!(validate_line(&table("colour = 1"))[0].path, "display.line");
        assert_eq!(validate_json(&table("compact = true")), vec![]);
        assert_eq!(validate_json(&table("pretty = true")).len(), 1);
    }
}
