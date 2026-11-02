//! Open-Meteo history, worldwide: past days from the archived model
//! forecasts (`historical-forecast-api`, up to yesterday), and normals computed
//! once from the ERA5 reanalysis (`archive-api`, 1991–2020) for the caller to
//! keep. No records: a 25 km grid is not a station's record.
//!
//! The normals request is heavy: 30 years of three daily variables weigh about
//! 235 calls of the 10,000 a day (about 277 KB), so it is made only when the
//! caller has no normals for the place.

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use serde::Deserialize;
use whether_core::history::{
    ClimateQuery, DailyValues, HistoryBuilder, HistoryKind, Normals, YearRange, climate_from_daily,
};
use whether_core::model::ProviderId;
use whether_core::requirements::HistoryRequirements;
use whether_core::time::{LocalDate, UnixTime, UtcOffset};
use whether_core::units::{Celsius, Mm};

use super::models;
use crate::error::ProviderError;
use crate::history::{HistoryContext, HistoryProvider};
use crate::http::{Attribution, Endpoint, HttpRequest, HttpResponse, Method};
use crate::url::{Coordinates, QueryBuilder};

/// The default archive (ERA5) base URL.
pub const DEFAULT_ARCHIVE_URL: &str = "https://archive-api.open-meteo.com/v1";
/// The default base URL of the archived model forecasts.
pub const DEFAULT_HISTORICAL_FORECAST_URL: &str =
    "https://historical-forecast-api.open-meteo.com/v1";

/// The years the normals cover.
const NORMALS: YearRange = YearRange::NORMALS_1991_2020;

/// Open-Meteo history settings.
#[derive(Clone, PartialEq, Eq)]
pub struct OpenMeteoHistorySettings {
    /// Archive (ERA5) base URL.
    pub archive_url: String,
    /// Archived model forecasts base URL.
    pub historical_forecast_url: String,
    /// Key for commercial plans (sent as `apikey`).
    pub api_key: Option<String>,
    /// Weather models for the past days, as for the forecast.
    pub models: Vec<String>,
    /// Whether to compute normals when the caller keeps none (off for a
    /// supplement behind a source with its own, such as ACIS).
    pub normals: bool,
}

impl Default for OpenMeteoHistorySettings {
    fn default() -> Self {
        Self {
            archive_url: DEFAULT_ARCHIVE_URL.to_string(),
            historical_forecast_url: DEFAULT_HISTORICAL_FORECAST_URL.to_string(),
            api_key: None,
            models: Vec::new(),
            normals: true,
        }
    }
}

impl core::fmt::Debug for OpenMeteoHistorySettings {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("OpenMeteoHistorySettings")
            .field("archive_url", &self.archive_url)
            .field("historical_forecast_url", &self.historical_forecast_url)
            .field("api_key", &self.api_key.as_ref().map(|_| "***"))
            .field("models", &self.models)
            .field("normals", &self.normals)
            .finish()
    }
}

/// The Open-Meteo history provider.
#[derive(Clone, Debug, Default)]
pub struct OpenMeteoHistory {
    settings: OpenMeteoHistorySettings,
}

#[derive(Deserialize)]
struct Response {
    utc_offset_seconds: i32,
    daily: Option<Daily>,
}

#[derive(Deserialize)]
struct Daily {
    time: Vec<i64>,
    #[serde(default)]
    temperature_2m_max: Vec<Option<f32>>,
    #[serde(default)]
    temperature_2m_min: Vec<Option<f32>>,
    #[serde(default)]
    precipitation_sum: Vec<Option<f32>>,
    /// Centimetres of snow.
    #[serde(default)]
    snowfall_sum: Vec<Option<f32>>,
}

fn iso(d: LocalDate) -> String {
    alloc::format!("{:04}-{:02}-{:02}", d.year, d.month, d.day)
}

