//! Golden protocol examples built from the core sample data. `cargo xtask schema`
//! writes them to `sdk/schema/examples/`, where the Rust and C++ tests read them.

use whether_core::testdata;

use crate::{CommandMsg, HostMessage, LogLevel, PROTOCOL_VERSION, PluginMessage, encode};

fn lines<T: serde::Serialize>(messages: &[T]) -> String {
    messages.iter().map(encode).collect()
}

/// `(file name, contents)` for every example, one message per line.
pub fn examples() -> Vec<(&'static str, String)> {
    let hello = HostMessage::Hello {
        protocol: PROTOCOL_VERSION,
        app_version: env!("CARGO_PKG_VERSION").into(),
        plugin_name: "notify".into(),
        config: serde_json::json!({ "lead_minutes": 60, "min_probability": 60 }),
    };
    let state = HostMessage::State {
        seq: 1,
        active: 0,
        view: Box::new(testdata::sample_view()),
        snapshots: None,
    };
    let ready = PluginMessage::Ready {
        name: "whether-notify".into(),
        protocol: PROTOCOL_VERSION,
    };
    let commands = [
        CommandMsg::Refresh { location: Some(0) },
        CommandMsg::SelectLocation { index: 1 },
        CommandMsg::Quit,
    ]
    .map(|command| PluginMessage::Command { command });
    let log = PluginMessage::Log {
        level: LogLevel::Info,
        message: "notify-send not found; notifications are off".into(),
    };
    vec![
        ("hello.jsonl", lines(&[hello])),
        ("state.jsonl", lines(&[state])),
        ("shutdown.jsonl", lines(&[HostMessage::Shutdown])),
        ("ready.jsonl", lines(&[ready])),
        ("command.jsonl", lines(&commands)),
        ("log.jsonl", lines(&[log])),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_plugin_line;

    #[test]
    fn plugin_examples_parse_and_host_examples_are_typed() {
        for (name, text) in examples() {
            assert!(text.ends_with('\n'), "{name}");
            for line in text.lines() {
                let value: serde_json::Value = serde_json::from_str(line).unwrap();
                let kind = value["type"].as_str().unwrap();
                if ["ready", "command", "log"].contains(&kind) {
                    parse_plugin_line(line.as_bytes()).unwrap();
                } else {
                    assert!(
                        ["hello", "state", "shutdown"].contains(&kind),
                        "{name}: {kind}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_state_example_carries_the_sample_view() {
        let (_, state) = examples()
            .into_iter()
            .find(|(n, _)| *n == "state.jsonl")
            .unwrap();
        let value: serde_json::Value = serde_json::from_str(&state).unwrap();
        assert_eq!(value["view"]["location"]["name"], "New York");
        assert!(
            value.get("snapshots").is_none(),
            "snapshots only on request"
        );
        assert!(
            value["view"]["attribution"]
                .as_str()
                .unwrap()
                .contains("Open-Meteo")
        );
    }
}
