//! Rough boxes around the areas regional services and weather models cover
//! (south, west, north, east, in degrees). Providers skip requests that would
//! only fail; validation refuses regional providers for places outside them.

use crate::models::Region;

type Box = (f64, f64, f64, f64);

/// The lower 48 states.
const LOWER_48: Box = (24.0, -125.0, 49.5, -66.5);

/// The areas the US National Weather Service forecasts: the lower 48, Alaska
/// (both sides of 180°), Hawaii, Puerto Rico and the Virgin Islands, Guam and the
/// Marianas, American Samoa. A place near a border may still be refused.
const US: [Box; 8] = [
    LOWER_48,
    (51.0, -180.0, 72.0, -129.0),
    (51.0, 172.0, 54.0, 180.0),
    (18.5, -161.0, 22.5, -154.5),
    (17.5, -68.0, 18.7, -64.4),
    (13.0, 144.5, 15.5, 146.0),
    (-14.6, -171.2, -14.0, -168.0),
    // The Florida Keys.
    (24.4, -82.0, 25.0, -80.0),
];

/// Around Canada's HRDPS continental grid. The grid is rotated, so places inside
/// this box may still have no data (Nashville, Denver); lists of regional models
/// therefore end with a global one.
const NORTH_AMERICA: Box = (36.0, -152.0, 80.0, -42.0);

fn inside((s, w, n, e): Box, latitude: f64, longitude: f64) -> bool {
    (s..=n).contains(&latitude) && (w..=e).contains(&longitude)
}

/// Whether a place is in the lower 48 states (where NOAA's CONUS models, such
/// as the National Blend of Models, have data).
pub fn in_lower_48(latitude: f64, longitude: f64) -> bool {
    inside(LOWER_48, latitude, longitude)
}

/// Whether a place is in an area the US National Weather Service covers.
pub fn covers_us(latitude: f64, longitude: f64) -> bool {
    US.iter().any(|b| inside(*b, latitude, longitude))
}

/// Whether a model for `region` may have data at a place.
pub fn in_region(region: Region, latitude: f64, longitude: f64) -> bool {
    match region {
        Region::Global => true,
        Region::Conus => in_lower_48(latitude, longitude),
        Region::NorthAmerica => inside(NORTH_AMERICA, latitude, longitude),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_york_is_in_every_us_area_and_oslo_in_none() {
        assert!(in_lower_48(40.7143, -74.0060) && covers_us(40.7143, -74.0060));
        assert!(in_region(Region::NorthAmerica, 40.7143, -74.0060));
        assert!(!covers_us(59.91, 10.75) && !in_region(Region::Conus, 59.91, 10.75));
    }
}
