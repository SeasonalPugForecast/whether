//! Ensemble members at the place from the Ensemble API.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use whether_core::storm::{ENSEMBLE_MODELS, EnsembleSeries};

use super::{
    ENSEMBLE_URL, HourlyBlock, MAX_FORECAST_DAYS, PlaceResponse, at_place, ensemble_domain,
    from_body, get,
};
use crate::error::ProviderError;
use crate::http::HttpRequest;
use crate::url::Coordinates;

/// The hourly variables of the ensemble request.
pub const ENSEMBLE_HOURLY: &str = "precipitation,snowfall";

/// `models`, or [`ENSEMBLE_MODELS`] when empty.
fn ensemble_list<'a>(models: &[&'a str]) -> Vec<&'a str> {
    if models.is_empty() {
        ENSEMBLE_MODELS.to_vec()
    } else {
        models.to_vec()
    }
}

/// The ensemble request for `models` at `loc` ([`ENSEMBLE_MODELS`] when
/// empty) over `forecast_days` (1 to [`MAX_FORECAST_DAYS`]).
pub fn ensemble_request(loc: &Coordinates, models: &[&str], forecast_days: u8) -> HttpRequest {
    let url = at_place(ENSEMBLE_URL, loc)
        .param("models", ensemble_list(models).join(","))
        .param("hourly", ENSEMBLE_HOURLY)
        .param(
            "forecast_days",
            forecast_days.clamp(1, MAX_FORECAST_DAYS).to_string(),
        )
        .param("timezone", "auto")
        .param("timeformat", "unixtime")
        .param("precipitation_unit", "mm")
        .finish();
    get(url)
}

/// The member a column belongs to, from the part of its key after the
/// variable: `_<domain>` is the control (0) and `_memberNN_<domain>` member
/// NN; without a domain, the same without the `_<domain>` ending.
fn member(rest: &str, domain: Option<&str>) -> Option<u32> {
    let rest = match domain {
        Some(d) => rest.strip_suffix(d)?.strip_suffix('_')?,
        None => rest,
    };
    if rest.is_empty() {
        return Some(0);
    }
    let digits = rest.strip_prefix("_member")?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok().filter(|n| *n > 0)
}

/// `var`'s columns for one model, by member.
fn members(
    hourly: &mut HourlyBlock,
    var: &str,
    domain: Option<&str>,
) -> Result<BTreeMap<u32, Vec<Option<f32>>>, ProviderError> {
    let keys: Vec<(String, u32)> = hourly
        .columns
        .keys()
        .filter_map(|k| {
            let rest = k.strip_prefix(var)?;
            member(rest, domain).map(|m| (k.clone(), m))
        })
        .collect();
    let mut out = BTreeMap::new();
    for (key, m) in keys {
        if let Some(column) = hourly.take_key(&key)? {
            out.insert(m, column);
        }
    }
    Ok(out)
}

/// One model's members, or `None` when the response has none.
fn take_ensemble(
    hourly: &mut HourlyBlock,
    model: &str,
    domain: Option<&str>,
) -> Result<Option<EnsembleSeries>, ProviderError> {
    let mut precipitation = members(hourly, "precipitation", domain)?;
    let mut snowfall = members(hourly, "snowfall", domain)?;
    let ids: Vec<u32> = {
        let mut ids: Vec<u32> = precipitation
            .keys()
            .chain(snowfall.keys())
            .copied()
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    };
    if ids.is_empty() {
        return Ok(None);
    }
    let n = hourly.time.len();
    let column = |map: &mut BTreeMap<u32, Vec<Option<f32>>>, id: u32| {
        map.remove(&id).unwrap_or_else(|| alloc::vec![None; n])
    };
    let mut series = EnsembleSeries {
        model: model.to_string(),
        run: None,
        times: hourly.times(),
        precipitation_mm: Vec::with_capacity(ids.len()),
        snowfall_cm: Vec::with_capacity(ids.len()),
    };
    for id in ids {
        series.precipitation_mm.push(column(&mut precipitation, id));
        series.snowfall_cm.push(column(&mut snowfall, id));
    }
    Ok(Some(series))
}

