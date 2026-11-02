//! Pirate Weather (<https://pirateweather.net>): a Dark Sky-compatible forecast
//! from a blend of NOAA (HRRR, NBM, GFS) and other models, with minutely
//! precipitation and alerts, under a monthly call budget.
//!
//! One request returns everything: `GET {base}/forecast/{key}/{lat},{lon}` with
//! `units=si` and `version=2`. The key goes in the `apikey` header, so it never
//! appears in a URL (the path carries a placeholder, which the API requires but
//! ignores when the header is present). `exclude` drops the blocks and models that
//! are not wanted, `extend=hourly` gives 168 hours instead of 48, and
//! `include=aimodels` adds the AI models to the blend.

mod forecast;
mod icons;
mod key_check;

pub use key_check::{KeyStatus, key_check_request, parse_key_check};

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
use crate::usage::{Budget, Period, QuotaError};

/// The default base URL.
pub const DEFAULT_BASE_URL: &str = "https://api.pirateweather.net";
/// Calls a month the free key allows.
pub const FREE_MONTHLY_CALLS: u32 = 10_000;
/// The default monthly budget per device, a margin under [`FREE_MONTHLY_CALLS`] for
/// other devices on the same key and for the month's boundary.
pub const DEFAULT_MONTHLY_CALL_LIMIT: u32 = 9_000;
/// The models whose results `exclude` can leave out of the blend (docs.pirateweather.net).
pub const MODELS: [&str; 17] = [
    "hrrr",
    "nbm",
    "gefs",
    "gfs",
    "rtma_ru",
    "ecmwf_ifs",
    "dwd_mosmix",
    "ecmwf_aifs",
    "aigefs",
    "aigfs",
    "raqdps",
    "silam",
    "hrdps",
    "gdps",
    "reps",
    "geps",
    "cmc",
];

/// Hours the API returns without `extend=hourly`.
const BASE_HOURS: u8 = 48;
/// The placeholder in the path, where the key would go.
const PATH_KEY: &str = "key";
/// Coordinate decimals sent (about 11 m).
const DECIMALS: u8 = 4;

/// Pirate Weather settings, with the API key already resolved.
#[derive(Clone, PartialEq, Eq)]
pub struct PirateWeatherSettings {
    /// The API key (the `apikey` header).
    pub api_key: String,
    /// Base URL, normally [`DEFAULT_BASE_URL`].
    pub base_url: String,
    /// Models left out of the blend (see [`MODELS`]); the next best serves.
    pub exclude_models: Vec<String>,
    /// Whether to add the AI models (`include=aimodels`).
    pub ai_models: bool,
    /// Calls this device may make per UTC month.
    pub monthly_call_limit: u32,
}

impl core::fmt::Debug for PirateWeatherSettings {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PirateWeatherSettings")
            .field("api_key", &"***")
            .field("base_url", &self.base_url)
            .field("exclude_models", &self.exclude_models)
            .field("ai_models", &self.ai_models)
            .field("monthly_call_limit", &self.monthly_call_limit)
            .finish()
    }
}

impl PirateWeatherSettings {
    /// Settings at the default URL and budget.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: DEFAULT_BASE_URL.to_string(),
            exclude_models: Vec::new(),
            ai_models: false,
            monthly_call_limit: DEFAULT_MONTHLY_CALL_LIMIT,
        }
    }
}

/// The Pirate Weather provider.
#[derive(Clone, Debug)]
pub struct PirateWeather {
    settings: PirateWeatherSettings,
}

impl PirateWeather {
    /// A provider with these settings.
    pub fn new(settings: PirateWeatherSettings) -> Self {
        Self { settings }
    }

    /// The `exclude` list: `summary` (the docs list it; the recording still held the
    /// text summaries, which nothing here shows), the blocks the requirements leave
    /// out, and the configured models. A model id with anything
    /// but letters, digits, `_` and `-` is dropped, so it cannot add parameters.
    fn exclude(&self, req: &DataRequirements) -> String {
        let mut parts: Vec<&str> = vec!["summary"];
        if req.minutely_hours == 0 {
            parts.push("minutely");
        }
        if req.hourly_hours == 0 {
            parts.push("hourly");
        }
        if req.daily_days == 0 {
            parts.push("daily");
        }
        if !req.alerts {
            parts.push("alerts");
        }
        parts.extend(
            self.settings
                .exclude_models
                .iter()
                .map(|m| m.trim())
                .filter(|m| {
                    !m.is_empty()
                        && m.chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                }),
        );
        parts.join(",")
    }

