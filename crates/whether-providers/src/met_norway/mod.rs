//! MET Norway (<https://api.met.no>): free, keyless forecasts under CC BY 4.0. Its
//! terms require a User-Agent with contact details and honouring `Expires`, and
//! conditional requests (`If-Modified-Since`) keep the load low.

mod forecast;
mod sun;
pub(crate) mod symbols;

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use whether_core::builder::SnapshotBuilder;
use whether_core::model::ProviderId;
use whether_core::requirements::DataRequirements;
use whether_core::time::UtcOffset;

use crate::error::ProviderError;
use crate::http::{
    Attribution, Capabilities, Endpoint, FetchContext, HttpRequest, HttpResponse, Method, Provider,
};
use crate::url::{Coordinates, QueryBuilder};

/// The default API base URL.
pub const DEFAULT_BASE_URL: &str = "https://api.met.no/weatherapi";

/// Coordinate decimals: MET asks for at most four (about 11 m).
const DECIMALS: u8 = 4;

/// Which forecast product to request.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MetProduct {
    /// All variables (UV, dew point, extremes, probabilities).
    #[default]
    Complete,
    /// Fewer variables, smaller responses.
    Compact,
}

/// MET Norway settings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetNorwaySettings {
    /// Contact details for the User-Agent (an email or URL), required by the terms.
    pub contact: String,
    /// The forecast product.
    pub product: MetProduct,
    /// API base URL.
    pub base_url: String,
}

impl MetNorwaySettings {
    /// Settings with `contact` and the defaults.
    pub fn new(contact: impl Into<String>) -> Self {
        Self {
            contact: contact.into(),
            product: MetProduct::default(),
            base_url: DEFAULT_BASE_URL.to_string(),
        }
    }
}

/// The MET Norway provider.
#[derive(Clone, Debug)]
pub struct MetNorway {
    settings: MetNorwaySettings,
}

/// `+HH:MM` for an offset.
fn offset_param(o: UtcOffset) -> String {
    let sign = if o.0 < 0 { '-' } else { '+' };
    let abs = o.0.unsigned_abs();
    alloc::format!("{sign}{:02}:{:02}", abs / 3600, abs % 3600 / 60)
}

impl MetNorway {
    /// A provider with these settings.
    pub fn new(settings: MetNorwaySettings) -> Self {
        Self { settings }
    }

    fn headers(&self, ctx: &FetchContext) -> Vec<(&'static str, String)> {
        // The contact goes in once: a host whose agent already carries it (the
        // iOS app's `whether/<v> (iOS; +<url>)`) is sent as it is.
        let contact = self.settings.contact.trim();
        let agent = if ctx.user_agent.contains(contact) {
            ctx.user_agent.clone()
        } else {
            alloc::format!("{} {contact}", ctx.user_agent)
        };
        let mut headers = vec![("User-Agent", agent)];
        if let Some(since) = &ctx.if_modified_since {
            headers.push(("If-Modified-Since", since.clone()));
        }
        headers
    }

    fn base(&self) -> &str {
        self.settings.base_url.trim_end_matches('/')
    }

    fn forecast_url(&self, loc: &Coordinates) -> String {
        let product = match self.settings.product {
            MetProduct::Complete => "complete",
            MetProduct::Compact => "compact",
        };
        let (lat, lon) = loc.fmt_param(DECIMALS);
        QueryBuilder::new(&alloc::format!(
            "{}/locationforecast/2.0/{product}",
            self.base()
        ))
        .param("lat", lat)
        .param("lon", lon)
        .finish()
    }

    fn sun_url(&self, loc: &Coordinates, ctx: &FetchContext, offset: UtcOffset) -> String {
        let d = ctx.now.local_date(offset);
        let (lat, lon) = loc.fmt_param(DECIMALS);
        QueryBuilder::new(&alloc::format!("{}/sunrise/3.0/sun", self.base()))
            .param("lat", lat)
            .param("lon", lon)
            .param(
                "date",
                alloc::format!("{:04}-{:02}-{:02}", d.year, d.month, d.day),
            )
            .param("offset", offset_param(offset))
            .finish()
    }
}

impl Provider for MetNorway {
    fn id(&self) -> ProviderId {
        ProviderId::MetNorway
    }

    fn attribution(&self) -> Attribution {
        Attribution {
            text: ProviderId::MetNorway.attribution(),
            url: "https://api.met.no",
            license: "CC BY 4.0",
        }
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::CURRENT | Capabilities::HOURLY | Capabilities::DAILY | Capabilities::SUN
    }

    fn honors_expires(&self) -> bool {
        true
    }

    fn explain(&self, error: ProviderError, _resp: &HttpResponse<'_>) -> ProviderError {
        match error {
            ProviderError::Unauthorized(message) => ProviderError::Unauthorized(alloc::format!(
                "{message}; MET Norway refuses requests without contact details, check [providers.met-norway] contact"
            )),
            other => other,
        }
    }

