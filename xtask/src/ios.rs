//! `cargo xtask ios build [--debug]`: the Rust core for the iOS app.
//!
//! Builds `whether-ffi` as static libraries for the iPhone, the simulator and
//! Apple-silicon Macs (all cross-compile from Linux: a static library needs no
//! Apple linker), generates the Swift bindings, and stages everything in
//! `target/ios-stage`. On a Mac it then wraps the libraries in
//! `ios/WhetherKit/WhetherFFI.xcframework`; from Linux, `cargo xtask ios remote`
//! does that step on the Mac over SSH.

use std::path::{Path, PathBuf};

use crate::{run_cmd, run_cmd_env, workspace_root};

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

/// The directories of the machine that builds. The compiler records them in panic locations and
/// debug info, so without a remap they end up in the shipped library, home directory included.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct BuildDirs {
    home: PathBuf,
    cargo_home: PathBuf,
    rustup_home: PathBuf,
    root: PathBuf,
}

impl BuildDirs {
    /// This machine's, or `None` without a `HOME`.
    fn from_env() -> Option<Self> {
        let home = PathBuf::from(std::env::var_os("HOME")?);
        let under_home = |var: &str, dir: &str| {
            std::env::var_os(var).map_or_else(|| home.join(dir), PathBuf::from)
        };
        Some(Self {
            cargo_home: under_home("CARGO_HOME", ".cargo"),
            rustup_home: under_home("RUSTUP_HOME", ".rustup"),
            root: workspace_root(),
            home,
        })
    }

    /// `--remap-path-prefix` flags, the general directory first: rustc uses the last match.
    fn remap_flags(&self) -> Vec<String> {
        [
            (&self.home, "/build/home"),
            (&self.cargo_home, "/cargo"),
            (&self.rustup_home, "/rustup"),
            (&self.root, "/whether"),
        ]
        .map(|(dir, to)| format!("--remap-path-prefix={}={to}", dir.display()))
        .to_vec()
    }

    /// What must not appear in a library built with [`Self::remap_flags`].
    fn needles(&self) -> Vec<String> {
        [&self.home, &self.cargo_home, &self.rustup_home, &self.root]
            .iter()
            .map(|dir| dir.display().to_string())
            // A home of `/` or `/root` would match too much or too little to mean anything.
            .filter(|dir| dir.len() > 5)
            .collect()
    }
}

/// `CARGO_ENCODED_RUSTFLAGS` (flags separated by U+001F) holding the flags already set in the
/// environment (`CARGO_ENCODED_RUSTFLAGS` wins over `RUSTFLAGS`, as in cargo) and then `ours`.
fn encoded_rustflags(encoded: Option<&str>, plain: Option<&str>, ours: &[String]) -> String {
    let mut all: Vec<&str> = match (encoded, plain) {
        (Some(encoded), _) => encoded.split('\u{1f}').collect(),
        (None, Some(plain)) => plain.split_whitespace().collect(),
        (None, None) => Vec::new(),
    };
    all.retain(|flag| !flag.is_empty());
    all.extend(ours.iter().map(String::as_str));
    all.join("\u{1f}")
}

/// Which of `needles` occur in `bytes`.
fn found_in<'a>(bytes: &[u8], needles: &'a [String]) -> Vec<&'a str> {
    needles
        .iter()
        .filter(|needle| {
            let needle = needle.as_bytes();
            !needle.is_empty() && bytes.windows(needle.len()).any(|window| window == needle)
        })
        .map(String::as_str)
        .collect()
}

