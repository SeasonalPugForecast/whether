//! `[display.tui]`: layout, icons, graph style, colors and custom rows.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;
use core::str::FromStr;

use ratatui::style::Color;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use whether_config::{DurationStr, Issue, Severity};
use whether_core::presentation::WidgetKind;

/// Which arrangement to draw.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum LayoutPreset {
    /// The largest preset that fits.
    #[default]
    Auto,
    /// Full-screen terminals on large monitors (≥ 160×45).
    Wide,
    /// ≥ 100×30.
    Dashboard,
    /// ≥ 50×18; also the ESP32 panel layout.
    Compact,
    /// Anything smaller.
    Minimal,
    /// `[[display.tui.rows]]`.
    Custom,
}

/// Glyphs for condition icons.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum IconStyle {
    /// Nerd Font Weather Icons (needs a patched font).
    #[default]
    #[serde(rename = "nerd-font")]
    NerdFont,
    /// Emoji.
    #[serde(rename = "emoji")]
    Emoji,
    /// Short words.
    #[serde(rename = "ascii")]
    Ascii,
}

/// How the hourly chart draws points.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum GraphStyle {
    /// Braille dots (2×4 per cell).
    #[default]
    Braille,
    /// Full blocks.
    Block,
    /// One dot per cell.
    Dot,
    /// No chart.
    None,
}

/// Whether to use color.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum ColorMode {
    /// Color unless `NO_COLOR` is set or `TERM=dumb`.
    #[default]
    Auto,
    /// Always.
    Always,
    /// Never.
    Never,
}

/// The terminal's background, for contrast.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Background {
    /// Dark background.
    #[default]
    Dark,
    /// Light background.
    Light,
}

/// Columns of the hourly table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum HourlyColumn {
    Time,
    Icon,
    Temp,
    Feels,
    PrecipProb,
    Precip,
    Wind,
    Humidity,
    Uv,
}

/// `[display.tui.colors]`: color names (`cyan`, `darkgray`) or `#rrggbb`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(default, deny_unknown_fields)]
pub struct TuiColors {
    /// Borders, titles and highlights.
    pub accent: Option<String>,
    /// Cold temperatures.
    pub cold: Option<String>,
    /// Warm temperatures.
    pub warm: Option<String>,
    /// Hot temperatures.
    pub hot: Option<String>,
    /// Secondary text.
    pub muted: Option<String>,
}

impl TuiColors {
    /// Every configured color with its key.
    pub fn entries(&self) -> [(&'static str, Option<&str>); 5] {
        [
            ("accent", self.accent.as_deref()),
            ("cold", self.cold.as_deref()),
            ("warm", self.warm.as_deref()),
            ("hot", self.hot.as_deref()),
            ("muted", self.muted.as_deref()),
        ]
    }
}

/// A row height or cell width in a custom layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Size {
    /// A number of terminal cells.
    Cells(u16),
    /// A percentage of the available space (`"40%"`).
    Percent(u8),
    /// The remaining space (`"fill"`).
    Fill,
}

impl Serialize for Size {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Cells(n) => s.serialize_u16(*n),
            Self::Percent(p) => s.collect_str(&format_args!("{p}%")),
            Self::Fill => s.serialize_str("fill"),
        }
    }
}

impl<'de> Deserialize<'de> for Size {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct SizeVisitor;
        impl Visitor<'_> for SizeVisitor {
            type Value = Size;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a number of cells, \"NN%\" or \"fill\"")
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Size, E> {
                u16::try_from(v)
                    .map(Size::Cells)
                    .map_err(|_| E::custom("too many cells"))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Size, E> {
                u64::try_from(v)
                    .map_err(|_| E::custom("sizes are not negative"))
                    .and_then(|v| self.visit_u64(v))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Size, E> {
                if v == "fill" {
                    return Ok(Size::Fill);
                }
                v.strip_suffix('%')
                    .and_then(|p| p.parse::<u8>().ok())
                    .filter(|p| *p <= 100)
                    .map(Size::Percent)
                    .ok_or_else(|| E::custom(format!("`{v}` is not \"NN%\" (0–100) or \"fill\"")))
            }
        }
        d.deserialize_any(SizeVisitor)
    }
}

#[cfg(feature = "schema")]
impl schemars::JsonSchema for Size {
    fn schema_name() -> alloc::borrow::Cow<'static, str> {
        "Size".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "Terminal cells, a percentage such as \"40%\", or \"fill\".",
            "oneOf": [
                { "type": "integer", "minimum": 0, "maximum": 65535 },
                { "type": "string", "pattern": "^(100|[1-9]?[0-9])%$" },
                { "const": "fill" }
            ]
        })
    }
}

/// One widget in a custom row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CellSpec {
    /// The widget.
    pub kind: WidgetKind,
    /// Its width (default: an equal share).
    #[serde(default)]
    pub width: Option<Size>,
}

/// One row of a custom layout.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct RowSpec {
    /// The row's height.
    pub height: Size,
    /// Widgets from left to right.
    pub widgets: Vec<CellSpec>,
}

