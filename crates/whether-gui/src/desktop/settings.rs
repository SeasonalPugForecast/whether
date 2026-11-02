//! The settings dialog: tabs of choices and toggle lists built from
//! the loaded configuration, and the config edits a saved form becomes. Only changed
//! keys are written, so the file does not fill up with defaults.
//!
//! Every setting is data: a choice writes its option's value to its key, a toggle
//! list writes the values that are on, in the list's order. Values the dialog does
//! not offer are kept as an extra "(current)" option, so opening and saving changes
//! nothing.

use slint::{ModelRc, SharedString, VecModel};
use toml::Value;
use whether_config::{
    DurationStr, LinuxConfig, ModelList, OwmApi, PresentationToml, ProvidersConfig,
};
use whether_core::model::ProviderId;
use whether_core::models::PRESETS;
use whether_core::presentation::{
    DailyIcon, DateFormat, DetailKind, HourlyMetric, PresentationConfig, TemperatureBasis,
    TimeFormat, WidgetKind,
};
use whether_core::units::UnitSystem;

use super::config::{DEFAULT_ACCENT, FontScale, GuiConfig, Section, SkyStyle};
use super::glue::{FixedTheme, sky_for};
use crate::model::UiModel;
use crate::{DesktopWindow, SettingChoice, SettingList, SettingToggle, SettingsPreview};

/// The dialog's tabs.
pub const TABS: [&str; 4] = ["General", "Forecast", "Cards", "Display"];
const GENERAL: usize = 0;
const FORECAST: usize = 1;
const CARDS: usize = 2;
const DISPLAY: usize = 3;

/// The desktop cards in `[display.gui] sections` order, as the dialog lists them.
pub const SECTIONS: [(Section, &str); 10] = [
    (Section::Current, "Current conditions"),
    (Section::Nowcast, "Next hours"),
    (Section::Hourly, "Hourly"),
    (Section::Precipitation, "Precipitation"),
    (Section::Daily, "Next days"),
    (Section::History, "History"),
    (Section::Details, "Details"),
    (Section::Sun, "Sun & moon"),
    (Section::AirQuality, "Air quality"),
    (Section::Radar, "Radar"),
];

const UNITS: [(UnitSystem, &str); 4] = [
    (UnitSystem::Auto, "Auto (from the locale)"),
    (UnitSystem::Metric, "Metric (°C, km/h)"),
    (UnitSystem::Imperial, "Imperial (°F, mph)"),
    (UnitSystem::Custom, "Custom ([units] table)"),
];

const CLOCKS: [(TimeFormat, &str); 2] =
    [(TimeFormat::H24, "24-hour"), (TimeFormat::H12, "12-hour")];

const DATES: [(DateFormat, &str); 9] = [
    (DateFormat::MonthNameDay, "Nov 1"),
    (DateFormat::DayMonthName, "1 Nov"),
    (DateFormat::Compact, "NOV1"),
    (DateFormat::MonthDay, "11/1"),
    (DateFormat::MonthDayPadded, "11/01"),
    (DateFormat::DayMonth, "1/11"),
    (DateFormat::DayMonthPadded, "01/11"),
    (DateFormat::DayMonthDots, "01.11."),
    (DateFormat::Iso, "2026-11-01"),
];

const REFRESH: [(u32, &str); 5] = [
    (5 * 60, "5 min"),
    (10 * 60, "10 min"),
    (15 * 60, "15 min"),
    (30 * 60, "30 min"),
    (60 * 60, "1 h"),
];

const PROVIDERS: [(ProviderId, &str); 3] = [
    (ProviderId::OpenWeather, "OpenWeather"),
    (ProviderId::OpenMeteo, "Open-Meteo"),
    (ProviderId::MetNorway, "MET Norway"),
];

const HOURS: [u8; 4] = [12, 24, 36, 48];

const DAY_ICONS: [(DailyIcon, &str); 2] = [
    (DailyIcon::Daylight, "Sunrise to sunset"),
    (DailyIcon::Provider, "Whole day (provider)"),
];
const TEMPERATURES: [(TemperatureBasis, &str); 2] = [
    (TemperatureBasis::Actual, "Actual"),
    (
        TemperatureBasis::FeelsLike,
        "Feels like (highs and lows too)",
    ),
];
const DAYS: [u8; 6] = [3, 5, 7, 10, 14, 16];

/// The past days the History card lists.
const HISTORY_DAYS: [u8; 4] = [3, 7, 14, 31];

/// The cards that show nothing without their widget (radar imagery, history).
const CARD_WIDGETS: [(Section, WidgetKind); 2] = [
    (Section::Radar, WidgetKind::Radar),
    (Section::History, WidgetKind::History),
];

