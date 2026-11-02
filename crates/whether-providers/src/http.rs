//! The provider contract: requests out, bytes in.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use bitflags::bitflags;
use whether_core::builder::SnapshotBuilder;
use whether_core::model::ProviderId;
use whether_core::requirements::DataRequirements;
use whether_core::time::{UnixTime, UtcOffset};

use crate::error::ProviderError;
use crate::url::{Coordinates, is_secret_header, redact_url};
use crate::usage::Budget;

/// Which API resource a request targets; selects the parser for its response.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum Endpoint {
    Forecast,
    Current,
    Hourly,
    Daily,
    Nowcast,
    AirQuality,
    Sun,
    Geocode,
    /// Details of one alert that another response named by id.
    Alert,
    /// Radar tiles and their metadata ([`crate::radar`]); no provider parses these.
    Radar,
    /// A point's metadata that names the resources to request next (the NWS
    /// `/points` lookup).
    Point,
    /// The weather stations near a place (NWS, ACIS).
    Stations,
    /// One station's latest observation (NWS).
    Observation,
    /// Observed days, normals and records (history providers).
    Climate,
}

/// The HTTP method of a request.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Method {
    /// GET, without a body.
    #[default]
    Get,
    /// POST with a JSON body (ACIS).
    Post,
}

bitflags! {
    /// What a provider can supply.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct Capabilities: u16 {
        /// Current conditions.
        const CURRENT = 1 << 0;
        /// Hourly forecast.
        const HOURLY = 1 << 1;
        /// Daily forecast.
        const DAILY = 1 << 2;
        /// 15-minute precipitation.
        const MINUTELY_PRECIP = 1 << 3;
        /// Air quality.
        const AIR_QUALITY = 1 << 4;
        /// Official alerts.
        const ALERTS = 1 << 5;
        /// Sunrise and sunset.
        const SUN = 1 << 6;
        /// Moon phase and times.
        const MOON = 1 << 7;
        /// Place search.
        const GEOCODING = 1 << 8;
    }
}

/// The credit a provider's data license requires.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Attribution {
    /// Text every display shows.
    pub text: &'static str,
    /// Link to the provider.
    pub url: &'static str,
    /// License name.
    pub license: &'static str,
}

/// Per-fetch context. The clock is read by the caller, never by providers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchContext {
    /// The current time.
    pub now: UnixTime,
    /// `whether/<version>`, extended by providers that need contact details.
    pub user_agent: String,
    /// The location's UTC offset, for providers that return only UTC times.
    pub utc_offset_hint: Option<UtcOffset>,
    /// `Last-Modified` from the previous response, for conditional requests.
    pub if_modified_since: Option<String>,
    /// The preferred language for provider text such as alert descriptions
    /// (`general.language`, e.g. `"en"` or `"de-AT"`).
    pub language: String,
}

/// An HTTP request planned by a provider.
#[derive(Clone, PartialEq, Eq)]
pub struct HttpRequest {
    /// Full URL including the query. May contain an API key; log it only through
    /// [`redact_url`] (the `Debug` impl does).
    pub url: String,
    /// Extra request headers.
    pub headers: Vec<(&'static str, String)>,
    /// The resource, which selects the parser.
    pub endpoint: Endpoint,
    /// Whether the fetch fails without this response.
    pub required: bool,
    /// Whether the request counts against the provider's call budget
    /// ([`Provider::budget`], at [`Provider::request_cost`] units); the session sends
    /// it only within budget.
    pub metered: bool,
    /// GET or POST.
    pub method: Method,
    /// The JSON body of a POST.
    pub body: Option<String>,
}

impl fmt::Debug for HttpRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpRequest")
            .field("url", &redact_url(&self.url))
            .field("headers", &RedactedHeaders(&self.headers))
            .field("endpoint", &self.endpoint)
            .field("required", &self.required)
            .field("metered", &self.metered)
            .field("method", &self.method)
            .field("body_len", &self.body.as_ref().map(String::len))
            .finish()
    }
}

/// `Debug` for request headers that masks the values of secret ones (an `apikey`).
struct RedactedHeaders<'a>(&'a [(&'static str, String)]);

impl fmt::Debug for RedactedHeaders<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(
                self.0.iter().map(|(name, value)| {
                    (*name, if is_secret_header(name) { "***" } else { value })
                }),
            )
            .finish()
    }
}

/// An HTTP response as seen by a parser.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HttpResponse<'a> {
    /// Status code.
    pub status: u16,
    /// Body bytes (decompressed, de-chunked).
    pub body: &'a [u8],
    /// The `Date` header.
    pub date: Option<UnixTime>,
    /// The `Expires` header.
    pub expires: Option<UnixTime>,
    /// The `Last-Modified` header.
    pub last_modified: Option<&'a str>,
}

impl<'a> HttpResponse<'a> {
    /// A response with just a status and body.
    pub fn new(status: u16, body: &'a [u8]) -> Self {
        Self {
            status,
            body,
            date: None,
            expires: None,
            last_modified: None,
        }
    }
}

