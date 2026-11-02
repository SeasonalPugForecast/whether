//! Durations written as `"90s"`, `"15m"`, `"1h30m"` or `"2h"`.

use alloc::string::String;
use core::fmt;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A duration in whole seconds, written with `h`, `m` and `s` parts in that order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct DurationStr(pub u32);

/// Why a duration string was rejected.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DurationError {
    /// The string was empty.
    #[error("empty duration; write e.g. \"15m\" or \"1h30m\"")]
    Empty,
    /// A number had no unit, or a unit had no number.
    #[error("`{0}` is not a duration; use h, m and s parts such as \"90s\", \"15m\" or \"1h30m\"")]
    Invalid(String),
    /// The duration does not fit in a `u32` of seconds.
    #[error("`{0}` is too long")]
    Overflow(String),
}

impl DurationStr {
    /// The duration in seconds.
    pub const fn secs(self) -> u32 {
        self.0
    }

    /// Parses `"90s"`, `"15m"`, `"1h30m"` or `"2h"`. Each unit appears at most once,
    /// in the order h, m, s; a bare number is rejected.
    pub fn parse(s: &str) -> Result<Self, DurationError> {
        if s.is_empty() {
            return Err(DurationError::Empty);
        }
        let invalid = || DurationError::Invalid(String::from(s));
        let overflow = || DurationError::Overflow(String::from(s));
        let mut total: u32 = 0;
        // Index into "hms" of the next allowed unit.
        let mut next_unit = 0;
        let mut number: Option<u32> = None;
        for c in s.chars() {
            if let Some(digit) = c.to_digit(10) {
                let n = number.unwrap_or(0);
                number = Some(
                    n.checked_mul(10)
                        .and_then(|n| n.checked_add(digit))
                        .ok_or_else(overflow)?,
                );
                continue;
            }
            let (unit, scale) = match c {
                'h' => (0, 3600),
                'm' => (1, 60),
                's' => (2, 1),
                _ => return Err(invalid()),
            };
            let n = number.take().ok_or_else(invalid)?;
            if unit < next_unit {
                return Err(invalid());
            }
            next_unit = unit + 1;
            total = n
                .checked_mul(scale)
                .and_then(|v| total.checked_add(v))
                .ok_or_else(overflow)?;
        }
        if number.is_some() {
            return Err(invalid());
        }
        Ok(Self(total))
    }
}

impl fmt::Display for DurationStr {
    /// The canonical form: `"1h30m"`, `"90s"` → `"1m30s"`, zero → `"0s"`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (h, m, s) = (self.0 / 3600, self.0 / 60 % 60, self.0 % 60);
        if self.0 == 0 {
            return f.write_str("0s");
        }
        if h > 0 {
            write!(f, "{h}h")?;
        }
        if m > 0 {
            write!(f, "{m}m")?;
        }
        if s > 0 {
            write!(f, "{s}s")?;
        }
        Ok(())
    }
}

impl Serialize for DurationStr {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for DurationStr {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct DurationVisitor;
        impl Visitor<'_> for DurationVisitor {
            type Value = DurationStr;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a duration string such as \"15m\" or \"1h30m\"")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<DurationStr, E> {
                DurationStr::parse(v).map_err(E::custom)
            }
        }
        deserializer.deserialize_str(DurationVisitor)
    }
}

#[cfg(feature = "schema")]
impl schemars::JsonSchema for DurationStr {
    fn schema_name() -> alloc::borrow::Cow<'static, str> {
        "Duration".into()
    }

    fn inline_schema() -> bool {
        true
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "pattern": "^(\\d+h)?(\\d+m)?(\\d+s)?$",
            "description": "A duration such as \"90s\", \"15m\" or \"1h30m\"."
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_unit_combinations() {
        for (text, secs) in [
            ("90s", 90),
            ("15m", 900),
            ("1h30m", 5400),
            ("2h", 7200),
            ("1h2m3s", 3723),
            ("0s", 0),
        ] {
            assert_eq!(DurationStr::parse(text), Ok(DurationStr(secs)), "{text}");
        }
    }

    #[test]
    fn rejects_malformed_strings() {
        assert_eq!(DurationStr::parse(""), Err(DurationError::Empty));
        for text in ["0", "15", "m", "15x", "1m1h", "1m2m", "1 h", "-5m", "h1"] {
            assert!(
                matches!(DurationStr::parse(text), Err(DurationError::Invalid(_))),
                "{text}"
            );
        }
        assert!(matches!(
            DurationStr::parse("99999999999s"),
            Err(DurationError::Overflow(_))
        ));
        assert!(matches!(
            DurationStr::parse("2000000h"),
            Err(DurationError::Overflow(_))
        ));
    }

    #[test]
    fn displays_canonically() {
        let show = |s| alloc::format!("{}", DurationStr(s));
        assert_eq!(show(0), "0s");
        assert_eq!(show(90), "1m30s");
        assert_eq!(show(5400), "1h30m");
        assert_eq!(show(3723), "1h2m3s");
    }

    #[test]
    fn round_trips_through_toml() {
        #[derive(Deserialize, Serialize, Debug, PartialEq)]
        struct T {
            refresh: DurationStr,
        }
        let t: T = toml::from_str("refresh = \"1h30m\"").unwrap();
        assert_eq!(t.refresh, DurationStr(5400));
        let err = toml::from_str::<T>("refresh = \"15\"")
            .unwrap_err()
            .to_string();
        assert!(err.contains("not a duration"), "{err}");
        assert!(toml::from_str::<T>("refresh = 15").is_err());
    }
}
