//! `cargo xtask dist [--target T]…`: release tarballs with the binary, license,
//! example config, man page and shell completions. `cargo xtask dist --assets`
//! regenerates the man page and completions committed under `packaging/` for the
//! .deb and the PKGBUILD.

use std::fs;
use std::path::{Path, PathBuf};

use clap_complete::aot::{Shell, generate};

use crate::run_cmd;

/// Where `cross` builds, apart from native builds.
const CROSS_TARGET_DIR: &str = "target/cross";

/// The host target, built with plain `cargo`.
const NATIVE: &str = "x86_64-unknown-linux-gnu";

/// Feature set per target .
fn features(target: &str) -> Option<&'static [&'static str]> {
    match target {
        "x86_64-unknown-linux-gnu" => Some(&[]),
        "aarch64-unknown-linux-gnu"
        | "armv7-unknown-linux-gnueabihf"
        | "arm-unknown-linux-gnueabihf" => Some(&["--features", "tui,gui,kiosk,line"]),
        "x86_64-unknown-linux-musl" | "aarch64-unknown-linux-musl" => {
            Some(&["--no-default-features", "--features", "tui,line"])
        }
        _ => None,
    }
}

/// The man page (roff).
pub(crate) fn man_page() -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    clap_mangen::Man::new(whether::command())
        .render(&mut out)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