/// The `/home/<name>` and `/Users/<name>` paths in `bytes`, once each: a build machine's home
/// directory as it ends up in a binary. `/Users/Shared` is the Mac's shared folder, not a person.
fn home_paths(bytes: &[u8]) -> Vec<String> {
    let mut found = Vec::new();
    for prefix in ["/home/", "/Users/"] {
        let prefix = prefix.as_bytes();
        let mut from = 0;
        while let Some(at) = bytes[from..]
            .windows(prefix.len())
            .position(|window| window == prefix)
        {
            let start = from + at + prefix.len();
            let name: String = bytes[start..]
                .iter()
                .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
                .map(|&b| char::from(b))
                .collect();
            if !name.is_empty() && name != "Shared" {
                let path = format!("{}{name}", String::from_utf8_lossy(prefix));
                if !found.contains(&path) {
                    found.push(path);
                }
            }
            from = start;
        }
    }
    found
}

/// Fails when `file` holds the build machine's own directories.
fn check_library(file: &Path, dirs: &BuildDirs) -> Result<(), String> {
    let bytes = std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let needles = dirs.needles();
    let leaked = found_in(&bytes, &needles);
    if leaked.is_empty() {
        return Ok(());
    }
    Err(format!(
        "{} still holds the build machine's paths ({}): the path remap did not cover them",
        file.display(),
        leaked.join(", ")
    ))
}

/// The binaries of an archived app that ship to people, relative to the `.xcarchive`.
const ARCHIVE_BINARIES: [&str; 2] = [
    "Products/Applications/Whether.app/Whether",
    "Products/Applications/Whether.app/PlugIns/WhetherWidgetExtension.appex/WhetherWidgetExtension",
];