/// `[display.tui]`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(default, deny_unknown_fields)]
pub struct TuiConfig {
    /// Arrangement.
    pub layout: LayoutPreset,
    /// Icon glyphs.
    pub icons: IconStyle,
    /// Large ASCII-art icon in the current panel.
    pub art: bool,
    /// Chart marker, or no chart.
    pub graph: GraphStyle,
    /// Color use.
    pub color: ColorMode,
    /// Terminal background.
    pub background: Background,
    /// Periodic redraw for relative times.
    pub redraw: DurationStr,
    /// Mouse support (click a tab to select it).
    pub mouse: bool,
    /// Hourly table columns.
    pub hourly_columns: Vec<HourlyColumn>,
    /// Color overrides.
    pub colors: TuiColors,
    /// Custom layout rows (`layout = "custom"`).
    pub rows: Vec<RowSpec>,
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            layout: LayoutPreset::Auto,
            icons: IconStyle::NerdFont,
            art: true,
            graph: GraphStyle::Braille,
            color: ColorMode::Auto,
            background: Background::Dark,
            redraw: DurationStr(30),
            mouse: false,
            hourly_columns: vec![
                HourlyColumn::Time,
                HourlyColumn::Icon,
                HourlyColumn::Temp,
                HourlyColumn::PrecipProb,
                HourlyColumn::Wind,
            ],
            colors: TuiColors::default(),
            rows: Vec::new(),
        }
    }
}

fn error(path: &str, message: String) -> Issue {
    Issue {
        severity: Severity::Error,
        path: path.to_string(),
        message,
    }
}

impl TuiConfig {
    /// Semantic problems: a custom layout without rows, empty rows, unparsable colors.
    pub fn validate(&self) -> Vec<Issue> {
        let mut issues = Vec::new();
        if self.layout == LayoutPreset::Custom && self.rows.is_empty() {
            issues.push(error(
                "display.tui.rows",
                "layout = \"custom\" needs at least one [[display.tui.rows]]".into(),
            ));
        }
        for (i, row) in self.rows.iter().enumerate() {
            if row.widgets.is_empty() {
                issues.push(error(
                    &format!("display.tui.rows[{i}].widgets"),
                    "a row needs at least one widget".into(),
                ));
            }
        }
        for (key, value) in self.colors.entries() {
            if let Some(value) = value
                && Color::from_str(value).is_err()
            {
                issues.push(error(
                    &format!("display.tui.colors.{key}"),
                    format!("`{value}` is not a color name or #rrggbb"),
                ));
            }
        }
        issues
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<TuiConfig, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }

    #[test]
    fn example_section_is_the_default() {
        let example: toml::Table = whether_config::example::EXAMPLE.parse().unwrap();
        let mut tui = example["display"]["tui"].as_table().unwrap().clone();
        tui.remove("presentation");
        let cfg: TuiConfig = toml::Value::Table(tui).try_into().unwrap();
        assert_eq!(cfg, TuiConfig::default());
        assert_eq!(cfg.validate(), vec![]);
    }

    #[test]
    fn the_example_custom_layout_parses() {
        let cfg = parse(
            "layout = \"custom\"\n\
             [[rows]]\nheight = 10\nwidgets = [{ kind = \"current\", width = \"40%\" }, { kind = \"details\", width = \"fill\" }]\n\
             [[rows]]\nheight = \"fill\"\nwidgets = [{ kind = \"hourly\" }]\n",
        )
        .unwrap();
        assert_eq!(cfg.rows.len(), 2);
        assert_eq!(cfg.rows[0].height, Size::Cells(10));
        assert_eq!(cfg.rows[0].widgets[0].width, Some(Size::Percent(40)));
        assert_eq!(cfg.rows[1].height, Size::Fill);
        assert_eq!(
            cfg.rows[1].widgets[0],
            CellSpec {
                kind: WidgetKind::Hourly,
                width: None
            }
        );
        assert_eq!(cfg.validate(), vec![]);
        let round_trip: TuiConfig = toml::from_str(&toml::to_string(&cfg).unwrap()).unwrap();
        assert_eq!(round_trip, cfg);
    }

    #[test]
    fn sizes_reject_nonsense() {
        for bad in [
            "height = \"101%\"",
            "height = \"wide\"",
            "height = -1",
            "height = 70000",
        ] {
            let text = alloc::format!("[[rows]]\n{bad}\nwidgets = [{{ kind = \"current\" }}]\n");
            assert!(parse(&text).is_err(), "{bad}");
        }
    }

    #[test]
    fn validation() {
        let custom = TuiConfig {
            layout: LayoutPreset::Custom,
            ..TuiConfig::default()
        };
        assert_eq!(custom.validate()[0].path, "display.tui.rows");
        let mut colors = TuiConfig::default();
        colors.colors.accent = Some("cyan".into());
        colors.colors.hot = Some("#ff5f5f".into());
        assert_eq!(colors.validate(), vec![]);
        colors.colors.cold = Some("blurple".into());
        assert_eq!(colors.validate()[0].path, "display.tui.colors.cold");
        assert!(parse("colour = 1").is_err(), "unknown keys are rejected");
        let empty_row = parse("[[rows]]\nheight = 3\nwidgets = []\n").unwrap();
        assert_eq!(empty_row.validate()[0].path, "display.tui.rows[0].widgets");
    }
}
