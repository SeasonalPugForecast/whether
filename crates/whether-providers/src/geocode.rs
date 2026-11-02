//! Place search shared by providers: names and postal codes to coordinates.

use alloc::string::String;
use alloc::vec::Vec;

use whether_core::model::ProviderId;

/// One place found by a geocoding search.
#[derive(Clone, Debug, PartialEq)]
pub struct GeocodeResult {
    /// Place name.
    pub name: String,
    /// Latitude.
    pub latitude: f64,
    /// Longitude.
    pub longitude: f64,
    /// ISO 3166-1 alpha-2 country code.
    pub country_code: Option<String>,
    /// First-level region (state, province).
    pub admin1: Option<String>,
    /// The postal code searched for, when known.
    pub postcode: Option<String>,
    /// IANA time zone.
    pub timezone: Option<String>,
    /// Elevation in metres.
    pub elevation: Option<f32>,
    /// Population.
    pub population: Option<u64>,
    /// Which service answered.
    pub source: ProviderId,
}

impl GeocodeResult {
    /// "New York, New York, US".
    pub fn display_name(&self) -> String {
        let mut parts: Vec<&str> = Vec::with_capacity(3);
        parts.push(&self.name);
        parts.extend(self.admin1.as_deref());
        parts.extend(self.country_code.as_deref());
        parts.join(", ")
    }
}

/// What to search for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeocodeQuery<'a> {
    /// A place name, optionally limited to a country.
    Name {
        /// The name, e.g. "New York" or "Tromsø, Norway".
        text: &'a str,
        /// ISO 3166-1 alpha-2 country code.
        country: Option<&'a str>,
    },
    /// A postal code in a country.
    Zip {
        /// The postal code, e.g. "10001".
        zip: &'a str,
        /// ISO 3166-1 alpha-2 country code, e.g. "US".
        country: &'a str,
    },
}

impl GeocodeQuery<'_> {
    /// Whether the query is long enough to search (at least two characters).
    pub fn is_searchable(&self) -> bool {
        let text = match self {
            Self::Name { text, .. } => text,
            Self::Zip { zip, .. } => zip,
        };
        text.trim().chars().count() >= 2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn display_names() {
        let mut r = GeocodeResult {
            name: "New York".to_string(),
            latitude: 40.7,
            longitude: -74.0,
            country_code: Some("US".to_string()),
            admin1: Some("New York".to_string()),
            postcode: None,
            timezone: None,
            elevation: None,
            population: None,
            source: ProviderId::OpenMeteo,
        };
        assert_eq!(r.display_name(), "New York, New York, US");
        r.admin1 = None;
        assert_eq!(r.display_name(), "New York, US");
    }

    #[test]
    fn short_queries_are_not_searchable() {
        assert!(
            !GeocodeQuery::Name {
                text: " a ",
                country: None
            }
            .is_searchable()
        );
        assert!(
            GeocodeQuery::Name {
                text: "Oslo",
                country: None
            }
            .is_searchable()
        );
        assert!(
            GeocodeQuery::Zip {
                zip: "10001",
                country: "US"
            }
            .is_searchable()
        );
    }
}
