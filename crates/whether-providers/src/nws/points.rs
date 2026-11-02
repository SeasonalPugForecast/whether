//! `/points/{lat},{lon}`: the forecast office's grid cell for a place, its time
//! zone and nearest town, and where its stations are listed.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use serde::Deserialize;
use whether_core::builder::SnapshotBuilder;
use whether_core::model::ResolvedLocation;

use crate::error::ProviderError;
use crate::http::FetchContext;

#[derive(Deserialize)]
struct Point {
    geometry: Option<Geometry>,
    properties: Properties,
}

#[derive(Deserialize)]
struct Geometry {
    /// `[longitude, latitude]`.
    coordinates: Vec<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Properties {
    grid_id: String,
    grid_x: u32,
    grid_y: u32,
    time_zone: Option<String>,
    relative_location: Option<Relative>,
}

#[derive(Deserialize)]
struct Relative {
    properties: Town,
}

#[derive(Deserialize)]
struct Town {
    city: Option<String>,
    state: Option<String>,
}

/// A forecast office's grid cell: `OKX/33,42`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Grid {
    /// The office (`OKX`).
    pub(super) office: String,
    /// Column.
    pub(super) x: u32,
    /// Row.
    pub(super) y: u32,
}

impl Grid {
    /// `OKX/33,42`, as the gridpoint paths write it.
    pub(super) fn path(&self) -> String {
        alloc::format!("{}/{},{}", self.office, self.x, self.y)
    }
}

fn read(body: &[u8]) -> Result<Point, ProviderError> {
    serde_json::from_slice(body).map_err(|e| ProviderError::Malformed(e.to_string()))
}

/// The grid cell named in a `/points` response.
pub(super) fn grid(body: &[u8]) -> Result<Grid, ProviderError> {
    let office = read(body)?.properties;
    // Office ids are letters; anything else must not reach a URL path.
    if office.grid_id.is_empty() || !office.grid_id.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(ProviderError::Malformed(alloc::format!(
            "unexpected forecast office `{}`",
            office.grid_id
        )));
    }
    Ok(Grid {
        office: office.grid_id,
        x: office.grid_x,
        y: office.grid_y,
    })
}

/// Sets the snapshot's location from a `/points` response: the point, its zone,
/// and the nearest town as the name. The offset is the caller's (the response
/// gives only the zone's name).
pub(super) fn parse(
    body: &[u8],
    ctx: &FetchContext,
    out: &mut SnapshotBuilder,
) -> Result<(), ProviderError> {
    let point = read(body)?;
    let (Some(&longitude), Some(&latitude)) = point.geometry.as_ref().map_or((None, None), |g| {
        (g.coordinates.first(), g.coordinates.get(1))
    }) else {
        return Err(ProviderError::Malformed(
            "the point has no coordinates".into(),
        ));
    };
    let town = point.properties.relative_location.map(|r| r.properties);
    out.set_location(ResolvedLocation {
        name: town
            .as_ref()
            .and_then(|t| t.city.clone())
            .unwrap_or_default(),
        region: town.and_then(|t| t.state),
        latitude,
        longitude,
        elevation_m: None,
        utc_offset: ctx.utc_offset_hint.unwrap_or_default(),
        tz_name: point.properties.time_zone,
        tz_abbrev: None,
        station: None,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn office_ids_stay_out_of_paths_unless_plain() {
        let body = |id: &str| {
            alloc::format!(r#"{{"properties":{{"gridId":"{id}","gridX":33,"gridY":42}}}}"#)
        };
        assert_eq!(grid(body("OKX").as_bytes()).unwrap().path(), "OKX/33,42");
        assert!(grid(body("../x").as_bytes()).is_err());
        assert!(grid(body("").as_bytes()).is_err());
    }
}
