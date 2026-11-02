//! The `whether` command-line application.

mod commands;
mod common;
mod logging;
mod registry;

use std::process::ExitCode;

use clap::Parser;

use whether::cli::{Cli, Cmd, ConfigCmd, ProvidersCmd};

fn run(cli: &Cli) -> anyhow::Result<ExitCode> {
    if matches!(cli.command, Some(Cmd::Daemon)) {
        return commands::daemon::run(cli);
    }
    let display = match &cli.command {
        None | Some(Cmd::Run) => return commands::run::run_display(cli, None, &[]),
        Some(Cmd::Tui) => return commands::run::run_display(cli, Some("tui"), &[]),
        Some(Cmd::Gui(args)) => {
            #[cfg(feature = "gui")]
            if args.screenshot.is_some() {
                return commands::screenshot::run(cli, args);
            }
            let extra: Vec<(String, toml::Value)> = if args.fullscreen {
                vec![("display.gui.kiosk".into(), toml::Value::Boolean(true))]
            } else {
                Vec::new()
            };
            return commands::run::run_display(cli, Some("gui"), &extra);
        }
        Some(Cmd::Line) => return commands::run::run_display(cli, Some("line"), &[]),
        Some(Cmd::Json { snapshot, compact }) => {
            let flag =
                |key: &str, on: bool| (format!("display.json.{key}"), toml::Value::Boolean(on));
            let mut extra = Vec::new();
            if *snapshot {
                extra.push(flag("snapshot", true));
            }
            if *compact {
                extra.push(flag("compact", true));
            }
            return commands::run::run_display(cli, Some("json"), &extra);
        }
        Some(other) => other,
    };
    logging::init(&logging::level(cli.verbose, None), None)?;
    match display {
        Cmd::Config { action } => match action {
            ConfigCmd::Init { force } => commands::config::run_init(cli, *force),
            ConfigCmd::Check => Ok(commands::config::run_check(cli)),
            ConfigCmd::Show { resolved } => commands::config::run_show(cli, *resolved),
            ConfigCmd::Path => {
                print!("{}", commands::config::paths(cli));
                Ok(ExitCode::SUCCESS)
            }
            ConfigCmd::Schema => {
                print!("{}", commands::config::schema_json());
                Ok(ExitCode::SUCCESS)
            }
            ConfigCmd::Strings => {
                print!("{}", commands::config::strings());
                Ok(ExitCode::SUCCESS)
            }
        },
        Cmd::Displays => {
            print!("{}", commands::displays::table());
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Locate(args) => commands::locate::run(cli, args),
        Cmd::Providers {
            action: Some(ProvidersCmd::Models),
        } => {
            print!("{}", commands::providers::models(cli));
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Providers { action: None } => {
            print!("{}", commands::providers::table());
            if let Ok(cfg) = common::load_config(cli, &[])
                && let Some(usage) = commands::providers::usage(&cfg)
            {
                println!("\n{usage}");
            }
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Fetch(args) => commands::fetch::run(cli, args),
        Cmd::History(args) => commands::history::run(cli, args),
        Cmd::Climate(args) => commands::climate::run(cli, args),
        // Handled above.
        Cmd::Run | Cmd::Tui | Cmd::Gui(_) | Cmd::Line | Cmd::Json { .. } | Cmd::Daemon => {
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    run(&cli).unwrap_or_else(|e| {
        eprintln!("whether: {e:#}");
        ExitCode::FAILURE
    })
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        super::Cli::command().debug_assert();
    }
}
