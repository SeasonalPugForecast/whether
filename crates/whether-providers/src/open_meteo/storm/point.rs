//! Several deterministic models' hours at the place, and one earlier run of
//! one model from the Single Runs API.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use whether_core::model::ResolvedLocation;
use whether_core::storm::{ModelRun, PointSeries};
use whether_core::time::{UnixTime, UtcOffset};

use super::{
    FORECAST_DAYS, FORECAST_URL, HourlyBlock, MAX_FORECAST_DAYS, SINGLE_RUNS_URL, at_place,
    from_body, get, model_list,
};
use crate::error::ProviderError;
use crate::http::HttpRequest;
use crate::url::{Coordinates, QueryBuilder};

/// The hourly variables of the point and single-run requests.
pub const POINT_HOURLY: &str = "precipitation,snowfall,wind_gusts_10m,pressure_msl";

const SECS_PER_DAY: i64 = 86_400;

/// The models' hours at a place, as one point request returns them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PointForecast {
    /// The place as Open-Meteo resolved it: its grid point, elevation, UTC
    /// offset and time zone. The name is empty.
    pub location: ResolvedLocation,
    /// One series per requested model that came back, in request order.
    pub series: Vec<PointSeries>,
}

/// Adds the variables, units and time format the point parser expects.
fn point_query(q: QueryBuilder, days: u8) -> QueryBuilder {
    q.param("hourly", POINT_HOURLY)
        .param("forecast_days", days.to_string())
        .param("timezone", "auto")
        .param("timeformat", "unixtime")
        .param("wind_speed_unit", "kmh")
        .param("precipitation_unit", "mm")
}

/// The point request for `models` at `loc` (the first four;
/// [`whether_core::storm::DETERMINISTIC_MODELS`] when empty), over
/// [`FORECAST_DAYS`].
pub fn point_request(loc: &Coordinates, models: &[&str]) -> HttpRequest {
    let q = at_place(FORECAST_URL, loc).param("models", model_list(models).join(","));
    get(point_query(q, FORECAST_DAYS).finish())
}

/// Parses a point response to a request for `models` (as passed to
/// [`point_request`]). A model Open-Meteo left out is missing from the
/// result; one that came back with gaps has `None`s.
pub fn parse_points(body: &[u8], models: &[&str]) -> Result<PointForecast, ProviderError> {
    let response: super::PlaceResponse = from_body(body)?;
    let location = response.location();
    let mut hourly = response.hourly()?;
    let models = model_list(models);
    let single = models.len() == 1;
    let mut series = Vec::new();
    for model in models {
        if let Some(s) = take_series(&mut hourly, model, single)? {
            series.push(s);
        }
    }
    if series.is_empty() {
        return Err(ProviderError::Malformed(
            "no requested model in the response".to_string(),
        ));
    }
    Ok(PointForecast { location, series })
}

/// `model`'s series from `hourly`, or `None` when it has none of the
/// variables. With `single`, unsuffixed columns are the model's.
fn take_series(
    hourly: &mut HourlyBlock,
    model: &str,
    single: bool,
) -> Result<Option<PointSeries>, ProviderError> {
    let mut found = false;
    let mut column = |var: &str| -> Result<Vec<Option<f32>>, ProviderError> {
        match hourly.take(var, model, single)? {
            Some(c) => {
                found = true;
                Ok(c)
            }
            None => Ok(alloc::vec![None; hourly.time.len()]),
        }
    };
    let precipitation_mm = column("precipitation")?;
    let snowfall_cm = column("snowfall")?;
    let gust_kmh = column("wind_gusts_10m")?;
    let pressure_hpa = column("pressure_msl")?;
    Ok(found.then(|| PointSeries {
        model: model.to_string(),
        run: None,
        times: hourly.times(),
        precipitation_mm,
        snowfall_cm,
        gust_kmh,
        pressure_hpa,
    }))
}

/// `t` as the Single Runs API's `run` value, `YYYY-MM-DDTHH:MM` in UTC.
fn run_param(t: UnixTime) -> String {
    let d = t.to_local(UtcOffset(0));
    alloc::format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}",
        d.date.year,
        d.date.month,
        d.date.day,
        d.hour,
        d.minute
    )
}

