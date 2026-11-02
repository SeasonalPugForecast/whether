//! Turning the application state into a line, waybar JSON or JSON documents.

use whether_core::glyph;
use whether_core::presentation::{DetailKind, PresentationConfig, WidgetKind};
use whether_core::strings::Strings;
use whether_core::time::UnixTime;
use whether_core::view::ViewModel;
use whether_display::{AppState, view_for};

use crate::config::{IconStyle, LineConfig, LineOutput};
use crate::template::{Field, Template};

/// Why there is nothing to show; the app turns these into exit codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum NoData {
    /// Nothing cached and nothing fetched (e.g. offline with an empty cache): exit 3.
    #[error("no weather data available yet")]
    Empty,
    /// Every provider failed and nothing is cached: exit 4.
    #[error("every provider failed and nothing is cached")]
    AllFailed,
}

impl NoData {
    /// The process exit code.
    pub fn exit_code(self) -> i32 {
        match self {
            Self::Empty => 3,
            Self::AllFailed => 4,
        }
    }
}

/// The location index for a configured name, else `fallback`.
pub fn location_index(state: &AppState, name: Option<&str>, fallback: usize) -> usize {
    name.and_then(|n| {
        state
            .locations
            .iter()
            .position(|l| l.name.eq_ignore_ascii_case(n))
    })
    .unwrap_or(fallback)
    .min(state.locations.len().saturating_sub(1))
}

/// Fails when location `i` has no snapshot.
pub fn require_data(state: &AppState, i: usize) -> Result<(), NoData> {
    match state.locations.get(i) {
        Some(l) if l.snapshot.is_some() => Ok(()),
        Some(l) if l.status.error.is_some() => Err(NoData::AllFailed),
        _ => Err(NoData::Empty),
    }
}

/// The presentation with every section the line's placeholders read.
pub fn line_presentation(base: &PresentationConfig) -> PresentationConfig {
    let mut p = base.clone();
    for widget in [
        WidgetKind::Current,
        WidgetKind::Summary,
        WidgetKind::Hourly,
        WidgetKind::Details,
        WidgetKind::AirQuality,
        WidgetKind::Sun,
    ] {
        if !p.widgets.contains(&widget) {
            p.widgets.push(widget);
        }
    }
    p.details = vec![
        DetailKind::Wind,
        DetailKind::Gusts,
        DetailKind::Humidity,
        DetailKind::DewPoint,
        DetailKind::Pressure,
        DetailKind::Uv,
        DetailKind::Visibility,
        DetailKind::CloudCover,
        DetailKind::Precipitation,
        DetailKind::FeelsLike,
    ];
    p
}

/// Placeholder values from a view model.
#[derive(Debug)]
pub struct Values<'a> {
    vm: &'a ViewModel,
    icons: IconStyle,
    missing: &'a str,
}

impl<'a> Values<'a> {
    /// Values from `vm`, with `missing` for unavailable ones.
    pub fn new(vm: &'a ViewModel, icons: IconStyle, missing: &'a str) -> Self {
        Self { vm, icons, missing }
    }

    fn detail(&self, kind: DetailKind) -> String {
        self.vm
            .details
            .iter()
            .find(|d| d.kind == kind)
            .map_or_else(|| self.missing.to_string(), |d| d.value.clone())
    }

    fn or_missing(&self, value: Option<&str>) -> String {
        value.map_or_else(|| self.missing.to_string(), str::to_string)
    }

