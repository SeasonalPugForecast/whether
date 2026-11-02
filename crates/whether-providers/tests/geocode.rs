//! Geocoding by name and ZIP against recorded fixtures, plus synthetic
//! OpenWeather ones for hand-picked values.

use whether_providers::{GeocodeQuery, HttpResponse, redact_url};
use whether_providers::{open_meteo, openweather};

const ZIP: GeocodeQuery<'static> = GeocodeQuery::Zip {
    zip: "10001",
    country: "US",
};
const NAME: GeocodeQuery<'static> = GeocodeQuery::Name {
    text: "New York",
    country: Some("US"),
};

fn near_10001(lat: f64, lon: f64) -> bool {
    (lat - 40.7143).abs() < 0.05 && (lon + 74.005_97).abs() < 0.05
}

#[test]
fn open_meteo_requests() {
    let base = open_meteo::geocode::DEFAULT_GEOCODING_URL;
    let zip = open_meteo::geocode::geocode_request(base, &ZIP, 5, "en");
    assert_eq!(
        zip.url,
        "https://geocoding-api.open-meteo.com/v1/search?name=10001&count=5&language=en&format=json&countryCode=US"
    );
    let name = open_meteo::geocode::geocode_request(
        base,
        &GeocodeQuery::Name {
            text: "Tromsø",
            country: None,
        },
        3,
        "en",
    );
    assert!(name.url.contains("name=Troms%C3%B8") && !name.url.contains("countryCode"));
}

#[test]
fn open_meteo_zip_10001() {
    let body = include_bytes!("fixtures/open_meteo/geocode_10001_us.json");
    let results = open_meteo::geocode::parse_geocode(&ZIP, &HttpResponse::new(200, body)).unwrap();
    assert_eq!(results.len(), 1);
    let r = &results[0];
    assert_eq!(r.display_name(), "New York, New York, US");
    assert!(near_10001(r.latitude, r.longitude));
    assert_eq!(r.timezone.as_deref(), Some("America/New_York"));
    assert_eq!(r.postcode.as_deref(), Some("10001"));
}

#[test]
fn open_meteo_many_and_none() {
    let many = include_bytes!("fixtures/open_meteo/geocode_springfield.json");
    let q = GeocodeQuery::Name {
        text: "Springfield",
        country: None,
    };
    let results = open_meteo::geocode::parse_geocode(&q, &HttpResponse::new(200, many)).unwrap();
    assert!(results.len() > 3);
    let names: Vec<String> = results
        .iter()
        .take(3)
        .map(whether_providers::GeocodeResult::display_name)
        .collect();
    insta::assert_debug_snapshot!(names);
    let none = include_bytes!("fixtures/open_meteo/geocode_none.json");
    let q = GeocodeQuery::Name {
        text: "zzqqxxy",
        country: None,
    };
    assert_eq!(
        open_meteo::geocode::parse_geocode(&q, &HttpResponse::new(200, none)).unwrap(),
        vec![]
    );
}

#[test]
fn openweather_requests_hide_the_key_when_redacted() {
    let base = openweather::DEFAULT_BASE_URL;
    let zip = openweather::geocode::geocode_request(base, "KEY", &ZIP, 1);
    assert_eq!(
        redact_url(&zip.url),
        "https://api.openweathermap.org/geo/1.0/zip?zip=10001,US&appid=***"
    );
    let name = openweather::geocode::geocode_request(base, "KEY", &NAME, 5);
    assert_eq!(
        redact_url(&name.url),
        "https://api.openweathermap.org/geo/1.0/direct?q=New%20York,US&limit=5&appid=***"
    );
}

#[test]
fn openweather_zip_and_direct() {
    let zip = include_bytes!("fixtures/openweather/geocode_zip_10001.synthetic.json");
    let r = openweather::geocode::parse_geocode(&ZIP, &HttpResponse::new(200, zip)).unwrap();
    assert_eq!(r[0].postcode.as_deref(), Some("10001"));
    assert!(near_10001(r[0].latitude, r[0].longitude));
    let direct = include_bytes!("fixtures/openweather/geocode_direct_new_york.synthetic.json");
    let r = openweather::geocode::parse_geocode(&NAME, &HttpResponse::new(200, direct)).unwrap();
    assert_eq!(r[0].display_name(), "New York, New York, US");
    let missing = openweather::geocode::parse_geocode(
        &ZIP,
        &HttpResponse::new(404, br#"{"cod":"404","message":"not found"}"#),
    )
    .unwrap();
    assert_eq!(missing, vec![]);
}

/// The recorded OpenWeather answers: the ZIP endpoint names the borough that holds
/// the ZIP's centroid and has no state; the direct endpoint finds the city.
#[test]
fn openweather_recorded() {
    let zip = include_bytes!("fixtures/openweather/geocode_zip_10001.json");
    let r = openweather::geocode::parse_geocode(&ZIP, &HttpResponse::new(200, zip)).unwrap();
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].display_name(), "Manhattan, US");
    assert_eq!(r[0].postcode.as_deref(), Some("10001"));
    assert!(near_10001(r[0].latitude, r[0].longitude));
    let direct = include_bytes!("fixtures/openweather/geocode_direct_new_york.json");
    let r = openweather::geocode::parse_geocode(&NAME, &HttpResponse::new(200, direct)).unwrap();
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].display_name(), "New York, New York, US");
    assert!(near_10001(r[0].latitude, r[0].longitude));
}
