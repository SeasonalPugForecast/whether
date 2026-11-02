//! `{placeholder}` templates for the line display. `{{` and `}}` are literal braces.

use std::fmt;

/// A value a template can show. Every value comes from the `ViewModel`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum Field {
    Location,
    Temp,
    Feels,
    Hi,
    Lo,
    Condition,
    Icon,
    Summary,
    Wind,
    WindDir,
    Humidity,
    Pressure,
    Uv,
    Aqi,
    AqiLabel,
    PrecipProb,
    Sunrise,
    Sunset,
    Updated,
    Provider,
    /// The first active alert's title; empty without alerts.
    Alert,
    /// Today's normal high (history).
    NormalHigh,
    /// Today's normal low.
    NormalLow,
    /// Today's record high with its year: "89° in 1938".
    RecordHigh,
    /// "4° warmer than yesterday".
    YesterdayDelta,
}

/// Placeholder names and their fields.
pub const FIELDS: &[(&str, Field)] = &[
    ("location", Field::Location),
    ("temp", Field::Temp),
    ("feels", Field::Feels),
    ("hi", Field::Hi),
    ("lo", Field::Lo),
    ("condition", Field::Condition),
    ("icon", Field::Icon),
    ("summary", Field::Summary),
    ("wind", Field::Wind),
    ("wind_dir", Field::WindDir),
    ("humidity", Field::Humidity),
    ("pressure", Field::Pressure),
    ("uv", Field::Uv),
    ("aqi", Field::Aqi),
    ("aqi_label", Field::AqiLabel),
    ("precip_prob", Field::PrecipProb),
    ("sunrise", Field::Sunrise),
    ("sunset", Field::Sunset),
    ("updated", Field::Updated),
    ("normal_high", Field::NormalHigh),
    ("normal_low", Field::NormalLow),
    ("record_high", Field::RecordHigh),
    ("yesterday_delta", Field::YesterdayDelta),
    ("provider", Field::Provider),
    ("alert", Field::Alert),
];

/// Why a template was rejected.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TemplateError {
    /// `{name}` is not a known placeholder.
    #[error("unknown placeholder `{{{name}}}`; valid: {}", valid_names())]
    Unknown {
        /// The name used.
        name: String,
    },
    /// A `{` without a closing `}`.
    #[error("unclosed `{{` at byte {0}; write `{{{{` for a literal brace")]
    Unclosed(usize),
    /// A `}` without an opening `{`.
    #[error("unmatched `}}` at byte {0}; write `}}}}` for a literal brace")]
    Unmatched(usize),
}

fn valid_names() -> String {
    FIELDS
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>()
        .join(", ")
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Piece {
    Text(String),
    Field(Field),
}

/// A parsed template.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Template(Vec<Piece>);

impl Template {
    /// Parses `text`.
    pub fn parse(text: &str) -> Result<Self, TemplateError> {
        let mut pieces = Vec::new();
        let mut literal = String::new();
        let mut chars = text.char_indices().peekable();
        while let Some((at, c)) = chars.next() {
            match c {
                '{' if chars.peek().is_some_and(|(_, n)| *n == '{') => {
                    chars.next();
                    literal.push('{');
                }
                '}' if chars.peek().is_some_and(|(_, n)| *n == '}') => {
                    chars.next();
                    literal.push('}');
                }
                '{' => {
                    let mut name = String::new();
                    let mut closed = false;
                    for (_, n) in chars.by_ref() {
                        if n == '}' {
                            closed = true;
                            break;
                        }
                        name.push(n);
                    }
                    if !closed {
                        return Err(TemplateError::Unclosed(at));
                    }
                    let field = FIELDS
                        .iter()
                        .find(|(n, _)| *n == name.trim())
                        .map(|(_, f)| *f)
                        .ok_or(TemplateError::Unknown { name })?;
                    if !literal.is_empty() {
                        pieces.push(Piece::Text(std::mem::take(&mut literal)));
                    }
                    pieces.push(Piece::Field(field));
                }
                '}' => return Err(TemplateError::Unmatched(at)),
                other => literal.push(other),
            }
        }
        if !literal.is_empty() {
            pieces.push(Piece::Text(literal));
        }
        Ok(Self(pieces))
    }

    /// The text with each field replaced by `value(field)`.
    pub fn render(&self, value: impl Fn(Field) -> String) -> String {
        let mut out = String::new();
        for piece in &self.0 {
            match piece {
                Piece::Text(t) => out.push_str(t),
                Piece::Field(f) => out.push_str(&value(*f)),
            }
        }
        out
    }
}

impl fmt::Display for Field {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = FIELDS
            .iter()
            .find(|(_, field)| field == self)
            .map_or("?", |(n, _)| n);
        f.write_str(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn show(field: Field) -> String {
        format!("<{field}>")
    }

    #[test]
    fn placeholders_and_literals() {
        let t = Template::parse("{icon} {temp} {condition}").unwrap();
        assert_eq!(t.render(show), "<icon> <temp> <condition>");
        let t = Template::parse("{{literal}} {location}: {{{temp}}}").unwrap();
        assert_eq!(t.render(show), "{literal} <location>: {<temp>}");
        assert_eq!(Template::parse("").unwrap().render(show), "");
        assert_eq!(Template::parse("{ temp }").unwrap().render(show), "<temp>");
    }

    #[test]
    fn errors() {
        let err = Template::parse("{tmp}").unwrap_err();
        assert_eq!(err, TemplateError::Unknown { name: "tmp".into() });
        assert!(err.to_string().contains("temp, feels"), "{err}");
        assert_eq!(
            Template::parse("a {temp").unwrap_err(),
            TemplateError::Unclosed(2)
        );
        assert_eq!(
            Template::parse("a } b").unwrap_err(),
            TemplateError::Unmatched(2)
        );
    }

    #[test]
    fn every_field_has_a_unique_name() {
        for (i, (name, field)) in FIELDS.iter().enumerate() {
            assert_eq!(field.to_string(), *name);
            assert!(FIELDS[i + 1..].iter().all(|(n, _)| n != name));
        }
    }
}
