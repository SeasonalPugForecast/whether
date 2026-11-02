//! Project automation, run as `cargo xtask <command>`.

mod ci;
mod cpp;
mod dist;
mod fixtures;
mod fw;
mod hooks;
mod icons;
mod ios;
mod ios_licenses;
mod ios_remote;
mod lane;
mod nostd;
mod remote;
mod schema;
mod sysroot;

use std::process::ExitCode;

const USAGE: &str = "\
usage: cargo xtask <command>

commands:
  ci                  fmt check, clippy, tests, no_std checks, cargo-deny
  nostd               check the no_std crates for riscv32imac-unknown-none-elf
  hooks install [--force]
                      install a git pre-push hook that runs `cargo xtask ci`
  fixtures --provider openweather|openweather-onecall4|open-meteo
                      record provider responses (OpenWeather needs OPENWEATHER_API_KEY;
                      onecall4 spends 6 of the day's One Call calls)
  schema [--check]    write (or check) the config and plugin protocol schemas
  icons fetch|raster  vendor GUI icons and fonts, render icon PNGs
  dist [--target T]…  release tarballs (binary, man page, completions)
  dist --assets       regenerate packaging/{man,completions}
  sysroot --from <user@host>
                      copy a Raspberry Pi OS sysroot from a Pi for ARMv6 builds
                      (dist --target arm-unknown-linux-gnueabihf)
  cpp [--preset P]    build and test the C++23 plugin SDK (sdk/cpp; default preset dev)
  fw build|flash|monitor|size --board <name> [--release] [--features a,b]
                      firmware (firmware/, boards from firmware/boards.toml)
  ios build [--debug] the iOS core: static libraries and Swift bindings; on a Mac
                      also ios/WhetherKit/WhetherFFI.xcframework
  ios licenses        write the app's open-source notices (ios/Whether/Whether/Acknowledgements.json)
  ios remote doctor|sync|build|test|app|shots [--host user@mac] [--dir d] [--dry-run]
                      the same on the Mac over SSH (host: ios/remote.local.toml)
  ios remote clean --lane <name> | --all-lanes
                      delete a lane's directory and simulator on the Mac
  remote doctor|sync|ci|run [--host user@host] [--fetch path]… [-- command…]
                      build on a faster Linux machine over SSH (host: remote.local.toml)
  remote clean --lane <name> | --all-lanes
                      delete a lane's directory on the build host
                      (a linked git worktree is a lane with its own directories)";

/// A parsed xtask command line.
#[derive(Debug, PartialEq, Eq)]
enum Command {
    Ci,
    Nostd,
    Fixtures { args: Vec<String> },
    Schema { args: Vec<String> },
    Icons { args: Vec<String> },
    Dist { args: Vec<String> },
    Sysroot { args: Vec<String> },
    Cpp { args: Vec<String> },
    HooksInstall { force: bool },
    Fw { args: Vec<String> },
    Ios { args: Vec<String> },
    Remote { args: Vec<String> },
    Help,
}

fn parse(args: &[String]) -> Result<Command, String> {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["ci"] => Ok(Command::Ci),
        ["nostd"] => Ok(Command::Nostd),
        ["fixtures", rest @ ..] => Ok(Command::Fixtures {
            args: rest.iter().map(|s| (*s).to_owned()).collect(),
        }),
        ["cpp", rest @ ..] => Ok(Command::Cpp {
            args: rest.iter().map(|s| (*s).to_owned()).collect(),
        }),
        ["dist", rest @ ..] => Ok(Command::Dist {
            args: rest.iter().map(|s| (*s).to_owned()).collect(),
        }),
        ["sysroot", rest @ ..] => Ok(Command::Sysroot {
            args: rest.iter().map(|s| (*s).to_owned()).collect(),
        }),
        ["icons", rest @ ..] => Ok(Command::Icons {
            args: rest.iter().map(|s| (*s).to_owned()).collect(),
        }),
        ["schema", rest @ ..] => Ok(Command::Schema {
            args: rest.iter().map(|s| (*s).to_owned()).collect(),
        }),
        ["hooks", "install"] => Ok(Command::HooksInstall { force: false }),
        ["hooks", "install", "--force"] => Ok(Command::HooksInstall { force: true }),
        [] | ["help" | "-h" | "--help"] => Ok(Command::Help),
        ["fw", rest @ ..] => Ok(Command::Fw {
            args: rest.iter().map(|s| (*s).to_owned()).collect(),
        }),
        ["ios", rest @ ..] => Ok(Command::Ios {
            args: rest.iter().map(|s| (*s).to_owned()).collect(),
        }),
        ["remote", rest @ ..] => Ok(Command::Remote {
            args: rest.iter().map(|s| (*s).to_owned()).collect(),
        }),
        [name] => Err(format!("unknown command `{name}`")),
        _ => Err(format!("unrecognized arguments: {}", args.join(" "))),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match parse(&args) {
        Ok(Command::Ci) => ci::run(),
        Ok(Command::Nostd) => nostd::run(),
        Ok(Command::Fixtures { args }) => {
            fixtures::run(&args.iter().map(String::as_str).collect::<Vec<_>>())
        }
        Ok(Command::Schema { args }) => {
            schema::run(&args.iter().map(String::as_str).collect::<Vec<_>>())
        }
        Ok(Command::Cpp { args }) => cpp::run(&args.iter().map(String::as_str).collect::<Vec<_>>()),
        Ok(Command::Dist { args }) => {
            dist::run(&args.iter().map(String::as_str).collect::<Vec<_>>())
        }
        Ok(Command::Sysroot { args }) => {
            sysroot::run(&args.iter().map(String::as_str).collect::<Vec<_>>())
        }
        Ok(Command::Icons { args }) => {
            icons::run_command(&args.iter().map(String::as_str).collect::<Vec<_>>())
        }
        Ok(Command::HooksInstall { force }) => hooks::install(force),
        Ok(Command::Fw { args }) => fw::run(&args.iter().map(String::as_str).collect::<Vec<_>>()),
        Ok(Command::Ios { args }) => ios::run(&args.iter().map(String::as_str).collect::<Vec<_>>()),
        Ok(Command::Remote { args }) => {
            remote::run(&args.iter().map(String::as_str).collect::<Vec<_>>())
        }
        Ok(Command::Help) => {
            println!("{USAGE}");
            Ok(())
        }
        Err(e) => Err(format!("{e}\n\n{USAGE}")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Runs `program args…` in the workspace root, streaming output.
pub(crate) fn run_cmd(program: &str, args: &[&str]) -> Result<(), String> {
    run_cmd_env(program, args, &[])
}

/// [`run_cmd`] with extra environment variables for the child.
pub(crate) fn run_cmd_env(
    program: &str,
    args: &[&str],
    envs: &[(&str, &str)],
) -> Result<(), String> {
    println!("$ {program} {}", args.join(" "));
    let status = std::process::Command::new(program)
        .args(args)
        .envs(envs.iter().copied())
        .current_dir(workspace_root())
        .status()
        .map_err(|e| format!("failed to start `{program}`: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "`{program} {}` failed with {status}",
            args.join(" ")
        ))
    }
}

/// The workspace root: the parent of the xtask crate directory.
pub(crate) fn workspace_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map_or_else(
            || std::path::PathBuf::from("."),
            std::path::Path::to_path_buf,
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn parses_known_commands() {
        assert_eq!(parse(&args("ci")), Ok(Command::Ci));
        assert_eq!(parse(&args("nostd")), Ok(Command::Nostd));
        assert_eq!(
            parse(&args("hooks install")),
            Ok(Command::HooksInstall { force: false })
        );
        assert_eq!(
            parse(&args("hooks install --force")),
            Ok(Command::HooksInstall { force: true })
        );
        assert_eq!(parse(&args("")), Ok(Command::Help));
    }

    #[test]
    fn parses_schema_arguments() {
        assert_eq!(parse(&args("schema")), Ok(Command::Schema { args: vec![] }));
        assert_eq!(
            parse(&args("schema --check")),
            Ok(Command::Schema {
                args: vec!["--check".into()]
            })
        );
    }

    #[test]
    fn parses_fixtures_arguments() {
        assert_eq!(
            parse(&args("fixtures --provider openweather")),
            Ok(Command::Fixtures {
                args: vec!["--provider".into(), "openweather".into()]
            })
        );
    }

    #[test]
    fn fw_takes_its_arguments() {
        assert_eq!(
            parse(&args("fw build --board cyd")),
            Ok(Command::Fw {
                args: vec!["build".into(), "--board".into(), "cyd".into()]
            })
        );
    }

    #[test]
    fn remote_takes_its_arguments() {
        assert_eq!(
            parse(&args("remote run -- cargo test")),
            Ok(Command::Remote {
                args: vec!["run".into(), "--".into(), "cargo".into(), "test".into()]
            })
        );
    }

    #[test]
    fn sysroot_takes_its_arguments() {
        assert_eq!(
            parse(&args("sysroot --from pi@pi.local")),
            Ok(Command::Sysroot {
                args: vec!["--from".into(), "pi@pi.local".into()]
            })
        );
    }

    #[test]
    fn rejects_unknown_input() {
        assert!(parse(&args("frobnicate")).is_err());
        assert!(parse(&args("ci extra")).is_err());
    }

    #[test]
    fn workspace_root_contains_the_workspace_manifest() {
        assert!(workspace_root().join("Cargo.toml").is_file());
    }
}