    /// The forecast request for `loc`.
    fn forecast_url(&self, loc: &Coordinates, req: &DataRequirements) -> String {
        let (lat, lon) = loc.fmt_param(DECIMALS);
        let base = self.settings.base_url.trim_end_matches('/');
        let mut q = QueryBuilder::new(&alloc::format!("{base}/forecast/{PATH_KEY}/{lat},{lon}"))
            .param("units", "si")
            .param("version", "2")
            .param("exclude", self.exclude(req));
        if req.hourly_hours > BASE_HOURS {
            q = q.param("extend", "hourly");
        }
        if self.settings.ai_models {
            q = q.param("include", "aimodels");
        }
        q.finish()
    }
}

impl Provider for PirateWeather {
    fn id(&self) -> ProviderId {
        ProviderId::PirateWeather
    }

    fn attribution(&self) -> Attribution {
        // The docs ask for no credit, but the data is the project's work and
        // blends public NOAA models; every display credits it.
        Attribution {
            text: ProviderId::PirateWeather.attribution(),
            url: "https://pirateweather.net",
            license: "Pirate Weather API terms",
        }
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::CURRENT
            | Capabilities::HOURLY
            | Capabilities::DAILY
            | Capabilities::MINUTELY_PRECIP
            | Capabilities::ALERTS
            | Capabilities::SUN
            | Capabilities::MOON
    }

    fn budget(&self) -> Option<Budget> {
        Some(Budget::monthly(self.settings.monthly_call_limit))
    }

    fn metered_calls_per_fetch(&self, _req: &DataRequirements) -> u32 {
        1
    }

    /// The API serves a cached answer for 15 minutes (`Cache-Control: max-age=900`),
    /// so asking sooner only spends the monthly budget.
    fn min_refresh_secs(&self) -> u32 {
        900
    }

    fn explain(&self, error: ProviderError, _resp: &HttpResponse<'_>) -> ProviderError {
        match error {
            ProviderError::Unauthorized(_) => ProviderError::Unauthorized(
                "Pirate Weather rejected the API key. Check it, and that it is subscribed to \
                 the Forecast API (a new subscription can take 20 minutes to apply)."
                    .to_string(),
            ),
            // 429: the key has used its calls for the month (other devices count too).
            ProviderError::RateLimited { .. } => ProviderError::Quota(QuotaError::Exhausted {
                limit: self.settings.monthly_call_limit,
                period: Period::Month,
            }),
            other => other,
        }
    }

    fn plan(
        &self,
        loc: &Coordinates,
        req: &DataRequirements,
        ctx: &FetchContext,
    ) -> Result<Vec<HttpRequest>, ProviderError> {
        let key = self.settings.api_key.trim();
        if key.is_empty() {
            return Err(ProviderError::Config(
                "Pirate Weather requires an API key (PIRATE_WEATHER_API_KEY)".to_string(),
            ));
        }
        Ok(vec![HttpRequest {
            url: self.forecast_url(loc, req),
            headers: vec![
                ("User-Agent", ctx.user_agent.clone()),
                ("apikey", key.to_string()),
            ],
            endpoint: Endpoint::Forecast,
            required: true,
            metered: true,
            method: Method::Get,
            body: None,
        }])
    }

    fn parse(
        &self,
        endpoint: Endpoint,
        resp: &HttpResponse<'_>,
        ctx: &FetchContext,
        out: &mut SnapshotBuilder,
    ) -> Result<(), ProviderError> {
        match endpoint {
            Endpoint::Forecast => forecast::parse(resp.body, ctx.now, ctx.utc_offset_hint, out),
            Endpoint::Current
            | Endpoint::Hourly
            | Endpoint::Daily
            | Endpoint::Nowcast
            | Endpoint::AirQuality
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
