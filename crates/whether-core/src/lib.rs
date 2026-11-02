#![cfg_attr(not(test), no_std)]
#![warn(missing_docs)]
//! Shared, `no_std` domain core: units, time, conditions, the weather model and the view model.
//!
//! Nothing in this crate performs I/O or reads the clock; callers pass `now` explicitly.

extern crate alloc;

pub mod builder;
pub mod condition;
pub mod derive;
pub mod glyph;
pub mod history;
pub mod icon;
pub mod merge;
pub mod model;
pub mod models;
pub mod packs;
pub mod presentation;
pub mod regions;
pub mod requirements;
pub mod scales;
pub mod strings;
pub mod sun;
#[cfg(any(test, feature = "testdata"))]
pub mod testdata;
pub mod theme;
pub mod time;
pub mod units;
pub mod view;
