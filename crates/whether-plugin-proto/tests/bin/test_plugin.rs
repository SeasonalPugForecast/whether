//! A plugin for the exec integration tests. `WHETHER_TEST_PLUGIN` picks the
//! behaviour:
//! - `echo`: ready; on each state, a refresh for the active location; exits on
//!   `shutdown`.
//! - `crash`: ready, one refresh, then exit 3.
//! - `silent`: never sends `ready`.
//! - `stubborn`: ready, then ignores `shutdown` and the end of input.

use std::io::{BufRead, Write};

fn send(line: &str) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

fn main() {
    let mode = std::env::var("WHETHER_TEST_PLUGIN").unwrap_or_else(|_| "echo".into());
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let Ok(message) = serde_json::from_str::<serde_json::Value>(&line) else {
            eprintln!("bad line: {line}");
            continue;
        };
        match (mode.as_str(), message["type"].as_str()) {
            (m, Some("hello")) if m != "silent" => {
                send(r#"{"type":"ready","name":"test-plugin","protocol":1}"#);
                send(r#"{"type":"log","level":"info","message":"hello"}"#);
                if mode == "crash" {
                    send(r#"{"type":"command","command":{"kind":"refresh","location":0}}"#);
                    std::process::exit(3);
                }
            }
            ("echo", Some("state")) => {
                let active = message["active"].as_u64().unwrap_or(0);
                send(&format!(
                    r#"{{"type":"command","command":{{"kind":"refresh","location":{active}}}}}"#
                ));
            }
            (m, Some("shutdown")) if m != "stubborn" && m != "silent" => return,
            _ => {}
        }
    }
    if mode == "stubborn" {
        std::thread::sleep(std::time::Duration::from_secs(30));
    }
}