    fn plan(
        &self,
        loc: &Coordinates,
        req: &DataRequirements,
        ctx: &FetchContext,
    ) -> Result<Vec<HttpRequest>, ProviderError> {
        if self.settings.contact.trim().is_empty() {
            return Err(ProviderError::Config(
                "MET Norway needs [providers.met-norway] contact (an email or URL)".into(),
            ));
        }
        let mut requests = vec![HttpRequest {
            url: self.forecast_url(loc),
            headers: self.headers(ctx),
            endpoint: Endpoint::Forecast,
            required: true,
            metered: false,
            method: Method::Get,
            body: None,
        }];
        // Sunrise needs the local date and offset; without them it is skipped.
        if !req.daily.is_empty()
            && let Some(offset) = ctx.utc_offset_hint
        {
            requests.push(HttpRequest {
                url: self.sun_url(loc, ctx, offset),
                headers: self.headers(&FetchContext {
                    if_modified_since: None,
                    ..ctx.clone()
                }),
                endpoint: Endpoint::Sun,
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
        ctx: &FetchContext,
        out: &mut SnapshotBuilder,
    ) -> Result<(), ProviderError> {
        match endpoint {
            Endpoint::Forecast => forecast::parse(resp.body, ctx.now, ctx.utc_offset_hint, out),
            Endpoint::Sun => {
                sun::parse(resp.body, ctx.utc_offset_hint.unwrap_or(UtcOffset(0)), out)
            }
            Endpoint::Current
            | Endpoint::Hourly
            | Endpoint::Daily
            | Endpoint::Nowcast
            | Endpoint::AirQuality
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

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> FetchContext {
        FetchContext {
            now: whether_core::testdata::SAMPLE_NOW,
            user_agent: "whether/0.1.0".into(),
            utc_offset_hint: Some(UtcOffset(-14_400)),
            if_modified_since: Some("Mon, 05 Oct 2026 14:00:00 GMT".into()),
            language: "en".into(),
        }
    }

    const LOC: Coordinates = Coordinates {
        latitude: 40.714_27,
        longitude: -74.005_97,
    };

    #[test]
    fn plans_with_contact_rounding_and_conditions() {
        let p = MetNorway::new(MetNorwaySettings::new("someone@example.org"));
        let plan = p.plan(&LOC, &DataRequirements::full(), &ctx()).unwrap();
        assert_eq!(
            plan[0].url,
            "https://api.met.no/weatherapi/locationforecast/2.0/complete?lat=40.7143&lon=-74.006"
        );
        assert_eq!(
            plan[0].headers,
            [
                ("User-Agent", "whether/0.1.0 someone@example.org".into()),
                ("If-Modified-Since", "Mon, 05 Oct 2026 14:00:00 GMT".into())
            ]
        );
        assert!(plan[0].required && !plan[0].metered);
        assert_eq!(plan[1].endpoint, Endpoint::Sun);
        assert_eq!(
            plan[1].url,
            "https://api.met.no/weatherapi/sunrise/3.0/sun?lat=40.7143&lon=-74.006&date=2026-10-05&offset=-04%3A00"
        );
        assert!(!plan[1].required);
    }

    #[test]
    fn the_contact_is_not_repeated_when_the_agent_has_it() {
        let p = MetNorway::new(MetNorwaySettings::new("https://example.org/whether"));
        let with_contact = FetchContext {
            user_agent: "whether/0.1.0 (iOS; +https://example.org/whether)".into(),
            ..ctx()
        };
        let plan = p
            .plan(&LOC, &DataRequirements::full(), &with_contact)
            .unwrap();
        assert_eq!(
            plan[0].headers[0],
            (
                "User-Agent",
                "whether/0.1.0 (iOS; +https://example.org/whether)".into()
            )
        );
    }

    #[test]
    fn compact_and_no_offset() {
        let p = MetNorway::new(MetNorwaySettings {
            product: MetProduct::Compact,
            ..MetNorwaySettings::new("x@y")
        });
        let no_offset = FetchContext {
            utc_offset_hint: None,
            if_modified_since: None,
            ..ctx()
        };
        let plan = p.plan(&LOC, &DataRequirements::full(), &no_offset).unwrap();
        assert_eq!(plan.len(), 1, "no sunrise without an offset");
        assert!(plan[0].url.contains("/2.0/compact?"));
        assert_eq!(plan[0].headers.len(), 1);
    }

    #[test]
    fn a_contact_is_required() {
        let p = MetNorway::new(MetNorwaySettings::new("  "));
        assert!(matches!(
            p.plan(&LOC, &DataRequirements::full(), &ctx()),
            Err(ProviderError::Config(_))
        ));
    }

    #[test]
    fn offsets_format_with_sign() {
        assert_eq!(offset_param(UtcOffset(-14_400)), "-04:00");
        assert_eq!(offset_param(UtcOffset(19_800)), "+05:30");
        assert_eq!(offset_param(UtcOffset(0)), "+00:00");
    }
}
