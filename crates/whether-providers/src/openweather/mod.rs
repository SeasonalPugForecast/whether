//! OpenWeather (<https://openweathermap.org>): the preferred provider.
//!
//! The free API offers current weather, a 5-day forecast in 3-hour steps and air
//! pollution. Open-Meteo, as supplement, fills what it lacks. With a "One Call by
//! Call" subscription, One Call 4.0 adds hourly, daily and 15-minute timelines under
//! a daily call budget.

pub mod codes;
mod free;
pub mod geocode;
mod key_check;
pub mod onecall4;

pub use key_check::{KeyStatus, key_check_request, parse_key_check};

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use whether_core::builder::SnapshotBuilder;
use whether_core::model::ProviderId;
use whether_core::requirements::DataRequirements;
use whether_core::time::UnixTime;

use crate::error::ProviderError;
use crate::http::{
    Attribution, Capabilities, Endpoint, FetchContext, HttpRequest, HttpResponse, Method, Provider,
};
use crate::url::{Coordinates, QueryBuilder};
use crate::usage::Budget;

/// Which OpenWeather API family to use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum OwmApi {
    /// The free 2.5 endpoints.
    #[default]
    Free,
    /// One Call 3.0 (closed to new subscriptions; not supported).
    OneCall3,
    /// One Call 4.0 ("One Call by Call" subscription), metered.
    OneCall4,
}

/// Calls per UTC day that the "One Call by Call" subscription includes for free.
pub const FREE_DAILY_CALLS: u32 = 1000;
/// The default One Call budget per device, leaving a margin below the free calls.
pub const DEFAULT_DAILY_CALL_LIMIT: u32 = 900;

/// OpenWeather settings, with the API key already resolved.
#[derive(Clone, PartialEq, Eq)]
pub struct OpenWeatherSettings {
    /// The API key (`appid`).
    pub api_key: String,
    /// API family.
    pub api: OwmApi,
    /// Base URL, normally `https://api.openweathermap.org`.
    pub base_url: String,
    /// One Call calls this device may make per UTC day; values above
    /// [`FREE_DAILY_CALLS`] are treated as [`FREE_DAILY_CALLS`].
    pub daily_call_limit: u32,
}

impl core::fmt::Debug for OpenWeatherSettings {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("OpenWeatherSettings")
            .field("api_key", &"***")
            .field("api", &self.api)
            .field("base_url", &self.base_url)
            .field("daily_call_limit", &self.daily_call_limit)
            .finish()
    }
}

/// The default base URL.
pub const DEFAULT_BASE_URL: &str = "https://api.openweathermap.org";

impl OpenWeatherSettings {
    /// Settings for the free API at the default URL.
    pub fn free(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            api: OwmApi::Free,
            base_url: DEFAULT_BASE_URL.to_string(),
            daily_call_limit: DEFAULT_DAILY_CALL_LIMIT,
        }
    }

    /// Settings for One Call 4.0 at the default URL with a daily call limit.
    pub fn onecall4(api_key: impl Into<String>, daily_call_limit: u32) -> Self {
        Self {
            api: OwmApi::OneCall4,
            daily_call_limit,
            ..Self::free(api_key)
        }
    }
}

/// The OpenWeather provider.
#[derive(Clone, Debug)]
pub struct OpenWeather {
    settings: OpenWeatherSettings,
}

/// Coordinate decimals sent to OpenWeather.
const DECIMALS: u8 = 4;

impl OpenWeather {
    /// A provider with these settings.
    pub fn new(settings: OpenWeatherSettings) -> Self {
        Self { settings }
    }