    /// The text for `field`.
    pub fn get(&self, field: Field) -> String {
        let vm = self.vm;
        let current = vm.current.as_ref();
        let hour = vm.hourly.as_ref().and_then(|h| h.hours.get(h.now_index));
        match field {
            Field::Location => vm.location.name.clone(),
            Field::Temp => self.or_missing(current.map(|c| c.temperature.text.as_str())),
            Field::Feels => self.or_missing(current.map(|c| c.feels_like.text.as_str())),
            Field::Hi => self.or_missing(current.map(|c| c.high.text.as_str())),
            Field::Lo => self.or_missing(current.map(|c| c.low.text.as_str())),
            Field::Condition => self.or_missing(current.map(|c| c.condition_label.as_str())),
            Field::Icon => match (current, self.icons) {
                (_, IconStyle::None) | (None, _) => String::new(),
                (Some(c), IconStyle::Emoji) => glyph::emoji(c.icon).to_string(),
                (Some(c), IconStyle::NerdFont) => glyph::nerd_font(c.icon).to_string(),
            },
            Field::Summary => self.or_missing(vm.summary.as_deref()),
            Field::Wind => self.detail(DetailKind::Wind),
            Field::WindDir => self.or_missing(hour.map(|h| h.wind.compass)),
            Field::Humidity => self.detail(DetailKind::Humidity),
            Field::Pressure => self.detail(DetailKind::Pressure),
            Field::Uv => self.detail(DetailKind::Uv),
            Field::Aqi => vm
                .air_quality
                .as_ref()
                .map_or_else(|| self.missing.to_string(), |a| a.index.to_string()),
            Field::AqiLabel => {
                self.or_missing(vm.air_quality.as_ref().map(|a| a.category_label.as_str()))
            }
            Field::PrecipProb => hour
                .and_then(|h| h.precip_probability)
                .map_or_else(|| self.missing.to_string(), |p| format!("{p}%")),
            Field::Sunrise => self.or_missing(vm.sun.as_ref().map(|s| s.sunrise.as_str())),
            Field::Sunset => self.or_missing(vm.sun.as_ref().map(|s| s.sunset.as_str())),
            Field::Updated => vm.status.updated.clone(),
            Field::Provider => vm.status.provider.clone(),
            // No alert is nothing to show, not a missing value.
            Field::Alert => vm
                .alerts
                .first()
                .map(|a| a.title.clone())
                .unwrap_or_default(),
            Field::NormalHigh | Field::NormalLow => {
                let high = field == Field::NormalHigh;
                let from_history = vm
                    .history
                    .as_ref()
                    .and_then(|h| h.today.as_ref())
                    .map(|c| if high { &c.normal_high } else { &c.normal_low });
                let from_day = vm
                    .daily
                    .as_ref()
                    .and_then(|d| d.days.first())
                    .map(|d| if high { &d.normal_high } else { &d.normal_low });
                self.or_missing(
                    from_history
                        .or(from_day)
                        .filter(|q| q.value.is_some())
                        .map(|q| q.text.as_str()),
                )
            }
            Field::RecordHigh => self.or_missing(
                vm.history
                    .as_ref()
                    .and_then(|h| h.today.as_ref())
                    .and_then(|c| c.record_high.as_deref()),
            ),
            Field::YesterdayDelta => self.or_missing(
                vm.history
                    .as_ref()
                    .and_then(|h| h.yesterday_delta.as_deref()),
            ),
        }
    }
}

/// A `snake_case` serde name (`SkyTheme`, `IconId`).
fn serde_name(value: &impl serde::Serialize) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// A parsed line configuration.
#[derive(Clone, Debug)]
pub struct LineFormat {
    /// The settings.
    pub cfg: LineConfig,
    format: Template,
    tooltip: Option<Template>,
}

impl LineFormat {
    /// Parses the templates.
    pub fn new(cfg: LineConfig) -> Result<Self, crate::template::TemplateError> {
        let format = Template::parse(&cfg.format)?;
        let tooltip = cfg.tooltip.as_deref().map(Template::parse).transpose()?;
        Ok(Self {
            cfg,
            format,
            tooltip,
        })
    }

