//! Resolving [`Secret`]s on Linux: environment variables, files and commands.
//!
//! Errors and logs name the source (variable, path or program), never the value.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::secret::Secret;

/// How long a `{ command = … }` secret may run.
pub const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

/// Why a secret could not be resolved.
#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    /// The environment variable is unset.
    #[error("environment variable `{0}` is not set")]
    EnvUnset(String),
    /// The secret resolved to an empty string.
    #[error("the secret from {0} is empty")]
    Empty(String),
    /// The file could not be read.
    #[error("cannot read secret file {path}: {source}")]
    File {
        /// The expanded path.
        path: String,
        /// The I/O error.
        source: std::io::Error,
    },
    /// The command could not be started.
    #[error("cannot run secret command `{program}`: {source}")]
    Spawn {
        /// Program name.
        program: String,
        /// The I/O error.
        source: std::io::Error,
    },
    /// The command exited unsuccessfully.
    #[error("secret command `{program}` failed ({status})")]
    Failed {
        /// Program name.
        program: String,
        /// Exit status.
        status: std::process::ExitStatus,
    },
    /// The command ran too long and was killed.
    #[error("secret command `{program}` timed out after {secs} s")]
    Timeout {
        /// Program name.
        program: String,
        /// The timeout.
        secs: u64,
    },
    /// The value is not UTF-8.
    #[error("the secret from {0} is not UTF-8")]
    NotUtf8(String),
    /// The secret has no source (an empty command list).
    #[error("the secret has no source")]
    NoSource,
}

/// Expands a leading `~` or `~/` with `HOME` from `env`.
pub fn expand_home(path: &str, env: &dyn Fn(&str) -> Option<String>) -> PathBuf {
    let rest = match path.strip_prefix('~') {
        Some("") => "",
        Some(rest) if rest.starts_with('/') => &rest[1..],
        _ => return PathBuf::from(path),
    };
    match env("HOME") {
        Some(home) if !home.is_empty() => Path::new(&home).join(rest),
        _ => PathBuf::from(path),
    }
}

/// Whether group or others may read the file.
#[cfg(unix)]
pub fn readable_by_others(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o044 != 0)
}

/// Whether group or others may read the file (never known off Unix).
#[cfg(not(unix))]
pub fn readable_by_others(_path: &Path) -> bool {
    false
}

impl Secret {
    /// The secret's value. Environment lookups go through `env` so tests can inject them.
    pub fn resolve(&self, env: &dyn Fn(&str) -> Option<String>) -> Result<String, SecretError> {
        match self {
            Self::Literal(value) => {
                tracing::info!(
                    "an API key is written in the config file; prefer {{ env = \"VAR\" }}"
                );
                non_empty(value.clone(), "the config file")
            }
            Self::Env { env: var } => {
                let value = env(var).ok_or_else(|| SecretError::EnvUnset(var.clone()))?;
                non_empty(value, &format!("${var}"))
            }
            Self::File { file } => {
                let path = expand_home(file, env);
                let shown = path.display().to_string();
                if readable_by_others(&path) {
                    tracing::warn!(path = %shown, "secret file is readable by other users; run chmod 600");
                }
                let bytes = std::fs::read(&path).map_err(|source| SecretError::File {
                    path: shown.clone(),
                    source,
                })?;
                let text =
                    String::from_utf8(bytes).map_err(|_| SecretError::NotUtf8(shown.clone()))?;
                non_empty(text.trim_end().to_string(), &shown)
            }
            Self::Command { command } => run_command(command, COMMAND_TIMEOUT),
        }
    }
}

fn non_empty(value: String, source: &str) -> Result<String, SecretError> {
    if value.is_empty() {
        Err(SecretError::Empty(source.to_string()))
    } else {
        Ok(value)
    }
}

