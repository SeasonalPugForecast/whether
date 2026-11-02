//! `[display.gui]`.

use std::collections::BTreeMap;
use std::fmt;

use schemars::JsonSchema;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use whether_config::{DurationStr, Issue, Severity};

use super::glue::{DEFAULT_SKIES, FixedTheme, Sky, WindowOptions};

/// Which window to open.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum GuiMode {
    /// The Mousam-inspired desktop window.
    #[default]
    Desktop,
    /// The small-screen UI (same as the ESP32-S3).
    Compact,
}

/// A card of the desktop window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum Section {
    Current,
    Hourly,
    Daily,
    Details,
    Sun,
    AirQuality,
    Radar,
    Precipitation,
    /// The next hours' precipitation (the nowcast widget).
    Nowcast,
    /// Past days, normals and records (the history widget).
    History,
}

/// `"auto"` or a fixed factor.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum FontScale {
    /// 1.0 in a window; scales with the screen height full screen and in kiosk mode.
    #[default]
    Auto,
    /// A fixed factor.
    Fixed(f32),
}

impl Serialize for FontScale {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Auto => s.serialize_str("auto"),
            Self::Fixed(f) => s.serialize_f32(*f),
        }
    }
}

impl<'de> Deserialize<'de> for FontScale {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl Visitor<'_> for V {
            type Value = FontScale;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("\"auto\" or a number such as 1.25")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<FontScale, E> {
                if v == "auto" {
                    Ok(FontScale::Auto)
                } else {
                    Err(E::custom(format!("`{v}` is not \"auto\"")))
                }
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<FontScale, E> {
                Ok(FontScale::Fixed(v as f32))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<FontScale, E> {
                Ok(FontScale::Fixed(v as f32))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<FontScale, E> {
                Ok(FontScale::Fixed(v as f32))
            }
        }
        d.deserialize_any(V)
    }
}

impl JsonSchema for FontScale {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "FontScale".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "oneOf": [{ "const": "auto" }, { "type": "number", "minimum": 0.5, "maximum": 4.0 }] })
    }
}

/// One sky palette override: `"#rrggbb"` colors.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct SkyToml {
    /// Gradient top.
    pub top: Option<String>,
    /// Gradient bottom.
    pub bottom: Option<String>,
    /// Text color.
    pub text: Option<String>,
}

/// `[display.gui.layout]`: the cards' columns per window width. Each
/// tier is a list of columns, each a list of cards from top to bottom; cards that
/// `sections` shows but a tier leaves out go at the end of its last column.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct LayoutToml {
    /// Windows from 900 to 1600 px wide: up to 2 columns (forecast, details).
    pub standard: Option<Vec<Vec<Section>>>,
    /// 1600 px and wider: up to 3 columns.
    pub wide: Option<Vec<Vec<Section>>>,
}

/// The cards each tier shows, column by column, top to bottom.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardLayout {
    /// One column (narrow windows).
    pub narrow: Vec<Section>,
    /// Forecast and details columns.
    pub standard: [Vec<Section>; 2],
    /// Three columns.
    pub wide: [Vec<Section>; 3],
}

impl Default for CardLayout {
    fn default() -> Self {
        GuiConfig::default().cards()
    }
}

/// A card's column when the tier has no `[display.gui.layout]` entry: standard
/// (forecast left, the rest right) and wide (now, forecast, details).
fn home(section: Section) -> (usize, usize) {
    match section {
        Section::Current | Section::Nowcast => (0, 0),
        Section::Hourly | Section::Daily | Section::Precipitation => (0, 1),
        Section::Radar | Section::History | Section::Sun => (1, 0),
        Section::Details | Section::AirQuality => (1, 2),
    }
}

/// `columns` of shown cards: the configured ones in their order, then the shown
/// cards they leave out at the end of the last column.
fn arrange<const N: usize>(columns: &[Vec<Section>], shown: &[Section]) -> [Vec<Section>; N] {
    let mut out: [Vec<Section>; N] = core::array::from_fn(|_| Vec::new());
    for (i, column) in columns.iter().take(N).enumerate() {
        out[i] = column
            .iter()
            .filter(|s| shown.contains(s))
            .copied()
            .collect();
    }
    for s in shown {
        if !out.iter().any(|c| c.contains(s)) {
            out[N - 1].push(*s);
        }
    }
    out
}

/// How strong the sky gradients are; `palette` overrides apply on top.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SkyStyle {
    /// The palettes as designed.
    #[default]
    Default,
    /// Closer to gray: calmer on a wall display.
    Muted,
    /// More saturated.
    Vivid,
}