const METRICS: [(HourlyMetric, &str); 7] = [
    (HourlyMetric::Temperature, "Temperature"),
    (HourlyMetric::FeelsLike, "Feels like"),
    (HourlyMetric::PrecipProbability, "Chance of rain"),
    (HourlyMetric::Precipitation, "Precipitation"),
    (HourlyMetric::Wind, "Wind"),
    (HourlyMetric::Humidity, "Humidity"),
    (HourlyMetric::Uv, "UV index"),
];

const DETAILS: [(DetailKind, &str); 12] = [
    (DetailKind::Wind, "Wind"),
    (DetailKind::Gusts, "Gusts"),
    (DetailKind::Humidity, "Humidity"),
    (DetailKind::DewPoint, "Dew point"),
    (DetailKind::Pressure, "Pressure"),
    (DetailKind::Uv, "UV index"),
    (DetailKind::Visibility, "Visibility"),
    (DetailKind::CloudCover, "Cloud cover"),
    (DetailKind::Precipitation, "Precipitation"),
    (DetailKind::FeelsLike, "Feels like"),
    (DetailKind::Normals, "Normals"),
    (DetailKind::Records, "Records"),
];

const ZOOMS: [(u8, &str); 5] = [
    (6, "6 · several states"),
    (7, "7"),
    (8, "8 · about 360 km"),
    (9, "9"),
    (10, "10 · a county"),
];

const BACKGROUNDS: [(bool, &str); 2] = [(true, "Follows the weather"), (false, "Fixed")];

const THEMES: [(FixedTheme, &str); 3] = [
    (FixedTheme::Auto, "Light by day, dark at night"),
    (FixedTheme::Light, "Light"),
    (FixedTheme::Dark, "Dark"),
];

const SKY_STYLES: [(SkyStyle, &str); 3] = [
    (SkyStyle::Default, "As designed"),
    (SkyStyle::Muted, "Muted"),
    (SkyStyle::Vivid, "Vivid"),
];

const ACCENTS: [(&str, &str); 7] = [
    ("#FFD166", "Sun yellow"),
    ("#4FC3F7", "Sky blue"),
    ("#4DD0A6", "Mint"),
    ("#FF8A65", "Coral"),
    ("#F48FB1", "Pink"),
    ("#B39DDB", "Lavender"),
    ("#FFFFFF", "White"),
];

/// Text sizes: binary fractions, so the file gets `1.25`, not `1.2499999`.
const SCALES: [(FontScale, &str); 6] = [
    (FontScale::Auto, "Auto (larger full screen)"),
    (FontScale::Fixed(0.75), "75 %"),
    (FontScale::Fixed(1.0), "100 %"),
    (FontScale::Fixed(1.25), "125 %"),
    (FontScale::Fixed(1.5), "150 %"),
    (FontScale::Fixed(2.0), "200 %"),
];

const PAGES: [(u32, &str); 5] = [
    (0, "Off (swipe or tap)"),
    (10, "10 s"),
    (20, "20 s"),
    (30, "30 s"),
    (60, "1 min"),
];

const CYCLE: [(u32, &str); 5] = [
    (0, "Off"),
    (30, "30 s"),
    (60, "1 min"),
    (5 * 60, "5 min"),
    (10 * 60, "10 min"),
];

/// One choice: a key and the values its options write.
#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    /// The tab it is on.
    pub tab: usize,
    /// Its label.
    pub label: &'static str,
    /// The dotted config key it writes.
    pub key: String,
    /// Option labels with the values they write.
    pub options: Vec<(String, Value)>,
    /// The chosen option.
    pub index: usize,
}

/// One entry of a toggle list.
#[derive(Clone, Debug, PartialEq)]
pub struct Toggle {
    /// Its label.
    pub label: &'static str,
    /// The value written while it is on.
    pub value: Value,
    /// Whether it is on.
    pub on: bool,
}

/// A list of toggles written as an array of the values that are on.
#[derive(Clone, Debug, PartialEq)]
pub struct Toggles {
    /// The tab it is on.
    pub tab: usize,
    /// Its label.
    pub label: &'static str,
    /// The dotted config key it writes.
    pub key: String,
    /// The order is written too (the dialog shows move buttons).
    pub ordered: bool,
    /// The entries, in order.
    pub items: Vec<Toggle>,
}

/// The dialog's state.
#[derive(Clone, Debug, PartialEq)]
pub struct SettingsForm {
    /// Every choice, in display order.
    pub choices: Vec<Choice>,
    /// Every toggle list, in display order.
    pub lists: Vec<Toggles>,
    /// The widgets shown, and their key: a card in [`CARD_WIDGETS`] turned on adds
    /// its widget to them, so the card has something to show.
    widgets: (String, Vec<Value>),
    /// `[display.gui]` as loaded, for the preview.
    gui_table: toml::Table,
    /// The sky shown when the dialog opened (a `SkyTheme` index), for the preview.
    pub sky: i32,
}

