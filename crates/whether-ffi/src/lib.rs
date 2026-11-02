//! The Rust core for the iOS app, exported to Swift with UniFFI.
//!
//! Swift does the I/O: [`fetch`] runs the sans-IO fetch session and hands every
//! request to a [`FetchHost`] that the app implements with `URLSession`. Swift
//! keeps the snapshots and the call ledger as the JSON this crate returns, and
//! asks [`view_model`] for the display contract, also as JSON (the shape external
//! plugins receive). Values arrive formatted; Swift only draws them.

uniffi::setup_scaffolding!();

mod alerts;
mod fetch;
mod history;
mod import;
mod key;
mod minutes;
mod pirate_weather;
mod radar;
mod search;
mod settings;
mod units;
mod view;

pub use alerts::{AlertSeverity, ExternalAlert, with_alerts};
pub use fetch::{
    AttemptInfo, Cached, FetchHost, FetchOutcome, Header, HostError, HostMethod, HostRequest,
    HostResponse, fetch,
};
pub use history::{
    HistoryOutcome, Trip, fetch_climate, fetch_history, history_due, plan_view, trip_problem,
};
pub use import::{ImportedConfig, ImportedPlace, import_config};
pub use key::{KeyCheck, calls_today, check_openweather_key, one_call_usage_text};
pub use minutes::{MinuteStep, minutes_wanted, with_minutes};
pub use pirate_weather::{check_pirate_weather_key, month_usage_text};
pub use radar::{
    RadarFrame, RadarSource, radar_attribution, radar_base_template, radar_covers,
    radar_covers_with, radar_frame_labels, radar_frames, radar_latest_valid, radar_meta_url,
    radar_openweather_credit, radar_openweather_frames, radar_openweather_max_zoom, radar_source,
    radar_tile_template,
};
pub use search::{FoundPlace, search_places};
pub use settings::{
    DEFAULT_DAILY_CALLS, ModelPreset, OffsetChange, Place, Settings, SourceChoice, Units,
    default_settings, default_sources, model_presets, models_from_noaa_blend, nws_covers,
};
pub use units::{
    DistanceOption, DistanceUnit, PrecipitationOption, PrecipitationUnit, PressureOption,
    PressureUnit, TemperatureOption, TemperatureUnit, UnitLabels, UnitOverrides, WindOption,
    WindUnit, unit_labels,
};
pub use view::{LocationEntry, chance_text, radar_step_words, severity_word, view_model};

/// Why a call failed.
#[derive(Debug, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum WhetherError {
    /// The server reported no change: keep the cached snapshot.
    #[error("not modified")]
    NotModified,
    /// No provider produced a forecast; `summary` lists what each one said.
    #[error("no forecast: {summary}")]
    NoForecast {
        /// One line per attempt.
        summary: String,
    },
    /// No history source answered; `summary` lists what each one said.
    #[error("no history: {summary}")]
    NoHistory {
        /// One line per attempt.
        summary: String,
    },
    /// A trip that cannot be planned (dates backwards, over 31 days).
    #[error("{message}")]
    Trip {
        /// Why.
        message: String,
    },
    /// A place search failed (no network, or the service's error).
    #[error("search failed: {message}")]
    Search {
        /// What went wrong.
        message: String,
    },
    /// A desktop `config.toml` could not be read.
    #[error("config: {message}")]
    Config {
        /// The parser's or validator's message.
        message: String,
    },
    /// JSON from the app (a cached snapshot or ledger) could not be read.
    #[error("invalid {what}: {message}")]
    InvalidJson {
        /// What was being read.
        what: String,
        /// The parser's message.
        message: String,
    },
}

/// The core's version, for the About screen.
#[uniffi::export]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// The contact the weather services' terms ask for (NWS, MET Norway): the
/// project URL, which is also the repository URL. `None` while it is empty or a
/// `.invalid` placeholder (before the project has a real one); services that
/// require a contact stay off then (no email, the URL only).
#[uniffi::export]
pub fn api_contact() -> Option<String> {
    contact_from(env!("CARGO_PKG_REPOSITORY"))
}

/// `url` as a contact, unless it is empty or a placeholder.
fn contact_from(url: &str) -> Option<String> {
    let url = url.trim();
    (!url.is_empty() && !url.contains(".invalid")).then(|| url.to_string())
}

/// The User-Agent the weather services see: `whether/<version> (iOS; +<contact>)`
/// (no `+` part while there is no [`api_contact`]).
pub(crate) fn user_agent() -> String {
    user_agent_with(api_contact().as_deref())
}

fn user_agent_with(contact: Option<&str>) -> String {
    match contact {
        Some(contact) => format!("whether/{} (iOS; +{contact})", env!("CARGO_PKG_VERSION")),
        None => format!("whether/{} (iOS)", env!("CARGO_PKG_VERSION")),
    }
}

/// `json` as `T`, or [`WhetherError::InvalidJson`] naming `what`.
fn from_json<T: serde::de::DeserializeOwned>(what: &str, json: &str) -> Result<T, WhetherError> {
    serde_json::from_str(json).map_err(|e| WhetherError::InvalidJson {
        what: what.to_string(),
        message: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_agent_names_the_version_and_platform() {
        let agent = user_agent();
        assert!(agent.starts_with(&format!("whether/{} (iOS", env!("CARGO_PKG_VERSION"))));
        assert!(agent.ends_with(')') && !agent.contains("+)"), "{agent}");
        assert!(
            !agent.contains(".invalid"),
            "a placeholder URL is not sent: {agent}"
        );
    }

    #[test]
    fn a_placeholder_is_no_api_contact() {
        assert_eq!(contact_from(""), None);
        assert_eq!(contact_from("https://example.invalid/whether"), None);
        assert_eq!(
            contact_from(" https://example.org/whether "),
            Some("https://example.org/whether".into())
        );
        // The workspace holds the real project URL, so MET Norway has its contact.
        assert_eq!(
            api_contact().as_deref(),
            Some("https://github.com/SeasonalPugForecast/whether")
        );
    }

    #[test]
    fn a_real_contact_is_the_url_only_in_the_user_agent() {
        assert_eq!(
            user_agent_with(Some("https://example.org/whether")),
            format!(
                "whether/{} (iOS; +https://example.org/whether)",
                env!("CARGO_PKG_VERSION")
            )
        );
        assert_eq!(
            user_agent_with(None),
            format!("whether/{} (iOS)", env!("CARGO_PKG_VERSION"))
        );
    }
}
