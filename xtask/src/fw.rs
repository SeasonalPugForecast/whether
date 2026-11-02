//! `cargo xtask fw build|flash|monitor|size --board <name> [--release] [--features …]`:
//! the firmware workspace (`firmware/`), with the board's target and toolchain
//! from `firmware/boards.toml`.
//!
//! The Xtensa toolchain needs espup's `~/export-esp.sh`, a POSIX sh file the
//! fish shell cannot source, so its `export KEY="VALUE"` lines are read
//! into the child environment here.

use std::path::{Path, PathBuf};
use std::process::Command;

/// What `fw` does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Build,
    Flash,
    Monitor,
    Size,
}

/// Parsed `fw` arguments.
#[derive(Debug, PartialEq, Eq)]
struct Args {
    action: Action,
    board: String,
    release: bool,
    features: Vec<String>,
}

const USAGE: &str =
    "usage: cargo xtask fw build|flash|monitor|size --board <name> [--release] [--features a,b]";

fn parse(args: &[&str]) -> Result<Args, String> {
    let (action, rest) = match args.split_first() {
        Some((&"build", rest)) => (Action::Build, rest),
        Some((&"flash", rest)) => (Action::Flash, rest),
        Some((&"monitor", rest)) => (Action::Monitor, rest),
        Some((&"size", rest)) => (Action::Size, rest),
        _ => return Err(USAGE.into()),
    };
    let mut board = None;
    let mut release = false;
    let mut features = Vec::new();
    let mut it = rest.iter();
    while let Some(arg) = it.next() {
        match *arg {
            "--board" => board = Some(it.next().ok_or("--board needs a name")?.to_string()),
            "--release" => release = true,
            "--features" => features.extend(
                it.next()
                    .ok_or("--features needs a list")?
                    .split(',')
                    .filter(|f| !f.is_empty())
                    .map(str::to_string),
            ),
            other => return Err(format!("unknown argument `{other}`; {USAGE}")),
        }
    }
    Ok(Args {
        action,
        board: board.ok_or_else(|| format!("--board is required; {USAGE}"))?,
        release,
        features,
    })
}

/// A board profile's build settings.
#[derive(Debug, PartialEq, Eq)]
struct Board {
    chip: String,
    target: String,
    toolchain: String,
    features: Vec<String>,
}

fn firmware_dir() -> PathBuf {
    crate::workspace_root().join("firmware")
}

/// The profile `name` from `boards.toml` text.
fn board(text: &str, name: &str) -> Result<Board, String> {
    let boards: toml::Table = toml::from_str(text).map_err(|e| format!("boards.toml: {e}"))?;
    let known = || boards.keys().cloned().collect::<Vec<_>>().join(", ");
    let b = boards
        .get(name)
        .and_then(toml::Value::as_table)
        .ok_or_else(|| format!("unknown board `{name}`; known: {}", known()))?;
    let field = |key: &str| {
        b.get(key)
            .and_then(toml::Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| format!("boards.toml: `{name}` has no `{key}`"))
    };
    Ok(Board {
        chip: field("chip")?,
        target: field("target")?,
        toolchain: field("toolchain")?,
        features: b
            .get("features")
            .and_then(toml::Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
    })
}

/// `export KEY="VALUE"` lines from espup's export file; `$KEY` references to
/// variables already known (such as `$PATH`) are expanded.
fn exports(text: &str, current: &dyn Fn(&str) -> Option<String>) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in text.lines() {
        let Some((key, value)) = line
            .trim()
            .strip_prefix("export ")
            .and_then(|l| l.split_once('='))
        else {
            continue;
        };
        let mut value = value.trim().trim_matches('"').to_string();
        for (name, known) in out
            .iter()
            .map(|(k, v)| (k.clone(), Some(v.clone())))
            .chain(["PATH", "HOME"].map(|k| (k.to_string(), current(k))))
            .collect::<Vec<_>>()
        {
            if let Some(known) = known {
                value = value.replace(&format!("${name}"), &known);
            }
        }
        out.retain(|(k, _)| k != key.trim());
        out.push((key.trim().to_string(), value));
    }
    out
}