/// What the appearance choices would look like.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Preview {
    /// Gradient top.
    pub top: slint::Color,
    /// Gradient bottom.
    pub bottom: slint::Color,
    /// Text on the sky.
    pub text: slint::Color,
    /// The accent.
    pub accent: slint::Color,
    /// The font scale (1.0 for auto).
    pub scale: f32,
}

/// A serde enum's TOML form (`"imperial"`, `"12h"`, `"air_quality"`).
fn toml_name(value: &impl serde::Serialize) -> Value {
    Value::try_from(value).unwrap_or_else(|_| Value::String(String::new()))
}

fn duration(secs: u32) -> Value {
    Value::String(DurationStr(secs).to_string())
}

/// Options from `(value, label)` pairs.
fn named<T>(items: &[(T, &'static str)], to: impl Fn(&T) -> Value) -> Vec<(String, Value)> {
    items
        .iter()
        .map(|(v, l)| ((*l).to_string(), to(v)))
        .collect()
}

/// A choice set to the option whose value is `current`, or to `current` added as
/// its own option.
fn choice(
    tab: usize,
    label: &'static str,
    key: impl Into<String>,
    mut options: Vec<(String, Value)>,
    current: Value,
) -> Choice {
    let index = options
        .iter()
        .position(|(_, v)| *v == current)
        .unwrap_or_else(|| {
            let shown = match &current {
                Value::String(s) => s.clone(),
                // A list of names, such as weather models: "a → b".
                Value::Array(items) if items.iter().all(Value::is_str) => items
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" → "),
                other => other.to_string(),
            };
            options.push((format!("{shown} (current)"), current));
            options.len() - 1
        });
    Choice {
        tab,
        label,
        key: key.into(),
        options,
        index,
    }
}

/// A toggle list of `all`, the `on` entries first in their order, then the rest.
fn toggles<T: PartialEq + serde::Serialize>(
    (tab, label, key): (usize, &'static str, String),
    all: &[(T, &'static str)],
    on: &[T],
) -> Toggles {
    let mut items: Vec<Toggle> = on
        .iter()
        .filter_map(|t| all.iter().find(|(a, _)| a == t))
        .map(|(t, l)| Toggle {
            label: l,
            value: toml_name(t),
            on: true,
        })
        .collect();
    items.extend(
        all.iter()
            .filter(|(t, _)| !on.contains(t))
            .map(|(t, l)| Toggle {
                label: l,
                value: toml_name(t),
                on: false,
            }),
    );
    Toggles {
        tab,
        label,
        key,
        ordered: true,
        items,
    }
}

/// The key of presentation `field`: the GUI's override when it sets one, so the
/// edit takes effect, else `[presentation]`.
fn presentation_key(over: &PresentationToml, field: &str) -> String {
    let overridden = match field {
        "widgets" => over.widgets.is_some(),
        "hourly_hours" => over.hourly_hours.is_some(),
        "hourly_metrics" => over.hourly_metrics.is_some(),
        "daily_days" => over.daily_days.is_some(),
        "history_days" => over.history_days.is_some(),
        "daily_icon" => over.daily_icon.is_some(),
        "details" => over.details.is_some(),
        _ => false,
    };
    if overridden {
        format!("display.gui.presentation.{field}")
    } else {
        format!("presentation.{field}")
    }
}

/// Open-Meteo's weather models, as the core's presets.
fn models_choice(providers: &ProvidersConfig) -> Choice {
    let list = |ids: &mut dyn Iterator<Item = &str>| {
        Value::Array(ids.map(|id| Value::String(id.to_string())).collect())
    };
    let presets = PRESETS
        .iter()
        .map(|p| (p.name.to_string(), list(&mut p.models.iter().copied())))
        .collect();
    let configured = providers
        .open_meteo
        .models
        .as_ref()
        .map_or(&[][..], ModelList::ids);
    choice(
        GENERAL,
        "Forecast models",
        "providers.open-meteo.models",
        presets,
        list(&mut configured.iter().map(String::as_str)),
    )
}

fn dates_choice(current: DateFormat) -> Choice {
    let dates = DATES
        .iter()
        .map(|(d, l)| ((*l).to_string(), toml_name(d)))
        .collect();
    choice(
        GENERAL,
        "Dates",
        "general.date_format",
        dates,
        toml_name(&current),
    )
}

fn general_choices(cfg: &LinuxConfig) -> Vec<Choice> {
    let general = &cfg.common.general;
    let providers = &cfg.common.providers;
    let custom = general.units == UnitSystem::Custom;
    let units = UNITS
        .iter()
        .filter(|(u, _)| *u != UnitSystem::Custom || custom)
        .map(|(u, l)| ((*l).to_string(), toml_name(u)))
        .collect();
    let clocks = CLOCKS
        .iter()
        .map(|(t, l)| ((*l).to_string(), toml_name(t)))
        .collect();
    let refresh = REFRESH
        .iter()
        .map(|(s, l)| ((*l).to_string(), duration(*s)))
        .collect();
    // The primary provider goes first; the others keep their order as fallbacks.
    let order = |first: ProviderId| {
        let rest = providers.order.iter().filter(|p| **p != first).copied();
        Value::Array(
            std::iter::once(first)
                .chain(rest)
                .map(|p| toml_name(&p))
                .collect(),
        )
    };
    let primary = PROVIDERS
        .iter()
        .filter(|(p, _)| *p != ProviderId::MetNorway || providers.met_norway.is_some())
        .map(|(p, l)| ((*l).to_string(), order(*p)))
        .collect();
    let owm_api = providers
        .openweather
        .as_ref()
        .and_then(|o| o.api)
        .unwrap_or_default();
    let apis = [
        (OwmApi::Free, "Free (5 days, 3-hour steps)"),
        (OwmApi::OneCall4, "One Call 4.0 (metered; alerts)"),
    ]
    .iter()
    .map(|(a, l)| ((*l).to_string(), toml_name(a)))
    .collect();
    let ids = |ids: &[ProviderId]| Value::Array(ids.iter().map(toml_name).collect());
    let gaps = vec![
        ("Open-Meteo".to_string(), ids(&[ProviderId::OpenMeteo])),
        (
            "Open-Meteo and NWS alerts".to_string(),
            ids(&[ProviderId::OpenMeteo, ProviderId::Nws]),
        ),
        ("NWS alerts".to_string(), ids(&[ProviderId::Nws])),
        ("Nothing".to_string(), Value::Array(Vec::new())),
    ];
    vec![
        choice(
            GENERAL,
            "Units",
            "general.units",
            units,
            toml_name(&general.units),
        ),
        choice(
            GENERAL,
            "Clock",
            "general.time_format",
            clocks,
            toml_name(&general.time_format),
        ),
        dates_choice(general.date_format),
        choice(
            GENERAL,
            "Refresh every",
            "general.refresh",
            refresh,
            duration(general.refresh.secs()),
        ),
        choice(
            GENERAL,
            "Weather from",
            "providers.order",
            primary,
            Value::Array(providers.order.iter().map(toml_name).collect()),
        ),
        models_choice(providers),
        choice(
            GENERAL,
            "OpenWeather API",
            "providers.openweather.api",
            apis,
            toml_name(&owm_api),
        ),
        choice(
            GENERAL,
            "Fill gaps from",
            "providers.supplement",
            gaps,
            Value::Array(providers.supplement.iter().map(toml_name).collect()),
        ),
    ]
}

/// The Display tab's appearance choices.
fn appearance_choices(gui: &GuiConfig) -> Vec<Choice> {
    vec![
        choice(
            DISPLAY,
            "Background",
            "display.gui.dynamic_background",
            named(&BACKGROUNDS, |b| Value::Boolean(*b)),
            Value::Boolean(gui.dynamic_background),
        ),
        choice(
            DISPLAY,
            "Fixed theme",
            "display.gui.theme",
            named(&THEMES, toml_name),
            toml_name(&gui.theme),
        ),
        choice(
            DISPLAY,
            "Sky colors",
            "display.gui.sky_style",
            named(&SKY_STYLES, toml_name),
            toml_name(&gui.sky_style),
        ),
        choice(
            DISPLAY,
            "Accent",
            "display.gui.accent",
            ACCENTS
                .iter()
                .map(|(hex, l)| ((*l).to_string(), Value::String((*hex).to_string())))
                .collect(),
            Value::String(gui.accent.clone().unwrap_or_else(|| {
                let [r, g, b] = DEFAULT_ACCENT;
                format!("#{r:02X}{g:02X}{b:02X}")
            })),
        ),
        choice(
            DISPLAY,
            "Text size",
            "display.gui.font_scale",
            named(&SCALES, toml_name),
            toml_name(&gui.font_scale),
        ),
    ]
}

/// The Forecast tab's choices.
fn forecast_choices(over: &PresentationToml, shown: &PresentationConfig) -> [Choice; 5] {
    let number = |n: u8| Value::Integer(i64::from(n));
    [
        choice(
            FORECAST,
            "Hours",
            presentation_key(over, "hourly_hours"),
            HOURS.iter().map(|h| (format!("{h}"), number(*h))).collect(),
            number(shown.hourly_hours),
        ),
        choice(
            FORECAST,
            "Days",
            presentation_key(over, "daily_days"),
            DAYS.iter().map(|d| (format!("{d}"), number(*d))).collect(),
            number(shown.daily_days),
        ),
        choice(
            FORECAST,
            "History days",
            presentation_key(over, "history_days"),
            HISTORY_DAYS
                .iter()
                .map(|d| (format!("{d}"), number(*d)))
                .collect(),
            number(shown.history_days),
        ),
        choice(
            FORECAST,
            "Day icons",
            presentation_key(over, "daily_icon"),
            named(&DAY_ICONS, toml_name),
            toml_name(&shown.daily_icon),
        ),
        choice(
            FORECAST,
            "Temperatures",
            presentation_key(over, "temperature_basis"),
            named(&TEMPERATURES, toml_name),
            toml_name(&shown.temperature_basis),
        ),
    ]
}

/// The form for a loaded configuration.
pub fn form(cfg: &LinuxConfig) -> SettingsForm {
    let gui = gui_config(cfg).unwrap_or_default();
    let over = cfg.display.presentation("gui").unwrap_or_default();
    let shown = cfg.presentation_for("gui").unwrap_or_default();
    let number = |n: u8| Value::Integer(i64::from(n));
    let mut choices = general_choices(cfg);
    choices.extend(appearance_choices(&gui));
    choices.extend(forecast_choices(&over, &shown));
    choices.extend([
        choice(
            CARDS,
            "Radar zoom",
            "radar.zoom",
            ZOOMS
                .iter()
                .map(|(z, l)| ((*l).to_string(), number(*z)))
                .collect(),
            number(cfg.radar.zoom),
        ),
        choice(
            DISPLAY,
            "Cycle locations every",
            "display.gui.cycle_locations",
            CYCLE
                .iter()
                .map(|(s, l)| ((*l).to_string(), duration(*s)))
                .collect(),
            duration(gui.cycle_locations.secs()),
        ),
        choice(
            DISPLAY,
            "Compact pages turn every",
            "display.gui.page_interval",
            PAGES
                .iter()
                .map(|(s, l)| ((*l).to_string(), duration(*s)))
                .collect(),
            duration(gui.page_interval.secs()),
        ),
    ]);
    let lists = vec![
        toggles(
            (
                FORECAST,
                "Chart and table",
                presentation_key(&over, "hourly_metrics"),
            ),
            &METRICS,
            &shown.hourly_metrics,
        ),
        toggles(
            (FORECAST, "Detail cards", presentation_key(&over, "details")),
            &DETAILS,
            &shown.details,
        ),
        toggles(
            (CARDS, "Cards", "display.gui.sections".to_string()),
            &SECTIONS,
            &gui.sections,
        ),
    ];
    let widgets = (
        presentation_key(&over, "widgets"),
        shown.widgets.iter().map(toml_name).collect(),
    );
    SettingsForm {
        choices,
        lists,
        widgets,
        gui_table: cfg.display.plugin_table("gui").unwrap_or_default(),
        sky: 0,
    }
}

/// `[display.gui]` of a loaded configuration (`None` when it does not parse).
pub fn gui_config(cfg: &LinuxConfig) -> Option<GuiConfig> {
    match cfg.display.plugin_table("gui") {
        Some(table) => Value::Table(table).try_into().ok(),
        None => Some(GuiConfig::default()),
    }
}

impl SettingsForm {
    /// Chooses option `option` of choice `i`.
    pub fn choose(&mut self, i: usize, option: usize) {
        if let Some(c) = self.choices.get_mut(i)
            && option < c.options.len()
        {
            c.index = option;
        }
    }

    /// Turns entry `item` of list `list` on or off.
    pub fn toggle(&mut self, list: usize, item: usize, on: bool) {
        if let Some(t) = self.lists.get_mut(list).and_then(|l| l.items.get_mut(item)) {
            t.on = on;
        }
    }

    /// Moves entry `item` of list `list` one place up (`up`) or down.
    pub fn move_item(&mut self, list: usize, item: usize, up: bool) {
        let Some(l) = self.lists.get_mut(list) else {
            return;
        };
        let other = if up {
            item.checked_sub(1)
        } else {
            Some(item + 1)
        };
        if let Some(other) = other.filter(|o| *o < l.items.len())
            && item < l.items.len()
        {
            l.items.swap(item, other);
        }
    }

    /// The window's look with the chosen `[display.gui]` values.
    pub fn preview(&self) -> Preview {
        let mut table = self.gui_table.clone();
        for c in &self.choices {
            if let Some(name) = c.key.strip_prefix("display.gui.")
                && let Some((_, value)) = c.options.get(c.index)
            {
                table.insert(name.to_string(), value.clone());
            }
        }
        let gui: GuiConfig = Value::Table(table).try_into().unwrap_or_default();
        let opts = gui.window_options(false);
        let model = UiModel {
            sky: self.sky,
            ..UiModel::default()
        };
        let (top, bottom, text) = sky_for(&model, &opts);
        let [red, green, blue] = opts.accent;
        Preview {
            top,
            bottom,
            text,
            accent: slint::Color::from_rgb_u8(red, green, blue),
            scale: match gui.font_scale {
                FontScale::Auto => 1.0,
                FontScale::Fixed(f) => f,
            },
        }
    }

    /// Whether `section`'s card is on.
    fn card_on(&self, section: Section) -> bool {
        let name = toml_name(&section);
        self.lists
            .iter()
            .filter(|l| l.key == "display.gui.sections")
            .flat_map(|l| &l.items)
            .any(|t| t.on && t.value == name)
    }
}

fn index(i: usize) -> i32 {
    i32::try_from(i).unwrap_or(0)
}

/// Shows `form` in the window's dialog.
pub fn show(ui: &DesktopWindow, form: &SettingsForm) {
    let tabs: Vec<SharedString> = TABS.iter().map(|t| (*t).into()).collect();
    ui.set_settings_tabs(ModelRc::new(VecModel::from(tabs)));
    let choices: Vec<SettingChoice> = form
        .choices
        .iter()
        .map(|c| SettingChoice {
            tab: index(c.tab),
            label: c.label.into(),
            options: ModelRc::new(VecModel::from(
                c.options
                    .iter()
                    .map(|(l, _)| SharedString::from(l.as_str()))
                    .collect::<Vec<_>>(),
            )),
            index: index(c.index),
        })
        .collect();
    ui.set_settings_choices(ModelRc::new(VecModel::from(choices)));
    show_lists(ui, form);
    show_preview(ui, form);
}

/// Shows the appearance preview (after a choice).
pub fn show_preview(ui: &DesktopWindow, form: &SettingsForm) {
    let p = form.preview();
    ui.set_settings_preview(SettingsPreview {
        top: p.top,
        bottom: p.bottom,
        text: p.text,
        accent: p.accent,
        scale: p.scale,
    });
}

/// Shows the toggle lists only (after a toggle or a move).
pub fn show_lists(ui: &DesktopWindow, form: &SettingsForm) {
    let lists: Vec<SettingList> = form
        .lists
        .iter()
        .map(|l| SettingList {
            tab: index(l.tab),
            label: l.label.into(),
            ordered: l.ordered,
            items: ModelRc::new(VecModel::from(
                l.items
                    .iter()
                    .map(|t| SettingToggle {
                        label: t.label.into(),
                        on: t.on,
                    })
                    .collect::<Vec<_>>(),
            )),
        })
        .collect();
    ui.set_settings_lists(ModelRc::new(VecModel::from(lists)));
}

/// The config edits that turn `before` into `after`: only changed keys.
pub fn edits(before: &SettingsForm, after: &SettingsForm) -> Vec<(String, Value)> {
    let mut out = Vec::new();
    for (b, a) in before.choices.iter().zip(&after.choices) {
        if a.index != b.index
            && let Some((_, value)) = a.options.get(a.index)
        {
            out.push((a.key.clone(), value.clone()));
        }
    }
    for (b, a) in before.lists.iter().zip(&after.lists) {
        if a.items != b.items {
            let on = a
                .items
                .iter()
                .filter(|t| t.on)
                .map(|t| t.value.clone())
                .collect();
            out.push((a.key.clone(), Value::Array(on)));
        }
    }
    let (key, shown) = &after.widgets;
    let mut widgets = shown.clone();
    for (section, kind) in CARD_WIDGETS {
        let widget = toml_name(&kind);
        if after.card_on(section) && !before.card_on(section) && !widgets.contains(&widget) {
            widgets.push(widget);
        }
    }
    if widgets != *shown {
        out.push((key.clone(), Value::Array(widgets)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLACE: &str = "[[location]]\nname = \"New York\"\nlatitude = 40.7\nlongitude = -74.0\n";

    fn cfg(extra: &str) -> LinuxConfig {
        whether_config::parse(&format!("{PLACE}{extra}")).unwrap()
    }

    fn choice_named<'a>(f: &'a SettingsForm, label: &str) -> (usize, &'a Choice) {
        f.choices
            .iter()
            .enumerate()
            .find(|(_, c)| c.label == label)
            .unwrap()
    }

    fn as_text(edits: Vec<(String, Value)>) -> Vec<(String, String)> {
        edits.into_iter().map(|(k, v)| (k, v.to_string())).collect()
    }

    #[test]
    fn an_unchanged_form_writes_nothing() {
        let f = form(&cfg(""));
        let (_, units) = choice_named(&f, "Units");
        assert_eq!(units.options.len(), 3, "no custom unless configured");
        let (_, refresh) = choice_named(&f, "Refresh every");
        assert_eq!(refresh.options[refresh.index].0, "15 min");
        let (_, primary) = choice_named(&f, "Weather from");
        assert_eq!(primary.options.len(), 2, "MET Norway needs its table");
        assert_eq!(primary.options[primary.index].0, "OpenWeather");
        assert!(
            f.choices
                .iter()
                .all(|c| !c.options[c.index].0.contains("current"))
        );
        assert_eq!(edits(&f, &f), vec![]);
        assert!(TABS.len() > f.choices.iter().map(|c| c.tab).max().unwrap());
    }

    #[test]
    fn model_presets_round_trip_and_unknown_lists_are_kept() {
        let models = |f: &SettingsForm| {
            let (_, c) = choice_named(f, "Forecast models");
            c.options[c.index].0.clone()
        };
        assert_eq!(models(&form(&cfg(""))), "Best match");
        let blend = cfg("[providers.open-meteo]\nmodels = \"ncep_nbm_conus\"\n");
        assert_eq!(
            models(&form(&blend)),
            "NOAA blend (US)",
            "a string is a list of one"
        );
        let custom = form(&cfg(
            "[providers.open-meteo]\nmodels = [\"knmi_seamless\", \"ecmwf_ifs025\"]\n",
        ));
        assert_eq!(models(&custom), "knmi_seamless → ecmwf_ifs025 (current)");
        assert_eq!(edits(&custom, &custom), vec![], "kept as it is");

        let before = form(&cfg(""));
        let (i, c) = choice_named(&before, "Forecast models");
        for (option, preset) in PRESETS.iter().enumerate().skip(1) {
            let mut after = before.clone();
            after.choose(i, option);
            let written = edits(&before, &after);
            assert_eq!(written.len(), 1, "{}", preset.name);
            let (key, value) = &written[0];
            assert_eq!(key, "providers.open-meteo.models");
            // What is written reads back as the same preset.
            let reread = cfg(&format!("[providers.open-meteo]\nmodels = {value}\n"));
            assert_eq!(models(&form(&reread)), preset.name);
        }
        // Back to best match from a preset writes an empty list.
        let mut after = form(&blend);
        let before = after.clone();
        after.choose(i, 0);
        assert_eq!(
            as_text(edits(&before, &after)),
            [("providers.open-meteo.models".to_string(), "[]".to_string())]
        );
        assert_eq!(c.options.len(), PRESETS.len());
    }

    #[test]
    fn choices_become_config_edits() {
        let before = form(&cfg(""));
        let mut after = before.clone();
        for (label, option) in [
            ("Units", 2),
            ("Clock", 1),
            ("Dates", 2),
            ("Refresh every", 3),
            ("Weather from", 1),
            ("OpenWeather API", 1),
            ("Fill gaps from", 1), // Open-Meteo and NWS alerts
            ("Hours", 3),
            ("Days", 1),
            ("Day icons", 1),
            ("Radar zoom", 4),
            ("Background", 1),
            ("Fixed theme", 2),
            ("Sky colors", 1),
            ("Accent", 1),
            ("Text size", 3),
            ("Cycle locations every", 2),
            ("Compact pages turn every", 2),
        ] {
            let (i, _) = choice_named(&before, label);
            after.choose(i, option);
        }
        after.choose(0, 99);
        assert_eq!(
            as_text(edits(&before, &after)),
            [
                ("general.units", "\"imperial\""),
                ("general.time_format", "\"12h\""),
                ("general.date_format", "\"MMMd\""),
                ("general.refresh", "\"30m\""),
                ("providers.order", "[\"open-meteo\", \"openweather\"]"),
                ("providers.openweather.api", "\"onecall-4\""),
                ("providers.supplement", "[\"open-meteo\", \"nws\"]"),
                ("display.gui.dynamic_background", "false"),
                ("display.gui.theme", "\"dark\""),
                ("display.gui.sky_style", "\"muted\""),
                ("display.gui.accent", "\"#4FC3F7\""),
                ("display.gui.font_scale", "1.25"),
                ("presentation.hourly_hours", "48"),
                ("presentation.daily_days", "5"),
                ("presentation.daily_icon", "\"provider\""),
                ("radar.zoom", "10"),
                ("display.gui.cycle_locations", "\"1m\""),
                ("display.gui.page_interval", "\"20s\""),
            ]
            .map(|(k, v)| (k.to_string(), v.to_string()))
        );
    }

    #[test]
    fn lists_write_what_is_on_in_order() {
        let before = form(&cfg(""));
        let mut after = before.clone();
        let details = after
            .lists
            .iter()
            .position(|l| l.label == "Detail cards")
            .unwrap();
        assert_eq!(after.lists[details].items[0].label, "Wind", "on first");
        after.move_item(details, 1, true);
        after.move_item(details, 0, true);
        after.toggle(details, 2, false);
        let cards = after.lists.len() - 1;
        let air = after.lists[cards]
            .items
            .iter()
            .position(|t| t.label == "Air quality")
            .unwrap();
        assert_eq!(air, 9, "last");
        after.toggle(cards, air, false);
        after.move_item(cards, air, false);
        let text = as_text(edits(&before, &after));
        assert_eq!(text.len(), 2);
        assert!(
            text[0].1.starts_with("[\"humidity\", \"wind\", \"uv\""),
            "{text:?}"
        );
        assert_eq!(text[1].0, "display.gui.sections");
        assert!(!text[1].1.contains("air_quality"), "{text:?}");
    }

    #[test]
    fn the_radar_card_turns_the_radar_widget_on() {
        let before = form(&cfg("[display.gui]\nsections = [\"current\"]\n"));
        let cards = before.lists.len() - 1;
        let radar = before.lists[cards]
            .items
            .iter()
            .position(|t| t.label == "Radar")
            .unwrap();
        let mut after = before.clone();
        after.toggle(cards, radar, true);
        let text = as_text(edits(&before, &after));
        assert_eq!(
            text[0],
            (
                "display.gui.sections".into(),
                "[\"current\", \"radar\"]".into()
            )
        );
        assert_eq!(text[1].0, "presentation.widgets");
        assert!(text[1].1.ends_with("\"radar\"]"), "{text:?}");
        let on = form(&cfg("[presentation]\nwidgets = [\"current\", \"radar\"]\n"));
        let mut on_after = on.clone();
        on_after.toggle(cards, radar, false);
        on_after.toggle(cards, radar, true);
        assert!(
            as_text(edits(&on, &on_after))
                .iter()
                .all(|(k, _)| k != "presentation.widgets"),
            "already on"
        );
    }

    #[test]
    fn the_history_card_turns_the_history_widget_on_with_the_radar() {
        let before = form(&cfg("[display.gui]\nsections = [\"current\"]\n"));
        let cards = before.lists.len() - 1;
        let at = |label: &str| {
            before.lists[cards]
                .items
                .iter()
                .position(|t| t.label == label)
                .unwrap()
        };
        let mut after = before.clone();
        after.toggle(cards, at("History"), true);
        after.toggle(cards, at("Radar"), true);
        let text = as_text(edits(&before, &after));
        assert_eq!(
            text[0],
            (
                "display.gui.sections".into(),
                "[\"current\", \"history\", \"radar\"]".into()
            )
        );
        assert_eq!(text[1].0, "presentation.widgets");
        assert!(
            text[1].1.ends_with("\"radar\", \"history\"]"),
            "both, once: {text:?}"
        );
        assert_eq!(text.len(), 2);
    }

    #[test]
    fn the_preview_follows_appearance_choices() {
        let mut f = form(&cfg(""));
        f.sky = 4; // rain
        let rain = f.preview();
        assert_eq!(rain.scale, 1.0);
        let (bg, _) = choice_named(&f, "Background");
        let (theme, _) = choice_named(&f, "Fixed theme");
        let (accent, _) = choice_named(&f, "Accent");
        let (size, _) = choice_named(&f, "Text size");
        f.choose(bg, 1);
        f.choose(theme, 2);
        f.choose(accent, 2);
        f.choose(size, 4);
        let dark = f.preview();
        assert_ne!(dark.top, rain.top, "a fixed dark sky");
        assert_eq!(dark.accent, slint::Color::from_rgb_u8(0x4D, 0xD0, 0xA6));
        assert_eq!(dark.scale, 1.5);
        let (style, _) = choice_named(&f, "Sky colors");
        f.choose(style, 2);
        assert_ne!(f.preview().top, dark.top, "vivid");
    }

    #[test]
    fn gui_overrides_and_unusual_values_are_kept() {
        let f = form(&cfg(
            "[general]\nunits = \"custom\"\nrefresh = \"20m\"\n[display.gui.presentation]\nhourly_hours = 18\n",
        ));
        let (_, units) = choice_named(&f, "Units");
        assert_eq!(units.options[units.index].0, "Custom ([units] table)");
        let (_, refresh) = choice_named(&f, "Refresh every");
        assert_eq!(refresh.options[refresh.index].0, "20m (current)");
        let (i, hours) = choice_named(&f, "Hours");
        assert_eq!(hours.key, "display.gui.presentation.hourly_hours");
        assert_eq!(hours.options[hours.index].0, "18 (current)");
        let mut after = f.clone();
        after.choose(i, 0);
        assert_eq!(
            as_text(edits(&f, &after)),
            [(
                "display.gui.presentation.hourly_hours".to_string(),
                "12".to_string()
            )]
        );
    }
}