fn at(column: &[Option<f32>], i: usize) -> Option<f32> {
    column.get(i).copied().flatten().filter(|v| v.is_finite())
}

impl OpenMeteoHistory {
    /// A provider with these settings.
    pub fn new(settings: OpenMeteoHistorySettings) -> Self {
        Self { settings }
    }

    fn query(
        base: &str,
        path: &str,
        loc: &Coordinates,
        from: LocalDate,
        to: LocalDate,
        daily: &str,
    ) -> QueryBuilder {
        let (lat, lon) = loc.fmt_param(4);
        QueryBuilder::new(&alloc::format!("{}{path}", base.trim_end_matches('/')))
            .param("latitude", lat)
            .param("longitude", lon)
            .param("start_date", iso(from))
            .param("end_date", iso(to))
            .param("daily", daily)
            .param("timezone", "auto")
            .param("timeformat", "unixtime")
    }

    fn request(
        &self,
        q: QueryBuilder,
        endpoint: Endpoint,
        required: bool,
        ctx: &HistoryContext,
    ) -> HttpRequest {
        let url = match &self.settings.api_key {
            Some(key) => q.param("apikey", key).finish(),
            None => q.finish(),
        };
        HttpRequest {
            url,
            headers: vec![("User-Agent", ctx.user_agent.clone())],
            endpoint,
            required,
            metered: false,
            method: Method::Get,
            body: None,
        }
    }

    /// The response as JSON, with several models merged in the configured order.
    fn read(&self, body: &[u8]) -> Result<Response, ProviderError> {
        let malformed = |e: serde_json::Error| ProviderError::Malformed(e.to_string());
        if self.settings.models.len() < 2 {
            return serde_json::from_slice(body).map_err(malformed);
        }
        let mut value: serde_json::Value = serde_json::from_slice(body).map_err(malformed)?;
        let order = models::candidates(&self.settings.models);
        if models::is_multi_model(&value, &order) {
            models::merge(&mut value, &order);
        }
        serde_json::from_value(value).map_err(malformed)
    }
}

/// Every day's values in a daily answer.
fn daily_values(daily: &Daily, offset: UtcOffset) -> Vec<DailyValues> {
    dates(daily, offset)
        .map(|(i, date)| {
            (
                date,
                at(&daily.temperature_2m_max, i),
                at(&daily.temperature_2m_min, i),
                at(&daily.precipitation_sum, i),
            )
        })
        .collect()
}

/// The local date of each daily step (local midnight, in UTC).
fn dates(daily: &Daily, offset: UtcOffset) -> impl Iterator<Item = (usize, LocalDate)> + '_ {
    daily
        .time
        .iter()
        .enumerate()
        .map(move |(i, t)| (i, UnixTime(*t).local_date(offset)))
}

impl HistoryProvider for OpenMeteoHistory {
    fn id(&self) -> ProviderId {
        ProviderId::OpenMeteo
    }

    fn attribution(&self) -> Attribution {
        Attribution {
            text: ProviderId::OpenMeteo.attribution(),
            url: "https://open-meteo.com",
            license: "CC BY 4.0; ERA5: contains modified Copernicus Climate Change Service information",
        }
    }

    fn plan(
        &self,
        loc: &Coordinates,
        req: &HistoryRequirements,
        ctx: &HistoryContext,
    ) -> Result<Vec<HttpRequest>, ProviderError> {
        let mut out = Vec::new();
        let (first, yesterday) = (ctx.first_date(req), ctx.day(-1));
        if first <= yesterday {
            let mut q = Self::query(
                &self.settings.historical_forecast_url,
                "/forecast",
                loc,
                first,
                yesterday,
                "temperature_2m_max,temperature_2m_min,precipitation_sum,snowfall_sum",
            );
            let chosen = models::for_location(&self.settings.models, loc);
            if !chosen.is_empty() {
                q = q.param("models", chosen.join(","));
            }
            out.push(self.request(q, Endpoint::Daily, true, ctx));
        }
        if self.settings.normals && ctx.normals.is_none() && !req.is_empty() {
            let q = Self::query(
                &self.settings.archive_url,
                "/archive",
                loc,
                LocalDate {
                    year: NORMALS.first,
                    month: 1,
                    day: 1,
                },
                LocalDate {
                    year: NORMALS.last,
                    month: 12,
                    day: 31,
                },
                "temperature_2m_max,temperature_2m_min,precipitation_sum",
            );
            // Without past days, the normals are all this source gives.
            let required = out.is_empty();
            out.push(self.request(q, Endpoint::Climate, required, ctx));
        }
        Ok(out)
    }

