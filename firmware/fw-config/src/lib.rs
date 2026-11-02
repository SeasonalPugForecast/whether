#![cfg_attr(not(feature = "build"), no_std)]
//! The firmware's runtime configuration: validated and resolved at build time by
//! `whether-fw/build.rs` (feature `build`), embedded as a postcard blob, decoded
//! on the device.
//!
//! Every type here is postcard-safe: no `untagged`, `flatten` or
//! `skip_serializing_if`, because postcard is not self-describing.

extern crate alloc;

#[cfg(feature = "build")]
pub mod build;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};
use whether_core::model::ProviderId;
use whether_core::presentation::PresentationConfig;
use whether_panel::PanelConfig;

/// Which display the device starts with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FwDisplay {
    /// The terminal-style display (mousefood).
    Tui,
    /// The Mousam-style panel (embedded-graphics).
    Panel,
    /// The Slint GUI (ESP32-S3 with PSRAM only).
    Gui,
}

/// How the device spends the time between refreshes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FwPower {
    /// Stay connected and keep the display lit (mains power).
    #[default]
    AlwaysOn,
    /// Wake on a timer, fetch, draw and deep-sleep (battery, e-paper).
    DeepSleep,
}

/// `[wifi]`, resolved.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wifi {
    /// Network name.
    pub ssid: String,
    /// Password; empty for open networks.
    pub password: String,
}

impl core::fmt::Debug for Wifi {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Wifi")
            .field("ssid", &self.ssid)
            .field("password", &"***")
            .finish()
    }
}

/// Backlight dimming at night, in the location's local time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Night {
    /// Start, minutes after midnight.
    pub from_min: u16,
    /// End, minutes after midnight.
    pub to_min: u16,
    /// Backlight percent while dimmed.
    pub brightness: u8,
}

/// `[device]`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    /// Board profile (`firmware/boards.toml`).
    pub board: String,
    /// The display shown at start.
    pub display: FwDisplay,
    /// Panel rotation in degrees: 0, 90, 180 or 270.
    pub rotation: u16,
    /// Backlight percent, 1–100.
    pub brightness: u8,
    /// Seconds between automatic page changes; 0 = manual only.
    pub page_interval_s: u32,
    /// Optional night dimming.
    pub night: Option<Night>,
    /// Always on, or deep sleep between refreshes.
    pub power: FwPower,
}

/// `[tls]`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tls {
    /// Verify certificates (false shows a permanent warning).
    pub verify: bool,
    /// Root certificate file (in `firmware/certs/`) per API host.
    pub roots: Vec<(String, String)>,
}

/// The root certificates compiled into the firmware (`firmware/certs/*.der`).
pub const KNOWN_ROOTS: [&str; 3] = [
    "isrg-root-x1.der",
    "usertrust-rsa.der",
    "harica-rootca-2015.der",
];

/// The root that verifies `host` unless `[tls] roots` says otherwise. The device
/// verifies the chain a server sends link by link, so this is the issuer of the
/// *last* certificate in that chain (observed 2026-06-28).
pub fn default_root(host: &str) -> Option<&'static str> {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host == "open-meteo.com" || host.ends_with(".open-meteo.com") {
        Some("isrg-root-x1.der")
    } else if host == "openweathermap.org" || host.ends_with(".openweathermap.org") {
        Some("usertrust-rsa.der")
    } else if host == "api.met.no" {
        Some("harica-rootca-2015.der")
    } else {
        None
    }
}

/// The root for `host`: an entry of `roots` (from `[tls] roots`) first, else
/// [`default_root`].
pub fn root_for<'a>(host: &str, roots: &'a [(String, String)]) -> Option<&'a str> {
    roots
        .iter()
        .find(|(h, _)| h.eq_ignore_ascii_case(host))
        .map(|(_, file)| file.as_str())
        .or_else(|| default_root(host))
}

/// A location with explicit coordinates (there is no geocoding on the device).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FwLocation {
    /// Display name.
    pub name: String,
    /// Latitude in degrees.
    pub latitude: f64,
    /// Longitude in degrees.
    pub longitude: f64,
    /// A provider override for this location.
    pub provider: Option<ProviderId>,
}

/// Which `OpenWeather` API a key is used with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FwOwmApi {
    /// The free 2.5 endpoints.
    Free,
    /// One Call 4.0, metered by `daily_call_limit`.
    OneCall4,
}