/// The environment espup prepared, if its file exists.
fn esp_env() -> Vec<(String, String)> {
    let Some(home) = std::env::var_os("HOME") else {
        return Vec::new();
    };
    let file = Path::new(&home).join("export-esp.sh");
    if let Ok(text) = std::fs::read_to_string(&file) {
        exports(&text, &|k| std::env::var(k).ok())
    } else {
        println!(
            "note: {} not found; run `espup install` for Xtensa boards",
            file.display()
        );
        Vec::new()
    }
}

/// The cargo arguments for building `args` on `b`.
fn cargo_args(args: &Args, b: &Board) -> Vec<String> {
    let mut features = vec![
        format!("board-{}", args.board),
        "display-tui".into(),
        "display-panel".into(),
    ];
    features.extend(b.features.iter().cloned());
    features.extend(args.features.iter().cloned());
    let mut out = vec![
        format!("+{}", b.toolchain),
        "build".into(),
        "-p".into(),
        "whether-fw".into(),
        "--target".into(),
        b.target.clone(),
        "--no-default-features".into(),
        "--features".into(),
        features.join(","),
    ];
    if args.release {
        out.push("--release".into());
    }
    out
}

fn run_fw(program: &str, args: &[String], env: &[(String, String)]) -> Result<(), String> {
    println!("$ (cd firmware) {program} {}", args.join(" "));
    let status = Command::new(program)
        .args(args)
        .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .current_dir(firmware_dir())
        .status()
        .map_err(|e| format!("failed to start `{program}`: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("`{program}` failed with {status}"))
    }
}

/// The share of the app partition an image of `image` bytes uses, from
/// espflash's "App/part. size: A/B bytes" line.
fn partition_use(espflash_output: &str) -> Option<(u64, u64)> {
    let line = espflash_output
        .lines()
        .find(|l| l.contains("App/part. size:"))?;
    let sizes = line.split(':').nth(1)?.split_whitespace().next()?;
    let (used, total) = sizes.split_once('/')?;
    let n = |s: &str| s.replace(',', "").parse::<u64>().ok();
    Some((n(used)?, n(total)?))
}

/// Whether rustup has the `esp` toolchain (installed by `espup install`).
fn esp_toolchain_installed() -> bool {
    Command::new("rustup")
        .args(["toolchain", "list"])
        .output()
        .is_ok_and(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .any(|l| l.starts_with("esp"))
        })
}

/// The gate's firmware checks: formatting, `fw-config` and `fw-sleep` on the host, and clippy on
/// `whether-fw` (which also validates the embedded example configuration) when the
/// esp toolchain is installed.
pub(crate) fn check() -> Result<(), String> {
    let manifest = |crate_dir: &str| {
        firmware_dir()
            .join(crate_dir)
            .join("Cargo.toml")
            .to_string_lossy()
            .to_string()
    };
    let workspace = manifest("");
    let fw_config = manifest("fw-config");
    let fw_sleep = manifest("fw-sleep");
    crate::run_cmd(
        "cargo",
        &[
            "+stable",
            "fmt",
            "--manifest-path",
            &workspace,
            "--all",
            "--check",
        ],
    )?;
    // The firmware's host-testable crates.
    for (manifest, features) in [(&fw_config, &["--features", "build"][..]), (&fw_sleep, &[])] {
        let mut clippy = vec!["+stable", "clippy", "--manifest-path", manifest.as_str()];
        clippy.extend_from_slice(features);
        clippy.extend_from_slice(&["--all-targets", "--", "-D", "warnings"]);
        crate::run_cmd("cargo", &clippy)?;
        let mut test = vec!["+stable", "test", "--manifest-path", manifest.as_str()];
        test.extend_from_slice(features);
        crate::run_cmd("cargo", &test)?;
    }
    if !esp_toolchain_installed() {
        println!(
            "note: the esp toolchain is not installed; skipping the firmware build (espup install)"
        );
        return Ok(());
    }
    let args = [
        "+esp",
        "clippy",
        "-p",
        "whether-fw",
        "--release",
        "--",
        "-D",
        "warnings",
    ]
    .map(str::to_string);
    // Lint against the example configuration, never a local whether-fw.toml (which
    // needs the Wi-Fi variables).
    let mut env = esp_env();
    let example = firmware_dir().join("whether-fw.example.toml");
    env.push((
        "WHETHER_FW_CONFIG".into(),
        example.to_string_lossy().into_owned(),
    ));
    run_fw("cargo", &args, &env)
}