    /// The output line for location `i`.
    pub fn render(
        &self,
        state: &AppState,
        i: usize,
        p: &PresentationConfig,
        s: &Strings,
        now: UnixTime,
    ) -> String {
        let presentation = line_presentation(p);
        let vm = view_for(state, i, &presentation, s, now);
        let values = Values::new(&vm, self.cfg.icons, &presentation.missing);
        let text = self.format.render(|f| values.get(f)).trim().to_string();
        match self.cfg.output {
            LineOutput::Plain => text,
            LineOutput::Waybar => {
                let tooltip = self
                    .tooltip
                    .as_ref()
                    .map(|t| t.render(|f| values.get(f)))
                    .unwrap_or_default();
                let mut class = vec!["whether".to_string(), serde_name(&vm.theme)];
                if vm.status.stale {
                    class.push("stale".into());
                }
                // `alert` and the first alert's level, e.g. `alert-very_poor`.
                if let Some(a) = vm.alerts.first() {
                    class.push("alert".into());
                    class.push(format!("alert-{}", serde_name(&a.level)));
                }
                let alt = vm
                    .current
                    .as_ref()
                    .map(|c| serde_name(&c.icon))
                    .unwrap_or_default();
                serde_json::json!({ "text": text, "tooltip": tooltip, "class": class, "alt": alt })
                    .to_string()
            }
        }
    }
}

