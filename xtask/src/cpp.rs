//! `cargo xtask cpp [--preset P]`: configures, builds and tests the `C++23` plugin
//! SDK in `sdk/cpp` with a `CMake` preset (`dev` by default: GCC, `ASan` + `UBSan`).

use std::path::Path;
use std::process::Command;

/// The compiler each preset uses.
fn compiler(preset: &str) -> Option<&'static str> {
    match preset {
        "dev" => Some("g++"),
        "clang-dev" => Some("clang++"),
        "release" => Some("c++"),
        _ => None,
    }
}

fn found(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Runs `program args…` in `dir`, streaming output.
fn run_in(dir: &Path, program: &str, args: &[&str]) -> Result<(), String> {
    println!("$ (cd {}) {program} {}", dir.display(), args.join(" "));
    let status = Command::new(program)
        .args(args)
        .current_dir(dir)
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

/// Builds and tests the SDK; skips with a notice when `CMake` or the preset's
/// compiler is missing.
pub(crate) fn build_and_test(preset: &str) -> Result<(), String> {
    let cxx = compiler(preset)
        .ok_or_else(|| format!("unknown preset `{preset}`; use dev, clang-dev or release"))?;
    if !found("cmake") || !found(cxx) {
        println!("note: cmake or {cxx} is missing; skipping the C++ SDK ({preset})");
        return Ok(());
    }
    let dir = crate::workspace_root().join("sdk/cpp");
    run_in(&dir, "cmake", &["--preset", preset])?;
    run_in(&dir, "cmake", &["--build", "--preset", preset])?;
    run_in(&dir, "ctest", &["--preset", preset])
}

pub(crate) fn run(args: &[&str]) -> Result<(), String> {
    match args {
        [] => build_and_test("dev"),
        ["--preset", preset] => build_and_test(preset),
        _ => Err("usage: cargo xtask cpp [--preset dev|clang-dev|release]".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_and_arguments() {
        assert_eq!(compiler("dev"), Some("g++"));
        assert_eq!(compiler("clang-dev"), Some("clang++"));
        assert!(compiler("msvc").is_none());
        assert!(run(&["--frob"]).is_err());
        assert!(run(&["--preset", "msvc"]).is_err());
    }
}
