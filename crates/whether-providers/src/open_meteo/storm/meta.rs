//! `meta.json`: when a domain's latest run started and became available.

use alloc::string::{String, ToString};

use serde::Deserialize;
use whether_core::storm::ModelRun;
use whether_core::time::UnixTime;

use super::{META_BASE_URL, from_body, get};
use crate::error::ProviderError;
use crate::http::HttpRequest;
use crate::url::encode_path_segment;

/// The fields of `meta.json` the outlook reads; all times are unix seconds.
#[derive(Deserialize)]
struct Meta {
    last_run_initialisation_time: i64,
    last_run_availability_time: i64,
    update_interval_seconds: Option<u32>,
}

/// The URL of `domain`'s `meta.json`, e.g. `ncep_gfs025`.
pub fn meta_url(domain: &str) -> String {
    alloc::format!(
        "{META_BASE_URL}/{}/static/meta.json",
        encode_path_segment(domain)
    )
}

/// The request for `domain`'s `meta.json` (a domain from
/// [`super::RUN_DOMAINS`] or [`super::ENSEMBLE_DOMAINS`]).
pub fn meta_request(domain: &str) -> HttpRequest {
    get(meta_url(domain))
}

/// The latest run that `domain`'s `meta.json` describes.
pub fn parse_run_meta(domain: &str, body: &[u8]) -> Result<ModelRun, ProviderError> {
    let meta: Meta = from_body(body)?;
    if meta.last_run_initialisation_time <= 0
        || meta.last_run_availability_time < meta.last_run_initialisation_time
    {
        return Err(ProviderError::Malformed(alloc::format!(
            "{domain}: run initialised at {} but available at {}",
            meta.last_run_initialisation_time,
            meta.last_run_availability_time
        )));
    }
    Ok(ModelRun {
        model: domain.to_string(),
        init: UnixTime(meta.last_run_initialisation_time),
        available: UnixTime(meta.last_run_availability_time),
    })
}

/// Seconds between `meta.json`'s runs (`update_interval_seconds`), which
/// spaces the earlier runs one may ask the Single Runs API for.
pub fn parse_update_interval(body: &[u8]) -> Result<u32, ProviderError> {
    let meta: Meta = from_body(body)?;
    meta.update_interval_seconds
        .filter(|s| *s > 0)
        .ok_or_else(|| ProviderError::Malformed("no update_interval_seconds".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The run fields of a `meta.json`, as recorded for `ncep_gfs025`.
    const GFS: &[u8] = br#"{"chunk_time_length":481,"data_end_time":1796043600,
        "last_run_availability_time":1794678661,"last_run_initialisation_time":1794657600,
        "last_run_modification_time":1794678661,"temporal_resolution_seconds":3600,
        "update_interval_seconds":21600}"#;

    #[test]
    fn urls_name_the_domain() {
        assert_eq!(
            meta_request("ncep_gfs025").url,
            "https://api.open-meteo.com/data/ncep_gfs025/static/meta.json"
        );
        assert_eq!(
            meta_url("../x?y"),
            "https://api.open-meteo.com/data/..%2Fx%3Fy/static/meta.json"
        );
    }

    #[test]
    fn runs_are_read() {
        assert_eq!(
            parse_run_meta("ncep_gfs025", GFS),
            Ok(ModelRun {
                model: "ncep_gfs025".to_string(),
                init: UnixTime(1_794_657_600),
                available: UnixTime(1_794_678_661),
            })
        );
        assert_eq!(parse_update_interval(GFS), Ok(21_600));
    }

    #[test]
    fn broken_files_are_errors() {
        assert!(matches!(
            parse_run_meta("x", br#"{"last_run_initialisation_time":5}"#),
            Err(ProviderError::Malformed(_))
        ));
        assert!(matches!(
            parse_run_meta(
                "x",
                br#"{"last_run_initialisation_time":10,"last_run_availability_time":5}"#
            ),
            Err(ProviderError::Malformed(_))
        ));
        assert!(matches!(
            parse_update_interval(
                br#"{"last_run_initialisation_time":10,"last_run_availability_time":50}"#
            ),
            Err(ProviderError::Malformed(_))
        ));
        assert_eq!(
            parse_run_meta("x", br#"{"error":true,"reason":"Not found"}"#),
            Err(ProviderError::Rejected("Not found".to_string()))
        );
    }
}
