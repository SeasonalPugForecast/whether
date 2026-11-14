//! Sea-level pressure on a grid around the place, for following lows.
//!
//! One forecast request carries every point of
//! [`whether_core::storm::grid_axes`] as comma-separated `latitude` and
//! `longitude` lists, row by row. The response is a JSON array with one
//! object per point in request order; each object echoes the model grid
//! point it snapped to, so the grids keep the requested axes.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use whether_core::storm::{PressureGrid, grid_axes};

use super::{
    COORDINATE_DECIMALS, FORECAST_DAYS, FORECAST_URL, HourlyBlock, PlaceResponse, from_body, get,
    model_list,
};
use crate::error::ProviderError;
use crate::http::HttpRequest;
use crate::url::{Coordinates, QueryBuilder};

/// The grid's row latitudes and column longitudes around `loc`, after the
/// same rounding the point request applies.
pub fn axes(loc: &Coordinates) -> (Vec<f32>, Vec<f32>) {
    let r = loc.rounded(COORDINATE_DECIMALS);
    grid_axes(r.latitude, r.longitude)
}

/// The grid request for `models` around `loc` (the first four;
/// [`whether_core::storm::DETERMINISTIC_MODELS`] when empty): `pressure_msl`
/// every 6 hours over [`FORECAST_DAYS`].
pub fn grid_request(loc: &Coordinates, models: &[&str]) -> HttpRequest {
    let (lats, lons) = axes(loc);
    let mut lat_list: Vec<String> = Vec::with_capacity(lats.len() * lons.len());
    let mut lon_list: Vec<String> = Vec::with_capacity(lats.len() * lons.len());
    for lat in &lats {
        for lon in &lons {
            let (a, o) = Coordinates {
                latitude: f64::from(*lat),
                longitude: f64::from(*lon),
            }
            .fmt_param(COORDINATE_DECIMALS);
            lat_list.push(a);
            lon_list.push(o);
        }
    }
    let url = QueryBuilder::new(FORECAST_URL)
        .param("latitude", lat_list.join(","))
        .param("longitude", lon_list.join(","))
        .param("models", model_list(models).join(","))
        .param("hourly", "pressure_msl")
        .param("forecast_days", FORECAST_DAYS.to_string())
        .param("temporal_resolution", "hourly_6")
        .param("timeformat", "unixtime")
        .finish();
    get(url)
}