/// The request for `model`'s run initialised at `run` (UTC), with the point
/// request's variables, covering `window_start..window_end`.
///
/// A run's hours start at its initialisation and last whole days, so the
/// request asks for the fewest days that reach `window_end`. A window that
/// starts before the run, is empty, or ends more than [`MAX_FORECAST_DAYS`]
/// after it is a [`ProviderError::Config`].
pub fn single_run_request(
    loc: &Coordinates,
    model: &str,
    run: UnixTime,
    window_start: UnixTime,
    window_end: UnixTime,
) -> Result<HttpRequest, ProviderError> {
    if window_end <= window_start {
        return Err(ProviderError::Config(
            "the event window is empty".to_string(),
        ));
    }
    if window_start < run {
        return Err(ProviderError::Config(
            "the event window starts before the run".to_string(),
        ));
    }
    let span = window_end.0 - run.0;
    let days = (span + SECS_PER_DAY - 1) / SECS_PER_DAY;
    let days = u8::try_from(days)
        .ok()
        .filter(|d| *d <= MAX_FORECAST_DAYS)
        .ok_or_else(|| {
            ProviderError::Config("the event window ends after the run's last day".to_string())
        })?;
    let q = at_place(SINGLE_RUNS_URL, loc)
        .param("models", model)
        .param("run", run_param(run));
    Ok(get(point_query(q, days.max(1)).finish()))
}

