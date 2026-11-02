//! One location's fetch: the sans-IO session, with Swift doing the HTTP.

use std::sync::Arc;

use whether_core::model::WeatherSnapshot;
use whether_core::requirements::{DataRequirements, HourlyFields, MAX_NOWCAST_HOURS};
use whether_core::time::{UnixTime, UtcOffset};
use whether_providers::{
    Attempt, Coordinates, FetchContext, FetchError, FetchSession, HttpResponse, Method, Provider,
    UsageLedger,
};

use crate::settings::{Place, Settings};
use crate::{WhetherError, from_json};

/// Decimals of every coordinate that leaves the phone: 2, about 1.1 km. That
/// is finer than any forecast grid, and coarse enough that the App Store privacy label
/// can say Coarse Location. Only the rounded place goes to a provider; the saved place
/// keeps its full coordinates. (The desktop's `coordinate_precision` is its own.)
pub(crate) const COORDINATE_DECIMALS: u8 = 2;

/// The coordinates of `place` as providers may see them: rounded to
/// [`COORDINATE_DECIMALS`]. The one place the rounding happens, so the forecast, the
/// history and climate fetches (and with them the plan and the widgets) all use it.
pub(crate) fn outgoing(place: &Place) -> Coordinates {
    Coordinates {
        latitude: place.latitude,
        longitude: place.longitude,
    }
    .rounded(COORDINATE_DECIMALS)
}

/// One request header.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct Header {
    /// Name.
    pub name: String,
    /// Value.
    pub value: String,
}

/// The HTTP method of a [`HostRequest`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum HostMethod {
    /// GET, without a body.
    Get,
    /// POST with a JSON body (`Content-Type: application/json`).
    Post,
}

/// A request for the app to perform. The URL may carry the API key: never log it.
#[derive(Clone, PartialEq, Eq, uniffi::Record)]
pub struct HostRequest {
    /// The full URL.
    pub url: String,
    /// Extra headers.
    pub headers: Vec<Header>,
    /// GET or POST.
    pub method: HostMethod,
    /// The JSON body of a POST.
    pub body: Option<String>,
}

impl std::fmt::Debug for HostRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostRequest")
            .field("url", &whether_providers::redact_url(&self.url))
            .field("headers", &RedactedHeaders(&self.headers))
            .field("method", &self.method)
            .field("body_len", &self.body.as_ref().map(String::len))
            .finish()
    }
}

/// `Debug` for headers that masks the values of secret ones (an `apikey`).
struct RedactedHeaders<'a>(&'a [Header]);

impl std::fmt::Debug for RedactedHeaders<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list()
            .entries(self.0.iter().map(|h| {
                let value = if whether_providers::is_secret_header(&h.name) {
                    "***"
                } else {
                    h.value.as_str()
                };
                (h.name.as_str(), value)
            }))
            .finish()
    }
}

/// The response to a [`HostRequest`].
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct HostResponse {
    /// Status code.
    pub status: u16,
    /// The body, decompressed.
    pub body: Vec<u8>,
    /// The `Date` header, as Unix seconds.
    pub date: Option<i64>,
    /// The `Expires` header, as Unix seconds.
    pub expires: Option<i64>,
    /// The `Last-Modified` header, verbatim.
    pub last_modified: Option<String>,
}

/// Why the app could not do what the fetch asked.
#[derive(Debug, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum HostError {
    /// The request or the save failed (no network, timeout, disk full).
    #[error("{message}")]
    Failed {
        /// What went wrong.
        message: String,
    },
}

impl From<uniffi::UnexpectedUniFFICallbackError> for HostError {
    fn from(e: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Failed { message: e.reason }
    }
}

/// What the app provides to a fetch.
#[uniffi::export(with_foreign)]
#[async_trait::async_trait]
pub trait FetchHost: Send + Sync {
    /// Performs the request (`URLSession`): a GET, or a POST with its JSON body.
    async fn send(&self, request: HostRequest) -> Result<HostResponse, HostError>;
    /// Stores the call ledger (JSON) durably. Called before every metered
    /// request; the request is not sent unless this succeeds.
    fn save_usage(&self, ledger_json: String) -> Result<(), HostError>;
}

