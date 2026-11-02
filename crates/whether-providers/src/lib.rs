#![cfg_attr(not(test), no_std)]
#![warn(missing_docs)]
//! Sans-IO weather providers: they build HTTP requests and parse response bytes into
//! snapshots. Network I/O lives in the Linux runtime and the firmware.

extern crate alloc;

pub mod acis;
mod aggregate;
pub mod error;
pub mod geocode;
pub mod history;
pub mod http;
pub mod met_norway;
pub mod nws;
pub mod open_meteo;
pub mod openweather;
pub mod pirate_weather;
pub mod radar;
pub mod regions;
pub mod schedule;
pub mod session;
pub mod url;
pub mod usage;

pub use error::{ProviderError, check_status};
pub use geocode::{GeocodeQuery, GeocodeResult};
pub use history::{HistoryContext, HistoryProvider, HistorySession, history_due};
pub use http::{
    Attribution, Capabilities, Endpoint, FetchContext, HttpRequest, HttpResponse, Method, Provider,
};
pub use session::{Attempt, FetchError, FetchSession, MAX_ALERT_DETAILS, MAX_FOLLOW_UPS};
pub use url::{
    Coordinates, QueryBuilder, encode_path_segment, is_secret_header, redact_url, secret_values,
};
pub use usage::{Budget, Period, QuotaError, Reservation, UsageLedger, paced_refresh_secs};
