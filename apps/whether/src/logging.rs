//! Logging: a file for interactive displays, stderr otherwise.

use std::fs::OpenOptions;
use std::path::Path;
use std::sync::Mutex;

use anyhow::Context;
use tracing_subscriber::EnvFilter;

/// The level: `-v` flags win, then `WHETHER_LOG`/`runtime.log_level`, then `warn`.
pub(crate) fn level(verbose: u8, configured: Option<&str>) -> String {
    match verbose {
        0 => configured.unwrap_or("warn").to_string(),
        1 => "info".into(),
        2 => "debug".into(),
        _ => "trace".into(),
    }
}

/// Installs the global subscriber, writing to `file` (appending) or stderr.
pub(crate) fn init(level: &str, file: Option<&Path>) -> anyhow::Result<()> {
    let filter =
        EnvFilter::try_new(level).with_context(|| format!("invalid log level `{level}`"))?;
    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false);
    let result = match file {
        Some(path) => {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)
                    .with_context(|| format!("creating {}", dir.display()))?;
            }
            let log = OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .with_context(|| format!("opening {}", path.display()))?;
            builder
                .with_ansi(false)
                .with_writer(Mutex::new(log))
                .try_init()
        }
        None => builder.with_writer(std::io::stderr).try_init(),
    };
    // A second init (tests) keeps the first subscriber.
    let _ = result;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verbosity_wins() {
        assert_eq!(level(0, None), "warn");
        assert_eq!(level(0, Some("debug")), "debug");
        assert_eq!(level(1, Some("error")), "info");
        assert_eq!(level(5, None), "trace");
    }

    #[test]
    fn bad_levels_are_errors() {
        assert!(init("not a level ===", None).is_err());
    }
}
