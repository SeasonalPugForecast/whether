//! `cargo xtask ios build [--debug]`: the Rust core for the iOS app.
//!
//! Builds `whether-ffi` as static libraries for the iPhone, the simulator and
//! Apple-silicon Macs (all cross-compile from Linux: a static library needs no
//! Apple linker), generates the Swift bindings, and stages everything in
//! `target/ios-stage`. On a Mac it then wraps the libraries in
//! `ios/WhetherKit/WhetherFFI.xcframework`; from Linux, `cargo xtask ios remote`
//! does that step on the Mac over SSH.

use std::path::{Path, PathBuf};

use crate::{run_cmd, workspace_root};

/// The iPhone, the simulator on Apple silicon, and the Mac (for `swift test`).
pub(crate) const TARGETS: [&str; 3] = [
    "aarch64-apple-ios",
    "aarch64-apple-ios-sim",
    "aarch64-apple-darwin",
];
/// The module the generated Swift imports.
const MODULE: &str = "whether_ffiFFI";
/// The library file in each target's stage directory.
pub(crate) const LIB: &str = "libwhether_ffi.a";
/// The generated Swift file.
pub(crate) const SWIFT: &str = "whether_ffi.swift";
/// Where the generated Swift goes in the package.
pub(crate) const SWIFT_DEST: &str = "ios/WhetherKit/Sources/WhetherKit";
/// The XCFramework, relative to the repository.
pub(crate) const XCFRAMEWORK: &str = "ios/WhetherKit/WhetherFFI.xcframework";

pub(crate) fn run(args: &[&str]) -> Result<(), String> {
    match args {
        ["build"] => build(true),
        ["build", "--debug"] => build(false),
        ["remote", rest @ ..] => crate::ios_remote::run(rest),
        ["licenses"] => crate::ios_licenses::run(),
        _ => Err(
            "usage: cargo xtask ios build [--debug]\n       cargo xtask ios licenses\n       cargo xtask ios remote doctor|sync|build|test|app|shots|device …"
                .into(),
        ),
    }
}

/// `target/ios-stage`: `<target>/libwhether_ffi.a`, `headers/` and the Swift file.
pub(crate) fn stage_dir() -> PathBuf {
    workspace_root().join("target/ios-stage")
}

/// Builds the static library for `target` into the stage.
fn static_lib(target: &str, release: bool) -> Result<(), String> {
    let mut args = vec![
        "rustc",
        "-p",
        "whether-ffi",
        "--lib",
        "--crate-type",
        "staticlib",
        "--target",
        target,
    ];
    if release {
        args.push("--release");
    }
    run_cmd("cargo", &args).map_err(|e| {
        format!(
            "{e}\n(missing target? run `rustup target add {}`)",
            TARGETS.join(" ")
        )
    })?;
    let profile = if release { "release" } else { "debug" };
    let built = workspace_root()
        .join("target")
        .join(target)
        .join(profile)
        .join(LIB);
    copy(&built, &stage_dir().join(target).join(LIB))
}

/// Generates the Swift bindings from the host library into the stage.
fn bindings() -> Result<(), String> {
    run_cmd("cargo", &["build", "-p", "whether-ffi", "--lib"])?;
    let library = workspace_root()
        .join("target/debug")
        .join(if cfg!(target_os = "macos") {
            "libwhether_ffi.dylib"
        } else {
            "libwhether_ffi.so"
        });
    let generated = workspace_root().join("target/ios-bindings");
    run_cmd(
        "cargo",
        &[
            "run",
            "-q",
            "-p",
            "whether-ffi",
            "--features",
            "bindgen",
            "--bin",
            "uniffi-bindgen",
            "--",
            "generate",
            "--library",
            &path(&library)?,
            "--language",
            "swift",
            "--out-dir",
            &path(&generated)?,
        ],
    )?;
    let stage = stage_dir();
    copy(&generated.join(SWIFT), &stage.join(SWIFT))?;
    let header = format!("{MODULE}.h");
    copy(
        &generated.join(&header),
        &stage.join("headers").join(&header),
    )?;
    copy(
        &generated.join(format!("{MODULE}.modulemap")),
        &stage.join("headers/module.modulemap"),
    )
}

