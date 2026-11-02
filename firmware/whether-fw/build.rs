#![allow(
    clippy::manual_let_else,
    clippy::map_unwrap_or,
    reason = "the linker helpers are esp-generate's code, kept as generated"
)]
//! Build script: esp-generate's linker setup, plus the firmware configuration
//! (`embed_config`).

fn main() {
    linker_be_nice();
    check_xtensa_linker_available();
    embed_config();
    // make sure linkall.x is the last linker script (otherwise might cause problems with flip-link)
    println!("cargo:rustc-link-arg=-Tlinkall.x");
}

#[cfg(unix)]
fn check_xtensa_linker_available() {
    println!("cargo:rerun-if-env-changed=PATH");

    let target = std::env::var("TARGET").unwrap_or_default();
    println!(
        "cargo:rerun-if-env-changed={}",
        cargo_linker_env_var(&target)
    );

    let linker = xtensa_linker(&target);

    let error = match std::process::Command::new(&linker)
        .arg("--version")
        .output()
    {
        Ok(_) => return,
        Err(error) => error,
    };

    let export_file = std::env::var("HOME")
        .map(|home| format!("{home}/export-esp.sh"))
        .unwrap_or_else(|_| "$HOME/export-esp.sh".to_string());

    panic!(
        "Xtensa linker `{linker}` was not found in PATH or could not be executed: {error}.\n\n\
         Xtensa targets on Unix require espup's environment export file to be sourced before building.\n\
         Try: `source {export_file}`\n\n\
         If the export file does not exist, run `espup install` first.\n\
         For more details, see:\n\
         https://github.com/esp-rs/espup?tab=readme-ov-file#environment-variables-setup"
    );
}

#[cfg(not(unix))]
fn check_xtensa_linker_available() {}

#[cfg(unix)]
fn xtensa_linker(target: &str) -> String {
    if let Some(linker) = cargo_configured_linker(target) {
        return linker;
    }

    target
        .strip_prefix("xtensa-")
        .and_then(|target| target.strip_suffix("-none-elf"))
        .map(|chip| format!("xtensa-{chip}-elf-gcc"))
        .unwrap_or_else(|| "xtensa-esp32-elf-gcc".to_string())
}

#[cfg(unix)]
fn cargo_configured_linker(target: &str) -> Option<String> {
    std::env::var(cargo_linker_env_var(target)).ok()
}

#[cfg(unix)]
fn cargo_linker_env_var(target: &str) -> String {
    let target = target.replace('-', "_").to_ascii_uppercase();
    format!("CARGO_TARGET_{target}_LINKER")
}

fn linker_be_nice() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        let kind = &args[1];
        let what = &args[2];

        match kind.as_str() {
            "undefined-symbol" => match what.as_str() {
                what if what.starts_with("_defmt_") => {
                    eprintln!();
                    eprintln!(
                        "💡 `defmt` not found - make sure `defmt.x` is added as a linker script and you have included `use defmt_rtt as _;`"
                    );
                    eprintln!();
                }
                "_stack_start" => {
                    eprintln!();
                    eprintln!("💡 Is the linker script `linkall.x` missing?");
                    eprintln!();
                }
                what if what.starts_with("esp_rtos_") => {
                    eprintln!();
                    eprintln!(
                        "💡 `esp-radio` has no scheduler enabled. Make sure you have initialized `esp-rtos` or provided an external scheduler."
                    );
                    eprintln!();
                }
                "embedded_test_linker_file_not_added_to_rustflags" => {
                    eprintln!();
                    eprintln!(
                        "💡 `embedded-test` not found - make sure `embedded-test.x` is added as a linker script for tests"
                    );
                    eprintln!();
                }
                "free"
                | "malloc"
                | "calloc"
                | "get_free_internal_heap_size"
                | "malloc_internal"
                | "realloc_internal"
                | "calloc_internal"
                | "free_internal" => {
                    eprintln!();
                    eprintln!(
                        "💡 Did you forget the `esp-alloc` dependency or didn't enable the `compat` feature on it?"
                    );
                    eprintln!();
                }
                _ => (),
            },
            // we don't have anything helpful for "missing-lib" yet
            _ => {
                std::process::exit(1);
            }
        }

        std::process::exit(0);
    }

    println!(
        "cargo:rustc-link-arg=-Wl,--error-handling-script={}",
        std::env::current_exe().unwrap().display()
    );
}