/// Completions for `shell`.
pub(crate) fn completions(shell: Shell) -> Vec<u8> {
    let mut cmd = whether::command();
    let mut out = Vec::new();
    generate(shell, &mut cmd, "whether", &mut out);
    out
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

fn copy(from: &Path, to: &Path) -> Result<(), String> {
    if let Some(dir) = to.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    fs::copy(from, to)
        .map(|_| ())
        .map_err(|e| format!("{}: {e}", from.display()))
}

/// Writes the man page and completions below `dir`, at the paths the packages use.
fn write_assets(dir: &Path) -> Result<(), String> {
    write(&dir.join("man/whether.1"), &man_page()?)?;
    for (shell, name) in COMPLETIONS {
        write(&dir.join("completions").join(name), &completions(shell))?;
    }
    Ok(())
}

/// Completion file names per shell.
const COMPLETIONS: [(Shell, &str); 3] = [
    (Shell::Bash, "whether.bash"),
    (Shell::Zsh, "_whether"),
    (Shell::Fish, "whether.fish"),
];

/// Built with plain cargo too: it needs only `rustup target add`. ring's C code is
/// freestanding, so the host gcc compiles it when no musl-gcc is installed.
const HOST_MUSL: &str = "x86_64-unknown-linux-musl";

fn build_host_musl(extra: &[&str]) -> Result<PathBuf, String> {
    let mut args = vec![
        "build",
        "--release",
        "--locked",
        "-p",
        "whether",
        "--target",
        HOST_MUSL,
    ];
    args.extend_from_slice(extra);
    println!("$ cargo {}", args.join(" "));
    let mut cmd = std::process::Command::new("cargo");
    cmd.args(&args).current_dir(crate::workspace_root());
    let cc = "CC_x86_64_unknown_linux_musl";
    if std::env::var_os(cc).is_none() {
        cmd.env(cc, "gcc");
    }
    let status = cmd
        .status()
        .map_err(|e| format!("failed to start cargo: {e}"))?;
    if !status.success() {
        return Err(format!(
            "the {HOST_MUSL} build failed ({status}); is the target installed? rustup target add {HOST_MUSL}"
        ));
    }
    Ok(crate::workspace_root().join(format!("target/{HOST_MUSL}/release/whether")))
}

/// ARMv6 (Pi Zero W) with plain cargo, clang and lld against the sysroot from
/// `cargo xtask sysroot` (Debian's armhf images are ARMv7).
fn build_with_sysroot(extra: &[&str]) -> Result<PathBuf, String> {
    let target = crate::sysroot::TARGET;
    let sysroot = crate::sysroot::dir();
    if !sysroot.join("usr/lib/arm-linux-gnueabihf/libc.so").exists() {
        return Err(format!(
            "`{target}` needs a Raspberry Pi OS sysroot in {}: cargo xtask sysroot --from <user@pi>",
            sysroot.display()
        ));
    }
    let mut args = vec![
        "build",
        "--release",
        "--locked",
        "-p",
        "whether",
        "--target",
        target,
    ];
    args.extend_from_slice(extra);
    println!("$ cargo {} (sysroot {})", args.join(" "), sysroot.display());
    let status = std::process::Command::new("cargo")
        .args(&args)
        .envs(crate::sysroot::build_env(&sysroot))
        .current_dir(crate::workspace_root())
        .status()
        .map_err(|e| format!("failed to start cargo: {e}"))?;
    if !status.success() {
        return Err(format!(
            "the {target} build failed ({status}); is the target installed? rustup target add {target}"
        ));
    }
    Ok(crate::workspace_root().join(format!("target/{target}/release/whether")))
}

/// Builds `whether-cross:<target>` from `packaging/cross/Dockerfile.<target>` when
/// one exists (Cross.toml names these images). Docker caches the layers, so this
/// is quick after the first time.
fn custom_image(target: &str) -> Result<(), String> {
    let dockerfile = format!("packaging/cross/Dockerfile.{target}");
    if !crate::workspace_root().join(&dockerfile).exists() {
        return Ok(());
    }
    let tag = format!("whether-cross:{target}");
    run_cmd(
        "docker",
        &[
            "build",
            "--tag",
            &tag,
            "--file",
            &dockerfile,
            "packaging/cross",
        ],
    )
}

fn build(target: &str) -> Result<PathBuf, String> {
    let extra = features(target).ok_or_else(|| format!("unsupported target `{target}`"))?;
    let root = crate::workspace_root();
    if target == NATIVE {
        let mut args = vec!["build", "--release", "--locked", "-p", "whether"];
        args.extend_from_slice(extra);
        run_cmd("cargo", &args)?;
        return Ok(root.join("target/release/whether"));
    }
    if target == HOST_MUSL {
        return build_host_musl(extra);
    }
    if target == crate::sysroot::TARGET {
        return build_with_sysroot(extra);
    }
    if std::process::Command::new("cross")
        .arg("--version")
        .output()
        .is_err()
    {
        return Err(format!(
            "`{target}` needs cross: cargo install cross --git https://github.com/cross-rs/cross (and Docker)"
        ));
    }
    custom_image(target)?;
    // Host artifacts (proc macros, build scripts) are built inside the container;
    // sharing target/ with native builds would reuse host-built ones it cannot load.
    let mut args = vec![
        "build",
        "--release",
        "--locked",
        "-p",
        "whether",
        "--target",
        target,
        "--target-dir",
        CROSS_TARGET_DIR,
    ];
    args.extend_from_slice(extra);
    run_cmd("cross", &args)?;
    Ok(root.join(format!("{CROSS_TARGET_DIR}/{target}/release/whether")))
}

fn package(target: &str) -> Result<PathBuf, String> {
    let root = crate::workspace_root();
    let binary = build(target)?;
    let name = format!("whether-{}-{target}", env!("CARGO_PKG_VERSION"));
    let stage = root.join("dist").join(&name);
    if stage.exists() {
        fs::remove_dir_all(&stage).map_err(|e| format!("{}: {e}", stage.display()))?;
    }
    copy(&binary, &stage.join("whether"))?;
    copy(&root.join("LICENSE"), &stage.join("LICENSE"))?;
    copy(
        &root.join("docs/config/whether.example.toml"),
        &stage.join("whether.example.toml"),
    )?;
    // README.md is copied when present.
    let readme = root.join("README.md");
    if readme.exists() {
        copy(&readme, &stage.join("README.md"))?;
    }
    write_assets(&stage)?;
    let tarball = root.join("dist").join(format!("{name}.tar.gz"));
    let dist = root.join("dist");
    run_cmd(
        "tar",
        &[
            "czf",
            tarball.to_str().ok_or("path")?,
            "-C",
            dist.to_str().ok_or("path")?,
            &name,
        ],
    )?;
    Ok(tarball)
}

pub(crate) fn run(args: &[&str]) -> Result<(), String> {
    let mut targets = Vec::new();
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match *arg {
            "--target" => targets.push(*rest.next().ok_or("--target needs a value")?),
            "--assets" => {
                let dir = crate::workspace_root().join("packaging");
                write_assets(&dir)?;
                println!("wrote the man page and completions under {}", dir.display());
                return Ok(());
            }
            other => {
                return Err(format!(
                    "unknown argument `{other}`; usage: cargo xtask dist [--target T]… | --assets"
                ));
            }
        }
    }
    if targets.is_empty() {
        targets.push(NATIVE);
    }
    for target in targets {
        let tarball = package(target)?;
        println!("wrote {}", tarball.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn man_page_and_completions() {
        let man = String::from_utf8(man_page().unwrap()).unwrap();
        assert!(
            man.contains(".TH whether") && man.contains("config"),
            "{man}"
        );
        let fish = String::from_utf8(completions(Shell::Fish)).unwrap();
        assert!(fish.contains("complete -c whether"), "{fish}");
        assert!(!completions(Shell::Bash).is_empty() && !completions(Shell::Zsh).is_empty());
    }

    #[test]
    fn packaged_assets_are_current() {
        let dir = crate::workspace_root().join("packaging");
        let stale = "is stale; run `cargo xtask dist --assets`";
        let man = fs::read(dir.join("man/whether.1")).unwrap_or_default();
        assert!(
            man == man_page().unwrap(),
            "packaging/man/whether.1 {stale}"
        );
        for (shell, name) in COMPLETIONS {
            let current = fs::read(dir.join("completions").join(name)).unwrap_or_default();
            assert!(
                current == completions(shell),
                "packaging/completions/{name} {stale}"
            );
        }
    }

    /// Every `[package.metadata.deb]` asset (base and variants) exists, and each
    /// copyright file carries the full MIT license.
    #[test]
    fn deb_assets_exist() {
        let root = crate::workspace_root();
        let app = root.join("apps/whether");
        let manifest: toml::Table =
            toml::from_str(&fs::read_to_string(app.join("Cargo.toml")).unwrap()).unwrap();
        let deb = &manifest["package"]["metadata"]["deb"];
        let mut tables = vec![deb];
        tables.extend(deb["variants"].as_table().unwrap().values());
        let license = fs::read_to_string(root.join("LICENSE")).unwrap();
        for table in tables {
            for asset in table["assets"].as_array().unwrap() {
                let src = asset[0].as_str().unwrap();
                if !src.starts_with("target/") {
                    assert!(app.join(src).exists(), "{src}");
                }
            }
            if let Some(file) = table.get("license-file") {
                let text = fs::read_to_string(app.join(file[0].as_str().unwrap())).unwrap();
                assert!(text.ends_with(&license) && text.contains("Slint"));
            }
            if let Some(dir) = table.get("maintainer-scripts") {
                assert!(app.join(dir.as_str().unwrap()).join("postinst").exists());
            }
        }
    }

    /// The PKGBUILD installs files that exist, and it and .SRCINFO carry the
    /// workspace version.
    #[test]
    fn pkgbuild_matches_the_tree() {
        let root = crate::workspace_root();
        let arch = root.join("packaging/arch");
        let pkgbuild = fs::read_to_string(arch.join("PKGBUILD")).unwrap();
        let srcinfo = fs::read_to_string(arch.join(".SRCINFO")).unwrap();
        let version = env!("CARGO_PKG_VERSION");
        assert!(pkgbuild.contains(&format!("\npkgver={version}\n")));
        assert!(srcinfo.contains(&format!("\tpkgver = {version}\n")));
        let installed: Vec<&str> = pkgbuild
            .split_whitespace()
            .filter(|w| w.starts_with("packaging/") || w.starts_with("docs/") || *w == "LICENSE")
            .collect();
        assert!(installed.len() >= 8, "{installed:?}");
        for path in installed {
            assert!(root.join(path).exists(), "{path}");
        }
        let icons = pkgbuild
            .lines()
            .find_map(|l| l.trim().strip_prefix("for icon in "))
            .unwrap();
        let icons: Vec<&str> = icons
            .split(';')
            .next()
            .unwrap()
            .split_whitespace()
            .collect();
        assert_eq!(icons.len(), 3);
        for icon in icons {
            assert!(
                root.join("packaging/icons/hicolor").join(icon).exists(),
                "{icon}"
            );
        }
    }

    #[test]
    fn cross_images_match_cross_toml() {
        let root = crate::workspace_root();
        let cross: toml::Table =
            toml::from_str(&fs::read_to_string(root.join("Cross.toml")).unwrap()).unwrap();
        for (target, table) in cross["target"].as_table().unwrap() {
            assert!(features(target).is_some(), "{target}");
            assert_eq!(
                table["image"].as_str(),
                Some(format!("whether-cross:{target}").as_str())
            );
            assert!(
                root.join(format!("packaging/cross/Dockerfile.{target}"))
                    .exists()
            );
        }
    }

    #[test]
    fn targets_and_arguments() {
        assert_eq!(features("x86_64-unknown-linux-gnu"), Some(&[][..]));
        assert!(
            features("aarch64-unknown-linux-musl")
                .unwrap()
                .contains(&"--no-default-features")
        );
        assert!(
            features(crate::sysroot::TARGET)
                .unwrap()
                .contains(&"tui,gui,kiosk,line"),
            "the Pi Zero W gets the kiosk build"
        );
        assert!(features("sparc64-sun-solaris").is_none());
        assert!(run(&["--frob"]).is_err());
        assert!(run(&["--target"]).is_err());
    }
}