/// Parses a response to [`single_run_request`] for `model` into its series,
/// tagged with `run`.
pub fn parse_single_run(
    body: &[u8],
    model: &str,
    run: ModelRun,
) -> Result<PointSeries, ProviderError> {
    let mut forecast = parse_points(body, &[model])?;
    // INVARIANT: `parse_points` returns at least one series or an error.
    let mut series = forecast.series.swap_remove(0);
    series.run = Some(run);
    Ok(series)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NEW_YORK: Coordinates = Coordinates {
        latitude: 40.7143,
        longitude: -74.005_97,
    };

    #[test]
    fn point_urls_carry_the_models_and_units() {
        let url = point_request(&NEW_YORK, &[]).url;
        assert_eq!(
            url,
            "https://api.open-meteo.com/v1/forecast?latitude=40.71&longitude=-74.01\
             &models=ecmwf_ifs025,gfs_seamless,gem_seamless,icon_seamless\
             &hourly=precipitation,snowfall,wind_gusts_10m,pressure_msl&forecast_days=10\
             &timezone=auto&timeformat=unixtime&wind_speed_unit=kmh&precipitation_unit=mm"
        );
        let one = point_request(&NEW_YORK, &["gfs_seamless"]).url;
        assert!(one.contains("&models=gfs_seamless&"));
        let many = point_request(&NEW_YORK, &["a", "b", "c", "d", "e"]).url;
        assert!(many.contains("&models=a,b,c,d&"));
    }

    #[test]
    fn suffixed_columns_split_by_model() {
        let body = br#"{"latitude":40.75,"longitude":-74.0,"elevation":27.0,
            "utc_offset_seconds":-18000,"timezone":"America/New_York",
            "timezone_abbreviation":"GMT-5","hourly":{"time":[0,3600],
            "precipitation_a":[0.5,null],"snowfall_a":[0.0,0.0],
            "wind_gusts_10m_a":[30.0,31.0],"pressure_msl_a":[1010.0,1009.5],
            "precipitation_b":[1.0,2.0]}}"#;
        let f = parse_points(body, &["a", "b", "c"]).unwrap();
        assert_eq!(f.location.utc_offset, UtcOffset(-18_000));
        assert_eq!(f.location.tz_name.as_deref(), Some("America/New_York"));
        assert_eq!(f.location.latitude, 40.75);
        assert_eq!(f.series.len(), 2, "c came back with nothing");
        let a = &f.series[0];
        assert_eq!(a.model, "a");
        assert_eq!(a.times, [UnixTime(0), UnixTime(3_600)]);
        assert_eq!(a.precipitation_mm, [Some(0.5), None]);
        assert_eq!(a.pressure_hpa, [Some(1010.0), Some(1009.5)]);
        let b = &f.series[1];
        assert_eq!(b.precipitation_mm, [Some(1.0), Some(2.0)]);
        assert_eq!(b.gust_kmh, [None, None]);
    }

    #[test]
    fn one_model_may_come_back_unsuffixed() {
        let body = br#"{"latitude":40.75,"longitude":-74.0,"utc_offset_seconds":0,
            "hourly":{"time":[0],"precipitation":[0.2],"pressure_msl":[1000.0]}}"#;
        let f = parse_points(body, &["gem_seamless"]).unwrap();
        assert_eq!(f.series[0].model, "gem_seamless");
        assert_eq!(f.series[0].precipitation_mm, [Some(0.2)]);
        // With several models an unsuffixed column belongs to none of them.
        assert!(matches!(
            parse_points(body, &["a", "b"]),
            Err(ProviderError::Malformed(_))
        ));
    }

    #[test]
    fn broken_hours_are_errors() {
        let short = br#"{"latitude":0,"longitude":0,"utc_offset_seconds":0,
            "hourly":{"time":[0,3600],"precipitation_a":[1.0]}}"#;
        assert!(matches!(
            parse_points(short, &["a"]),
            Err(ProviderError::Malformed(_))
        ));
        let backwards = br#"{"latitude":0,"longitude":0,"utc_offset_seconds":0,
            "hourly":{"time":[3600,0],"precipitation_a":[1.0,1.0]}}"#;
        assert!(matches!(
            parse_points(backwards, &["a"]),
            Err(ProviderError::Malformed(_))
        ));
        let no_hours = br#"{"latitude":0,"longitude":0,"utc_offset_seconds":0}"#;
        assert!(matches!(
            parse_points(no_hours, &["a"]),
            Err(ProviderError::Malformed(_))
        ));
        assert!(matches!(
            parse_points(b"", &["a"]),
            Err(ProviderError::Malformed(_))
        ));
    }

    /// 2026-11-13 00:00 UTC.
    const RUN: UnixTime = UnixTime(1_794_528_000);

    #[test]
    fn single_runs_cover_the_window() {
        let start = UnixTime(RUN.0 + 30 * 3_600);
        let end = UnixTime(RUN.0 + 54 * 3_600);
        let url = single_run_request(&NEW_YORK, "ecmwf_ifs025", RUN, start, end)
            .unwrap()
            .url;
        assert_eq!(
            url,
            "https://single-runs-api.open-meteo.com/v1/forecast?latitude=40.71&longitude=-74.01\
             &models=ecmwf_ifs025&run=2026-11-13T00%3A00\
             &hourly=precipitation,snowfall,wind_gusts_10m,pressure_msl&forecast_days=3\
             &timezone=auto&timeformat=unixtime&wind_speed_unit=kmh&precipitation_unit=mm"
        );
        let exact = single_run_request(&NEW_YORK, "x", RUN, RUN, UnixTime(RUN.0 + 86_400))
            .unwrap()
            .url;
        assert!(exact.contains("&forecast_days=1&"));
        let twelve = UnixTime(RUN.0 + 12 * 3_600);
        let later = single_run_request(&NEW_YORK, "x", twelve, end, UnixTime(end.0 + 1))
            .unwrap()
            .url;
        assert!(later.contains("&run=2026-11-13T12%3A00&"));
    }

    #[test]
    fn windows_the_run_cannot_cover_are_refused() {
        let req = |start: i64, end: i64| {
            single_run_request(
                &NEW_YORK,
                "ecmwf_ifs025",
                RUN,
                UnixTime(RUN.0 + start),
                UnixTime(RUN.0 + end),
            )
        };
        assert!(matches!(req(-3_600, 3_600), Err(ProviderError::Config(_))));
        assert!(matches!(req(3_600, 3_600), Err(ProviderError::Config(_))));
        assert!(matches!(
            req(0, 16 * 86_400 + 1),
            Err(ProviderError::Config(_))
        ));
        assert!(req(0, 16 * 86_400).is_ok());
    }

    #[test]
    fn single_runs_carry_their_run() {
        let body = br#"{"latitude":40.75,"longitude":-74.0,"utc_offset_seconds":-18000,
            "hourly":{"time":[1794528000],"precipitation":[0.1],"snowfall":[0.0],
            "wind_gusts_10m":[null],"pressure_msl":[1012.0]}}"#;
        let run = ModelRun {
            model: "ecmwf_ifs025".to_string(),
            init: RUN,
            available: UnixTime(RUN.0 + 7 * 3_600),
        };
        let s = parse_single_run(body, "ecmwf_ifs025", run.clone()).unwrap();
        assert_eq!(s.run, Some(run.clone()));
        assert_eq!(s.gust_kmh, [None]);
        // The reason Open-Meteo gives for an unknown model.
        let reason =
            "Invalid value: Cannot initialize MultiDomains from invalid String value bogus";
        let body = alloc::format!(r#"{{"error":true,"reason":"{reason}"}}"#);
        assert_eq!(
            parse_single_run(body.as_bytes(), "bogus", run),
            Err(ProviderError::Rejected(reason.to_string()))
        );
    }
}
