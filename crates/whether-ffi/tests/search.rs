//! `search_places` with a fake host answering from recorded geocoding responses.

use std::sync::{Arc, Mutex};

use whether_ffi::{FetchHost, HostError, HostRequest, HostResponse, WhetherError, search_places};

const SPRINGFIELD: &str =
    include_str!("../../whether-providers/tests/fixtures/open_meteo/geocode_springfield.json");
const ZIP: &str =
    include_str!("../../whether-providers/tests/fixtures/open_meteo/geocode_10001_us.json");

struct Fake {
    urls: Mutex<Vec<String>>,
    fail: bool,
}

#[async_trait::async_trait]
impl FetchHost for Fake {
    async fn send(&self, request: HostRequest) -> Result<HostResponse, HostError> {
        self.urls.lock().unwrap().push(request.url.clone());
        if self.fail {
            return Err(HostError::Failed {
                message: "offline".into(),
            });
        }
        let body = if request.url.contains("10001") {
            ZIP
        } else {
            SPRINGFIELD
        };
        Ok(HostResponse {
            status: 200,
            body: body.as_bytes().to_vec(),
            date: None,
            expires: None,
            last_modified: None,
        })
    }

    fn save_usage(&self, _: String) -> Result<(), HostError> {
        Ok(())
    }
}

fn host(fail: bool) -> Arc<Fake> {
    Arc::new(Fake {
        urls: Mutex::new(Vec::new()),
        fail,
    })
}

fn search(query: &str, host: &Arc<Fake>) -> Result<Vec<whether_ffi::FoundPlace>, WhetherError> {
    pollster::block_on(search_places(
        query.into(),
        Some("US".into()),
        "en".into(),
        Arc::clone(host) as Arc<dyn FetchHost>,
    ))
}

#[test]
fn names_and_postal_codes() {
    let h = host(false);
    let places = search("Springfield", &h).unwrap();
    assert!(places.len() > 1);
    assert!(places[0].display_name.starts_with("Springfield, "));
    assert!(places.iter().all(|p| p.timezone.is_some()));
    let mut names: Vec<&str> = places.iter().map(|p| p.display_name.as_str()).collect();
    names.dedup();
    assert_eq!(names.len(), places.len(), "no repeated names");
    let zip = search(" 10001 ", &h).unwrap();
    assert_eq!(zip[0].name, "New York");
    assert_eq!(zip[0].country_code.as_deref(), Some("US"));
    let urls = h.urls.lock().unwrap().clone();
    assert!(urls[1].contains("10001"), "{urls:?}");
}

#[test]
fn short_queries_and_failures() {
    let h = host(false);
    assert_eq!(search("a", &h).unwrap(), vec![]);
    assert_eq!(
        h.urls.lock().unwrap().len(),
        0,
        "nothing sent for one letter"
    );
    let err = search("Brooklyn", &host(true)).unwrap_err();
    assert!(matches!(err, WhetherError::Search { .. }), "{err:?}");
}
