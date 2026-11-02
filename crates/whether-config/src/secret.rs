//! Secrets in configuration: a literal, an environment variable, a file or a command.
//!
//! `Debug` never shows the value, there is no `Display`, and serializing a literal
//! writes `"***"`. Resolution happens in the `std` loader.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use serde::de::{self, MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Where a secret comes from.
#[derive(Clone, PartialEq, Eq)]
pub enum Secret {
    /// The value itself (discouraged: prefer `{ env = … }`).
    Literal(String),
    /// `{ env = "VAR" }`: an environment variable.
    Env {
        /// Variable name.
        env: String,
    },
    /// `{ file = "~/.config/whether/owm.key" }`: a file's trimmed contents.
    File {
        /// Path; `~` expands to the home directory.
        file: String,
    },
    /// `{ command = ["pass", "show", "owm"] }`: a command's trimmed stdout (Linux only).
    Command {
        /// Program and arguments, run without a shell.
        command: Vec<String>,
    },
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

impl Serialize for Secret {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Literal(_) => serializer.serialize_str("***"),
            Self::Env { env } => single_entry(serializer, "env", env),
            Self::File { file } => single_entry(serializer, "file", file),
            Self::Command { command } => single_entry(serializer, "command", command),
        }
    }
}

fn single_entry<S: Serializer, V: Serialize + ?Sized>(
    serializer: S,
    key: &str,
    value: &V,
) -> Result<S::Ok, S::Error> {
    let mut map = serializer.serialize_map(Some(1))?;
    map.serialize_entry(key, value)?;
    map.end()
}

const FORMS: &str =
    "a string, { env = \"VAR\" }, { file = \"path\" } or { command = [\"program\", \"arg\"] }";

impl<'de> Deserialize<'de> for Secret {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct SecretVisitor;
        impl<'de> Visitor<'de> for SecretVisitor {
            type Value = Secret;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(FORMS)
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Secret, E> {
                Ok(Secret::Literal(String::from(v)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Secret, A::Error> {
                let key: String = map.next_key()?.ok_or_else(|| {
                    de::Error::custom(alloc::format!("empty table; expected {FORMS}"))
                })?;
                let secret = match key.as_str() {
                    "env" => Secret::Env {
                        env: map.next_value()?,
                    },
                    "file" => Secret::File {
                        file: map.next_value()?,
                    },
                    "command" => Secret::Command {
                        command: map.next_value()?,
                    },
                    other => {
                        return Err(de::Error::unknown_field(other, &["env", "file", "command"]));
                    }
                };
                if let Some(extra) = map.next_key::<String>()? {
                    return Err(de::Error::custom(alloc::format!(
                        "unexpected key `{extra}`: a secret table has exactly one of env, file or command"
                    )));
                }
                Ok(secret)
            }
        }
        deserializer.deserialize_any(SecretVisitor)
    }
}

#[cfg(feature = "schema")]
impl schemars::JsonSchema for Secret {
    fn schema_name() -> alloc::borrow::Cow<'static, str> {
        "Secret".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let one = |key: &str, value: serde_json::Value| {
            serde_json::json!({
                "type": "object",
                "properties": { key: value },
                "required": [key],
                "additionalProperties": false
            })
        };
        schemars::json_schema!({
            "description": "An API key: a literal string, or where to read it from.",
            "oneOf": [
                { "type": "string", "description": "The key itself (prefer { env = \"VAR\" })." },
                one("env", serde_json::json!({ "type": "string", "minLength": 1 })),
                one("file", serde_json::json!({ "type": "string", "minLength": 1 })),
                one("command", serde_json::json!({
                    "type": "array",
                    "items": { "type": "string" },
                    "minItems": 1
                }))
            ]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec;

    #[derive(Deserialize, Serialize, Debug)]
    struct T {
        key: Secret,
    }

    fn parse(text: &str) -> Result<Secret, String> {
        toml::from_str::<T>(text)
            .map(|t| t.key)
            .map_err(|e| e.to_string())
    }

    #[test]
    fn parses_every_form() {
        assert_eq!(parse("key = \"abc\""), Ok(Secret::Literal("abc".into())));
        assert_eq!(
            parse("key = { env = \"OPENWEATHER_API_KEY\" }"),
            Ok(Secret::Env {
                env: "OPENWEATHER_API_KEY".into()
            })
        );
        assert_eq!(
            parse("key = { file = \"~/k\" }"),
            Ok(Secret::File { file: "~/k".into() })
        );
        assert_eq!(
            parse("key = { command = [\"pass\", \"show\", \"owm\"] }"),
            Ok(Secret::Command {
                command: vec!["pass".into(), "show".into(), "owm".into()]
            })
        );
    }

    #[test]
    fn rejects_other_shapes() {
        assert!(parse("key = {}").unwrap_err().contains("empty table"));
        assert!(
            parse("key = { path = \"x\" }")
                .unwrap_err()
                .contains("unknown field `path`")
        );
        assert!(
            parse("key = { env = \"A\", file = \"b\" }")
                .unwrap_err()
                .contains("exactly one")
        );
        assert!(parse("key = 5").is_err());
        assert!(parse("key = { command = \"pass show\" }").is_err());
    }

    #[test]
    fn never_reveals_a_literal() {
        let literal = Secret::Literal("hunter2".into());
        assert_eq!(alloc::format!("{literal:?}"), "Secret(***)");
        let env = Secret::Env {
            env: "SECRET_VAR".into(),
        };
        assert_eq!(alloc::format!("{env:?}"), "Secret(***)");
        assert_eq!(
            toml::to_string(&T { key: literal }).unwrap(),
            "key = \"***\"\n"
        );
        let written = toml::to_string(&T { key: env }).unwrap();
        assert!(written.contains("env = \"SECRET_VAR\""), "{written}");
    }
}
