//! Protocol v1 messages. Within a major version changes are
//! additive only, so both sides ignore unknown fields.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use whether_core::model::WeatherSnapshot;
use whether_core::view::ViewModel;

/// Host → plugin.
#[derive(Clone, Debug, PartialEq, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HostMessage {
    /// The first message: protocol version, app version and the plugin's config.
    Hello {
        /// [`crate::PROTOCOL_VERSION`].
        protocol: u32,
        /// The app's version.
        app_version: String,
        /// The plugin's name (`exec:<name>`).
        plugin_name: String,
        /// `[display.exec.<name>].config`, passed verbatim (never secrets).
        config: serde_json::Value,
    },
    /// What to show, sent on every change.
    State {
        /// Increases with every state message.
        seq: u64,
        /// The shown location.
        active: usize,
        /// The formatted view of the active location (boxed: it is large).
        view: Box<ViewModel>,
        /// Raw data per location, only with `send_snapshot = true`.
        // `Arc`: shared with the app's state, not copied per message.
        #[serde(skip_serializing_if = "Option::is_none")]
        snapshots: Option<Vec<Option<Arc<WeatherSnapshot>>>>,
    },
    /// The host is stopping; exit promptly.
    Shutdown,
}

/// A request from the plugin to the app.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CommandMsg {
    /// Fetch now (one location, or all), within the refresh floor and call budgets.
    Refresh {
        /// Location index, or all when absent.
        #[serde(default)]
        location: Option<usize>,
    },
    /// Show another location.
    SelectLocation {
        /// Location index.
        index: usize,
    },
    /// Stop (when the plugin is the main display).
    Quit,
}

/// Severity of a plugin log line.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    /// Something failed.
    Error,
    /// Something is off.
    Warn,
    /// Progress.
    Info,
    /// Details.
    Debug,
}

/// Plugin → host.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PluginMessage {
    /// The answer to `hello`, within 5 seconds.
    Ready {
        /// The plugin's own name.
        name: String,
        /// The protocol version it speaks.
        protocol: u32,
    },
    /// A request to the app.
    Command {
        /// What to do.
        command: CommandMsg,
    },
    /// A line for the app's log (stdout is the protocol channel).
    Log {
        /// Severity.
        level: LogLevel,
        /// The text.
        message: String,
    },
}
