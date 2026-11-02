//! Resolving configured locations to coordinates, and place search.

use std::collections::BTreeMap;

use whether_config::{LocationConfig, ProvidersConfig};
use whether_core::model::{OffsetChange, ProviderId, ResolvedLocation};
use whether_core::time::{UnixTime, UtcOffset, parse_utc_offset};
use whether_providers::openweather::DEFAULT_BASE_URL;
use whether_providers::{GeocodeQuery, GeocodeResult, open_meteo, openweather};

use crate::cache::Cache;
use crate::transport::Transport;

/// Why a location could not be resolved.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LocateError {
    /// The entry has neither coordinates, a zip nor a query.
    #[error("location `{0}` has no coordinates, zip or query")]
    Unplaced(String),
    /// Geocoding found nothing.
    #[error("no place found for `{0}`")]
    NotFound(String),
    /// Every geocoder failed.
    #[error("geocoding `{query}` failed: {reason}")]
    Failed {
        /// What was searched.
        query: String,
        /// The last error.
        reason: String,
    },
}

/// The UTC offset of the IANA zone `name` at `now`.
pub fn offset_for(name: &str, now: UnixTime) -> Option<UtcOffset> {
    let zone = jiff::tz::TimeZone::get(name).ok()?;
    let at = jiff::Timestamp::from_second(now.0).ok()?;
    Some(UtcOffset(zone.to_offset(at).seconds()))
}

/// The UTC offset changes (DST) of the IANA zone `name` after `from`, up to `to`
///; empty for unknown zones.
pub fn offset_changes(name: &str, from: UnixTime, to: UnixTime) -> Vec<OffsetChange> {
    let (Ok(zone), Ok(start)) = (
        jiff::tz::TimeZone::get(name),
        jiff::Timestamp::from_second(from.0),
    ) else {
        return Vec::new();
    };
    zone.following(start)
        .take_while(|t| t.timestamp().as_second() <= to.0)
        .map(|t| OffsetChange {
            at: UnixTime(t.timestamp().as_second()),
            offset: UtcOffset(t.offset().seconds()),
        })
        .collect()
}

/// What a search needs besides the query.
pub struct Geocoder<'a> {
    /// Provider settings (geocoding URLs, the OpenWeather key).
    pub providers: &'a ProvidersConfig,
    /// Environment lookups for secrets.
    pub env: &'a dyn Fn(&str) -> Option<String>,
    /// HTTP.
    pub transport: &'a dyn Transport,
    /// Language of result names.
    pub language: &'a str,
}

impl std::fmt::Debug for Geocoder<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Geocoder")
            .field("language", &self.language)
            .finish_non_exhaustive()
    }
}

