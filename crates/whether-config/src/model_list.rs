//! `[providers.open-meteo] models`: one model id, or several in priority order
//!. A string may also hold comma-separated ids, as Open-Meteo's own
//! parameter does.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use serde::de::{self, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Weather model ids in priority order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModelList(pub Vec<String>);

impl ModelList {
    /// The ids, in order.
    pub fn ids(&self) -> &[String] {
        &self.0
    }
}

/// Splits `"a, b"` into `["a", "b"]`, dropping empty parts.
fn split(s: &str) -> Vec<String> {
    s.split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(String::from)
        .collect()
}

impl Serialize for ModelList {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ModelList {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ModelsVisitor;
        impl<'de> Visitor<'de> for ModelsVisitor {
            type Value = ModelList;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a model id such as \"ncep_nbm_conus\", or a list of ids")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<ModelList, E> {
                Ok(ModelList(split(v)))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<ModelList, A::Error> {
                let mut ids = Vec::new();
                while let Some(id) = seq.next_element::<String>()? {
                    ids.push(String::from(id.trim()));
                }
                Ok(ModelList(ids))
            }
        }
        deserializer.deserialize_any(ModelsVisitor)
    }
}

#[cfg(feature = "schema")]
impl schemars::JsonSchema for ModelList {
    fn schema_name() -> alloc::borrow::Cow<'static, str> {
        "ModelList".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "Weather models in priority order: one id, or a list of up to 4.",
            "anyOf": [
                { "type": "string" },
                { "type": "array", "items": { "type": "string" }, "maxItems": 4 }
            ]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize, Serialize)]
    struct T {
        models: ModelList,
    }

    fn parse(toml_text: &str) -> Result<Vec<String>, toml::de::Error> {
        toml::from_str::<T>(toml_text).map(|t| t.models.0)
    }

    #[test]
    fn a_string_or_a_list() {
        assert_eq!(
            parse("models = \"ncep_nbm_conus\"").unwrap(),
            ["ncep_nbm_conus"]
        );
        assert_eq!(
            parse("models = [\"ncep_hrrr_conus\", \"ncep_nbm_conus\"]").unwrap(),
            ["ncep_hrrr_conus", "ncep_nbm_conus"]
        );
        assert_eq!(
            parse("models = \"gfs_seamless, ecmwf_ifs025\"").unwrap(),
            ["gfs_seamless", "ecmwf_ifs025"]
        );
        assert_eq!(parse("models = []").unwrap(), Vec::<String>::new());
        let err = parse("models = 3").unwrap_err().to_string();
        assert!(err.contains("a model id"), "{err}");
    }

    #[test]
    fn written_as_a_list() {
        let t = T {
            models: ModelList(alloc::vec!["ncep_nbm_conus".into()]),
        };
        assert_eq!(
            toml::to_string(&t).unwrap(),
            "models = [\"ncep_nbm_conus\"]\n"
        );
    }
}