/// The JSON document for location `i`: the view model or the raw snapshot.
pub fn render_json(
    state: &AppState,
    i: usize,
    snapshot: bool,
    compact: bool,
    p: &PresentationConfig,
    s: &Strings,
    now: UnixTime,
) -> Result<String, serde_json::Error> {
    let value = if snapshot {
        serde_json::to_value(state.locations.get(i).and_then(|l| l.snapshot.as_deref()))?
    } else {
        serde_json::to_value(view_for(state, i, p, s, now))?
    };
    if compact {
        serde_json::to_string(&value)
    } else {
        serde_json::to_string_pretty(&value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use whether_config::LocationConfig;
    use whether_core::testdata;
    use whether_display::LocationState;

    fn state(stale: bool) -> AppState {
        state_of(testdata::sample_snapshot(), stale)
    }

    fn state_of(snapshot: whether_core::model::WeatherSnapshot, stale: bool) -> AppState {
        let mut loc = LocationState::new(LocationConfig {
            name: Some("New York".into()),
            ..Default::default()
        });
        loc.snapshot = Some(Arc::new(snapshot));
        loc.status.last_success = Some(testdata::SAMPLE_NOW);
        loc.status.stale = stale;
        AppState {
            locations: vec![loc],
            ..Default::default()
        }
    }

    fn line(cfg: LineConfig, stale: bool) -> String {
        LineFormat::new(cfg).unwrap().render(
            &state(stale),
            0,
            &PresentationConfig::default(),
            &Strings::default(),
            testdata::SAMPLE_NOW,
        )
    }

    #[test]
    fn default_line() {
        insta::assert_snapshot!(line(LineConfig::default(), false));
    }

    #[test]
    fn every_placeholder() {
        let format = crate::template::FIELDS
            .iter()
            .map(|(n, _)| format!("{n}={{{n}}}"))
            .collect::<Vec<_>>()
            .join(" | ");
        insta::assert_snapshot!(line(
            LineConfig {
                format,
                icons: IconStyle::NerdFont,
                ..Default::default()
            },
            false
        ));
    }

    #[test]
    fn waybar_shape() {
        let text = line(
            LineConfig {
                output: LineOutput::Waybar,
                ..Default::default()
            },
            true,
        );
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!(v["text"].is_string() && v["tooltip"].as_str().unwrap().starts_with("New York: "));
        let class: Vec<&str> = v["class"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_str().unwrap())
            .collect();
        assert_eq!(class[0], "whether");
        assert_eq!(class.last(), Some(&"stale"));
        assert_ne!(v["alt"].as_str().unwrap(), "");
        assert!(!text.contains('\n'), "one object per line");
        insta::assert_snapshot!(text);
    }

    #[test]
    fn alerts_show_in_the_line_and_waybar_classes() {
        let render = |cfg: LineConfig, presentation: &PresentationConfig| {
            LineFormat::new(cfg).unwrap().render(
                &state_of(testdata::sample_snapshot_with_alerts(), false),
                0,
                presentation,
                &Strings::default(),
                testdata::SAMPLE_NOW,
            )
        };
        let with_alerts = testdata::presentation_with_alerts();
        let plain = LineConfig {
            format: "{temp} {alert}".into(),
            icons: IconStyle::None,
            ..Default::default()
        };
        assert_eq!(render(plain.clone(), &with_alerts), "15° Flood Warning");
        // Without the alerts widget the view carries none.
        assert_eq!(render(plain, &PresentationConfig::default()), "15°");
        let waybar = render(
            LineConfig {
                output: LineOutput::Waybar,
                ..Default::default()
            },
            &with_alerts,
        );
        let v: serde_json::Value = serde_json::from_str(&waybar).unwrap();
        assert_eq!(
            v["class"],
            serde_json::json!(["whether", "clear_day", "alert", "alert-very_poor"])
        );
    }

    #[test]
    fn history_placeholders() {
        let mut state = state(false);
        state.locations[0].history = Some(Arc::new(testdata::history_10001()));
        let cfg = LineConfig {
            format: "{normal_high}/{normal_low} {record_high} | {yesterday_delta}".into(),
            icons: IconStyle::None,
            ..Default::default()
        };
        let render = |widgets: Vec<WidgetKind>| {
            LineFormat::new(cfg.clone()).unwrap().render(
                &state,
                0,
                &PresentationConfig {
                    units: whether_core::units::UnitPrefs::IMPERIAL,
                    widgets,
                    ..PresentationConfig::default()
                },
                &Strings::default(),
                testdata::SAMPLE_NOW,
            )
        };
        let shown = render(vec![WidgetKind::History]);
        assert!(shown.starts_with("69°/55° 94° in 1941 | "), "{shown}");
        assert!(
            shown.contains("than yesterday") || shown.contains("As warm"),
            "{shown}"
        );
        // Without the history widget, the daily list still has today's normals.
        assert_eq!(render(vec![WidgetKind::Daily]), "69°/55° — | —");
        assert_eq!(line(cfg.clone(), false), "—/— — | —", "no history yet");
    }

    #[test]
    fn no_icon_trims_the_line() {
        let text = line(
            LineConfig {
                icons: IconStyle::None,
                ..Default::default()
            },
            false,
        );
        assert!(!text.starts_with(' '));
    }

    #[test]
    fn json_documents() {
        let s = state(false);
        let p = PresentationConfig::default();
        let strings = Strings::default();
        let vm = render_json(&s, 0, false, false, &p, &strings, testdata::SAMPLE_NOW).unwrap();
        assert!(vm.contains("\n  \"tabs\""));
        let raw = render_json(&s, 0, true, true, &p, &strings, testdata::SAMPLE_NOW).unwrap();
        assert!(!raw.contains('\n') && raw.contains("\"provider\""));
    }

    #[test]
    fn data_requirements_and_location_choice() {
        let s = state(false);
        assert_eq!(require_data(&s, 0), Ok(()));
        assert_eq!(require_data(&AppState::default(), 0), Err(NoData::Empty));
        let mut failed = AppState {
            locations: vec![LocationState::new(LocationConfig::default())],
            ..Default::default()
        };
        failed.locations[0].status.error = Some("down".into());
        assert_eq!(require_data(&failed, 0), Err(NoData::AllFailed));
        assert_eq!(NoData::AllFailed.exit_code(), 4);
        assert_eq!(location_index(&s, Some("new_york"), 3), 0);
        assert_eq!(location_index(&s, Some("Nowhere"), 0), 0);
    }
}