pub(crate) fn run(args: &[&str]) -> Result<(), String> {
    let args = parse(args)?;
    let boards = firmware_dir().join("boards.toml");
    let text =
        std::fs::read_to_string(&boards).map_err(|e| format!("{}: {e}", boards.display()))?;
    let b = board(&text, &args.board)?;
    let env = esp_env();
    let profile = if args.release { "release" } else { "debug" };
    let elf = firmware_dir()
        .join("target")
        .join(&b.target)
        .join(profile)
        .join("whether-fw");
    let elf = elf.to_string_lossy().to_string();
    match args.action {
        Action::Build => run_fw("cargo", &cargo_args(&args, &b), &env),
        Action::Flash => {
            run_fw("cargo", &cargo_args(&args, &b), &env)?;
            let flash = ["flash", "--monitor", "--chip", &b.chip, &elf].map(str::to_string);
            run_fw("espflash", &flash, &env)
        }
        Action::Monitor => {
            let monitor = ["monitor", "--chip", &b.chip, "--elf", &elf].map(str::to_string);
            run_fw("espflash", &monitor, &env)
        }
        Action::Size => {
            run_fw("cargo", &cargo_args(&args, &b), &env)?;
            let image = firmware_dir().join("target").join("whether-fw.bin");
            let out = Command::new("espflash")
                .args(["save-image", "--chip", &b.chip, &elf])
                .arg(&image)
                .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
                .output()
                .map_err(|e| format!("failed to start espflash: {e}"))?;
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            let (used, total) = partition_use(&text)
                .ok_or_else(|| format!("espflash printed no sizes:\n{text}"))?;
            let percent = used * 100 / total.max(1);
            println!("app image: {used} of {total} bytes ({percent} %)");
            if percent > 90 {
                return Err(format!(
                    "the image uses {percent} % of the app partition (limit 90 %)"
                ));
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments() {
        assert_eq!(
            parse(&["build", "--board", "cyd", "--release", "--features", "a,b"]),
            Ok(Args {
                action: Action::Build,
                board: "cyd".into(),
                release: true,
                features: vec!["a".into(), "b".into()],
            })
        );
        assert!(parse(&["build"]).is_err());
        assert!(parse(&["burn", "--board", "cyd"]).is_err());
        assert!(parse(&["size", "--board", "cyd", "--frob"]).is_err());
    }

    #[test]
    fn boards_come_from_the_profile_file() {
        let text = std::fs::read_to_string(firmware_dir().join("boards.toml")).unwrap();
        let b = board(&text, "esp32-wroom-spi").unwrap();
        assert_eq!(b.target, "xtensa-esp32-none-elf");
        assert_eq!(b.toolchain, "esp");
        assert!(board(&text, "esp8266").unwrap_err().contains("known: "));
        let args = parse(&["build", "--board", "esp32-wroom-spi"]).unwrap();
        let cargo = cargo_args(&args, &b);
        assert_eq!(cargo[0], "+esp");
        assert!(cargo.contains(&"board-esp32-wroom-spi,display-tui,display-panel".to_string()));
    }

    #[test]
    fn export_lines_expand_known_variables() {
        let text = "export LIBCLANG_PATH=\"/x/lib\"\nexport PATH=\"/x/bin:$PATH\"\n# comment\n";
        let env = exports(text, &|k| (k == "PATH").then(|| "/usr/bin".to_string()));
        assert_eq!(
            env,
            [
                ("LIBCLANG_PATH".to_string(), "/x/lib".to_string()),
                ("PATH".to_string(), "/x/bin:/usr/bin".to_string())
            ]
        );
    }

    #[test]
    fn sizes_from_espflash() {
        let out = "Chip type:         esp32\nApp/part. size:    524,464/4,128,768 bytes, 12.70%\n";
        assert_eq!(partition_use(out), Some((524_464, 4_128_768)));
        assert_eq!(partition_use("nothing"), None);
    }
}
