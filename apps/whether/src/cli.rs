//! Command-line arguments.

use std::path::PathBuf;

use clap::{ArgAction, Parser, Subcommand, ValueEnum};

/// Weather for the terminal, the desktop and status bars.
#[derive(Debug, Parser)]
#[command(name = "whether", version, about)]
pub struct Cli {
    /// Use only this configuration file.
    #[arg(short, long, global = true, value_name = "PATH")]
    pub config: Option<PathBuf>,
    /// Display to run (see `whether displays`).
    #[arg(short, long, global = true, value_name = "NAME")]
    pub display: Option<String>,
    /// Location to show first, by name or 1-based index.
    #[arg(short, long, global = true, value_name = "NAME|INDEX")]
    pub location: Option<String>,
    /// Override the unit system.
    #[arg(long, global = true, value_enum)]
    pub units: Option<Units>,
    /// Never use the network; show cached data.
    #[arg(long, global = true)]
    pub offline: bool,
    /// More logging (-v info, -vv debug, -vvv trace).
    #[arg(short, long, global = true, action = ArgAction::Count)]
    pub verbose: u8,
    /// line and json: print again on every change.
    #[arg(long, global = true)]
    pub watch: bool,
    /// Show data from a running `whether daemon` instead of fetching.
    #[arg(long, global = true)]
    pub attach: bool,
    /// What to do (default: run the selected display).
    #[command(subcommand)]
    pub command: Option<Cmd>,
}

/// `--units` values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Units {
    /// °C, km/h, hPa, mm, km.
    Metric,
    /// °F, mph, inHg, in, mi.
    Imperial,
}

/// Subcommands.
#[derive(Debug, Subcommand)]
pub enum Cmd {
    /// Run the selected display (the default).
    Run,
    /// Run the terminal display.
    Tui,
    /// Run the desktop display.
    Gui(GuiArgs),
    /// Print one line for status bars.
    Line,
    /// Print JSON for scripts.
    Json {
        /// The raw snapshot instead of the view model.
        #[arg(long)]
        snapshot: bool,
        /// One line instead of pretty JSON.
        #[arg(long)]
        compact: bool,
    },
    /// Inspect and create the configuration.
    Config {
        /// Which action.
        #[command(subcommand)]
        action: ConfigCmd,
    },
    /// List the compiled-in displays.
    Displays,
    /// Search for a place; print a `[[location]]` snippet or add it.
    Locate(LocateArgs),
    /// List providers: authentication, capabilities, limits and attribution.
    Providers {
        /// What to list instead of the providers.
        #[command(subcommand)]
        action: Option<ProvidersCmd>,
    },
    /// Fetch once and print the snapshot as JSON (debugging).
    Fetch(FetchArgs),
    /// Print a place's history: normals, records and the last days as observed.
    History(HistoryArgs),
    /// How chosen dates were over chosen years, for planning a trip (--location
    /// may be any place's name).
    Climate(ClimateArgs),
    /// Fetch in the background and serve displays on a socket (use them with --attach).
    Daemon,
}

/// `whether gui`.
#[derive(Debug, Default, clap::Args)]
pub struct GuiArgs {
    /// Full screen without decorations (like `kiosk = true`).
    #[arg(long)]
    pub fullscreen: bool,
    /// Render one frame headlessly to this PNG and exit.
    #[arg(long, value_name = "OUT.png")]
    pub screenshot: Option<std::path::PathBuf>,
    /// Screenshot size, e.g. 1920x1080.
    #[arg(long, value_name = "WxH", requires = "screenshot")]
    pub size: Option<String>,
    /// Screenshot the compact window.
    #[arg(long, requires = "screenshot")]
    pub compact: bool,
    /// Screenshot this cached snapshot (a cache JSON file) instead of the cache.
    #[arg(long, value_name = "CACHE.json", requires = "screenshot")]
    pub fixture: Option<std::path::PathBuf>,
}

/// `whether locate`.
#[derive(Debug, clap::Args)]
pub struct LocateArgs {
    /// A place name, or a postal code with --zip.
    pub query: String,
    /// Search a postal code (needs --country).
    #[arg(long, requires = "country")]
    pub zip: bool,
    /// ISO 3166-1 alpha-2 country code, e.g. US.
    #[arg(long, value_name = "CC")]
    pub country: Option<String>,
    /// How many results to show.
    #[arg(long, default_value_t = 5)]
    pub count: u8,
    /// Append the first result to the configuration file.
    #[arg(long)]
    pub add: bool,
    /// The name to give the added location.
    #[arg(long, requires = "add")]
    pub name: Option<String>,
}

/// `whether fetch`.
#[derive(Debug, clap::Args)]
pub struct FetchArgs {
    /// Use only this provider (openweather, open-meteo).
    #[arg(long, value_name = "ID")]
    pub provider: Option<String>,
    /// Print only the snapshot, without the attempts.
    #[arg(long)]
    pub raw: bool,
}

/// `whether history`.
#[derive(Debug, Default, clap::Args)]
pub struct HistoryArgs {
    /// How many past days (1–31; default: `history_days`).
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u8).range(1..=31))]
    pub days: Option<u8>,
    /// Print JSON: the history as shown and as received.
    #[arg(long)]
    pub json: bool,
    /// List the ACIS stations near the place instead, to choose one for
    /// `[history.acis] station`.
    #[arg(long)]
    pub station: bool,
}

/// `whether climate`.
#[derive(Debug, Default, clap::Args)]
pub struct ClimateArgs {
    /// The first date: MM-DD (the next one) or YYYY-MM-DD.
    #[arg(long, value_name = "DATE")]
    pub from: String,
    /// The last date (default: the first; at most 31 days on).
    #[arg(long, value_name = "DATE")]
    pub to: Option<String>,
    /// The years to look back over, e.g. 1995-2024 (default: the 30 before this one).
    #[arg(long, value_name = "FIRST-LAST")]
    pub years: Option<String>,
    /// Print JSON: the query, the view and the answer as received.
    #[arg(long)]
    pub json: bool,
}

/// `whether providers …`.
#[derive(Debug, Subcommand)]
pub enum ProvidersCmd {
    /// List the weather models `[providers.open-meteo] models` accepts, marking
    /// those with data for the first location.
    Models,
}

/// `whether config …`.
#[derive(Debug, Subcommand)]
pub enum ConfigCmd {
    /// Write the commented example configuration.
    Init {
        /// Overwrite an existing file.
        #[arg(long)]
        force: bool,
    },
    /// Validate the configuration (exit 2 on errors).
    Check,
    /// Print the effective configuration (secrets masked).
    Show {
        /// Show units and presentation as resolved for the default display.
        #[arg(long)]
        resolved: bool,
    },
    /// Print where configuration files are read from.
    Path,
    /// Print the JSON Schema for editor completion.
    Schema,
    /// List every text key and its default.
    Strings,
}
