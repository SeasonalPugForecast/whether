//! The external display plugin protocol: JSON Lines over a child's stdio or the
//! daemon socket. One JSON object per `\n`-terminated UTF-8 line, at
//! most [`MAX_LINE`] bytes.

#[cfg(feature = "examples")]
pub mod examples;
pub mod exec;
pub mod messages;
pub mod socket;

pub use messages::{CommandMsg, HostMessage, LogLevel, PluginMessage};

/// The protocol version this crate speaks.
pub const PROTOCOL_VERSION: u32 = 1;

/// The longest line either side accepts, newline excluded (1 MiB).
pub const MAX_LINE: usize = 1 << 20;

/// The message types a plugin may send.
const PLUGIN_TYPES: [&str; 3] = ["ready", "command", "log"];

/// Why a plugin line was rejected.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ProtocolError {
    /// The line is longer than [`MAX_LINE`].
    #[error("line of {len} bytes exceeds the {MAX_LINE}-byte limit")]
    TooLong {
        /// Its length.
        len: usize,
    },
    /// The line is not UTF-8.
    #[error("line is not UTF-8")]
    NotUtf8,
    /// The line is not JSON.
    #[error("invalid JSON: {0}")]
    Json(String),
    /// The JSON is not an object with a string `type`, or its fields are wrong.
    #[error("unexpected message shape: {0}")]
    Shape(String),
    /// `type` names no plugin message.
    #[error("unknown message type `{0}`")]
    UnknownType(String),
}

/// Parses one line from a plugin (a trailing `\n` or `\r\n` is allowed). Unknown
/// fields are ignored.
pub fn parse_plugin_line(line: &[u8]) -> Result<PluginMessage, ProtocolError> {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    if line.len() > MAX_LINE {
        return Err(ProtocolError::TooLong { len: line.len() });
    }
    let text = std::str::from_utf8(line).map_err(|_| ProtocolError::NotUtf8)?;
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| ProtocolError::Json(e.to_string()))?;
    let kind = value
        .as_object()
        .ok_or_else(|| ProtocolError::Shape("not a JSON object".into()))?
        .get("type")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| ProtocolError::Shape("missing string field `type`".into()))?;
    if !PLUGIN_TYPES.contains(&kind) {
        return Err(ProtocolError::UnknownType(kind.into()));
    }
    serde_json::from_value(value).map_err(|e| ProtocolError::Shape(e.to_string()))
}

/// One line for `message`, `\n`-terminated.
///
/// # Panics
///
/// Never for protocol messages: they hold only strings, numbers and maps with
/// string keys.
pub fn encode<T: serde::Serialize>(message: &T) -> String {
    // INVARIANT: protocol messages only hold strings, numbers and maps with string
    // keys, which serde_json always serializes.
    let mut line = serde_json::to_string(message).expect("protocol messages serialize");
    line.push('\n');
    line
}

/// Either direction, for the combined JSON Schema.
#[derive(serde::Serialize, schemars::JsonSchema)]
#[serde(untagged)]
#[allow(dead_code, reason = "only its schema is used")]
enum AnyMessage {
    Host(HostMessage),
    Plugin(PluginMessage),
}

/// The protocol's JSON Schema (draft 2020-12): every host and plugin message.
pub fn schema() -> schemars::Schema {
    let mut schema = schemars::schema_for!(AnyMessage);
    schema.insert(
        "title".into(),
        format!("whether plugin protocol v{PROTOCOL_VERSION}").into(),
    );
    schema
}

#[cfg(test)]
mod tests;
