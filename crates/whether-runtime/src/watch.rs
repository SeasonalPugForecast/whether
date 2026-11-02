//! Watches the configuration files for changes by polling their
//! modification time and size: no watcher dependency, and editors that replace a
//! file by renaming are seen like any other write.

use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

/// How often the files are checked.
pub const POLL: Duration = Duration::from_secs(2);
/// How long a change must hold before it is reloaded, so a save in several steps
/// reloads once.
pub const SETTLE: Duration = Duration::from_millis(500);

/// What identifies a file's content cheaply; `None` when it does not exist.
type Stamp = Option<(Option<SystemTime>, u64)>;

fn stamp(path: &PathBuf) -> Stamp {
    std::fs::metadata(path)
        .ok()
        .map(|m| (m.modified().ok(), m.len()))
}

/// The watched files and what was last seen of them.
#[derive(Debug, Default)]
pub struct ConfigWatch {
    files: Vec<(PathBuf, Stamp)>,
    /// When the latest unreported change was seen.
    pending: Option<Instant>,
}

impl ConfigWatch {
    /// Watches `paths` from their current state (duplicates are watched once).
    pub fn new(paths: impl IntoIterator<Item = PathBuf>) -> Self {
        let mut watch = Self::default();
        watch.rebase(paths);
        watch
    }

    /// Starts over from the current state of `paths`, forgetting pending changes
    /// (after a reload, or the service's own edits).
    pub fn rebase(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        self.files.clear();
        for path in paths {
            if !self.files.iter().any(|(p, _)| *p == path) {
                let s = stamp(&path);
                self.files.push((path, s));
            }
        }
        self.pending = None;
    }

    /// Whether nothing is watched.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Checks the files at `now`; `true` once a change has held for [`SETTLE`].
    pub fn poll(&mut self, now: Instant) -> bool {
        let mut changed = false;
        for (path, seen) in &mut self.files {
            let current = stamp(path);
            if current != *seen {
                *seen = current;
                changed = true;
            }
        }
        if changed {
            self.pending = Some(now);
            return false;
        }
        match self.pending {
            Some(since) if now.duration_since(since) >= SETTLE => {
                self.pending = None;
                true
            }
            _ => false,
        }
    }

    /// How long to sleep before the next [`Self::poll`]: [`POLL`], or less while a
    /// change settles.
    pub fn next_poll(&self, now: Instant) -> Duration {
        self.pending.map_or(POLL, |since| {
            SETTLE.saturating_sub(now.duration_since(since)).min(POLL)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_change_reloads_once_after_it_settles() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.toml");
        std::fs::write(&file, "a = 1\n").unwrap();
        let missing = dir.path().join("later.toml");
        let mut watch = ConfigWatch::new([file.clone(), missing.clone(), file.clone()]);
        assert_eq!(watch.files.len(), 2, "duplicates are watched once");
        let t0 = Instant::now();
        assert!(!watch.poll(t0), "nothing changed");
        assert_eq!(watch.next_poll(t0), POLL);

        std::fs::write(&file, "a = 22\n").unwrap();
        assert!(!watch.poll(t0), "a change waits to settle");
        assert!(watch.next_poll(t0) <= SETTLE);
        assert!(!watch.poll(t0 + SETTLE / 2));
        assert!(watch.poll(t0 + SETTLE), "settled");
        assert!(!watch.poll(t0 + SETTLE * 3), "reported once");

        // A file that appears counts as a change.
        std::fs::write(&missing, "b = 1\n").unwrap();
        assert!(!watch.poll(t0));
        assert!(watch.poll(t0 + SETTLE));
    }

    #[test]
    fn rebase_forgets_the_service_s_own_edits() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.toml");
        std::fs::write(&file, "a = 1\n").unwrap();
        let mut watch = ConfigWatch::new([file.clone()]);
        std::fs::write(&file, "a = 1\nb = 2\n").unwrap();
        watch.rebase([file]);
        let t = Instant::now();
        assert!(!watch.poll(t) && !watch.poll(t + SETTLE));
        assert!(!ConfigWatch::default().poll(t) && ConfigWatch::default().is_empty());
    }
}