impl Geocoder<'_> {
    /// The OpenWeather key, when it resolves.
    fn openweather_key(&self) -> Option<(String, String)> {
        let owm = self.providers.openweather.as_ref()?;
        let key = owm.api_key.as_ref()?.resolve(self.env).ok()?;
        let base = owm
            .base_url
            .clone()
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
        Some((base, key))
    }

    fn via_openweather(
        &self,
        query: &GeocodeQuery<'_>,
        count: u8,
    ) -> Option<Result<Vec<GeocodeResult>, String>> {
        let (base, key) = self.openweather_key()?;
        let request = openweather::geocode::geocode_request(&base, &key, query, count);
        Some(
            self.transport
                .send(&request)
                .map_err(|e| e.to_string())
                .and_then(|resp| {
                    openweather::geocode::parse_geocode(query, &resp.as_response())
                        .map_err(|e| e.to_string())
                }),
        )
    }

    fn via_open_meteo(
        &self,
        query: &GeocodeQuery<'_>,
        count: u8,
    ) -> Result<Vec<GeocodeResult>, String> {
        let base = self
            .providers
            .open_meteo
            .geocoding_url
            .as_deref()
            .unwrap_or(open_meteo::geocode::DEFAULT_GEOCODING_URL);
        let request = open_meteo::geocode::geocode_request(base, query, count, self.language);
        let resp = self.transport.send(&request).map_err(|e| e.to_string())?;
        open_meteo::geocode::parse_geocode(query, &resp.as_response()).map_err(|e| e.to_string())
    }

    /// Up to `count` places for `query`, trying the providers in configured order
    /// (OpenWeather only with a key) until one has results.
    pub fn search(
        &self,
        query: &GeocodeQuery<'_>,
        count: u8,
    ) -> Result<Vec<GeocodeResult>, LocateError> {
        let text = match query {
            GeocodeQuery::Name { text, .. } => (*text).to_string(),
            GeocodeQuery::Zip { zip, country } => format!("{zip} {country}"),
        };
        let mut last_error = None;
        let mut order: Vec<ProviderId> = self.providers.order.clone();
        if !order.contains(&ProviderId::OpenMeteo) {
            order.push(ProviderId::OpenMeteo);
        }
        for id in order {
            let outcome = match id {
                ProviderId::OpenWeather => self.via_openweather(query, count),
                ProviderId::OpenMeteo => Some(self.via_open_meteo(query, count)),
                ProviderId::MetNorway
                | ProviderId::Nws
                | ProviderId::Acis
                | ProviderId::PirateWeather => None,
            };
            match outcome {
                Some(Ok(results)) if !results.is_empty() => return Ok(results),
                Some(Ok(_)) | None => {}
                Some(Err(e)) => last_error = Some(e),
            }
        }
        match last_error {
            Some(reason) => Err(LocateError::Failed {
                query: text,
                reason,
            }),
            None => Err(LocateError::NotFound(text)),
        }
    }
}

/// A search result as a resolved location named `name`.
pub fn resolved_from(
    result: &GeocodeResult,
    name: Option<&str>,
    now: UnixTime,
) -> ResolvedLocation {
    ResolvedLocation {
        name: name.map_or_else(|| result.name.clone(), str::to_string),
        region: result.admin1.clone(),
        latitude: result.latitude,
        longitude: result.longitude,
        elevation_m: result.elevation,
        utc_offset: result
            .timezone
            .as_deref()
            .and_then(|tz| offset_for(tz, now))
            .unwrap_or_default(),
        tz_name: result.timezone.clone(),
        tz_abbrev: None,
        station: None,
    }
}

/// The geocode cache key of a configured location, if it needs geocoding.
fn cache_key(loc: &LocationConfig) -> Option<String> {
    match (&loc.zip, &loc.query) {
        (Some(zip), _) => Some(format!(
            "zip:{}:{}",
            zip.trim(),
            loc.country.as_deref().unwrap_or("").to_uppercase()
        )),
        (None, Some(query)) => Some(format!("query:{}", query.trim().to_lowercase())),
        (None, None) => None,
    }
}

/// `loc`'s coordinates as written, or from the geocode cache; `None` when it has
/// never been geocoded. Never touches the network.
pub fn known_coordinates(loc: &LocationConfig, cache: &Cache) -> Option<(f64, f64)> {
    if let (Some(latitude), Some(longitude)) = (loc.latitude, loc.longitude) {
        return Some((latitude, longitude));
    }
    let hit = cache.load_geocode().remove(&cache_key(loc)?)?;
    Some((hit.latitude, hit.longitude))
}

