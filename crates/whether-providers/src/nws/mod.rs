//! The US National Weather Service (<https://api.weather.gov>), free and keyless:
//! official watches, warnings and advisories as a supplement to any forecast
//! and, as a primary provider, the forecasters' gridded forecast
//!. Its terms ask for a User-Agent that identifies the application.

mod alerts;
mod grid;
mod icons;
mod observation;
mod points;

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
use crate::regions::covers_us;
use crate::url::{Coordinates, QueryBuilder};

/// The default API base URL.
pub const DEFAULT_BASE_URL: &str = "https://api.weather.gov";

/// Coordinate decimals: the API rejects more than four.
const DECIMALS: u8 = 4;

/// The NWS provider: alerts only, or the forecast as well.
#[derive(Clone, Debug)]
pub struct Nws {
    base_url: String,
    forecast: bool,
    /// With the forecast: the nearest station's observation as the current
    /// conditions.
    observe: bool,
}

impl Default for Nws {
    fn default() -> Self {
        Self::new(DEFAULT_BASE_URL)
    }
}

impl Nws {
    /// Alerts only (a supplement), from `base_url`.
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            forecast: false,
            observe: false,
        }
    }

    /// The forecast and alerts (a primary provider), from `base_url`, with the
    /// nearest station's observation as the current conditions.
    pub fn forecaster(base_url: &str) -> Self {
        Self {
            forecast: true,
            observe: true,
            ..Self::new(base_url)
        }
    }

    /// Whether the forecaster takes the current conditions from the nearest
    /// station (`[providers.nws] current = "observed"`) or the forecast hour.
    #[must_use]
    pub fn observing(self, observe: bool) -> Self {
        Self { observe, ..self }
    }

    fn request(
        &self,
        path: &str,
        endpoint: Endpoint,
        required: bool,
        ctx: &FetchContext,
    ) -> HttpRequest {
        HttpRequest {
            url: alloc::format!("{}{path}", self.base_url),
            headers: vec![
                (
                    "User-Agent",
                    alloc::format!("{} (personal weather app)", ctx.user_agent),
                ),
                ("Accept", "application/geo+json".to_string()),
            ],
            endpoint,
            required,
            metered: false,
            method: Method::Get,
            body: None,
        }
    }

    fn alerts(&self, loc: &Coordinates, required: bool, ctx: &FetchContext) -> HttpRequest {
        let (lat, lon) = loc.fmt_param(DECIMALS);
        let path = QueryBuilder::new("/alerts/active")
            .param("point", alloc::format!("{lat},{lon}"))
            .finish();
        self.request(&path, Endpoint::Alert, required, ctx)
    }
}

impl Provider for Nws {
    fn id(&self) -> ProviderId {
        ProviderId::Nws
    }

    fn attribution(&self) -> Attribution {
        Attribution {
            text: ProviderId::Nws.attribution(),
            url: "https://www.weather.gov",
            license: "Public domain (US government work)",
        }
    }

    fn capabilities(&self) -> Capabilities {
        if self.forecast {
            Capabilities::CURRENT
                | Capabilities::HOURLY
                | Capabilities::DAILY
                | Capabilities::ALERTS
        } else {
            Capabilities::ALERTS
        }
    }

    fn plan(
        &self,
        loc: &Coordinates,
        req: &DataRequirements,
        ctx: &FetchContext,
    ) -> Result<Vec<HttpRequest>, ProviderError> {
        if !self.forecast && !req.alerts {
            return Ok(Vec::new());
        }
        if !covers_us(loc) {
            return Err(ProviderError::OutOfArea("the National Weather Service"));
        }
        if !self.forecast {
            return Ok(vec![self.alerts(loc, true, ctx)]);
        }
        // The point lookup names the grid; its follow-ups fetch the forecast.
        let (lat, lon) = loc.fmt_param(DECIMALS);
        let mut requests = vec![self.request(
            &alloc::format!("/points/{lat},{lon}"),
            Endpoint::Point,
            true,
            ctx,
        )];
        if req.alerts {
            requests.push(self.alerts(loc, false, ctx));
        }
        Ok(requests)
    }

    fn follow_up(
        &self,
        endpoint: Endpoint,
        resp: &HttpResponse<'_>,
        ctx: &FetchContext,
    ) -> Result<Vec<HttpRequest>, ProviderError> {
        match endpoint {
            Endpoint::Point => {
                let grid = points::grid(resp.body)?.path();
                let mut requests = vec![
                    self.request(
                        &alloc::format!("/gridpoints/{grid}"),
                        Endpoint::Forecast,
                        true,
                        ctx,
                    ),
                    self.request(
                        &alloc::format!("/gridpoints/{grid}/forecast"),
                        Endpoint::Daily,
                        false,
                        ctx,
                    ),
                ];
                if self.observe {
                    requests.push(self.request(
                        &alloc::format!("/gridpoints/{grid}/stations?limit=3"),
                        Endpoint::Stations,
                        false,
                        ctx,
                    ));
                }
                Ok(requests)
            }
            Endpoint::Stations => Ok(observation::first_station(resp.body)?
                .map(|id| {
                    self.request(
                        &alloc::format!("/stations/{id}/observations/latest"),
                        Endpoint::Observation,
                        false,
                        ctx,
                    )
                })
                .into_iter()
                .collect()),
            _ => Ok(Vec::new()),
        }
    }

    fn parse(
        &self,
        endpoint: Endpoint,
        resp: &HttpResponse<'_>,
        ctx: &FetchContext,
        out: &mut SnapshotBuilder,
    ) -> Result<(), ProviderError> {
        match endpoint {
            Endpoint::Point => points::parse(resp.body, ctx, out),
            Endpoint::Forecast => grid::parse(resp.body, ctx, out),
            Endpoint::Daily => icons::parse_periods(resp.body, out),
            // The list only names the station to ask (`follow_up`).
            Endpoint::Stations => Ok(()),
            Endpoint::Observation => observation::parse(resp.body, ctx, out),
            _ => alerts::parse(resp, ctx, out),
        }
    }
}
