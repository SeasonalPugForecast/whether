//! Test helpers shared by the runtime's unit tests.

use std::sync::Mutex;

use whether_providers::{HttpRequest, redact_url};

use crate::transport::{OwnedResponse, Transport, TransportError};

/// A transport that answers by URL substring (first match wins) and records URLs.
pub(crate) struct Replay {
    routes: Vec<(&'static str, u16, &'static str)>,
    seen: Mutex<Vec<String>>,
}

impl Replay {
    pub(crate) fn new(routes: Vec<(&'static str, u16, &'static str)>) -> Self {
        Self {
            routes,
            seen: Mutex::new(Vec::new()),
        }
    }

    /// Every URL requested, in order.
    pub(crate) fn seen(&self) -> Vec<String> {
        self.seen.lock().unwrap().clone()
    }
}

impl Transport for Replay {
    fn send(&self, req: &HttpRequest) -> Result<OwnedResponse, TransportError> {
        self.seen.lock().unwrap().push(req.url.clone());
        self.routes
            .iter()
            .find(|(part, _, _)| req.url.contains(part))
            .map(|(_, status, body)| OwnedResponse::new(*status, *body))
            .ok_or_else(|| TransportError::Failed(format!("no route for {}", redact_url(&req.url))))
    }
}