/// Parses a response to [`grid_request`] for `loc` and `models` into one grid
/// per model that came back, in request order. A point where a model has no
/// data holds `None`s.
pub fn parse_grids(
    body: &[u8],
    loc: &Coordinates,
    models: &[&str],
) -> Result<Vec<PressureGrid>, ProviderError> {
    let points: Vec<PlaceResponse> = from_body(body)?;
    let (lats, lons) = axes(loc);
    let expected = lats.len() * lons.len();
    if points.len() != expected {
        return Err(ProviderError::Malformed(alloc::format!(
            "{} grid points for {expected} requested",
            points.len()
        )));
    }
    let mut blocks = points
        .into_iter()
        .map(PlaceResponse::hourly)
        .collect::<Result<Vec<HourlyBlock>, _>>()?;
    let times = blocks.first().map(HourlyBlock::times).unwrap_or_default();
    if blocks
        .iter()
        .any(|b| b.time.len() != times.len() || b.time.iter().zip(&times).any(|(a, t)| *a != t.0))
    {
        return Err(ProviderError::Malformed(
            "grid points have different hours".to_string(),
        ));
    }
    let models = model_list(models);
    let single = models.len() == 1;
    let mut grids = Vec::new();
    for model in models {
        let mut columns = Vec::with_capacity(expected);
        for block in &mut blocks {
            columns.push(block.take("pressure_msl", model, single)?);
        }
        if columns.iter().all(Option::is_none) {
            continue;
        }
        let pressure_hpa = (0..times.len())
            .map(|t| {
                columns
                    .iter()
                    .map(|c| c.as_ref().and_then(|c| c.get(t).copied().flatten()))
                    .collect()
            })
            .collect();
        grids.push(PressureGrid {
            model: model.to_string(),
            run: None,
            lats: lats.clone(),
            lons: lons.clone(),
            times: times.clone(),
            pressure_hpa,
        });
    }
    if grids.is_empty() {
        return Err(ProviderError::Malformed(
            "no requested model in the grid".to_string(),
        ));
    }
    Ok(grids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;
    use whether_core::time::UnixTime;

    const NEW_YORK: Coordinates = Coordinates {
        latitude: 40.7143,
        longitude: -74.005_97,
    };

    #[test]
    fn urls_list_every_point_row_by_row() {
        let url = grid_request(&NEW_YORK, &[]).url;
        let query = url.split_once('?').map(|(_, q)| q).unwrap_or_default();
        let params: Vec<(&str, &str)> =
            query.split('&').filter_map(|p| p.split_once('=')).collect();
        let names: Vec<&str> = params.iter().map(|(k, _)| *k).collect();
        assert_eq!(
            names,
            [
                "latitude",
                "longitude",
                "models",
                "hourly",
                "forecast_days",
                "temporal_resolution",
                "timeformat"
            ]
        );
        let lats: Vec<&str> = params[0].1.split(',').collect();
        let lons: Vec<&str> = params[1].1.split(',').collect();
        assert_eq!((lats.len(), lons.len()), (49, 49));
        assert_eq!(
            &lats[..8],
            ["33.25"; 7]
                .iter()
                .chain(&["35.75"])
                .copied()
                .collect::<Vec<_>>()
        );
        assert_eq!(
            &lons[..7],
            ["-81.5", "-79", "-76.5", "-74", "-71.5", "-69", "-66.5"]
        );
        assert_eq!(lats[48], "48.25");
        assert_eq!(lons[48], "-66.5");
        assert_eq!(
            &params[2..],
            [
                (
                    "models",
                    "ecmwf_ifs025,gfs_seamless,gem_seamless,icon_seamless"
                ),
                ("hourly", "pressure_msl"),
                ("forecast_days", "10"),
                ("temporal_resolution", "hourly_6"),
                ("timeformat", "unixtime")
            ]
        );
    }

    /// A grid response in the recorded shape for a 7×7 grid: two models, two
    /// steps, and model `b` with no data at point 3.
    fn body(points: usize) -> String {
        let objects: Vec<String> = (0..points)
            .map(|i| {
                let b = if i == 3 { "null,null".to_string() } else { format!("{i}.5,{i}.25") };
                format!(
                    r#"{{"latitude":0,"longitude":0,"utc_offset_seconds":0,"hourly":{{"time":[0,21600],"pressure_msl_a":[{i},{i}],"pressure_msl_b":[{b}]}}}}"#
                )
            })
            .collect();
        format!("[{}]", objects.join(","))
    }

    #[test]
    fn points_fill_each_models_grid() {
        let grids = parse_grids(body(49).as_bytes(), &NEW_YORK, &["a", "b", "c"]).unwrap();
        assert_eq!(grids.len(), 2, "c came back nowhere");
        let (lats, lons) = axes(&NEW_YORK);
        let a = &grids[0];
        assert_eq!((a.lats.clone(), a.lons.clone()), (lats, lons));
        assert_eq!(a.times, [UnixTime(0), UnixTime(21_600)]);
        assert_eq!(a.pressure_hpa.len(), 2);
        assert_eq!(a.pressure_hpa[1].len(), 49);
        assert_eq!(a.value(1, 1, 2), Some(9.0));
        let b = &grids[1];
        assert_eq!(b.value(0, 0, 3), None);
        assert_eq!(b.value(1, 0, 4), Some(4.25));
    }

    #[test]
    fn short_or_mixed_grids_are_errors() {
        assert!(matches!(
            parse_grids(body(48).as_bytes(), &NEW_YORK, &["a"]),
            Err(ProviderError::Malformed(_))
        ));
        let mixed = body(49).replacen("[0,21600]", "[0,3600]", 1);
        assert!(matches!(
            parse_grids(mixed.as_bytes(), &NEW_YORK, &["a"]),
            Err(ProviderError::Malformed(_))
        ));
        // The reason Open-Meteo gives for lists of different lengths.
        let reason = "Parameter 'latitude' and 'longitude' must have the same number of elements";
        let body = format!(r#"{{"error":true,"reason":"{reason}"}}"#);
        assert_eq!(
            parse_grids(body.as_bytes(), &NEW_YORK, &[]),
            Err(ProviderError::Rejected(reason.to_string()))
        );
    }
}
