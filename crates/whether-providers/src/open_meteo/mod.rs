//! Open-Meteo (<https://open-meteo.com>): the fallback and supplement provider.
//! Free for non-commercial use under CC BY 4.0; every variable can be requested
//! separately, which keeps device payloads around 2 KB.

mod air;
mod forecast;
pub mod geocode;
pub mod history;
pub mod models;
mod vars;

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use whether_core::builder::SnapshotBuilder;
use whether_core::model::ProviderId;
use whether_core::requirements::DataRequirements;

use crate::error::ProviderError;
use crate::http::{
    Attribution, Capabilities, Endpoint, FetchContext, HttpRequest, HttpResponse, Method, Provider,
};
use crate::url::{Coordinates, QueryBuilder};

/// The default forecast base URL.
pub const DEFAULT_BASE_URL: &str = "https://api.open-meteo.com/v1";
/// The default air-quality base URL.
pub const DEFAULT_AIR_QUALITY_URL: &str = "https://air-quality-api.open-meteo.com/v1";

/// Open-Meteo settings.
#[derive(Clone, PartialEq, Eq)]
pub struct OpenMeteoSettings {
    /// Forecast API base URL.
    pub base_url: String,
    /// Air-quality API base URL.
    pub air_quality_url: String,
    /// Key for commercial plans (sent as `apikey`).
    pub api_key: Option<String>,
    /// Weather models in priority order (`models`); empty means
    /// Open-Meteo's best match.
    pub models: Vec<String>,
}

impl Default for OpenMeteoSettings {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_string(),
            air_quality_url: DEFAULT_AIR_QUALITY_URL.to_string(),
            api_key: None,
            models: Vec::new(),
        }
    }
}

impl core::fmt::Debug for OpenMeteoSettings {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("OpenMeteoSettings")
            .field("base_url", &self.base_url)
            .field("air_quality_url", &self.air_quality_url)
            .field("api_key", &self.api_key.as_ref().map(|_| "***"))
            .field("models", &self.models)
            .finish()
    }
}

/// The Open-Meteo provider.
#[derive(Clone, Debug, Default)]
pub struct OpenMeteo {
    settings: OpenMeteoSettings,
    /// The models joined with commas, which tells supplements apart.
    variant: Option<String>,
}

/// Coordinate decimals sent to Open-Meteo.
const DECIMALS: u8 = 4;

impl OpenMeteo {
    /// A provider with these settings.
    pub fn new(settings: OpenMeteoSettings) -> Self {
        let variant = (!settings.models.is_empty()).then(|| settings.models.join(","));
        Self { settings, variant }
    }

    fn start(base: &str, path: &str, loc: &Coordinates) -> QueryBuilder {
        let (lat, lon) = loc.fmt_param(DECIMALS);
        QueryBuilder::new(&alloc::format!("{}{path}", base.trim_end_matches('/')))
            .param("latitude", lat)
            .param("longitude", lon)
    }

    fn with_key(&self, q: QueryBuilder) -> String {
        match &self.settings.api_key {
            Some(key) => q.param("apikey", key).finish(),
            None => q.finish(),
        }
    }

    /// The forecast URL for `req`.
    fn forecast_url(&self, loc: &Coordinates, req: &DataRequirements) -> String {
        let mut q = Self::start(&self.settings.base_url, "/forecast", loc);
        let current = vars::current(req.current);
        if !current.is_empty() {
            q = q.param("current", current);
        }
        let hourly = vars::hourly(req.hourly);
        if !hourly.is_empty() {
            q = q.param("hourly", hourly);
        }
        let daily = vars::daily(req.daily);
        if !daily.is_empty() {
            q = q.param("daily", daily);
        }
        q = q
            .param("timezone", "auto")
            .param("timeformat", "unixtime")
            .param("wind_speed_unit", "ms")
            .param("forecast_days", req.daily_days.clamp(1, 16).to_string());
        if !req.hourly.is_empty() {
            q = q.param("forecast_hours", req.hourly_hours.max(1).to_string());
            if req.past_hours > 0 {
                q = q.param("past_hours", req.past_hours.to_string());
            }
        }
        if req.minutely_hours > 0 {
            q = q.param("minutely_15", "precipitation").param(
                "forecast_minutely_15",
                (u16::from(req.minutely_hours) * 4).to_string(),
            );
        }
        let models = models::for_location(&self.settings.models, loc);
        if !models.is_empty() {
            q = q.param("models", models.join(","));
        }
        self.with_key(q)
    }

    fn air_quality_url(&self, loc: &Coordinates) -> String {
        let q = Self::start(&self.settings.air_quality_url, "/air-quality", loc)
            .param("current", vars::AIR_QUALITY)
            .param("timezone", "auto")
            .param("timeformat", "unixtime");
        self.with_key(q)
    }
}

impl Provider for OpenMeteo {
    fn id(&self) -> ProviderId {
        ProviderId::OpenMeteo
    }

    fn attribution(&self) -> Attribution {
        Attribution {
            text: ProviderId::OpenMeteo.attribution(),
            url: "https://open-meteo.com",
            license: "CC BY 4.0",
        }
    }

    fn variant(&self) -> Option<&str> {
        self.variant.as_deref()
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::all() - Capabilities::ALERTS
    }

    fn plan(
        &self,
        loc: &Coordinates,
        req: &DataRequirements,
        _ctx: &FetchContext,
    ) -> Result<Vec<HttpRequest>, ProviderError> {
        let mut requests = vec![HttpRequest {
            url: self.forecast_url(loc, req),
            headers: vec![],
            endpoint: Endpoint::Forecast,
            required: true,
            metered: false,
            method: Method::Get,
            body: None,
        }];
        if req.air_quality {
            requests.push(HttpRequest {
                url: self.air_quality_url(loc),
                headers: vec![],
                endpoint: Endpoint::AirQuality,
                required: false,
                metered: false,
                method: Method::Get,
                body: None,
            });
        }
        Ok(requests)
    }

    fn parse(
        &self,
        endpoint: Endpoint,
        resp: &HttpResponse<'_>,
        _ctx: &FetchContext,
        out: &mut SnapshotBuilder,
    ) -> Result<(), ProviderError> {
        match endpoint {
            Endpoint::Forecast => forecast::parse(resp.body, out, &self.settings.models),
            Endpoint::AirQuality => air::parse(resp.body, out),
            Endpoint::Current
            | Endpoint::Hourly
            | Endpoint::Daily
            | Endpoint::Nowcast
            | Endpoint::Sun
            | Endpoint::Geocode
            | Endpoint::Alert
            | Endpoint::Radar
            | Endpoint::Point
            | Endpoint::Stations
            | Endpoint::Observation
            | Endpoint::Climate => Ok(()),
        }
    }
}