    fn parse(
        &self,
        endpoint: Endpoint,
        resp: &HttpResponse<'_>,
        ctx: &HistoryContext,
        out: &mut HistoryBuilder,
    ) -> Result<(), ProviderError> {
        let r = self.read(resp.body)?;
        let daily = r
            .daily
            .ok_or_else(|| ProviderError::Malformed("no daily values".into()))?;
        let offset = UtcOffset(r.utc_offset_seconds);
        out.set_kind(HistoryKind::Reanalysis);
        if endpoint == Endpoint::Daily {
            for (i, date) in dates(&daily, offset) {
                let values = (
                    at(&daily.temperature_2m_max, i),
                    at(&daily.temperature_2m_min, i),
                    at(&daily.precipitation_sum, i),
                    at(&daily.snowfall_sum, i),
                );
                if date >= ctx.today || values == (None, None, None, None) {
                    continue;
                }
                let day = out.day_mut(date);
                day.temp_max = values.0.map(Celsius);
                day.temp_min = values.1.map(Celsius);
                day.precip_sum = values.2.map(Mm);
                day.snow_sum = values.3.map(|cm| Mm(cm * 10.0));
            }
        } else {
            let values = daily_values(&daily, offset);
            out.set_normals(Normals::from_daily(NORMALS, ProviderId::OpenMeteo, &values));
            out.set_period(NORMALS);
        }
        Ok(())
    }

    /// The archive (ERA5) from the first year's first date to the last year's
    /// last date, reduced per date in core. Thirty years weigh about 235 of the
    /// 10,000 daily calls, so callers keep the answer.
    fn plan_climate(
        &self,
        loc: &Coordinates,
        query: &ClimateQuery,
        ctx: &HistoryContext,
    ) -> Result<Vec<HttpRequest>, ProviderError> {
        // The trip's month and day in the first and last years; 29 February
        // becomes the 28th in a common year.
        let in_year = |year: i32, d: LocalDate| LocalDate {
            year,
            month: d.month,
            day: d.day.min(whether_core::time::days_in_month(year, d.month)),
        };
        let across_new_year = query.to.year > query.from.year;
        let start = in_year(query.years.first, query.from);
        let end = in_year(query.years.last + i32::from(across_new_year), query.to);
        let q = Self::query(
            &self.settings.archive_url,
            "/archive",
            loc,
            start,
            end,
            "temperature_2m_max,temperature_2m_min,precipitation_sum",
        );
        Ok(vec![self.request(q, Endpoint::Climate, true, ctx)])
    }

    fn parse_climate(
        &self,
        _endpoint: Endpoint,
        resp: &HttpResponse<'_>,
        query: &ClimateQuery,
        _ctx: &HistoryContext,
        out: &mut HistoryBuilder,
    ) -> Result<(), ProviderError> {
        let r = self.read(resp.body)?;
        let daily = r
            .daily
            .ok_or_else(|| ProviderError::Malformed("no daily values".into()))?;
        let values = daily_values(&daily, UtcOffset(r.utc_offset_seconds));
        out.set_kind(HistoryKind::Reanalysis);
        out.set_period(query.years);
        for c in climate_from_daily(query, &values) {
            *out.climate_mut(c.date) = c;
        }
        Ok(())
    }
}
