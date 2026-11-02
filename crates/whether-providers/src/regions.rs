//! The region boxes of `whether_core::regions`, for [`Coordinates`].

use whether_core::models::Region;
use whether_core::regions;

use crate::url::Coordinates;

/// Whether `loc` is in the lower 48 states (where NOAA's CONUS models, such as the
/// National Blend of Models, have data).
pub fn in_lower_48(loc: &Coordinates) -> bool {
    regions::in_lower_48(loc.latitude, loc.longitude)
}

/// Whether `loc` is in an area the US National Weather Service covers.
pub fn covers_us(loc: &Coordinates) -> bool {
    regions::covers_us(loc.latitude, loc.longitude)
}

/// Whether a model for `region` may have data at `loc`.
pub fn in_region(region: Region, loc: &Coordinates) -> bool {
    regions::in_region(region, loc.latitude, loc.longitude)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(latitude: f64, longitude: f64) -> Coordinates {
        Coordinates {
            latitude,
            longitude,
        }
    }

    #[test]
    fn regions_hold_the_places_checked_against_open_meteo() {
        let new_york = at(40.7143, -74.0060);
        let oslo = at(59.91, 10.75);
        assert!(in_region(Region::Conus, &new_york));
        assert!(in_region(Region::Conus, &at(25.76, -80.19)), "Miami");
        assert!(!in_region(Region::Conus, &oslo));
        assert!(!in_region(Region::Conus, &at(61.2, -149.9)), "Anchorage");
        // HRDPS had data for these on 2026-08-04.
        for (lat, lon) in [
            (40.7143, -74.0060),
            (41.88, -87.63),
            (64.84, -147.72),
            (47.56, -52.71),
        ] {
            assert!(
                in_region(Region::NorthAmerica, &at(lat, lon)),
                "{lat}, {lon}"
            );
        }
        // …and none for these.
        for (lat, lon) in [
            (25.76, -80.19),
            (29.76, -95.37),
            (19.43, -99.13),
            (59.91, 10.75),
        ] {
            assert!(
                !in_region(Region::NorthAmerica, &at(lat, lon)),
                "{lat}, {lon}"
            );
        }
        assert!(in_region(Region::Global, &oslo));
    }

    #[test]
    fn the_us_includes_its_territories() {
        for (lat, lon) in [
            (61.2, -149.9),
            (21.31, -157.86),
            (18.47, -66.11),
            (13.47, 144.75),
        ] {
            assert!(covers_us(&at(lat, lon)), "{lat}, {lon}");
        }
        assert!(!covers_us(&at(59.91, 10.75)));
    }
}
