//! `cargo xtask nostd`: proves the shared crates build without `std`.

use crate::run_cmd;

/// The bare-metal target used for `no_std` checks (ESP32-C6 class RISC-V).
pub(crate) const TARGET: &str = "riscv32imac-unknown-none-elf";

/// `(package, extra cargo args)` pairs checked for [`TARGET`].
pub(crate) const CHECKS: &[(&str, &[&str])] = &[
    ("whether-core", &["--no-default-features"]),
    (
        "whether-core",
        &["--no-default-features", "--features", "serde"],
    ),
    ("whether-providers", &["--no-default-features"]),
    ("whether-config", &["--no-default-features"]),
    ("whether-tui", &["--no-default-features"]),
    (
        "whether-gui",
        &["--no-default-features", "--features", "mcu"],
    ),
    ("whether-panel", &[]),
];

/// Cargo arguments for one check.
pub(crate) fn check_args<'a>(package: &'a str, extra: &[&'a str]) -> Vec<&'a str> {
    let mut args = vec!["check", "--target", TARGET, "--package", package];
    args.extend_from_slice(extra);
    args
}

pub(crate) fn run() -> Result<(), String> {
    for (package, extra) in CHECKS {
        run_cmd("cargo", &check_args(package, extra))?;
        println!("ok: {package} {}", extra.join(" "));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_args_target_the_bare_metal_triple() {
        let args = check_args("whether-core", &["--no-default-features"]);
        assert_eq!(
            args,
            [
                "check",
                "--target",
                TARGET,
                "--package",
                "whether-core",
                "--no-default-features"
            ]
        );
    }

    #[test]
    fn every_check_names_a_workspace_crate() {
        let root = crate::workspace_root();
        for (package, _) in CHECKS {
            assert!(
                root.join("crates").join(package).is_dir(),
                "{package} missing"
            );
        }
    }
}