/// The board this build is for, from the `board-*` feature.
fn board() -> String {
    let boards: Vec<String> = std::env::vars()
        .filter_map(|(k, _)| k.strip_prefix("CARGO_FEATURE_BOARD_").map(str::to_string))
        .collect();
    match boards.as_slice() {
        [one] => one.to_ascii_lowercase().replace('_', "-"),
        [] => panic!("enable exactly one board-* feature (see firmware/boards.toml)"),
        many => panic!("enable exactly one board-* feature, not {many:?}"),
    }
}

/// Validates the firmware configuration with fw-config and embeds it as
/// `$OUT_DIR/config.bin` (postcard). Errors fail the build; nothing secret is
/// printed.
fn embed_config() {
    use fw_config::FwDisplay;
    use fw_config::build::{Target, build};

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    println!("cargo:rerun-if-env-changed=WHETHER_FW_CONFIG");
    let user = root.join("whether-fw.toml");
    let example = root.join("whether-fw.example.toml");
    let (path, is_example) = match std::env::var_os("WHETHER_FW_CONFIG") {
        // `cargo xtask ci` points here at the example, so its lint never depends on
        // a local configuration or its secrets.
        Some(p) => {
            let p = std::path::PathBuf::from(p);
            let is_example = p.file_name() == example.file_name();
            (p, is_example)
        }
        None if user.exists() => (user, false),
        None => (example, true),
    };
    println!("cargo:rerun-if-changed={}", path.display());
    println!(
        "cargo:rerun-if-changed={}",
        root.join("whether-fw.toml").display()
    );
    let boards_path = root.join("boards.toml");
    println!("cargo:rerun-if-changed={}", boards_path.display());
    // The example reads the Wi-Fi network from WHETHER_WIFI_SSID and
    // WHETHER_WIFI_PASSWORD; only without them is there nothing to join.
    if is_example {
        let wifi_set = std::env::var_os("WHETHER_WIFI_SSID").is_some_and(|s| !s.is_empty());
        if wifi_set {
            println!(
                "cargo:warning=no firmware/whether-fw.toml: building with the example configuration and the Wi-Fi network from WHETHER_WIFI_SSID"
            );
        } else {
            println!(
                "cargo:warning=no firmware/whether-fw.toml and WHETHER_WIFI_SSID is unset: building with the example configuration (the device will not join a network)"
            );
        }
    }
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));

    let board = board();
    // The example is written for the ESP32-WROOM; it follows whichever board is built.
    let text = if is_example {
        text.replace(
            "board = \"esp32-wroom-spi\"",
            &format!("board = \"{board}\""),
        )
    } else {
        text
    };
    let boards: toml::Table = std::fs::read_to_string(&boards_path)
        .ok()
        .and_then(|t| toml::from_str(&t).ok())
        .unwrap_or_else(|| panic!("cannot read {}", boards_path.display()));
    let psram = boards
        .get(&board)
        .and_then(|b| b.get("psram"))
        .and_then(toml::Value::as_bool)
        .unwrap_or_else(|| panic!("board `{board}` is not in firmware/boards.toml"));
    let mut displays = Vec::new();
    if std::env::var_os("CARGO_FEATURE_DISPLAY_TUI").is_some() {
        displays.push(FwDisplay::Tui);
    }
    if std::env::var_os("CARGO_FEATURE_DISPLAY_PANEL").is_some() {
        displays.push(FwDisplay::Panel);
    }
    let target = Target {
        board: &board,
        psram,
        displays: &displays,
    };
    // The example has no real network; placeholders stand in for unset values.
    let env = |name: &str| {
        std::env::var(name).ok().or_else(|| {
            (is_example && name.starts_with("WHETHER_WIFI_")).then(|| "example".to_string())
        })
    };
    match build(&text, &env, &target) {
        Ok(built) => {
            for var in &built.env_vars {
                println!("cargo:rerun-if-env-changed={var}");
            }
            for warning in &built.warnings {
                println!("cargo:warning={warning}");
            }
            let bytes = built.config.encode().expect("the configuration encodes");
            let out = std::path::Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR"))
                .join("config.bin");
            std::fs::write(&out, bytes).expect("writing config.bin");
        }
        Err(issues) => {
            for issue in &issues {
                println!("cargo:warning={}: {issue}", path.display());
            }
            let errors = issues
                .iter()
                .filter(|i| i.severity == fw_config::build::Severity::Error)
                .count();
            panic!(
                "{} has {errors} error(s); see the warnings above",
                path.display()
            );
        }
    }
}