/// Fails when an archived app or its widget holds a `/home/<name>` or `/Users/<name>` path.
pub(crate) fn check_archive(archive: &Path) -> Result<(), String> {
    let mut problems = Vec::new();
    for binary in ARCHIVE_BINARIES {
        let file = archive.join(binary);
        let bytes = std::fs::read(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        let paths = home_paths(&bytes);
        if !paths.is_empty() {
            problems.push(format!("{binary}: {}", paths.join(", ")));
        }
    }
    if problems.is_empty() {
        return Ok(());
    }
    Err(format!(
        "the archive holds home-directory paths:\n  {}",
        problems.join("\n  ")
    ))
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
    let dirs = BuildDirs::from_env();
    let flags = dirs.as_ref().map(|dirs| {
        encoded_rustflags(
            std::env::var("CARGO_ENCODED_RUSTFLAGS").ok().as_deref(),
            std::env::var("RUSTFLAGS").ok().as_deref(),
            &dirs.remap_flags(),
        )
    });
    let envs: Vec<(&str, &str)> = flags
        .iter()
        .map(|flags| ("CARGO_ENCODED_RUSTFLAGS", flags.as_str()))
        .collect();
    run_cmd_env("cargo", &args, &envs).map_err(|e| {
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
    let staged = stage_dir().join(target).join(LIB);
    copy(&built, &staged)?;
    match &dirs {
        Some(dirs) => check_library(&staged, dirs),
        None => Ok(()),
    }
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

    fn dirs() -> BuildDirs {
        BuildDirs {
            home: "/home/ann".into(),
            cargo_home: "/home/ann/.cargo".into(),
            rustup_home: "/data/rustup".into(),
            root: "/home/ann/src/whether".into(),
        }
    }

    #[test]
    fn the_remap_puts_the_general_home_first_so_the_specific_dirs_win() {
        assert_eq!(
            dirs().remap_flags(),
            [
                "--remap-path-prefix=/home/ann=/build/home",
                "--remap-path-prefix=/home/ann/.cargo=/cargo",
                "--remap-path-prefix=/data/rustup=/rustup",
                "--remap-path-prefix=/home/ann/src/whether=/whether",
            ]
        );
    }

    #[test]
    fn flags_already_set_are_kept_before_ours() {
        let ours = ["--remap-path-prefix=/a=/b".to_string()];
        assert_eq!(
            encoded_rustflags(None, None, &ours),
            "--remap-path-prefix=/a=/b"
        );
        assert_eq!(
            encoded_rustflags(None, Some("-C opt-level=1  -Dwarnings"), &ours),
            "-C\u{1f}opt-level=1\u{1f}-Dwarnings\u{1f}--remap-path-prefix=/a=/b"
        );
        // Like cargo, the encoded variable wins over the plain one.
        assert_eq!(
            encoded_rustflags(Some("-Cx\u{1f}-Cy"), Some("-Cplain"), &ours),
            "-Cx\u{1f}-Cy\u{1f}--remap-path-prefix=/a=/b"
        );
        assert_eq!(encoded_rustflags(Some(""), None, &[]), "");
    }

    #[test]
    fn a_library_with_the_builders_paths_is_found_out() {
        let needles = dirs().needles();
        assert_eq!(needles.len(), 4);
        let clean =
            b"\0panicked at /cargo/registry/src/x/lib.rs:1\0/rustup/toolchains/a\0".to_vec();
        assert_eq!(found_in(&clean, &needles), Vec::<&str>::new());
        let leaky = b"\0/home/ann/.cargo/registry/src/x/lib.rs\0".to_vec();
        assert_eq!(
            found_in(&leaky, &needles),
            ["/home/ann", "/home/ann/.cargo"]
        );
        assert_eq!(found_in(b"", &needles), Vec::<&str>::new());
    }

    #[test]
    fn a_tiny_home_is_no_needle() {
        let tiny = BuildDirs {
            home: "/".into(),
            cargo_home: "/.cargo".into(),
            rustup_home: "/root".into(),
            root: "/w".into(),
        };
        assert_eq!(tiny.needles(), ["/.cargo"]);
    }

    #[test]
    fn home_paths_are_found_once_each_and_the_shared_folder_is_not_a_person() {
        let bytes =
            b"x/home/ann/.cargo/a\0/home/ann/b\0/Users/bob/src\0/Users/Shared/w\0/home/\0/home//x";
        assert_eq!(home_paths(bytes), ["/home/ann", "/Users/bob"]);
        assert_eq!(
            home_paths(b"/cargo/registry /rustup/toolchains /whether/crates"),
            Vec::<String>::new()
        );
        assert_eq!(home_paths(b""), Vec::<String>::new());
    }

    #[test]
    fn an_archive_is_clean_only_when_every_shipped_binary_is() {
        let dir = tempfile::tempdir().unwrap();
        for binary in ARCHIVE_BINARIES {
            let file = dir.path().join(binary);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(&file, b"\0/cargo/registry/src/x\0").unwrap();
        }
        assert_eq!(check_archive(dir.path()), Ok(()));
        std::fs::write(dir.path().join(ARCHIVE_BINARIES[1]), b"/home/ann/.cargo/x").unwrap();
        let error = check_archive(dir.path()).unwrap_err();
        assert!(
            error.contains("WhetherWidgetExtension: /home/ann"),
            "{error}"
        );
        assert!(
            !error.contains("Products/Applications/Whether.app/Whether:"),
            "{error}"
        );
        // A binary that is not there is an error, not a pass.
        std::fs::remove_file(dir.path().join(ARCHIVE_BINARIES[0])).unwrap();
        assert!(check_archive(dir.path()).is_err());
    }

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

    /// Handing a tuple that holds a `String` back from a task-group child crashed optimized
    /// builds (`EXC_BAD_ACCESS` in `TaskGroup.offer`), so group children return one plain value.
    #[test]
    fn task_groups_hand_back_no_tuples() {
        fn walk(dir: &Path, bad: &mut Vec<String>) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, bad);
                } else if path.extension().is_some_and(|e| e == "swift") {
                    let text = std::fs::read_to_string(&path).unwrap();
                    for (n, line) in text.lines().enumerate() {
                        let squeezed: String = line.split_whitespace().collect();
                        if squeezed.contains("TaskGroup(of:(") {
                            bad.push(format!("{}:{}", path.display(), n + 1));
                        }
                    }
                }
            }
        }
        let mut bad = Vec::new();
        walk(&workspace_root().join("ios"), &mut bad);
        assert!(bad.is_empty(), "task groups returning tuples: {bad:?}");
    }
}
