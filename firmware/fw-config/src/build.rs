//! Build-time side (feature `build`, std): parses `whether-fw.toml`, validates it
//! with the shared rules plus the firmware's own, resolves `{ env = … }` secrets
//! and produces the [`FwRuntimeConfig`] that `build.rs` embeds.

use std::cell::RefCell;
use std::collections::BTreeMap;

use serde::Deserialize;
use whether_config::secret::Secret;
use whether_config::types::{MetProduct, OwmApi};
use whether_config::{CommonConfig, DurationStr, resolve_presentation};
pub use whether_config::{Issue, Severity};
use whether_core::model::ProviderId;
use whether_panel::PanelConfig;
use whether_tui::config::LayoutPreset;

use crate::{
    Device, FwDisplay, FwGuiConfig, FwLocation, FwOwmApi, FwPower, FwProvider, FwRuntimeConfig,
    FwTuiConfig, Night, Tls, Wifi,
};

/// What the firmware being built supports.
#[derive(Clone, Copy, Debug)]
pub struct Target<'a> {
    /// The board profile being built.
    pub board: &'a str,
    /// Whether the board has PSRAM (the Slint GUI needs it).
    pub psram: bool,
    /// The displays compiled into this build.
    pub displays: &'a [FwDisplay],
}

/// A built configuration and the environment variables it read (for
/// `cargo:rerun-if-env-changed`).
#[derive(Debug)]
pub struct Built {
    /// The configuration to embed.
    pub config: FwRuntimeConfig,
    /// Variables looked up while resolving secrets and the locale.
    pub env_vars: Vec<String>,
    /// Warnings to print (`cargo:warning`).
    pub warnings: Vec<Issue>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WifiToml {
    ssid: Secret,
    password: Secret,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NightToml {
    from: String,
    to: String,
    brightness: u8,
}

fn default_display() -> FwDisplay {
    FwDisplay::Panel
}
fn default_brightness() -> u8 {
    80
}
fn default_page_interval() -> DurationStr {
    DurationStr(15)
}
fn yes() -> bool {
    true
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeviceToml {
    board: String,
    #[serde(default = "default_display")]
    display: FwDisplay,
    #[serde(default)]
    rotation: u16,
    #[serde(default = "default_brightness")]
    brightness: u8,
    #[serde(default = "default_page_interval")]
    page_interval: DurationStr,
    #[serde(default)]
    night: Option<NightToml>,
    #[serde(default)]
    power: FwPower,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TlsToml {
    #[serde(default = "yes")]
    verify: bool,
    #[serde(default)]
    roots: BTreeMap<String, String>,
}

impl Default for TlsToml {
    fn default() -> Self {
        Self {
            verify: true,
            roots: BTreeMap::new(),
        }
    }
}

fn compact() -> LayoutPreset {
    LayoutPreset::Compact
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TuiToml {
    #[serde(default = "compact")]
    layout: LayoutPreset,
    #[serde(default = "yes")]
    art: bool,
}

impl Default for TuiToml {
    fn default() -> Self {
        Self {
            layout: LayoutPreset::Compact,
            art: true,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GuiToml {
    #[serde(default = "yes")]
    dynamic_background: bool,
}

impl Default for GuiToml {
    fn default() -> Self {
        Self {
            dynamic_background: true,
        }
    }
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct DisplayToml {
    tui: TuiToml,
    panel: PanelConfig,
    gui: GuiToml,
}

/// Collects issues with paths.
#[derive(Default)]
struct Issues(Vec<Issue>);

impl Issues {
    fn error(&mut self, path: &str, message: impl Into<String>) {
        self.push(Severity::Error, path, message);
    }
    fn warning(&mut self, path: &str, message: impl Into<String>) {
        self.push(Severity::Warning, path, message);
    }
    fn push(&mut self, severity: Severity, path: &str, message: impl Into<String>) {
        self.0.push(Issue {
            severity,
            path: path.into(),
            message: message.into(),
        });
    }
}

/// Takes `[name]` out of `table` and deserializes it (default when absent).
fn section<T: for<'de> Deserialize<'de> + Default>(
    table: &mut toml::Table,
    name: &str,
    issues: &mut Issues,
) -> Option<T> {
    match table.remove(name) {
        None => Some(T::default()),
        Some(value) => value
            .try_into()
            .map_err(|e: toml::de::Error| {
                issues.error(name, e.message().trim().to_string());
            })
            .ok(),
    }
}

/// Takes a required `[name]` out of `table`.
fn required<T: for<'de> Deserialize<'de>>(
    table: &mut toml::Table,
    name: &str,
    issues: &mut Issues,
) -> Option<T> {
    match table.remove(name) {
        None => {
            issues.error(name, format!("[{name}] is required"));
            None
        }
        Some(value) => value
            .try_into()
            .map_err(|e: toml::de::Error| {
                issues.error(name, e.message().trim().to_string());
            })
            .ok(),
    }
}

/// `"HH:MM"` as minutes after midnight.
fn minutes(s: &str) -> Option<u16> {
    let (h, m) = s.split_once(':')?;
    let (h, m): (u16, u16) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60).then_some(h * 60 + m)
}

fn device(t: DeviceToml, target: &Target<'_>, issues: &mut Issues) -> Device {
    if t.board != target.board {
        issues.error(
            "device.board",
            format!(
                "`{}` does not match the board being built (`{}`)",
                t.board, target.board
            ),
        );
    }
    if !target.displays.contains(&t.display) {
        issues.error(
            "device.display",
            format!("{:?} is not compiled into this build", t.display),
        );
    }
    if t.display == FwDisplay::Gui && !target.psram {
        issues.error(
            "device.display",
            "the Slint GUI needs PSRAM; this board has none (use \"panel\")",
        );
    }
    if ![0, 90, 180, 270].contains(&t.rotation) {
        issues.error("device.rotation", "use 0, 90, 180 or 270");
    }
    if !(1..=100).contains(&t.brightness) {
        issues.error("device.brightness", "use 1 to 100 (percent)");
    }
    let night = t.night.and_then(|n| {
        let (from, to) = (minutes(&n.from), minutes(&n.to));
        if from.is_none() || to.is_none() {
            issues.error("device.night", "from and to are \"HH:MM\" times");
        }
        if n.brightness > 100 {
            issues.error("device.night.brightness", "use 0 to 100 (percent)");
        }
        Some(Night {
            from_min: from?,
            to_min: to?,
            brightness: n.brightness.min(100),
        })
    });
    Device {
        board: t.board,
        display: t.display,
        rotation: t.rotation,
        brightness: t.brightness,
        page_interval_s: t.page_interval.0,
        night,
        power: t.power,
    }
}

fn resolve(
    secret: &Secret,
    env: &dyn Fn(&str) -> Option<String>,
    path: &str,
    issues: &mut Issues,
) -> Option<String> {
    secret
        .resolve(env)
        .map_err(|e| issues.error(path, e.to_string()))
        .ok()
}

fn providers(
    ids: &[ProviderId],
    common: &CommonConfig,
    env: &dyn Fn(&str) -> Option<String>,
    path: &str,
    issues: &mut Issues,
) -> Vec<FwProvider> {
    let p = &common.providers;
    let mut out = Vec::new();
    for id in ids {
        let provider = match id {
            ProviderId::OpenMeteo => Some(FwProvider::OpenMeteo {
                api_key: p
                    .open_meteo
                    .api_key
                    .as_ref()
                    .and_then(|k| resolve(k, env, "providers.open-meteo.api_key", issues)),
            }),
            ProviderId::OpenWeather => {
                let owm = p.openweather.as_ref();
                let key = owm.and_then(|o| o.api_key.as_ref());
                match key.map(|k| k.resolve(env)) {
                    Some(Ok(api_key)) => Some(FwProvider::OpenWeather {
                        api_key,
                        api: match owm.and_then(|o| o.api).unwrap_or_default() {
                            OwmApi::OneCall4 => FwOwmApi::OneCall4,
                            OwmApi::Free | OwmApi::OneCall3 => FwOwmApi::Free,
                        },
                        daily_call_limit: owm
                            .and_then(|o| o.daily_call_limit)
                            .unwrap_or(whether_providers::openweather::DEFAULT_DAILY_CALL_LIMIT),
                    }),
                    // Like on Linux: without a key OpenWeather is left out.
                    Some(Err(e)) => {
                        issues.warning(path, format!("OpenWeather left out of the firmware ({e})"));
                        None
                    }
                    None => None,
                }
            }
            ProviderId::MetNorway => {
                let met = p.met_norway.as_ref();
                met.and_then(|m| m.contact.clone())
                    .map(|contact| FwProvider::MetNorway {
                        contact,
                        compact: met.and_then(|m| m.product) == Some(MetProduct::Compact),
                    })
            }
            // Its responses (polygons, full texts) outgrow the device's buffers.
            ProviderId::Nws => {
                issues.warning(
                    path,
                    "NWS alerts are left out of the firmware (Linux and iPhone only)",
                );
                None
            }
            // History only; validation refuses it in the forecast lists.
            ProviderId::Acis => None,
            // Host-only: a 60 KB response outgrows the device's buffers and its
            // monthly budget is not a day reservation.
            ProviderId::PirateWeather => {
                issues.warning(
                    path,
                    "Pirate Weather is left out of the firmware (Linux and iPhone only)",
                );
                None
            }
        };
        out.extend(provider);
    }
    out
}

/// Locations need coordinates: there is no geocoding on the device.
fn check_locations(common: &CommonConfig, issues: &mut Issues) {
    for (i, loc) in common.location.iter().enumerate() {
        let path = format!("location[{i}]");
        if loc.latitude.is_none() || loc.longitude.is_none() {
            issues.error(
                &path,
                "the device needs latitude and longitude (no geocoding on the device; use `whether locate`)",
            );
        }
        if loc.query.is_some() || loc.zip.is_some() {
            issues.error(&path, "query and zip are not supported on the device");
        }
    }
}

/// The runtime configuration from validated parts.
#[allow(
    clippy::too_many_arguments,
    reason = "one argument per resolved section"
)]
fn assemble(
    wifi: Wifi,
    device: Device,
    tls: TlsToml,
    display: DisplayToml,
    common: CommonConfig,
    chain: Vec<FwProvider>,
    supplements: Vec<FwProvider>,
    presentation: whether_core::presentation::PresentationConfig,
) -> FwRuntimeConfig {
    FwRuntimeConfig {
        wifi,
        device,
        tls: Tls {
            verify: tls.verify,
            roots: tls.roots.into_iter().collect(),
        },
        locations: common
            .location
            .iter()
            .enumerate()
            .map(|(i, l)| FwLocation {
                name: l
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("Location {}", i + 1)),
                latitude: l.latitude.unwrap_or_default(),
                longitude: l.longitude.unwrap_or_default(),
                provider: l.provider,
            })
            .collect(),
        providers: chain,
        supplements,
        presentation,
        strings: common.strings.into_iter().collect(),
        refresh_s: common.general.effective_refresh().0,
        coordinate_precision: common.general.coordinate_precision,
        user_agent: format!("whether-fw/{}", env!("CARGO_PKG_VERSION")),
        tui: FwTuiConfig {
            layout: display.tui.layout,
            art: display.tui.art,
        },
        panel: display.panel,
        gui: FwGuiConfig {
            dynamic_background: display.gui.dynamic_background,
        },
    }
}

/// Parses, validates and resolves `text` for `target`. Errors come back together;
/// warnings travel in [`Built::warnings`].
pub fn build(
    text: &str,
    env: &dyn Fn(&str) -> Option<String>,
    target: &Target<'_>,
) -> Result<Built, Vec<Issue>> {
    // Record every variable the configuration reads.
    let read = RefCell::new(Vec::<String>::new());
    let env = |name: &str| {
        let mut r = read.borrow_mut();
        if !r.iter().any(|n| n == name) {
            r.push(name.to_string());
        }
        env(name)
    };
    let mut issues = Issues::default();
    let mut table: toml::Table = toml::from_str(text).map_err(|e| {
        vec![Issue {
            severity: Severity::Error,
            path: String::new(),
            message: e.to_string(),
        }]
    })?;
    let wifi: Option<WifiToml> = required(&mut table, "wifi", &mut issues);
    let device_toml: Option<DeviceToml> = required(&mut table, "device", &mut issues);
    let tls: Option<TlsToml> = section(&mut table, "tls", &mut issues);
    let display: Option<DisplayToml> = section(&mut table, "display", &mut issues);
    let common: Option<CommonConfig> = toml::Value::Table(table)
        .try_into()
        .map_err(|e: toml::de::Error| issues.error("", e.message().trim().to_string()))
        .ok();
    let (Some(wifi), Some(device_toml), Some(tls), Some(display), Some(common)) =
        (wifi, device_toml, tls, display, common)
    else {
        return Err(issues.0);
    };
    issues.0.extend(common.validate());
    check_locations(&common, &mut issues);
    for (host, file) in &tls.roots {
        if !crate::KNOWN_ROOTS.contains(&file.as_str()) {
            issues.error(
                &format!("tls.roots.\"{host}\""),
                format!(
                    "`{file}` is not compiled in; use one of {}",
                    crate::KNOWN_ROOTS.join(", ")
                ),
            );
        }
    }
    let device = device(device_toml, target, &mut issues);
    let ssid = resolve(&wifi.ssid, &env, "wifi.ssid", &mut issues);
    if ssid.as_deref().is_some_and(|s| s.trim().is_empty()) {
        issues.error("wifi.ssid", "the network name is empty");
    }
    let password = resolve(&wifi.password, &env, "wifi.password", &mut issues);
    let chain = providers(
        &common.providers.order,
        &common,
        &env,
        "providers.order",
        &mut issues,
    );
    if chain.is_empty() {
        issues.error("providers.order", "no provider is usable on the device");
    }
    let supplements = providers(
        &common.providers.supplement,
        &common,
        &env,
        "providers.supplement",
        &mut issues,
    );
    let locale = ["LC_ALL", "LC_MEASUREMENT", "LANG"]
        .into_iter()
        .find_map(|name| env(name).filter(|v| !v.is_empty()));
    let presentation = resolve_presentation(
        &common.general,
        &common.units,
        &common.presentation,
        locale.as_deref(),
    );
    if issues.0.iter().any(|i| i.severity == Severity::Error) {
        return Err(issues.0);
    }
    let (Some(ssid), Some(password)) = (ssid, password) else {
        return Err(issues.0);
    };
    let config = assemble(
        Wifi { ssid, password },
        device,
        tls,
        display,
        common,
        chain,
        supplements,
        presentation,
    );
    let warnings = issues.0;
    Ok(Built {
        config,
        env_vars: read.into_inner(),
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = include_str!("../../whether-fw.example.toml");

    fn env(name: &str) -> Option<String> {
        match name {
            "WHETHER_WIFI_SSID" => Some("home".into()),
            "WHETHER_WIFI_PASSWORD" => Some("hunter2".into()),
            "LANG" => Some("en_US.UTF-8".into()),
            _ => None,
        }
    }

    const WROOM: Target<'static> = Target {
        board: "esp32-wroom-spi",
        psram: false,
        displays: &[FwDisplay::Tui, FwDisplay::Panel],
    };

    #[test]
    fn the_example_builds_and_round_trips_through_postcard() {
        let built = build(EXAMPLE, &env, &WROOM).unwrap();
        let c = &built.config;
        assert_eq!(c.wifi.ssid, "home");
        assert_eq!(c.device.display, FwDisplay::Panel);
        assert_eq!(c.device.page_interval_s, 15);
        assert_eq!(c.device.power, FwPower::AlwaysOn);
        assert_eq!(c.locations[0].name, "New York");
        // No OpenWeather key in this environment: Open-Meteo alone, with a warning.
        assert_eq!(
            c.providers.iter().map(FwProvider::id).collect::<Vec<_>>(),
            [ProviderId::OpenMeteo]
        );
        assert!(
            built
                .warnings
                .iter()
                .any(|w| w.message.contains("OpenWeather"))
        );
        assert_eq!(
            c.presentation.units,
            whether_core::units::UnitPrefs::IMPERIAL,
            "en_US resolves to imperial"
        );
        assert!(
            built
                .env_vars
                .contains(&"WHETHER_WIFI_PASSWORD".to_string())
        );
        assert!(built.env_vars.contains(&"OPENWEATHER_API_KEY".to_string()));
        let bytes = c.encode().unwrap();
        assert_eq!(&FwRuntimeConfig::decode(&bytes).unwrap(), c);
        assert!(
            !format!("{c:?}").contains("hunter2"),
            "Debug hides the password"
        );
    }

    #[test]
    fn firmware_rules_are_enforced() {
        let errors = |text: &str, target: &Target<'_>| -> Vec<String> {
            build(text, &env, target)
                .unwrap_err()
                .into_iter()
                .filter(|i| i.severity == Severity::Error)
                .map(|i| i.path)
                .collect()
        };
        let other_board = Target {
            board: "cyd",
            ..WROOM
        };
        assert!(errors(EXAMPLE, &other_board).contains(&"device.board".to_string()));
        let gui = EXAMPLE.replace("display = \"panel\"", "display = \"gui\"");
        let with_gui = Target {
            displays: &[FwDisplay::Gui],
            ..WROOM
        };
        assert!(errors(&gui, &with_gui).contains(&"device.display".to_string()));
        let query = EXAMPLE.replace(
            "latitude = 40.7143\nlongitude = -74.0060",
            "query = \"New York\"",
        );
        assert!(errors(&query, &WROOM).contains(&"location[0]".to_string()));
        let no_wifi = |n: &str| match n {
            "WHETHER_WIFI_SSID" => Some(" ".into()),
            other => env(other),
        };
        let issues = build(EXAMPLE, &no_wifi, &WROOM).unwrap_err();
        assert!(issues.iter().any(|i| i.path == "wifi.ssid"));
        let missing = EXAMPLE.replace("[wifi]", "[wifi_typo]");
        assert_ne!(errors(&missing, &WROOM), Vec::<String>::new());
        let sleepy = EXAMPLE.replace("power = \"always-on\"", "power = \"deep-sleep\"");
        assert_eq!(
            build(&sleepy, &env, &WROOM).unwrap().config.device.power,
            FwPower::DeepSleep
        );
        let typo = EXAMPLE.replace("power = \"always-on\"", "power = \"hibernate\"");
        assert_ne!(errors(&typo, &WROOM), Vec::<String>::new());
        let rotation = EXAMPLE.replace("rotation = 0", "rotation = 45");
        assert!(errors(&rotation, &WROOM).contains(&"device.rotation".to_string()));
        let root = EXAMPLE.replace(
            "verify = true",
            "verify = true\nroots = { \"api.met.no\" = \"missing.der\" }",
        );
        assert!(errors(&root, &WROOM).contains(&"tls.roots.\"api.met.no\"".to_string()));
    }

    #[test]
    fn night_times() {
        assert_eq!(minutes("22:00"), Some(1320));
        assert_eq!(minutes("07:05"), Some(425));
        assert_eq!(minutes("24:00"), None);
        assert_eq!(minutes("7"), None);
    }
}