    /// One Call 4.0 requests, most important first so a tight budget keeps them:
    /// current (required), the first hourly page, daily, the nowcast, further hourly
    /// pages. All are metered except air pollution, a free-plan endpoint.
    fn plan_onecall4(
        &self,
        loc: &Coordinates,
        req: &DataRequirements,
        ctx: &FetchContext,
    ) -> Vec<HttpRequest> {
        let key = self.settings.api_key.trim();
        let call = |path: &str, endpoint, required, start: Option<UnixTime>| {
            let mut q = self
                .url(&alloc::format!("/data/4.0/onecall{path}"), loc)
                .param("units", "metric");
            if let Some(start) = start {
                q = q.param("start", start.0.to_string());
            }
            HttpRequest {
                url: q.param("appid", key).finish(),
                headers: vec![],
                endpoint,
                required,
                metered: true,
                method: Method::Get,
                body: None,
            }
        };
        let page_start = |page: u8| {
            let first = onecall4::hour_start(ctx.now);
            UnixTime(first.0 + i64::from(page) * i64::from(onecall4::HOURS_PER_PAGE) * 3600)
        };
        let pages = onecall4::hourly_pages(req);
        let mut requests = vec![call("/current", Endpoint::Current, true, None)];
        if pages > 0 {
            requests.push(call(
                "/timeline/1h",
                Endpoint::Hourly,
                false,
                Some(page_start(0)),
            ));
        }
        if !req.daily.is_empty() {
            requests.push(call("/timeline/1day", Endpoint::Daily, false, None));
        }
        if req.minutely_hours > 0 {
            requests.push(call("/timeline/15min", Endpoint::Nowcast, false, None));
        }
        for page in 1..pages {
            requests.push(call(
                "/timeline/1h",
                Endpoint::Hourly,
                false,
                Some(page_start(page)),
            ));
        }
        if req.air_quality {
            requests.push(HttpRequest {
                url: self
                    .url("/data/2.5/air_pollution", loc)
                    .param("appid", key)
                    .finish(),
                headers: vec![],
                endpoint: Endpoint::AirQuality,
                required: false,
                metered: false,
                method: Method::Get,
                body: None,
            });
        }
        requests
    }

    fn url(&self, path: &str, loc: &Coordinates) -> QueryBuilder {
        let base = self.settings.base_url.trim_end_matches('/');
        let (lat, lon) = loc.fmt_param(DECIMALS);
        QueryBuilder::new(&alloc::format!("{base}{path}"))
            .param("lat", lat)
            .param("lon", lon)
    }
}

/// Number of 3-hour steps covering the requested hours and days (1–40).
pub fn forecast_steps(req: &DataRequirements) -> u8 {
    let hours = u16::from(req.hourly_hours).max(u16::from(req.daily_days) * 24);
    u8::try_from(hours.div_ceil(3).clamp(1, 40)).unwrap_or(40)
}

impl Provider for OpenWeather {
    fn id(&self) -> ProviderId {
        ProviderId::OpenWeather
    }

    fn attribution(&self) -> Attribution {
        Attribution {
            text: ProviderId::OpenWeather.attribution(),
            url: "https://openweathermap.org",
            license: "CC BY-SA 4.0",
        }
    }

    fn capabilities(&self) -> Capabilities {
        let free = Capabilities::CURRENT
            | Capabilities::HOURLY
            | Capabilities::DAILY
            | Capabilities::AIR_QUALITY
            | Capabilities::SUN
            | Capabilities::GEOCODING;
        match self.settings.api {
            OwmApi::OneCall4 => {
                free | Capabilities::MINUTELY_PRECIP | Capabilities::MOON | Capabilities::ALERTS
            }
            OwmApi::Free | OwmApi::OneCall3 => free,
        }
    }

    fn budget(&self) -> Option<Budget> {
        (self.settings.api == OwmApi::OneCall4)
            .then(|| Budget::daily(self.settings.daily_call_limit.min(FREE_DAILY_CALLS)))
    }

    fn metered_calls_per_fetch(&self, req: &DataRequirements) -> u32 {
        match self.settings.api {
            OwmApi::OneCall4 => onecall4::calls_per_fetch(req),
            OwmApi::Free | OwmApi::OneCall3 => 0,
        }
    }

    fn min_refresh_secs(&self) -> u32 {
        600
    }

    /// One Call 4.0 alert details: one metered call.
    fn alert_request(&self, id: &str, _ctx: &FetchContext) -> Option<HttpRequest> {
        if self.settings.api != OwmApi::OneCall4
            || id.is_empty()
            || id.len() > onecall4::MAX_ALERT_ID_LEN
        {
            return None;
        }
        let base = self.settings.base_url.trim_end_matches('/');
        let path = alloc::format!(
            "{base}/data/4.0/onecall/alert/{}",
            crate::url::encode_path_segment(id)
        );
        Some(HttpRequest {
            url: QueryBuilder::new(&path)
                .param("appid", self.settings.api_key.trim())
                .finish(),
            headers: vec![],
            endpoint: Endpoint::Alert,
            required: false,
            metered: true,
            method: Method::Get,
            body: None,
        })
    }

