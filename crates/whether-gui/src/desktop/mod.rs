//! Desktop host: window glue and the headless screenshot harness.

pub mod config;
pub mod glue;
pub mod plugin;
pub mod screenshot;
pub mod settings;

pub use plugin::{GuiPlugin, factory};
