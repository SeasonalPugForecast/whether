//! `cargo xtask ci`: the gate every commit must pass.

use crate::{nostd, run_cmd};

/// Extra feature sets that clippy and the tests check besides the defaults. `desktop` and `mcu`
/// are mutually exclusive, so `--all-features` is never used.
pub(crate) const FEATURE_SETS: &[(&str, &[&str])] = &[
    ("whether-core", &["--features", "serde,testdata"]),
    ("whether-core", &["--features", "schema"]),
    ("whether-config", &["--features", "schema"]),
    ("whether-providers", &["--features", "serde"]),
    ("whether-tui", &["--features", "schema"]),
    (
        "whether-gui",
        &["--no-default-features", "--features", "kiosk"],
    ),
    ("whether-plugin-proto", &["--features", "examples"]),
    // Needs SDL2 (sdl2-compat on Arch).
    ("whether-sim", &["--features", "window"]),
];

/// Slint files the MCU build compiles: `Path` does not build without std in Slint
/// 1.18, so none of them may use it.
const MCU_UI_FILES: &[&str] = &[
    "crates/whether-gui/ui/mcu.slint",
    "crates/whether-gui/ui/compact.slint",
    "crates/whether-gui/ui/components/shared.slint",
    "crates/whether-gui/ui/components/theme.slint",
    "crates/whether-gui/ui/components/structs.slint",
    "crates/whether-gui/ui/icons-png/icons.slint",
];

/// Lines (outside `//` comments) that mention `Path`, as `file:line`.
fn path_uses(name: &str, text: &str) -> Vec<String> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| {
            let code = line.split("//").next().unwrap_or("");
            code.contains("Path")
        })
        .map(|(i, _)| format!("{name}:{}", i + 1))
        .collect()
}

fn check_mcu_ui() -> Result<(), String> {
    let root = crate::workspace_root();
    let mut found = Vec::new();
    for file in MCU_UI_FILES {
        let text = std::fs::read_to_string(root.join(file)).map_err(|e| format!("{file}: {e}"))?;
        found.extend(path_uses(file, &text));
    }
    if found.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "`Path` in Slint files shared with the MCU build: {}",
            found.join(", ")
        ))
    }
}

pub(crate) fn run() -> Result<(), String> {
    run_cmd("cargo", &["fmt", "--all", "--check"])?;
    check_mcu_ui()?;
    run_cmd(
        "cargo",
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    )?;
    for (package, extra) in FEATURE_SETS {
        let mut args = vec!["clippy", "--package", package, "--all-targets"];
        args.extend_from_slice(extra);
        args.extend_from_slice(&["--", "-D", "warnings"]);
        run_cmd("cargo", &args)?;
    }
    run_cmd("cargo", &["test", "--workspace"])?;
    for (package, extra) in FEATURE_SETS {
        let mut args = vec!["test", "--package", package];
        args.extend_from_slice(extra);
        run_cmd("cargo", &args)?;
    }
    nostd::run()?;
    // The SDK tests read the protocol examples, so they run with every change.
    crate::cpp::build_and_test("dev")?;
    crate::fw::check()?;
    if cargo_subcommand_available("deny") {
        run_cmd("cargo", &["deny", "check"])?;
    } else {
        println!("note: cargo-deny is not installed; skipping `cargo deny check`");
    }
    println!("ci: all checks passed");
    Ok(())
}

fn cargo_subcommand_available(name: &str) -> bool {
    std::process::Command::new("cargo")
        .args([name, "--version"])
        .output()
        .is_ok_and(|o| o.status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_sets_never_use_all_features() {
        for (_, extra) in FEATURE_SETS {
            assert!(!extra.contains(&"--all-features"));
        }
    }

    #[test]
    fn finds_path_outside_comments() {
        let text = "// no `Path` here\nRectangle {}\nPath { commands: \"M 0 0\"; }\n";
        assert_eq!(path_uses("x.slint", text), vec!["x.slint:3".to_string()]);
    }

    #[test]
    fn mcu_ui_files_are_path_free() {
        check_mcu_ui().unwrap();
    }

    #[test]
    fn unknown_cargo_subcommands_are_unavailable() {
        assert!(!cargo_subcommand_available(
            "definitely-not-a-cargo-subcommand"
        ));
    }
}