/// A weather provider: plans requests and parses responses. Implementations hold
/// only settings; they never do I/O or read the clock.
pub trait Provider: Send + Sync {
    /// Which provider this is.
    fn id(&self) -> ProviderId;
    /// The required credit.
    fn attribution(&self) -> Attribution;
    /// What it can supply.
    fn capabilities(&self) -> Capabilities;
    /// The shortest allowed refresh interval.
    fn min_refresh_secs(&self) -> u32 {
        300
    }
    /// How much this provider's metered requests ([`HttpRequest::metered`]) may cost
    /// per UTC day or month, or `None` when it sends none.
    fn budget(&self) -> Option<Budget> {
        None
    }
    /// What a metered `request` costs against [`Self::budget`]: one unit unless the
    /// provider bills by weight (records, say).
    fn request_cost(&self, _request: &HttpRequest) -> u32 {
        1
    }
    /// The most metered requests one fetch for `req` makes.
    fn metered_calls_per_fetch(&self, _req: &DataRequirements) -> u32 {
        0
    }
    /// The most cost one fetch for `req` incurs, for pacing refreshes (see
    /// [`Budget::paced_refresh_secs`]). Equal to the calls when every request costs 1.
    fn metered_cost_per_fetch(&self, req: &DataRequirements) -> u32 {
        self.metered_calls_per_fetch(req)
    }
    /// Which variant of the provider this is (Open-Meteo's weather model), so
    /// the same provider with other settings can supplement it.
    fn variant(&self) -> Option<&str> {
        None
    }
    /// Whether the caller must wait until the `Expires` time before refetching.
    fn honors_expires(&self) -> bool {
        false
    }
    /// Rewrites a status error with provider-specific guidance (e.g. why a key was
    /// rejected). The default keeps the error unchanged.
    fn explain(&self, error: ProviderError, _resp: &HttpResponse<'_>) -> ProviderError {
        error
    }
    /// The requests needed for `loc` and `req`, in order.
    fn plan(
        &self,
        loc: &Coordinates,
        req: &DataRequirements,
        ctx: &FetchContext,
    ) -> Result<Vec<HttpRequest>, ProviderError>;
    /// The request for the details of alert `id`, which a parsed response named with
    /// [`SnapshotBuilder::refer_alert`]. The session sends it with
    /// [`Endpoint::Alert`] unless the caller already has that alert.
    fn alert_request(&self, _id: &str, _ctx: &FetchContext) -> Option<HttpRequest> {
        None
    }
    /// Requests that a successful response to `endpoint` calls for next, derived
    /// from the response alone (providers keep no state): the NWS `/points` lookup
    /// names the forecast grid and the stations. They run right after
    /// it; a follow-up of a required request is required. None by default.
    fn follow_up(
        &self,
        _endpoint: Endpoint,
        _resp: &HttpResponse<'_>,
        _ctx: &FetchContext,
    ) -> Result<Vec<HttpRequest>, ProviderError> {
        Ok(Vec::new())
    }
    /// Parses one response into `out`.
    fn parse(
        &self,
        endpoint: Endpoint,
        resp: &HttpResponse<'_>,
        ctx: &FetchContext,
        out: &mut SnapshotBuilder,
    ) -> Result<(), ProviderError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;
    use alloc::string::ToString;

    #[test]
    fn request_debug_output_hides_api_keys() {
        let req = HttpRequest {
            url: "https://api.openweathermap.org/data/2.5/weather?lat=40&lon=-75&appid=SECRET123"
                .to_string(),
            headers: Vec::new(),
            endpoint: Endpoint::Current,
            required: true,
            metered: false,
            method: Method::Get,
            body: None,
        };
        let debug = format!("{req:?}");
        assert!(!debug.contains("SECRET123"), "{debug}");
        let post = HttpRequest {
            method: Method::Post,
            body: Some("{\"sid\":\"LGA\"}".to_string()),
            ..req
        };
        let debug = format!("{post:?}");
        assert!(
            debug.contains("body_len: Some(13)") && !debug.contains("LGA"),
            "{debug}"
        );
        assert!(debug.contains("appid=***"));
    }

    #[test]
    fn request_debug_output_hides_key_headers() {
        let req = HttpRequest {
            url: "https://api.example/forecast/key/40,-75".to_string(),
            headers: alloc::vec![
                ("User-Agent", "whether/1".to_string()),
                ("apikey", "SECRET123".to_string()),
                ("Authorization", "Bearer SECRET456".to_string()),
            ],
            endpoint: Endpoint::Forecast,
            required: true,
            metered: true,
            method: Method::Get,
            body: None,
        };
        let debug = format!("{req:?}");
        assert!(!debug.contains("SECRET"), "{debug}");
        assert!(debug.contains("whether/1"), "{debug}");
        assert!(debug.contains(r#"("apikey", "***")"#), "{debug}");
    }
}
