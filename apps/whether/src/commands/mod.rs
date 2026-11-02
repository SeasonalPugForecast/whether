//! Subcommand implementations.

pub(crate) mod climate;
pub(crate) mod config;
pub(crate) mod daemon;
pub(crate) mod displays;
pub(crate) mod fetch;
pub(crate) mod history;
pub(crate) mod locate;
pub(crate) mod providers;
pub(crate) mod run;
#[cfg(feature = "gui")]
pub(crate) mod screenshot;
#[cfg(test)]
pub(crate) mod testing;
