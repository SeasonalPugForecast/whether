//! Linux runtime: HTTP transport, fetch scheduling and caching.

mod atomic;
pub mod cache;
pub mod fetch;
pub mod history;
pub mod locate;
pub mod paths;
pub mod providers;
pub mod radar;
/// Due times, backoff and jitter, shared with the firmware.
pub use whether_providers::schedule as scheduler;
pub mod service;
#[cfg(test)]
mod testing;
pub mod transport;
pub mod usage;
pub mod watch;

pub use cache::{Cache, CachedSnapshot};
pub use fetch::{Fetched, fetch_location};
pub use locate::{Geocoder, LocateError, known_coordinates, resolve_location};
pub use providers::{Chain, build_chain};
pub use scheduler::{Jitter, Scheduler};
pub use service::{
    ServiceHandle, ServiceOptions, Worker, initial_state, refresh_once, spawn_service,
};
pub use transport::{OwnedResponse, Transport, TransportError, UreqTransport, user_agent};
pub use usage::{UsageError, UsageGuard, UsageStore};