pub(crate) fn path(p: &Path) -> Result<String, String> {
    p.to_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("{} is not UTF-8", p.display()))
}

fn copy(from: &Path, to: &Path) -> Result<(), String> {
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::copy(from, to)
        .map(|_| ())
        .map_err(|e| format!("copying {} to {}: {e}", from.display(), to.display()))
}

/// `xcodebuild` arguments that wrap the staged libraries (under `stage`, as the
/// Mac sees it) into `output`.
pub(crate) fn xcframework_args(stage: &str, output: &str) -> Vec<String> {
    let mut args = vec!["-create-xcframework".to_string()];
    for target in TARGETS {
        args.extend([
            "-library".into(),
            format!("{stage}/{target}/{LIB}"),
            "-headers".into(),
            format!("{stage}/headers"),
        ]);
    }
    args.extend(["-output".into(), output.into()]);
    args
}

/// Builds and stages everything; returns the stage directory.
pub(crate) fn stage(release: bool) -> Result<PathBuf, String> {
    let stage = stage_dir();
    if stage.exists() {
        std::fs::remove_dir_all(&stage).map_err(|e| format!("{}: {e}", stage.display()))?;
    }
    for target in TARGETS {
        static_lib(target, release)?;
    }
    bindings()?;
    copy(
        &stage.join(SWIFT),
        &workspace_root().join(SWIFT_DEST).join(SWIFT),
    )?;
    Ok(stage)
}

fn build(release: bool) -> Result<(), String> {
    let stage = stage(release)?;
    if !cfg!(target_os = "macos") {
        println!(
            "staged the libraries and bindings in {}; `cargo xtask ios remote build` makes the \
             XCFramework on the Mac",
            stage.display()
        );
        return Ok(());
    }
    let output = workspace_root().join(XCFRAMEWORK);
    if output.exists() {
        std::fs::remove_dir_all(&output).map_err(|e| format!("{}: {e}", output.display()))?;
    }
    let args = xcframework_args(&path(&stage)?, &path(&output)?);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    run_cmd("xcodebuild", &args)?;
    println!("wrote {}", output.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_xcframework_takes_every_slice_with_its_headers() {
        let args = xcframework_args(".stage", "out.xcframework");
        assert_eq!(args[0], "-create-xcframework");
        assert_eq!(args.iter().filter(|a| *a == "-library").count(), 3);
        assert!(args.contains(&".stage/aarch64-apple-ios-sim/libwhether_ffi.a".to_string()));
        assert_eq!(args.last().unwrap(), "out.xcframework");
    }

    /// The screenshot launch arguments exist only in debug builds: every
    /// `ProcessInfo.processInfo.arguments` in the app is inside `#if DEBUG`.
    #[test]
    fn launch_arguments_are_debug_only() {
        fn walk(dir: &Path, bad: &mut Vec<String>) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, bad);
                } else if path.extension().is_some_and(|e| e == "swift") {
                    let text = std::fs::read_to_string(&path).unwrap();
                    let mut depth: Vec<bool> = Vec::new(); // true: a `#if DEBUG` block
                    for (n, line) in text.lines().enumerate() {
                        let t = line.trim();
                        if t.starts_with("#if ") {
                            depth.push(t == "#if DEBUG");
                        } else if t == "#endif" {
                            depth.pop();
                        } else if t.starts_with("#else") || t.starts_with("#elseif") {
                            if let Some(last) = depth.last_mut() {
                                *last = false;
                            }
                        } else if line.contains("ProcessInfo.processInfo.arguments")
                            && !depth.iter().any(|d| *d)
                        {
                            bad.push(format!("{}:{}", path.display(), n + 1));
                        }
                    }
                }
            }
        }
        let mut bad = Vec::new();
        walk(&workspace_root().join("ios/Whether"), &mut bad);
        assert!(
            bad.is_empty(),
            "launch arguments outside #if DEBUG: {bad:?}"
        );
    }
}
