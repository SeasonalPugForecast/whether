//! Active alerts for a point (`/alerts/active?point=`).

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use serde::Deserialize;
use whether_core::builder::SnapshotBuilder;
use whether_core::model::Alert;
use whether_core::time::parse_rfc3339;

use crate::error::ProviderError;
use crate::http::{FetchContext, HttpResponse};
use crate::openweather::onecall4::severity_of;
use crate::url::Coordinates;

#[derive(Deserialize)]
struct Collection {
    features: Vec<Feature>,
}

#[derive(Deserialize)]
struct Feature {
    properties: Properties,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Properties {
    id: String,
    event: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    instruction: Option<String>,
    #[serde(default)]
    sender_name: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    message_type: Option<String>,
    effective: Option<String>,
    onset: Option<String>,
    ends: Option<String>,
    expires: Option<String>,
    #[serde(default)]
    parameters: Parameters,
}

#[derive(Default, Deserialize)]
struct Parameters {
    #[serde(rename = "VTEC", default)]
    vtec: Vec<String>,
}

/// The event's identity across updates, from its VTEC string
/// (`/O.EXT.KMOB.FL.W.0048.…/` → `KMOB.FL.W.0048`): every update of a warning
/// has a new message id but the same office, phenomenon, significance and
/// number, so an update is not taken for a new alert.
fn event_key(vtec: &str) -> Option<String> {
    let parts: Vec<&str> = vtec.trim_matches('/').split('.').collect();
    (parts.len() >= 6).then(|| parts[2..6].join("."))
}

/// The NWS's own page for a point (`forecast.weather.gov`, checked 2026-09-24):
/// its forecast, with the active hazards listed on top. The alert's `@id` is
/// the `GeoJSON` on `api.weather.gov`, which is no page to read, and `web` is only
/// the NWS home page. The point is rounded to about 1 km, like every coordinate
/// the phone sends.
fn page_url(latitude: f64, longitude: f64) -> String {
    let (lat, lon) = Coordinates {
        latitude,
        longitude,
    }
    .fmt_param(2);
    alloc::format!("https://forecast.weather.gov/MapClick.php?lat={lat}&lon={lon}")
}

/// Adds the active alerts in `resp` to `out`. The location is the session's (a
/// supplement's builder starts with the requested place and the primary's
/// offset) or the `/points` lookup's.
pub(super) fn parse(
    resp: &HttpResponse<'_>,
    ctx: &FetchContext,
    out: &mut SnapshotBuilder,
) -> Result<(), ProviderError> {
    let collection: Collection =
        serde_json::from_slice(resp.body).map_err(|e| ProviderError::Malformed(e.to_string()))?;
    for p in collection.features.into_iter().map(|f| f.properties) {
        let actual = p.status.as_deref().is_none_or(|s| s == "Actual");
        let cancelled = p.message_type.as_deref() == Some("Cancel");
        let time = |t: &Option<String>| t.as_deref().and_then(parse_rfc3339);
        let (Some(start), Some(end)) = (
            time(&p.onset).or(time(&p.effective)),
            time(&p.ends).or(time(&p.expires)),
        ) else {
            continue;
        };
        if !actual || cancelled || end <= ctx.now {
            continue;
        }
        let mut description = p.description.unwrap_or_default().trim().to_string();
        if let Some(instruction) = p
            .instruction
            .as_deref()
            .map(str::trim)
            .filter(|i| !i.is_empty())
        {
            description.push_str("\n\n");
            description.push_str(instruction);
        }
        let url = out.location().map(|l| page_url(l.latitude, l.longitude));
        out.push_alert(Alert {
            id: Some(
                p.parameters
                    .vtec
                    .first()
                    .and_then(|v| event_key(v))
                    .unwrap_or(p.id),
            ),
            severity: severity_of(&p.event),
            title: p.event,
            description,
            start,
            end,
            source: p
                .sender_name
                .unwrap_or_else(|| "National Weather Service".to_string()),
            checked_at: Some(ctx.now),
            url,
        });
    }
    Ok(())
}