/// Coordinates and time zone for `loc`. Coordinates are used as written; a zip or
/// query is geocoded once and then served from the cache.
pub fn resolve_location(
    loc: &LocationConfig,
    geocoder: &Geocoder<'_>,
    cache: &Cache,
    now: UnixTime,
) -> Result<ResolvedLocation, LocateError> {
    let name = loc.name.clone().unwrap_or_default();
    let configured_offset = loc
        .utc_offset
        .as_deref()
        .and_then(parse_utc_offset)
        .or_else(|| loc.timezone.as_deref().and_then(|tz| offset_for(tz, now)));
    if let (Some(latitude), Some(longitude)) = (loc.latitude, loc.longitude) {
        return Ok(ResolvedLocation {
            name,
            region: None,
            latitude,
            longitude,
            elevation_m: loc.elevation,
            utc_offset: configured_offset.unwrap_or_default(),
            tz_name: loc.timezone.clone(),
            tz_abbrev: None,
            station: None,
        });
    }
    let key = cache_key(loc).ok_or_else(|| LocateError::Unplaced(name.clone()))?;
    let mut cached: BTreeMap<String, ResolvedLocation> = cache.load_geocode();
    let mut resolved = if let Some(hit) = cached.get(&key) {
        hit.clone()
    } else {
        let query = match (&loc.zip, &loc.query) {
            (Some(zip), _) => GeocodeQuery::Zip {
                zip,
                country: loc.country.as_deref().unwrap_or(""),
            },
            (None, Some(text)) => GeocodeQuery::Name {
                text,
                country: loc.country.as_deref(),
            },
            (None, None) => return Err(LocateError::Unplaced(name)),
        };
        let results = geocoder.search(&query, 1)?;
        let first = results
            .first()
            .ok_or_else(|| LocateError::NotFound(key.clone()))?;
        let fresh = resolved_from(first, None, now);
        cached.insert(key, fresh.clone());
        if let Err(e) = cache.store_geocode(&cached) {
            tracing::warn!("geocode cache not saved: {e}");
        }
        fresh
    };
    resolved.name = name;
    if let Some(offset) = configured_offset {
        resolved.utc_offset = offset;
    } else if let Some(tz) = resolved.tz_name.as_deref() {
        // Recompute so daylight saving changes are picked up.
        resolved.utc_offset = offset_for(tz, now).unwrap_or(resolved.utc_offset);
    }
    if loc.timezone.is_some() {
        resolved.tz_name.clone_from(&loc.timezone);
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Replay;

    /// 2026-10-05 12:30 EDT.
    const NOW: UnixTime = UnixTime(1_791_217_800);

    #[test]
    fn dst_changes_come_from_the_zone() {
        let oct8 = UnixTime(1_791_417_600);
        let month = UnixTime(oct8.0 + 30 * 86_400);
        assert_eq!(
            offset_changes("America/New_York", oct8, month),
            [OffsetChange {
                at: UnixTime(1_793_512_800),
                offset: UtcOffset(-18_000),
            }],
            "2026-11-01 02:00 EDT becomes 01:00 EST"
        );
        assert_eq!(
            offset_changes("America/New_York", oct8, UnixTime(oct8.0 + 86_400)),
            []
        );
        assert_eq!(offset_changes("Asia/Tokyo", oct8, month), [], "no DST");
        assert_eq!(offset_changes("Nowhere/Atlantis", oct8, month), []);
    }

    macro_rules! fixture {
        ($path:literal) => {
            include_str!(concat!("../../whether-providers/tests/fixtures/", $path))
        };
    }

    fn zip_10001() -> LocationConfig {
        LocationConfig {
            name: Some("Home".into()),
            zip: Some("10001".into()),
            country: Some("US".into()),
            ..Default::default()
        }
    }

    fn geocoder<'a>(
        providers: &'a ProvidersConfig,
        transport: &'a Replay,
        env: &'a dyn Fn(&str) -> Option<String>,
    ) -> Geocoder<'a> {
        Geocoder {
            providers,
            env,
            transport,
            language: "en",
        }
    }

    #[test]
    fn zones_give_offsets_with_daylight_saving() {
        assert_eq!(
            offset_for("America/New_York", NOW),
            Some(UtcOffset(-14_400))
        );
        assert_eq!(
            offset_for("America/New_York", UnixTime(1_798_761_600)),
            Some(UtcOffset(-18_000)),
            "January"
        );
        assert_eq!(offset_for("Not/AZone", NOW), None);
    }

    #[test]
    fn coordinates_need_no_network() {
        let transport = Replay::new(vec![]);
        let providers = ProvidersConfig::default();
        let loc = LocationConfig {
            name: Some("Berlin".into()),
            latitude: Some(52.52),
            longitude: Some(13.41),
            timezone: Some("Europe/Berlin".into()),
            ..Default::default()
        };
        let dir = tempfile::tempdir().unwrap();
        let r = resolve_location(
            &loc,
            &geocoder(&providers, &transport, &|_| None),
            &Cache::at(dir.path()),
            NOW,
        )
        .unwrap();
        assert_eq!(
            (r.name.as_str(), r.latitude, r.utc_offset),
            ("Berlin", 52.52, UtcOffset(7200))
        );
        assert_eq!(transport.seen(), [] as [std::string::String; 0]);
    }

    #[test]
    fn zips_use_open_meteo_without_a_key_and_then_the_cache() {
        let transport = Replay::new(vec![(
            "geocoding-api.open-meteo.com",
            200,
            fixture!("open_meteo/geocode_10001_us.json"),
        )]);
        let providers = ProvidersConfig::default();
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path());
        let g = geocoder(&providers, &transport, &|_| None);
        let first = resolve_location(&zip_10001(), &g, &cache, NOW).unwrap();
        assert_eq!(first.name, "Home");
        assert!((first.latitude - 40.7143).abs() < 0.01);
        assert_eq!(first.utc_offset, UtcOffset(-14_400));
        assert_eq!(first.tz_name.as_deref(), Some("America/New_York"));
        let second = resolve_location(&zip_10001(), &g, &cache, NOW).unwrap();
        assert_eq!(second, first);
        assert_eq!(
            transport.seen().len(),
            1,
            "the second call is served from the cache"
        );
    }

    #[test]
    fn zips_prefer_openweather_with_a_key() {
        let transport = Replay::new(vec![
            (
                "api.openweathermap.org/geo/1.0/zip",
                200,
                fixture!("openweather/geocode_zip_10001.synthetic.json"),
            ),
            (
                "geocoding-api.open-meteo.com",
                200,
                fixture!("open_meteo/geocode_10001_us.json"),
            ),
        ]);
        let providers = ProvidersConfig::default();
        let env = |k: &str| (k == "OPENWEATHER_API_KEY").then(|| "KEY".to_string());
        let results = geocoder(&providers, &transport, &env)
            .search(
                &GeocodeQuery::Zip {
                    zip: "10001",
                    country: "US",
                },
                1,
            )
            .unwrap();
        assert_eq!(results[0].source, ProviderId::OpenWeather);
        assert_eq!(transport.seen().len(), 1);
    }

    #[test]
    fn nothing_found_and_failures() {
        let none = Replay::new(vec![(
            "open-meteo",
            200,
            fixture!("open_meteo/geocode_none.json"),
        )]);
        let providers = ProvidersConfig::default();
        let q = GeocodeQuery::Name {
            text: "zzqqxxy",
            country: None,
        };
        assert!(matches!(
            geocoder(&providers, &none, &|_| None).search(&q, 3),
            Err(LocateError::NotFound(_))
        ));
        let down = Replay::new(vec![]);
        assert!(matches!(
            geocoder(&providers, &down, &|_| None).search(&q, 3),
            Err(LocateError::Failed { .. })
        ));
        let dir = tempfile::tempdir().unwrap();
        let unplaced = LocationConfig {
            name: Some("x".into()),
            ..Default::default()
        };
        assert_eq!(
            resolve_location(
                &unplaced,
                &geocoder(&providers, &down, &|_| None),
                &Cache::at(dir.path()),
                NOW
            ),
            Err(LocateError::Unplaced("x".into()))
        );
    }
}