/// What the app has from earlier fetches of the place.
#[derive(Clone, Debug, Default, PartialEq, Eq, uniffi::Record)]
pub struct Cached {
    /// The last snapshot (from [`FetchOutcome::snapshot_json`]): its alerts are
    /// reused instead of being requested again.
    pub snapshot_json: Option<String>,
    /// `Last-Modified` of the last response, for a conditional request.
    pub last_modified: Option<String>,
    /// The call ledger last saved through [`FetchHost::save_usage`].
    pub usage_json: Option<String>,
}

/// One request's outcome.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AttemptInfo {
    /// Provider name.
    pub provider: String,
    /// The resource.
    pub endpoint: String,
    /// The error, if it failed.
    pub error: Option<String>,
}

/// A fetched forecast.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct FetchOutcome {
    /// The snapshot as JSON: cache it and pass it to `view_model`.
    pub snapshot_json: String,
    /// The provider that served it.
    pub provider: String,
    /// `Last-Modified` to send next time.
    pub last_modified: Option<String>,
    /// Every request's outcome, including fallbacks.
    pub attempts: Vec<AttemptInfo>,
}

fn name(value: &impl serde::Serialize) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(s)) => s,
        _ => String::new(),
    }
}

pub(crate) fn attempt_info(a: &Attempt) -> AttemptInfo {
    AttemptInfo {
        provider: name(&a.provider),
        endpoint: format!("{:?}", a.endpoint).to_lowercase(),
        error: a.error.as_ref().map(ToString::to_string),
    }
}

/// Fetches `place`'s forecast at Unix time `now`, through `host`.
#[uniffi::export]
#[allow(clippy::needless_pass_by_value)] // UniFFI passes arguments by value
pub async fn fetch(
    place: Place,
    settings: Settings,
    cached: Cached,
    now: i64,
    host: Arc<dyn FetchHost>,
) -> Result<FetchOutcome, WhetherError> {
    let known_alerts = match &cached.snapshot_json {
        Some(json) => from_json::<WeatherSnapshot>("cached snapshot", json)?.alerts,
        None => Vec::new(),
    };
    let (primary, supplements) = settings.providers(&place);
    let metered = primary
        .iter()
        .chain(&supplements)
        .any(|p| p.budget().is_some());
    let ledger = match (&cached.usage_json, metered) {
        (_, false) => None,
        (Some(json), true) => Some(from_json::<UsageLedger>("call ledger", json)?),
        (None, true) => Some(UsageLedger::default()),
    };
    let primary: Vec<&dyn Provider> = primary.iter().map(AsRef::as_ref).collect();
    let supplements: Vec<&dyn Provider> = supplements.iter().map(AsRef::as_ref).collect();
    let presentation = settings.presentation(&place);
    let ctx = FetchContext {
        now: UnixTime(now),
        user_agent: crate::user_agent(),
        utc_offset_hint: place.utc_offset_seconds.map(UtcOffset),
        if_modified_since: cached.last_modified.clone(),
        language: settings.language.clone(),
    };
    let loc = outgoing(&place);
    let mut session = FetchSession::new(
        &primary,
        &supplements,
        loc,
        {
            let mut req = DataRequirements::from_presentation(&presentation);
            // The longest nowcast, so changing its hours needs no new fetch.
            req.minutely_hours = MAX_NOWCAST_HOURS;
            // Every hour in full, for the Hourly tab's details.
            if !req.hourly.is_empty() {
                req.hourly |= HourlyFields::all();
            }
            req
        },
        ctx,
    )
    .with_known_alerts(known_alerts);
    if let Some(ledger) = ledger {
        session = session.with_usage(ledger);
    }
    let mut last_modified = None;
    while let Some(request) = session.next_request() {
        let request = request.clone();
        if request.metered {
            let saved = match session.usage().map(serde_json::to_string) {
                Some(Ok(json)) => host.save_usage(json).map_err(|e| e.to_string()),
                Some(Err(e)) => Err(e.to_string()),
                None => Err("no call ledger".to_string()),
            };
            if let Err(e) = saved {
                session.on_transport_error(&format!("call ledger not saved: {e}"));
                continue;
            }
        }
        let outgoing = host_request(&request);
        match host.send(outgoing).await {
            Ok(resp) => {
                if resp.status == 200 && resp.last_modified.is_some() {
                    last_modified.clone_from(&resp.last_modified);
                }
                session.on_response(&HttpResponse {
                    status: resp.status,
                    body: &resp.body,
                    date: resp.date.map(UnixTime),
                    expires: resp.expires.map(UnixTime),
                    last_modified: resp.last_modified.as_deref(),
                });
            }
            Err(e) => session.on_transport_error(&e.to_string()),
        }
    }
    outcome(session.finish(UnixTime(now)), &place, last_modified)
}

