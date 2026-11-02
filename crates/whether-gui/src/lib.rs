#![cfg_attr(not(feature = "std"), no_std)]
//! Graphical display: Slint UI for desktop and (optionally) ESP32-S3.

extern crate alloc;

/// The Slint-generated components. Lints are relaxed for generated code only.
#[allow(
    unreachable_pub,
    missing_debug_implementations,
    missing_docs,
    clippy::all,
    clippy::pedantic
)]
mod generated {
    slint::include_modules!();
}

pub use generated::*;

pub mod compact;
pub mod model;

#[cfg(feature = "desktop")]
pub mod charts;
#[cfg(feature = "desktop")]
pub mod desktop;
