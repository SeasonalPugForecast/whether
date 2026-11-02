//! Errors from loading configuration files.

use std::fmt;
use std::path::PathBuf;

use crate::validate::{Issue, Severity};

/// Why the configuration could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// A file could not be read.
    #[error("{}: {source}", path.display())]
    Io {
        /// The file.
        path: PathBuf,
        /// The I/O error.
        source: std::io::Error,
    },
    /// A file has a syntax error, an unknown key or a wrongly typed value.
    #[error("{}", ParseDisplay(self))]
    Parse {
        /// The file.
        path: PathBuf,
        /// 1-based line and column, when known.
        position: Option<(usize, usize)>,
        /// What is wrong.
        message: String,
        /// The offending line with a caret under the column.
        snippet: String,
    },
    /// An environment or command-line override is malformed or has a bad value.
    #[error("invalid override: {0}")]
    Override(String),
    /// A `[display]` table is malformed.
    #[error("display: {0}")]
    Display(String),
    /// Validation found errors (the warnings are included too).
    #[error("{}", IssuesDisplay(.0))]
    Invalid(Vec<Issue>),
}

struct ParseDisplay<'a>(&'a ConfigError);

impl fmt::Display for ParseDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ConfigError::Parse {
            path,
            position,
            message,
            snippet,
        } = self.0
        else {
            return Ok(());
        };
        match position {
            Some((line, col)) => write!(f, "{}:{line}:{col}: {message}", path.display())?,
            None => write!(f, "{}: {message}", path.display())?,
        }
        if !snippet.is_empty() {
            write!(f, "\n{snippet}")?;
        }
        Ok(())
    }
}

struct IssuesDisplay<'a>(&'a [Issue]);

impl fmt::Display for IssuesDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let errors = self
            .0
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .count();
        write!(f, "the configuration has {errors} error(s)")?;
        for issue in self.0 {
            write!(f, "\n  {issue}")?;
        }
        Ok(())
    }
}

impl ConfigError {
    /// A parse error for `path`, locating `err` in `text`.
    pub(crate) fn parse(path: PathBuf, text: &str, err: &toml::de::Error) -> Self {
        let (position, snippet) = match err.span() {
            Some(span) => {
                let (line, col, content) = locate(text, span.start);
                let caret = format!("{}^", " ".repeat(col - 1));
                (Some((line, col)), format!("  {content}\n  {caret}"))
            }
            None => (None, String::new()),
        };
        Self::Parse {
            path,
            position,
            message: err.message().trim().to_string(),
            snippet,
        }
    }
}

/// The 1-based line and column (in characters) of byte `offset`, and that line's text.
fn locate(text: &str, offset: usize) -> (usize, usize, &str) {
    let offset = offset.min(text.len());
    let line_start = text[..offset].rfind('\n').map_or(0, |i| i + 1);
    let line_end = text[line_start..]
        .find('\n')
        .map_or(text.len(), |i| line_start + i);
    let line = text[..line_start].matches('\n').count() + 1;
    let col = text[line_start..offset].chars().count() + 1;
    (line, col, text[line_start..line_end].trim_end_matches('\r'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locates_offsets() {
        let text = "a = 1\nbé = 2\n";
        assert_eq!(locate(text, 0), (1, 1, "a = 1"));
        assert_eq!(locate(text, 6), (2, 1, "bé = 2"));
        assert_eq!(locate(text, 10), (2, 4, "bé = 2"));
        assert_eq!(locate(text, 99), (3, 1, ""));
    }

    #[test]
    fn parse_errors_show_file_line_and_caret() {
        let text = "version = 1\nrefresh = = 2\n";
        let err = toml::from_str::<toml::Table>(text).unwrap_err();
        let shown = ConfigError::parse("/c/config.toml".into(), text, &err).to_string();
        let mut lines = shown.lines();
        assert!(
            lines.next().unwrap().starts_with("/c/config.toml:2:11: "),
            "{shown}"
        );
        assert_eq!(lines.next(), Some("  refresh = = 2"));
        assert_eq!(lines.next(), Some("            ^"));
    }

    #[test]
    fn invalid_lists_every_issue() {
        let issue = |severity, path: &str| Issue {
            severity,
            path: path.into(),
            message: "m".into(),
        };
        let err = ConfigError::Invalid(vec![
            issue(Severity::Error, "a"),
            issue(Severity::Warning, "b"),
        ]);
        assert_eq!(
            err.to_string(),
            "the configuration has 1 error(s)\n  error: a: m\n  warning: b: m"
        );
    }
}