/// The request as the app sees it.
pub(crate) fn host_request(request: &whether_providers::HttpRequest) -> HostRequest {
    HostRequest {
        url: request.url.clone(),
        headers: request
            .headers
            .iter()
            .map(|(n, v)| Header {
                name: (*n).to_string(),
                value: v.clone(),
            })
            .collect(),
        method: match request.method {
            Method::Get => HostMethod::Get,
            Method::Post => HostMethod::Post,
        },
        body: request.body.clone(),
    }
}

/// The session's result for the app.
fn outcome(
    result: Result<(WeatherSnapshot, Vec<Attempt>), FetchError>,
    place: &Place,
    last_modified: Option<String>,
) -> Result<FetchOutcome, WhetherError> {
    match result {
        Ok((mut snapshot, attempts)) => {
            snapshot.offset_changes = place.core_offset_changes();
            let snapshot_json =
                serde_json::to_string(&snapshot).map_err(|e| WhetherError::InvalidJson {
                    what: "snapshot".into(),
                    message: e.to_string(),
                })?;
            Ok(FetchOutcome {
                snapshot_json,
                provider: name(&snapshot.provider),
                last_modified,
                attempts: attempts.iter().map(attempt_info).collect(),
            })
        }
        Err(FetchError::NotModified) => Err(WhetherError::NotModified),
        Err(FetchError::AllProvidersFailed(attempts)) => Err(WhetherError::NoForecast {
            summary: attempts
                .iter()
                .map(attempt_info)
                .map(|a| {
                    format!(
                        "{} {}: {}",
                        a.provider,
                        a.endpoint,
                        a.error.unwrap_or_else(|| "ok".into())
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
        }),
        Err(FetchError::Model(e)) => Err(WhetherError::NoForecast {
            summary: e.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use whether_providers::{Endpoint, HttpRequest};

    #[test]
    fn posts_reach_the_app_with_their_body() {
        let request = HttpRequest {
            url: "https://data.rcc-acis.org/StnData".into(),
            headers: vec![("Accept", "application/json".into())],
            endpoint: Endpoint::Climate,
            required: true,
            metered: false,
            method: Method::Post,
            body: Some(r#"{"sid":"LGA"}"#.into()),
        };
        let host = host_request(&request);
        assert_eq!(host.method, HostMethod::Post);
        assert_eq!(host.body.as_deref(), Some(r#"{"sid":"LGA"}"#));
        assert_eq!(host.headers[0].name, "Accept");
        let debug = format!("{host:?}");
        assert!(
            debug.contains("body_len: Some(13)") && !debug.contains("LGA"),
            "{debug}"
        );
        let get = host_request(&HttpRequest {
            method: Method::Get,
            body: None,
            ..request
        });
        assert_eq!((get.method, get.body), (HostMethod::Get, None));
    }

    #[test]
    fn debug_output_hides_key_headers() {
        let host = host_request(&HttpRequest {
            url: "https://api.pirateweather.net/forecast/key/40,-75".into(),
            headers: vec![("apikey", "SECRET123".into()), ("Accept", "*/*".into())],
            endpoint: Endpoint::Forecast,
            required: true,
            metered: true,
            method: Method::Get,
            body: None,
        });
        assert_eq!(
            host.headers[0].value, "SECRET123",
            "Swift needs the real key"
        );
        let debug = format!("{host:?}");
        assert!(!debug.contains("SECRET123"), "{debug}");
        assert!(debug.contains(r#"("apikey", "***")"#), "{debug}");
    }
}
