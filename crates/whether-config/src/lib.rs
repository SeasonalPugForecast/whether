#![cfg_attr(not(any(feature = "std", test)), no_std)]
//! TOML configuration types and validation; the `std` feature adds loading and layering.

extern crate alloc;

pub mod duration;
#[cfg(feature = "std")]
pub mod edit;
#[cfg(feature = "std")]
pub mod error;
#[cfg(feature = "std")]
pub mod example;
#[cfg(feature = "std")]
pub mod linux;
#[cfg(feature = "std")]
pub mod load;
pub mod merge;
pub mod model_list;
pub mod resolve;
#[cfg(feature = "schema")]
pub mod schema;
pub mod secret;
#[cfg(feature = "std")]
pub mod secret_resolve;
pub mod types;
pub mod validate;

pub use duration::{DurationError, DurationStr};
#[cfg(feature = "std")]
pub use error::ConfigError;
#[cfg(feature = "std")]
pub use load::{
    DisplaySection, LinuxConfig, LoadOptions, RadarConfig, RuntimeConfig, is_exec_name, load, parse,
};
pub use model_list::ModelList;
pub use resolve::{for_location, resolve_presentation, resolve_units};
pub use secret::Secret;
pub use types::{
    AcisToml, CommonConfig, General, HistoryConfig, LocationConfig, MetNorwayToml, MetProduct,
    NwsCurrent, NwsToml, OpenMeteoToml, OpenWeatherToml, OwmApi, PirateWeatherToml,
    PresentationToml, ProvidersConfig, UnitsOverride,
};
pub use validate::{Issue, Severity, has_errors, validate_presentation};
