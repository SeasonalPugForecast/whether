#![no_std]
//! whether on ESP32: the platform layer (networking, and later the displays and
//! input) around the shared `no_std` crates.

extern crate alloc;

pub mod diag;
pub mod fetch;
pub mod ledger;
pub mod net;
pub mod power;
pub mod providers;
pub mod state;