impl SkyStyle {
    /// `color` moved away from (vivid) or toward (muted) its gray.
    pub fn apply(self, [r, g, b]: [u8; 3]) -> [u8; 3] {
        let amount = match self {
            Self::Default => return [r, g, b],
            Self::Muted => -0.45,
            Self::Vivid => 0.35,
        };
        let gray = 0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let mix = |c: u8| {
            (f32::from(c) + (f32::from(c) - gray) * amount)
                .round()
                .clamp(0.0, 255.0) as u8
        };
        [mix(r), mix(g), mix(b)]
    }
}

/// The accent color when `accent` is not set.
pub const DEFAULT_ACCENT: [u8; 3] = [0xFF, 0xD1, 0x66];

/// `[display.gui]`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct GuiConfig {
    /// Window kind.
    pub mode: GuiMode,
    /// Initial window size (desktop mode).
    pub size: [u32; 2],
    /// Sky gradient follows conditions and time of day.
    pub dynamic_background: bool,
    /// Palette when the background is not dynamic.
    pub theme: FixedTheme,
    /// Sky gradients as designed, muted or vivid.
    pub sky_style: SkyStyle,
    /// Accent color (`#rrggbb`) for highlights, checkboxes and the sun; unset:
    /// sun yellow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accent: Option<String>,
    /// Desktop cards to show, in order (within each column of wider windows).
    pub sections: Vec<Section>,
    /// Per-tier columns (default: each card in its usual column).
    pub layout: LayoutToml,
    /// Full screen without decorations.
    pub kiosk: bool,
    /// Rotate through locations (`"0s"` = off).
    pub cycle_locations: DurationStr,
    /// Compact mode: turn the pages on a timer (`"0s"` = off), for screens
    /// without touch.
    pub page_interval: DurationStr,
    /// Font scale.
    pub font_scale: FontScale,
    /// Sky palette overrides by theme name (`clear_day`, `rain`, …).
    pub palette: BTreeMap<String, SkyToml>,
}

impl Default for GuiConfig {
    fn default() -> Self {
        Self {
            mode: GuiMode::Desktop,
            size: [1000, 720],
            dynamic_background: true,
            theme: FixedTheme::Auto,
            sky_style: SkyStyle::Default,
            accent: None,
            sections: vec![
                Section::Current,
                Section::Nowcast,
                Section::Hourly,
                Section::Precipitation,
                Section::Radar,
                Section::Details,
                Section::Daily,
                Section::History,
                Section::Sun,
                Section::AirQuality,
            ],
            layout: LayoutToml::default(),
            kiosk: false,
            cycle_locations: DurationStr(0),
            page_interval: DurationStr(0),
            font_scale: FontScale::Auto,
            palette: BTreeMap::new(),
        }
    }
}

/// The shortest page interval: anything faster is unreadable, and slow panels
/// (a Pi Zero redraws in about a second) would do little else.
pub const MIN_PAGE_INTERVAL_S: u32 = 5;

/// Palette names in `SkyTheme` order.
pub const SKY_NAMES: [&str; 9] = [
    "clear_day",
    "clear_night",
    "cloudy_day",
    "cloudy_night",
    "rain",
    "snow",
    "storm",
    "fog",
    "twilight",
];

