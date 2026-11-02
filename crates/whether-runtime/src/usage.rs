//! The persisted call ledger: `$XDG_STATE_HOME/whether/usage.json`, shared
//! by every whether process under an exclusive lock.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use whether_providers::UsageLedger;

use crate::atomic::write_atomic;

/// Why the ledger cannot be used. Metered calls are disabled until it is fixed.
#[derive(Debug, thiserror::Error)]
pub enum UsageError {
    /// The file or lock could not be read or written.
    #[error("{}: {source}", path.display())]
    Io {
        /// The file.
        path: PathBuf,
        /// The I/O error.
        source: io::Error,
    },
    /// The ledger is unreadable; it is never reset silently.
    #[error("{} is corrupt ({message}); metered calls stay off until it is fixed or deleted", path.display())]
    Corrupt {
        /// The file.
        path: PathBuf,
        /// The parser's message.
        message: String,
    },
}

/// Where the ledger lives.
#[derive(Clone, Debug)]
pub struct UsageStore {
    path: PathBuf,
}

/// Exclusive access to the ledger until dropped.
#[derive(Debug)]
pub struct UsageGuard {
    _lock: File,
    path: PathBuf,
}

impl UsageStore {
    /// The ledger at `path` (normally `state_dir()/usage.json`).
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The ledger file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Waits for exclusive access. The lock is on a separate `usage.lock`, because
    /// saving replaces `usage.json`.
    pub fn lock(&self) -> Result<UsageGuard, UsageError> {
        let io = |source| UsageError::Io {
            path: self.path.clone(),
            source,
        };
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir).map_err(io)?;
        }
        let lock_path = self.path.with_extension("lock");
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock_path)
            .map_err(io)?;
        file.lock().map_err(io)?;
        Ok(UsageGuard {
            _lock: file,
            path: self.path.clone(),
        })
    }
}

impl UsageGuard {
    /// The ledger; a missing file is an empty ledger.
    pub fn load(&self) -> Result<UsageLedger, UsageError> {
        match fs::read_to_string(&self.path) {
            Ok(text) => serde_json::from_str(&text).map_err(|e| UsageError::Corrupt {
                path: self.path.clone(),
                message: e.to_string(),
            }),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(UsageLedger::new()),
            Err(source) => Err(UsageError::Io {
                path: self.path.clone(),
                source,
            }),
        }
    }

    /// Saves the ledger atomically and durably.
    pub fn save(&self, ledger: &UsageLedger) -> Result<(), UsageError> {
        let json = serde_json::to_vec(ledger).map_err(|e| UsageError::Io {
            path: self.path.clone(),
            source: io::Error::other(e),
        })?;
        write_atomic(&self.path, &json).map_err(|source| UsageError::Io {
            path: self.path.clone(),
            source,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use whether_core::model::ProviderId;
    use whether_core::time::UnixTime;

    const NOON: UnixTime = UnixTime(1_791_201_600);
    const OWM: ProviderId = ProviderId::OpenWeather;

    #[test]
    fn missing_files_are_empty_and_saves_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let store = UsageStore::at(dir.path().join("state/usage.json"));
        let guard = store.lock().unwrap();
        let mut ledger = guard.load().unwrap();
        assert_eq!(ledger, UsageLedger::new());
        ledger
            .try_spend(OWM, whether_providers::Budget::daily(5), 1, NOON)
            .unwrap();
        guard.save(&ledger).unwrap();
        assert_eq!(guard.load().unwrap(), ledger);
    }

    #[test]
    fn a_ledger_saved_before_periods_is_read_and_carried_on() {
        // The `usage.json` earlier versions wrote: one UTC day for all.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("usage.json");
        fs::write(&path, r#"{"day":20731,"used":{"openweather":7}}"#).unwrap();
        let guard = UsageStore::at(&path).lock().unwrap();
        let mut ledger = guard.load().unwrap();
        assert_eq!(ledger.used(OWM, NOON), 7);
        ledger
            .try_spend(OWM, whether_providers::Budget::daily(8), 1, NOON)
            .unwrap();
        guard.save(&ledger).unwrap();
        assert_eq!(guard.load().unwrap().used(OWM, NOON), 8);
    }

    #[test]
    fn corruption_is_an_error_not_a_reset() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("usage.json");
        fs::write(&path, "{\"day\":").unwrap();
        let err = UsageStore::at(&path).lock().unwrap().load().unwrap_err();
        assert!(matches!(err, UsageError::Corrupt { .. }), "{err}");
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "{\"day\":",
            "the file is left alone"
        );
    }

    #[test]
    fn concurrent_spenders_never_exceed_the_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("usage.json");
        let granted = Arc::new(AtomicU32::new(0));
        let workers: Vec<_> = (0..4)
            .map(|_| {
                let (path, granted) = (path.clone(), Arc::clone(&granted));
                std::thread::spawn(move || {
                    // Each thread opens its own lock file handle, like separate processes.
                    let store = UsageStore::at(path);
                    for _ in 0..40 {
                        let guard = store.lock().unwrap();
                        let mut ledger = guard.load().unwrap();
                        if ledger
                            .try_spend(OWM, whether_providers::Budget::daily(50), 1, NOON)
                            .is_ok()
                        {
                            guard.save(&ledger).unwrap();
                            granted.fetch_add(1, Ordering::SeqCst);
                        }
                    }
                })
            })
            .collect();
        for w in workers {
            w.join().unwrap();
        }
        assert_eq!(granted.load(Ordering::SeqCst), 50);
        let ledger = UsageStore::at(&path).lock().unwrap().load().unwrap();
        assert_eq!(ledger.used(OWM, NOON), 50);
    }
}
