use super::*;

fn parse(line: &str) -> Result<PluginMessage, ProtocolError> {
    parse_plugin_line(line.as_bytes())
}

#[test]
fn plugin_messages_parse() {
    assert_eq!(
        parse(r#"{"type":"ready","name":"x","protocol":1}"#),
        Ok(PluginMessage::Ready {
            name: "x".into(),
            protocol: 1
        })
    );
    assert_eq!(
        parse("{\"type\":\"command\",\"command\":{\"kind\":\"refresh\"}}\r\n"),
        Ok(PluginMessage::Command {
            command: CommandMsg::Refresh { location: None }
        })
    );
    assert_eq!(
        parse(r#"{"type":"command","command":{"kind":"select_location","index":2}}"#),
        Ok(PluginMessage::Command {
            command: CommandMsg::SelectLocation { index: 2 }
        })
    );
}

#[test]
fn unknown_fields_are_ignored() {
    assert_eq!(
        parse(r#"{"type":"log","level":"warn","message":"m","since":"v2","extra":{"a":1}}"#),
        Ok(PluginMessage::Log {
            level: LogLevel::Warn,
            message: "m".into()
        })
    );
}

#[test]
fn malformed_lines_map_to_typed_errors() {
    assert!(matches!(parse("{\"type\":"), Err(ProtocolError::Json(_))));
    assert!(matches!(parse("[1,2]"), Err(ProtocolError::Shape(_))));
    assert!(matches!(
        parse(r#"{"name":"x"}"#),
        Err(ProtocolError::Shape(_))
    ));
    assert!(matches!(
        parse(r#"{"type":"ready","name":"x"}"#),
        Err(ProtocolError::Shape(_))
    ));
    assert!(matches!(
        parse(r#"{"type":"command","command":{"kind":"reboot"}}"#),
        Err(ProtocolError::Shape(_))
    ));
    assert_eq!(
        parse(r#"{"type":"hello"}"#),
        Err(ProtocolError::UnknownType("hello".into()))
    );
    assert_eq!(parse_plugin_line(b"\xff\xfe"), Err(ProtocolError::NotUtf8));
    let long = vec![b' '; MAX_LINE + 1];
    assert_eq!(
        parse_plugin_line(&long),
        Err(ProtocolError::TooLong { len: MAX_LINE + 1 })
    );
}

#[test]
fn lines_are_single_and_terminated() {
    let line = encode(&PluginMessage::Log {
        level: LogLevel::Info,
        message: "two\nlines".into(),
    });
    assert!(line.ends_with('\n'));
    assert_eq!(line.matches('\n').count(), 1, "{line}");
}

#[test]
fn host_messages_serialize() {
    insta::assert_snapshot!(encode(&HostMessage::Hello {
        protocol: PROTOCOL_VERSION,
        app_version: "0.1.0".into(),
        plugin_name: "notify".into(),
        config: serde_json::json!({ "lead_minutes": 60 }),
    }));
    insta::assert_snapshot!(encode(&HostMessage::Shutdown));
}

#[test]
fn schema_covers_both_directions() {
    let schema = serde_json::to_value(schema()).unwrap();
    let defs = schema
        .pointer("/$defs")
        .and_then(|d| d.as_object())
        .unwrap();
    for name in ["HostMessage", "PluginMessage", "ViewModel", "CommandMsg"] {
        assert!(defs.contains_key(name), "{name}");
    }
    assert_eq!(
        schema.pointer("/title").and_then(|t| t.as_str()),
        Some("whether plugin protocol v1")
    );
}
