//! Test helpers shared by the commands: a fake transport, the test place and
//! the time the fixtures were recorded.

use std::sync::Mutex;

use clap::Parser;
use whether::cli::Cli;
use whether_core::model::ResolvedLocation;
use whether_core::time::{UnixTime, UtcOffset};
use whether_providers::{HttpRequest, redact_url};
use whether_runtime::{OwnedResponse, Transport, TransportError};

/// 2026-11-02 10:00 EST, when the fixtures were recorded.
pub(crate) const NOV_2: UnixTime = UnixTime(1_793_631_600);

/// Answers by URL substring and counts the requests.
pub(crate) struct Fake {
    routes: Vec<(&'static str, &'static str)>,
    /// Requests sent so far.
    pub(crate) sent: Mutex<usize>,
}

impl Transport for Fake {
    fn send(&self, req: &HttpRequest) -> Result<OwnedResponse, TransportError> {
        *self.sent.lock().unwrap() += 1;
        self.routes
            .iter()
            .find(|(part, _)| req.url.contains(part))
            .map(|(_, body)| OwnedResponse::new(200, *body))
            .ok_or_else(|| TransportError::Failed(format!("no route for {}", redact_url(&req.url))))
    }
}

/// A transport answering `routes` (URL part, body).
pub(crate) fn fake(routes: Vec<(&'static str, &'static str)>) -> Fake {
    Fake {
        routes,
        sent: Mutex::new(0),
    }
}

/// ZIP 10001, in EST.
pub(crate) fn new_york() -> ResolvedLocation {
    ResolvedLocation {
        name: "New York".into(),
        latitude: 40.7143,
        longitude: -74.005_97,
        utc_offset: UtcOffset(-5 * 3_600),
        ..ResolvedLocation::default()
    }
}

/// The command line `whether <args>`.
pub(crate) fn cli(args: &[&str]) -> Cli {
    Cli::parse_from([&["whether"], args].concat())
}