    fn explain(&self, error: ProviderError, _resp: &HttpResponse<'_>) -> ProviderError {
        match error {
            ProviderError::Unauthorized(message) if message.contains("One Call by Call") => {
                ProviderError::Unauthorized(
                    "One Call needs the \"One Call by Call\" subscription, which can take a while \
                     to activate; until then set providers.openweather.api = \"free\""
                        .to_string(),
                )
            }
            ProviderError::RateLimited { .. } if self.settings.api == OwmApi::OneCall4 => {
                ProviderError::Quota(crate::usage::QuotaError::Exhausted {
                    limit: self.settings.daily_call_limit.min(FREE_DAILY_CALLS),
                    period: crate::usage::Period::Day,
                })
            }
            ProviderError::Unauthorized(_) => ProviderError::Unauthorized(
                "OpenWeather rejected the API key. New keys can take a few hours to activate."
                    .to_string(),
            ),
            other => other,
        }
    }

    fn plan(
        &self,
        loc: &Coordinates,
        req: &DataRequirements,
        ctx: &FetchContext,
    ) -> Result<Vec<HttpRequest>, ProviderError> {
        if self.settings.api_key.trim().is_empty() {
            return Err(ProviderError::Config(
                "OpenWeather requires providers.openweather.api_key".to_string(),
            ));
        }
        match self.settings.api {
            OwmApi::OneCall3 => {
                return Err(ProviderError::Config(
                    "OpenWeather One Call 3.0 is not supported; use api = \"onecall-4\" or \"free\""
                        .to_string(),
                ));
            }
            OwmApi::OneCall4 => return Ok(self.plan_onecall4(loc, req, ctx)),
            OwmApi::Free => {}
        }
        let key = self.settings.api_key.trim();
        let request = |path: &str, endpoint, required, extra: Option<(&'static str, String)>| {
            let mut q = self.url(path, loc);
            if endpoint != Endpoint::AirQuality {
                q = q.param("units", "metric");
            }
            if let Some((k, v)) = extra {
                q = q.param(k, v);
            }
            HttpRequest {
                url: q.param("appid", key).finish(),
                headers: vec![],
                endpoint,
                required,
                metered: false,
                method: Method::Get,
                body: None,
            }
        };
        let mut requests = vec![
            request("/data/2.5/weather", Endpoint::Current, true, None),
            request(
                "/data/2.5/forecast",
                Endpoint::Forecast,
                true,
                Some(("cnt", forecast_steps(req).to_string())),
            ),
        ];
        if req.air_quality {
            requests.push(request(
                "/data/2.5/air_pollution",
                Endpoint::AirQuality,
                false,
                None,
            ));
        }
        Ok(requests)
    }

    fn parse(
        &self,
        endpoint: Endpoint,
        resp: &HttpResponse<'_>,
        ctx: &FetchContext,
        out: &mut SnapshotBuilder,
    ) -> Result<(), ProviderError> {
        match (endpoint, self.settings.api) {
            (Endpoint::Current, OwmApi::OneCall4) => onecall4::parse_current(resp.body, out),
            (Endpoint::Current, _) => free::parse_current(resp.body, out),
            (Endpoint::Forecast, _) => free::parse_forecast(resp.body, out),
            (Endpoint::Hourly, _) => onecall4::parse_hourly(resp.body, out),
            (Endpoint::Daily, _) => onecall4::parse_daily(resp.body, out),
            (Endpoint::Nowcast, _) => onecall4::parse_nowcast(resp.body, out),
            (Endpoint::AirQuality, _) => free::parse_air(resp.body, out),
            (Endpoint::Alert, OwmApi::OneCall4) => onecall4::parse_alert(resp.body, ctx, out),
            (
                Endpoint::Sun
                | Endpoint::Geocode
                | Endpoint::Alert
                | Endpoint::Radar
                | Endpoint::Point
                | Endpoint::Stations
                | Endpoint::Observation
                | Endpoint::Climate,
                _,
            ) => Ok(()),
        }
    }
}