/// Runs `command` (program and arguments) without a shell and with stdin closed, returning its trimmed stdout.
fn run_command(command: &[String], timeout: Duration) -> Result<String, SecretError> {
    let (program, args) = command.split_first().ok_or(SecretError::NoSource)?;
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|source| SecretError::Spawn {
            program: program.clone(),
            source,
        })?;
    // Read stdout on another thread so a chatty command cannot block on a full pipe.
    let mut stdout = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut out = Vec::new();
        if let Some(pipe) = stdout.as_mut() {
            let _ = pipe.read_to_end(&mut out);
        }
        out
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < timeout => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(SecretError::Timeout {
                    program: program.clone(),
                    secs: timeout.as_secs(),
                });
            }
            Err(source) => {
                return Err(SecretError::Spawn {
                    program: program.clone(),
                    source,
                });
            }
        }
    };
    let out = reader.join().unwrap_or_default();
    if !status.success() {
        return Err(SecretError::Failed {
            program: program.clone(),
            status,
        });
    }
    let text = String::from_utf8(out).map_err(|_| SecretError::NotUtf8(format!("`{program}`")))?;
    non_empty(text.trim().to_string(), &format!("`{program}`"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_with(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |k| {
            pairs
                .iter()
                .find(|(name, _)| *name == k)
                .map(|(_, v)| (*v).to_string())
        }
    }

    fn strings(argv: &[&str]) -> Vec<String> {
        argv.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn env_secrets() {
        let env = env_with(&[("KEY", "abc"), ("BLANK", "")]);
        let secret = |name: &str| Secret::Env { env: name.into() };
        assert_eq!(secret("KEY").resolve(&env).unwrap(), "abc");
        assert!(
            matches!(secret("MISSING").resolve(&env), Err(SecretError::EnvUnset(v)) if v == "MISSING")
        );
        assert!(matches!(
            secret("BLANK").resolve(&env),
            Err(SecretError::Empty(_))
        ));
    }

    #[test]
    fn literal_secrets() {
        let env = env_with(&[]);
        assert_eq!(Secret::Literal("abc".into()).resolve(&env).unwrap(), "abc");
        assert!(Secret::Literal(String::new()).resolve(&env).is_err());
    }

    #[test]
    fn file_secrets_expand_home_and_trim() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("owm.key"), "abc\n").unwrap();
        let home = dir.path().to_str().unwrap().to_string();
        let env = move |k: &str| (k == "HOME").then(|| home.clone());
        let secret = Secret::File {
            file: "~/owm.key".into(),
        };
        assert_eq!(secret.resolve(&env).unwrap(), "abc");
        let missing = Secret::File {
            file: "~/nope.key".into(),
        };
        let err = missing.resolve(&env).unwrap_err().to_string();
        assert!(err.contains("nope.key"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn group_readable_files_are_detected() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("k");
        std::fs::write(&path, "abc").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(readable_by_others(&path));
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(!readable_by_others(&path));
    }

    #[test]
    fn expands_only_a_leading_tilde() {
        let env = env_with(&[("HOME", "/home/u")]);
        assert_eq!(expand_home("~", &env), PathBuf::from("/home/u"));
        assert_eq!(expand_home("~/a/b", &env), PathBuf::from("/home/u/a/b"));
        assert_eq!(expand_home("~user/a", &env), PathBuf::from("~user/a"));
        assert_eq!(expand_home("/etc/k", &env), PathBuf::from("/etc/k"));
        assert_eq!(expand_home("~/a", &env_with(&[])), PathBuf::from("~/a"));
    }

    #[test]
    fn command_secrets() {
        let env = env_with(&[]);
        let secret = Secret::Command {
            command: strings(&["printf", "abc\\n"]),
        };
        assert_eq!(secret.resolve(&env).unwrap(), "abc");
        let failing = Secret::Command {
            command: strings(&["false"]),
        };
        assert!(matches!(
            failing.resolve(&env),
            Err(SecretError::Failed { .. })
        ));
        let missing = Secret::Command {
            command: strings(&["whether-no-such-program"]),
        };
        assert!(matches!(
            missing.resolve(&env),
            Err(SecretError::Spawn { .. })
        ));
        assert!(matches!(
            Secret::Command { command: vec![] }.resolve(&env),
            Err(SecretError::NoSource)
        ));
    }

    #[test]
    fn commands_time_out() {
        let started = Instant::now();
        let result = run_command(&strings(&["sleep", "5"]), Duration::from_millis(100));
        assert!(matches!(result, Err(SecretError::Timeout { .. })));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn errors_never_contain_the_value() {
        let env = env_with(&[("KEY", "")]);
        let err = Secret::Env { env: "KEY".into() }
            .resolve(&env)
            .unwrap_err()
            .to_string();
        assert_eq!(err, "the secret from $KEY is empty");
        let failed = Secret::Command {
            command: strings(&["sh", "-c", "echo hunter2; exit 3"]),
        };
        let err = failed.resolve(&env).unwrap_err().to_string();
        assert!(!err.contains("hunter2"), "{err}");
    }
}