/// `#rrggbb` as RGB.
pub fn parse_hex(text: &str) -> Option<[u8; 3]> {
    let hex = text.strip_prefix('#')?;
    if hex.len() != 6 || !hex.is_ascii() {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

impl GuiConfig {
    /// Problems serde cannot catch: palette names and colors, sizes and scales.
    pub fn validate(&self) -> Vec<Issue> {
        let error = |path: String, message: String| Issue {
            severity: Severity::Error,
            path,
            message,
        };
        let mut issues = Vec::new();
        for (name, sky) in &self.palette {
            if !SKY_NAMES.contains(&name.as_str()) {
                issues.push(error(
                    format!("display.gui.palette.{name}"),
                    format!("unknown sky; use one of {}", SKY_NAMES.join(", ")),
                ));
            }
            for (key, value) in [
                ("top", &sky.top),
                ("bottom", &sky.bottom),
                ("text", &sky.text),
            ] {
                if let Some(v) = value
                    && parse_hex(v).is_none()
                {
                    issues.push(error(
                        format!("display.gui.palette.{name}.{key}"),
                        format!("`{v}` is not #rrggbb"),
                    ));
                }
            }
        }
        if let Some(accent) = &self.accent
            && parse_hex(accent).is_none()
        {
            issues.push(error(
                "display.gui.accent".into(),
                format!("`{accent}` is not #rrggbb"),
            ));
        }
        if (1..MIN_PAGE_INTERVAL_S).contains(&self.page_interval.secs()) {
            issues.push(error(
                "display.gui.page_interval".into(),
                format!("at least \"{MIN_PAGE_INTERVAL_S}s\", or \"0s\" for off"),
            ));
        }
        for (tier, columns, max) in [
            ("standard", &self.layout.standard, 2),
            ("wide", &self.layout.wide, 3),
        ] {
            let Some(columns) = columns else { continue };
            let path = format!("display.gui.layout.{tier}");
            if columns.is_empty() || columns.len() > max {
                issues.push(error(path.clone(), format!("1 to {max} columns")));
            }
            let all: Vec<&Section> = columns.iter().flatten().collect();
            if let Some(twice) = all
                .iter()
                .enumerate()
                .find(|(i, s)| all[..*i].contains(s))
                .map(|(_, s)| **s)
            {
                let name =
                    toml::Value::try_from(twice).map_or_else(|_| String::new(), |v| v.to_string());
                issues.push(error(path, format!("{name} is listed twice")));
            }
        }
        if self.size[0] < 320 || self.size[1] < 240 {
            issues.push(error(
                "display.gui.size".into(),
                "at least [320, 240]".into(),
            ));
        }
        if let FontScale::Fixed(f) = self.font_scale
            && !(0.5..=4.0).contains(&f)
        {
            issues.push(error(
                "display.gui.font_scale".into(),
                format!("{f} is outside 0.5..=4.0"),
            ));
        }
        issues
    }

    /// The cards each tier shows: `sections` decides which and their
    /// order; `[display.gui.layout]` or each card's usual column decides where.
    pub fn cards(&self) -> CardLayout {
        let mut shown: Vec<Section> = Vec::new();
        for s in &self.sections {
            if !shown.contains(s) {
                shown.push(*s);
            }
        }
        let in_column = |tier: fn((usize, usize)) -> usize, n: usize| -> Vec<Vec<Section>> {
            (0..n)
                .map(|c| {
                    shown
                        .iter()
                        .filter(|s| tier(home(**s)) == c)
                        .copied()
                        .collect()
                })
                .collect()
        };
        let standard = self
            .layout
            .standard
            .clone()
            .unwrap_or_else(|| in_column(|h| h.0, 2));
        let wide = self
            .layout
            .wide
            .clone()
            .unwrap_or_else(|| in_column(|h| h.1, 3));
        CardLayout {
            standard: arrange(&standard, &shown),
            wide: arrange(&wide, &shown),
            narrow: shown,
        }
    }

    /// The window options for this config (`fullscreen`: kiosk or `--fullscreen`).
    pub fn window_options(&self, fullscreen: bool) -> WindowOptions {
        let mut skies: [Sky; 9] = DEFAULT_SKIES.map(|sky| Sky {
            top: self.sky_style.apply(sky.top),
            bottom: self.sky_style.apply(sky.bottom),
            ..sky
        });
        for (i, name) in SKY_NAMES.iter().enumerate() {
            if let Some(o) = self.palette.get(*name) {
                let sky = &mut skies[i];
                if let Some(c) = o.top.as_deref().and_then(parse_hex) {
                    sky.top = c;
                }
                if let Some(c) = o.bottom.as_deref().and_then(parse_hex) {
                    sky.bottom = c;
                }
                if let Some(c) = o.text.as_deref().and_then(parse_hex) {
                    sky.text = c;
                }
            }
        }
        WindowOptions {
            skies,
            dynamic_background: self.dynamic_background,
            theme: self.theme,
            auto_scale: self.font_scale == FontScale::Auto && (fullscreen || self.kiosk),
            fixed_scale: match self.font_scale {
                FontScale::Auto => 1.0,
                FontScale::Fixed(f) => f,
            },
            units_label: "°F".into(),
            cards: self.cards(),
            accent: self
                .accent
                .as_deref()
                .and_then(parse_hex)
                .unwrap_or(DEFAULT_ACCENT),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_section_is_the_default() {
        let example: toml::Table = whether_config::example::EXAMPLE.parse().unwrap();
        let mut gui = example["display"]["gui"].as_table().unwrap().clone();
        gui.remove("presentation");
        let cfg: GuiConfig = toml::Value::Table(gui).try_into().unwrap();
        assert_eq!(cfg, GuiConfig::default());
        assert_eq!(cfg.validate(), vec![]);
    }

    #[test]
    fn palettes_and_scales() {
        let cfg: GuiConfig = toml::from_str(
            "font_scale = 1.25\nkiosk = true\nsections = [\"current\", \"daily\"]\n[palette.rain]\ntop = \"#102030\"\n",
        )
        .unwrap();
        assert_eq!(cfg.validate(), vec![]);
        let opts = cfg.window_options(false);
        assert_eq!(opts.skies[4].top, [0x10, 0x20, 0x30]);
        assert!(!opts.auto_scale && (opts.fixed_scale - 1.25).abs() < 1e-6);
        assert_eq!(opts.cards.narrow, [Section::Current, Section::Daily]);
        assert_eq!(
            opts.cards.standard,
            [vec![Section::Current, Section::Daily], vec![]]
        );
        assert!(GuiConfig::default().window_options(true).auto_scale);
    }

    #[test]
    fn cards_follow_the_order_and_the_tier_layouts() {
        use Section::{
            AirQuality, Current, Daily, Details, History, Hourly, Nowcast, Precipitation, Radar,
            Sun,
        };
        let d = GuiConfig::default().cards();
        assert_eq!(
            d.narrow,
            [
                Current,
                Nowcast,
                Hourly,
                Precipitation,
                Radar,
                Details,
                Daily,
                History,
                Sun,
                AirQuality
            ]
        );
        assert_eq!(
            d.standard,
            [
                vec![Current, Nowcast, Hourly, Precipitation, Daily],
                vec![Radar, Details, History, Sun, AirQuality]
            ]
        );
        assert_eq!(
            d.wide,
            [
                vec![Current, Nowcast, Radar, History, Sun],
                vec![Hourly, Precipitation, Daily],
                vec![Details, AirQuality]
            ]
        );
        let cfg: GuiConfig = toml::from_str(
            "sections = [\"daily\", \"current\", \"sun\", \"radar\", \"daily\"]\n\
             [layout]\nstandard = [[\"sun\"], [\"current\", \"hourly\"]]\n",
        )
        .unwrap();
        assert_eq!(cfg.validate(), vec![]);
        let c = cfg.cards();
        assert_eq!(
            c.narrow,
            [Daily, Current, Sun, Radar],
            "once each, in order"
        );
        assert_eq!(
            c.standard,
            [vec![Sun], vec![Current, Daily, Radar]],
            "hidden cards dropped, left-out ones appended to the last column"
        );
        assert_eq!(c.wide, [vec![Current, Sun, Radar], vec![Daily], vec![]]);
    }

    #[test]
    fn sky_styles_and_accents() {
        assert_eq!(SkyStyle::Default.apply([10, 120, 240]), [10, 120, 240]);
        let [r, _, b] = SkyStyle::Muted.apply([10, 120, 240]);
        assert!(r > 10 && b < 240, "toward gray");
        let [r, _, b] = SkyStyle::Vivid.apply([10, 120, 240]);
        assert!(r < 10 && b == 255, "away from gray");
        assert_eq!(
            SkyStyle::Vivid.apply([90, 90, 90]),
            [90, 90, 90],
            "grays stay"
        );
        let cfg: GuiConfig = toml::from_str(
            "sky_style = \"muted\"\naccent = \"#4FC3F7\"\n[palette.rain]\ntop = \"#102030\"\n",
        )
        .unwrap();
        let opts = cfg.window_options(false);
        assert_eq!(opts.accent, [0x4F, 0xC3, 0xF7]);
        assert_eq!(opts.skies[4].top, [0x10, 0x20, 0x30], "overrides win");
        assert_eq!(
            opts.skies[0].top,
            SkyStyle::Muted.apply(DEFAULT_SKIES[0].top)
        );
        assert_eq!(
            GuiConfig::default().window_options(false).accent,
            DEFAULT_ACCENT
        );
    }

    #[test]
    fn validation() {
        let bad: GuiConfig = toml::from_str(
            "size = [100, 100]\nfont_scale = 9\npage_interval = \"2s\"\n\
             accent = \"gold\"\n\
             [palette.purple]\ntop = \"blue\"\n\
             [layout]\nstandard = [[\"sun\"], [\"hourly\"], [\"daily\"]]\nwide = [[\"sun\", \"radar\"], [\"sun\"]]\n",
        )
        .unwrap();
        let paths: Vec<_> = bad.validate().into_iter().map(|i| i.path).collect();
        assert_eq!(
            paths,
            vec![
                "display.gui.palette.purple",
                "display.gui.palette.purple.top",
                "display.gui.accent",
                "display.gui.page_interval",
                "display.gui.layout.standard",
                "display.gui.layout.wide",
                "display.gui.size",
                "display.gui.font_scale"
            ]
        );
        let rotating: GuiConfig = toml::from_str("page_interval = \"20s\"").unwrap();
        assert_eq!(
            (rotating.validate(), rotating.page_interval.secs()),
            (vec![], 20)
        );
        assert!(toml::from_str::<GuiConfig>("font_scale = \"huge\"").is_err());
        assert!(toml::from_str::<GuiConfig>("colour = 1").is_err());
    }

    #[test]
    fn hex_colors() {
        assert_eq!(parse_hex("#2F80ED"), Some([0x2F, 0x80, 0xED]));
        assert_eq!(parse_hex("2F80ED"), None);
        assert_eq!(parse_hex("#2F80E"), None);
    }
}
