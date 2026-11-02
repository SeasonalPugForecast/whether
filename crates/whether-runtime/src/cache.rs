//! The snapshot and geocoding cache: JSON files under `$XDG_CACHE_HOME/whether/v1`.
//! Files hold only data, never URLs or keys.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use whether_core::model::{ProviderId, ResolvedLocation, WeatherSnapshot};
use whether_core::time::UnixTime;
use whether_providers::Coordinates;

use crate::atomic::write_atomic;

/// The cache format version.
pub const SCHEMA: u32 = 1;

/// A stored snapshot with what the scheduler needs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CachedSnapshot {
    /// Format version ([`SCHEMA`]).
    pub schema: u32,
    /// When it was stored.
    pub saved_at: UnixTime,
    /// The provider that served it.
    pub provider: ProviderId,
    /// `Last-Modified` for conditional requests.
    pub last_modified: Option<String>,
    /// Do not refetch before this time (MET Norway's `Expires`).
    pub not_before: Option<UnixTime>,
    /// The data.
    pub snapshot: WeatherSnapshot,
}

/// The cache directory.
#[derive(Clone, Debug)]
pub struct Cache {
    dir: PathBuf,
}

impl Cache {
    /// A cache rooted at `dir` (created on first write).
    pub fn at(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// The directory.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The cache key of a location: the provider and coordinates to 3 decimals.
    pub fn key(provider: ProviderId, c: Coordinates) -> String {
        format!("{}-{:.3}-{:.3}", provider.id(), c.latitude, c.longitude)
    }

    fn snapshot_path(&self, key: &str) -> PathBuf {
        self.dir.join("snapshots").join(format!("{key}.json"))
    }

    fn geocode_path(&self) -> PathBuf {
        self.dir.join("geocode.json")
    }

    /// The stored snapshot. A corrupt file is logged and deleted; another schema is
    /// ignored.
    pub fn load(&self, key: &str) -> Option<CachedSnapshot> {
        let path = self.snapshot_path(key);
        let text = fs::read_to_string(&path).ok()?;
        match serde_json::from_str::<CachedSnapshot>(&text) {
            Ok(cached) if cached.schema == SCHEMA => Some(cached),
            Ok(_) => None,
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "deleting corrupt cache file");
                let _ = fs::remove_file(&path);
                None
            }
        }
    }

    /// Stores a snapshot atomically.
    pub fn store(&self, key: &str, value: &CachedSnapshot) -> io::Result<()> {
        let json = serde_json::to_vec(value).map_err(io::Error::other)?;
        write_atomic(&self.snapshot_path(key), &json)
    }

    /// Geocoding results by query key.
    pub fn load_geocode(&self) -> BTreeMap<String, ResolvedLocation> {
        let path = self.geocode_path();
        let Ok(text) = fs::read_to_string(&path) else {
            return BTreeMap::new();
        };
        serde_json::from_str(&text).unwrap_or_else(|e| {
            tracing::warn!(path = %path.display(), error = %e, "ignoring corrupt geocode cache");
            BTreeMap::new()
        })
    }

    /// Stores geocoding results atomically.
    pub fn store_geocode(&self, map: &BTreeMap<String, ResolvedLocation>) -> io::Result<()> {
        let json = serde_json::to_vec_pretty(map).map_err(io::Error::other)?;
        write_atomic(&self.geocode_path(), &json)
    }

    /// Deletes snapshots whose key is not in `keep`.
    pub fn prune(&self, keep: &[String]) -> io::Result<()> {
        let dir = self.dir.join("snapshots");
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e),
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            if path.extension().is_some_and(|e| e == "json") && !keep.iter().any(|k| k == stem) {
                fs::remove_file(&path)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use whether_core::testdata;

    const LOC: Coordinates = Coordinates {
        latitude: 40.7143,
        longitude: -74.005_97,
    };

    fn cached() -> CachedSnapshot {
        CachedSnapshot {
            schema: SCHEMA,
            saved_at: testdata::SAMPLE_NOW,
            provider: ProviderId::OpenMeteo,
            last_modified: Some("Mon, 05 Oct 2026 16:00:00 GMT".into()),
            not_before: None,
            snapshot: testdata::sample_snapshot(),
        }
    }

    #[test]
    fn keys_round_coordinates() {
        assert_eq!(
            Cache::key(ProviderId::OpenMeteo, LOC),
            "open-meteo-40.714--74.006"
        );
    }

    #[test]
    fn round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path());
        let key = Cache::key(ProviderId::OpenMeteo, LOC);
        assert_eq!(cache.load(&key), None);
        cache.store(&key, &cached()).unwrap();
        assert_eq!(cache.load(&key), Some(cached()));
    }

    #[test]
    fn corrupt_files_are_deleted_and_other_schemas_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path());
        fs::create_dir_all(dir.path().join("snapshots")).unwrap();
        let path = dir.path().join("snapshots/bad.json");
        fs::write(&path, "{").unwrap();
        assert_eq!(cache.load("bad"), None);
        assert!(!path.exists());
        let mut old = cached();
        old.schema = 0;
        cache.store("old", &old).unwrap();
        assert_eq!(cache.load("old"), None);
    }

    #[test]
    fn geocode_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path());
        assert!(cache.load_geocode().is_empty());
        let mut map = BTreeMap::new();
        map.insert(
            "zip:10001:US".to_string(),
            testdata::sample_snapshot().location,
        );
        cache.store_geocode(&map).unwrap();
        assert_eq!(cache.load_geocode(), map);
    }

    #[test]
    fn prune_keeps_only_listed_keys() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path());
        cache.prune(&[]).unwrap();
        cache.store("a", &cached()).unwrap();
        cache.store("b", &cached()).unwrap();
        cache.prune(&["a".to_string()]).unwrap();
        assert!(cache.load("a").is_some() && cache.load("b").is_none());
    }

    #[test]
    fn concurrent_writes_leave_a_valid_file() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Arc::new(Cache::at(dir.path()));
        let writers: Vec<_> = (0..2)
            .map(|_| {
                let cache = Arc::clone(&cache);
                std::thread::spawn(move || {
                    for _ in 0..100 {
                        cache.store("same", &cached()).unwrap();
                    }
                })
            })
            .collect();
        for w in writers {
            w.join().unwrap();
        }
        assert_eq!(cache.load("same"), Some(cached()));
        let leftovers = fs::read_dir(dir.path().join("snapshots")).unwrap().count();
        assert_eq!(leftovers, 1, "no temporary files remain");
    }
}