/// A provider with its resolved settings.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FwProvider {
    /// `OpenWeather`.
    OpenWeather {
        /// The resolved key.
        api_key: String,
        /// Which API.
        api: FwOwmApi,
        /// Metered calls allowed per UTC day.
        daily_call_limit: u32,
    },
    /// Open-Meteo.
    OpenMeteo {
        /// A commercial key, if any.
        api_key: Option<String>,
    },
    /// MET Norway.
    MetNorway {
        /// Contact for the User-Agent.
        contact: String,
        /// The compact product instead of complete.
        compact: bool,
    },
}

impl FwProvider {
    /// Which provider this is.
    pub fn id(&self) -> ProviderId {
        match self {
            Self::OpenWeather { .. } => ProviderId::OpenWeather,
            Self::OpenMeteo { .. } => ProviderId::OpenMeteo,
            Self::MetNorway { .. } => ProviderId::MetNorway,
        }
    }
}

impl core::fmt::Debug for FwProvider {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Keys never reach a log.
        f.debug_tuple("FwProvider").field(&self.id()).finish()
    }
}

/// `[display.tui]` on the device.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FwTuiConfig {
    /// The layout preset.
    pub layout: whether_tui::config::LayoutPreset,
    /// Large ASCII-art icon.
    pub art: bool,
}

/// `[display.gui]` on the device (ESP32-S3 only).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FwGuiConfig {
    /// Sky gradient that follows the weather.
    pub dynamic_background: bool,
}

/// Everything the firmware needs, resolved at build time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FwRuntimeConfig {
    /// `[wifi]`.
    pub wifi: Wifi,
    /// `[device]`.
    pub device: Device,
    /// `[tls]`.
    pub tls: Tls,
    /// Locations in config order.
    pub locations: Vec<FwLocation>,
    /// The provider chain: first is primary, later ones are fallbacks.
    pub providers: Vec<FwProvider>,
    /// Supplements that fill gaps (none by default on the device).
    pub supplements: Vec<FwProvider>,
    /// Resolved presentation (units already decided at build time).
    pub presentation: PresentationConfig,
    /// `[strings]` overrides.
    pub strings: BTreeMap<String, String>,
    /// Seconds between refreshes.
    pub refresh_s: u32,
    /// Decimals sent for coordinates.
    pub coordinate_precision: u8,
    /// `whether-fw/<version>`.
    pub user_agent: String,
    /// `[display.tui]`.
    pub tui: FwTuiConfig,
    /// `[display.panel]`.
    pub panel: PanelConfig,
    /// `[display.gui]`.
    pub gui: FwGuiConfig,
}

/// Why the embedded blob could not be decoded (a build/firmware mismatch).
pub type DecodeError = postcard::Error;

impl FwRuntimeConfig {
    /// Decodes the embedded blob.
    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        postcard::from_bytes(bytes)
    }

    /// Encodes for embedding.
    pub fn encode(&self) -> Result<Vec<u8>, DecodeError> {
        postcard::to_allocvec(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec;

    #[test]
    fn hosts_map_to_the_issuer_of_their_last_certificate() {
        assert_eq!(default_root("api.open-meteo.com"), Some("isrg-root-x1.der"));
        assert_eq!(
            default_root("Air-Quality-API.open-meteo.com."),
            Some("isrg-root-x1.der")
        );
        assert_eq!(
            default_root("api.openweathermap.org"),
            Some("usertrust-rsa.der")
        );
        assert_eq!(default_root("api.met.no"), Some("harica-rootca-2015.der"));
        assert_eq!(default_root("example.com"), None);
        assert_eq!(default_root("evil-open-meteo.com"), None);
        for root in [
            default_root("api.open-meteo.com"),
            default_root("api.openweathermap.org"),
            default_root("api.met.no"),
        ] {
            assert!(KNOWN_ROOTS.contains(&root.unwrap()));
        }
    }

    #[test]
    fn configured_roots_come_first() {
        let roots = vec![("API.met.no".to_string(), "isrg-root-x1.der".to_string())];
        assert_eq!(root_for("api.met.no", &roots), Some("isrg-root-x1.der"));
        assert_eq!(
            root_for("api.open-meteo.com", &roots),
            Some("isrg-root-x1.der")
        );
        assert_eq!(root_for("example.com", &roots), None);
    }
}