/// Parses an ensemble response to a request for `models` (as passed to
/// [`ensemble_request`]) into one series per model that came back, in
/// request order, members in order with the control first. Member counts
/// come from the response.
pub fn parse_ensembles(body: &[u8], models: &[&str]) -> Result<Vec<EnsembleSeries>, ProviderError> {
    let response: PlaceResponse = from_body(body)?;
    let mut hourly = response.hourly()?;
    let models = ensemble_list(models);
    let single = models.len() == 1;
    let mut out = Vec::new();
    for model in models {
        let domain = ensemble_domain(model).unwrap_or(model);
        let mut series = take_ensemble(&mut hourly, model, Some(domain))?;
        if series.is_none() && single {
            series = take_ensemble(&mut hourly, model, None)?;
        }
        out.extend(series);
    }
    if out.is_empty() {
        return Err(ProviderError::Malformed(
            "no requested ensemble in the response".to_string(),
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use whether_core::time::UnixTime;

    const NEW_YORK: Coordinates = Coordinates {
        latitude: 40.7143,
        longitude: -74.005_97,
    };

    #[test]
    fn urls_carry_the_models_and_days() {
        assert_eq!(
            ensemble_request(&NEW_YORK, &[], 10).url,
            "https://ensemble-api.open-meteo.com/v1/ensemble?latitude=40.71&longitude=-74.01\
             &models=ecmwf_ifs025,gfs025&hourly=precipitation,snowfall&forecast_days=10\
             &timezone=auto&timeformat=unixtime&precipitation_unit=mm"
        );
        assert!(
            ensemble_request(&NEW_YORK, &["gfs025"], 0)
                .url
                .contains("&models=gfs025&hourly=precipitation,snowfall&forecast_days=1&")
        );
        assert!(
            ensemble_request(&NEW_YORK, &[], 40)
                .url
                .contains("&forecast_days=16&")
        );
    }

    #[test]
    fn keys_name_their_member() {
        let d = Some("ncep_gefs025");
        assert_eq!(member("_ncep_gefs025", d), Some(0));
        assert_eq!(member("_member07_ncep_gefs025", d), Some(7));
        assert_eq!(member("_member30_ncep_gefs025", d), Some(30));
        assert_eq!(member("_member07_ecmwf_ifs025_ensemble", d), None);
        assert_eq!(member("_member_ncep_gefs025", d), None);
        assert_eq!(member("_member00_ncep_gefs025", d), None);
        assert_eq!(member("_probability_ncep_gefs025", d), None);
        assert_eq!(member("", None), Some(0));
        assert_eq!(member("_member12", None), Some(12));
        assert_eq!(member("_member1x", None), None);
        assert_eq!(member("_probability", None), None);
    }

    #[test]
    fn members_are_grouped_by_model_in_order() {
        // The key shape of a recorded two-model response, cut to two hours
        // and a few members.
        let body = br#"{"latitude":40.75,"longitude":-74.0,"utc_offset_seconds":-18000,
            "hourly":{"time":[0,3600],
            "precipitation_ecmwf_ifs025_ensemble":[0.0,0.1],
            "precipitation_member02_ecmwf_ifs025_ensemble":[0.2,0.2],
            "precipitation_member10_ecmwf_ifs025_ensemble":[1.0,1.0],
            "precipitation_member01_ecmwf_ifs025_ensemble":[0.1,null],
            "snowfall_ecmwf_ifs025_ensemble":[0.0,0.0],
            "snowfall_member01_ecmwf_ifs025_ensemble":[0.0,0.0],
            "precipitation_ncep_gefs025":[0.5,0.5],
            "precipitation_member01_ncep_gefs025":[0.6,0.6],
            "snowfall_ncep_gefs025":[0.0,0.0]}}"#;
        let e = parse_ensembles(body, &[]).unwrap();
        assert_eq!(e.len(), 2);
        let ecmwf = &e[0];
        assert_eq!(ecmwf.model, "ecmwf_ifs025");
        assert_eq!(ecmwf.times, [UnixTime(0), UnixTime(3_600)]);
        assert_eq!(ecmwf.member_count(), 4);
        assert_eq!(ecmwf.precipitation_mm[0], [Some(0.0), Some(0.1)]);
        assert_eq!(ecmwf.precipitation_mm[1], [Some(0.1), None]);
        assert_eq!(ecmwf.precipitation_mm[2], [Some(0.2), Some(0.2)]);
        assert_eq!(ecmwf.precipitation_mm[3], [Some(1.0), Some(1.0)]);
        assert_eq!(ecmwf.snowfall_cm.len(), 4);
        assert_eq!(ecmwf.snowfall_cm[2], [None, None]);
        let gefs = &e[1];
        assert_eq!(gefs.model, "gfs025");
        assert_eq!(gefs.member_count(), 2);
        assert_eq!(gefs.snowfall_cm[1], [None, None]);
    }

    #[test]
    fn one_ensemble_may_come_back_unsuffixed() {
        // Shape of a recorded response to `models=gfs025` alone.
        let body = br#"{"latitude":40.75,"longitude":-74.0,"utc_offset_seconds":0,
            "hourly":{"time":[0],"precipitation":[0.1],"precipitation_member01":[0.2],
            "snowfall":[0.0],"snowfall_member01":[0.0]}}"#;
        let e = parse_ensembles(body, &["gfs025"]).unwrap();
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].member_count(), 2);
        assert_eq!(e[0].precipitation_mm[1], [Some(0.2)]);
        assert!(matches!(
            parse_ensembles(body, &[]),
            Err(ProviderError::Malformed(_))
        ));
    }

    #[test]
    fn error_bodies_are_rejections() {
        assert_eq!(
            parse_ensembles(
                br#"{"error":true,"reason":"Invalid value: Cannot initialize MultiDomains from invalid String value bogus"}"#,
                &["bogus"]
            ),
            Err(ProviderError::Rejected(
                "Invalid value: Cannot initialize MultiDomains from invalid String value bogus"
                    .to_string()
            ))
        );
    }
}
