//! The `whether` command line definition, shared with xtask for the man page and
//! shell completions.

pub mod cli;

/// The full command-line definition.
pub fn command() -> clap::Command {
    <cli::Cli as clap::CommandFactory>::command()
}
