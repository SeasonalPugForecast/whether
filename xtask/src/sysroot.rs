//! `cargo xtask sysroot --from <user@host>`: copies a Raspberry Pi OS (Raspbian)
//! sysroot from a running Pi for ARMv6 builds (Pi Zero, Zero W, Pi 1). Debian's
//! armhf is ARMv7, so these boards need Raspbian's own C runtime and libraries;
//! the Pi itself has exactly those. The GUI's `-dev` packages are fetched on the Pi
//! with `apt-get download` (no root, nothing installed) and unpacked here.
//! `cargo xtask dist --target arm-unknown-linux-gnueabihf` then builds with clang
//! and lld against the copy, without Docker.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::run_cmd;

/// The Rust target for ARMv6 with hard float.
pub(crate) const TARGET: &str = "arm-unknown-linux-gnueabihf";

/// The Debian multiarch triple of the libraries.
const TRIPLE: &str = "arm-linux-gnueabihf";

/// Headers, `.pc` files and linker symlinks for what the kiosk build links
/// (fontconfig, xkbcommon, libinput, udev, gbm, drm) and their pkg-config
/// requirements.
const DEV_PACKAGES: [&str; 15] = [
    "libfontconfig-dev",
    "libxkbcommon-dev",
    "libinput-dev",
    "libudev-dev",
    "libgbm-dev",
    "libdrm-dev",
    "libfreetype-dev",
    "libexpat1-dev",
    "libpng-dev",
    "zlib1g-dev",
    "libbrotli-dev",
    "libbz2-dev",
    "libmtdev-dev",
    "libevdev-dev",
    "libwacom-dev",
];

/// Where the sysroot lives (`target/` is not committed).
pub(crate) fn dir() -> PathBuf {
    crate::workspace_root().join("target/sysroot").join(TARGET)
}

/// The environment for building [`TARGET`] against `sysroot` with clang and lld:
/// Rust code is ARMv6 by target, and C code gets the matching flags.
pub(crate) fn build_env(sysroot: &Path) -> Vec<(String, String)> {
    let sr = sysroot.display();
    let cflags = format!("--sysroot={sr} -march=armv6 -mfpu=vfp -mfloat-abi=hard -marm");
    let pkgconfig = format!("{sr}/usr/lib/{TRIPLE}/pkgconfig:{sr}/usr/share/pkgconfig");
    [
        (
            "CARGO_TARGET_ARM_UNKNOWN_LINUX_GNUEABIHF_LINKER",
            "clang".to_string(),
        ),
        (
            "CARGO_TARGET_ARM_UNKNOWN_LINUX_GNUEABIHF_RUSTFLAGS",
            format!(
                "-C link-arg=--target={TRIPLE} -C link-arg=--sysroot={sr} \
                 -C link-arg=-fuse-ld=lld -C link-arg=-march=armv6"
            ),
        ),
        ("CC_arm_unknown_linux_gnueabihf", "clang".to_string()),
        ("CXX_arm_unknown_linux_gnueabihf", "clang++".to_string()),
        ("AR_arm_unknown_linux_gnueabihf", "llvm-ar".to_string()),
        ("CFLAGS_arm_unknown_linux_gnueabihf", cflags.clone()),
        ("CXXFLAGS_arm_unknown_linux_gnueabihf", cflags),
        ("PKG_CONFIG_SYSROOT_DIR", sr.to_string()),
        ("PKG_CONFIG_LIBDIR", pkgconfig),
        ("PKG_CONFIG_ALLOW_CROSS", "1".to_string()),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect()
}

/// Runs `command` on `host` over SSH and returns its standard output.
fn ssh_output(host: &str, command: &str) -> Result<String, String> {
    println!("$ ssh {host} '{command}'");
    let out = Command::new("ssh")
        .args(["-o", "BatchMode=yes", host, command])
        .output()
        .map_err(|e| format!("failed to start ssh: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "ssh {host} failed ({}): {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The value of `key` in an os-release file.
fn os_release_value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines()
        .find_map(|l| l.strip_prefix(key)?.strip_prefix('='))
        .map(|v| v.trim_matches('"'))
}

/// Refuses hosts whose libraries are not Raspbian armhf.
fn check_host(os_release: &str, arch: &str) -> Result<(), String> {
    let id = os_release_value(os_release, "ID").unwrap_or_default();
    if arch != "armhf" || id != "raspbian" {
        return Err(format!(
            "the host runs `{id}` for `{arch}`; ARMv6 builds need Raspberry Pi OS 32-bit \
             (raspbian, armhf). Debian armhf is ARMv7: use --target armv7-unknown-linux-gnueabihf"
        ));
    }
    Ok(())
}

fn pull(host: &str) -> Result<(), String> {
    let os_release = ssh_output(host, "cat /etc/os-release")?;
    let arch = ssh_output(host, "dpkg --print-architecture")?;
    check_host(&os_release, &arch)?;
    let sr = dir();
    let lib = sr.join("usr/lib");
    let multiarch = lib.join(TRIPLE);
    std::fs::create_dir_all(&multiarch).map_err(|e| format!("{}: {e}", multiarch.display()))?;
    let path = |p: &Path| p.to_str().map(str::to_string).ok_or("non-UTF-8 path");
    let usr = format!("{}/", path(&sr.join("usr"))?);
    let lib_dir = format!("{}/", path(&lib)?);
    let multiarch_dir = format!("{}/", path(&multiarch)?);
    run_cmd("rsync", &["-a", &format!("{host}:/usr/include"), &usr])?;
    // The GCC runtime (crtbegin.o, libgcc) built for ARMv6.
    run_cmd(
        "rsync",
        &[
            "-a",
            "--include=gcc/",
            &format!("--include=gcc/{TRIPLE}/***"),
            "--exclude=*",
            &format!("{host}:/usr/lib/gcc"),
            &lib_dir,
        ],
    )?;
    // Top-level libraries and pkg-config files, without plugin directories.
    run_cmd(
        "rsync",
        &[
            "-a",
            "--include=pkgconfig/***",
            "--exclude=*/",
            &format!("{host}:/usr/lib/{TRIPLE}/"),
            &multiarch_dir,
        ],
    )?;
    run_cmd(
        "rsync",
        &[
            "-a",
            &format!("{host}:/usr/lib/ld-linux-armhf.so.3"),
            &lib_dir,
        ],
    )?;
    let lib_link = sr.join("lib");
    if lib_link.symlink_metadata().is_err() {
        std::os::unix::fs::symlink("usr/lib", &lib_link)
            .map_err(|e| format!("{}: {e}", lib_link.display()))?;
    }

    let remote = ssh_output(
        host,
        &format!(
            "d=$(mktemp -d) && cd \"$d\" && apt-get download {} >/dev/null && echo \"$d\"",
            DEV_PACKAGES.join(" ")
        ),
    )?;
    let debs = crate::workspace_root().join("target/sysroot/debs");
    let _ = std::fs::remove_dir_all(&debs);
    std::fs::create_dir_all(&debs).map_err(|e| format!("{}: {e}", debs.display()))?;
    let fetched = run_cmd(
        "rsync",
        &[
            "-a",
            &format!("{host}:{remote}/"),
            &format!("{}/", path(&debs)?),
        ],
    );
    ssh_output(host, &format!("rm -rf '{remote}'"))?;
    fetched?;
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&debs)
        .map_err(|e| format!("{}: {e}", debs.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "deb"))
        .collect();
    entries.sort();
    for deb in &entries {
        extract(deb, &sr)?;
    }
    let source = os_release_value(&os_release, "PRETTY_NAME").unwrap_or("unknown");
    std::fs::write(sr.join("SOURCE"), format!("{host}: {source}\n"))
        .map_err(|e| format!("{}: {e}", sr.display()))?;
    println!(
        "sysroot from {host} ({source}) with {} dev packages in {}",
        entries.len(),
        sr.display()
    );
    Ok(())
}

/// Unpacks a .deb's data archive into `root` (bsdtar reads the ar container).
fn extract(deb: &Path, root: &Path) -> Result<(), String> {
    let list = Command::new("bsdtar")
        .arg("-tf")
        .arg(deb)
        .output()
        .map_err(|e| format!("failed to start bsdtar: {e}"))?;
    let listing = String::from_utf8_lossy(&list.stdout);
    let data = listing
        .lines()
        .find(|l| l.starts_with("data.tar"))
        .ok_or_else(|| format!("{}: no data archive", deb.display()))?;
    let status = Command::new("sh")
        .arg("-c")
        .arg("bsdtar -xOf \"$1\" \"$2\" | bsdtar -xf - -C \"$3\"")
        .args(["sh"])
        .arg(deb)
        .arg(data)
        .arg(root)
        .status()
        .map_err(|e| format!("failed to start sh: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{}: unpacking failed ({status})", deb.display()))
    }
}

pub(crate) fn run(args: &[&str]) -> Result<(), String> {
    match args {
        ["--from", host] => pull(host),
        _ => Err("usage: cargo xtask sysroot --from <user@host>".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_build_targets_armv6_against_the_sysroot() {
        let env = build_env(Path::new("/sr"));
        let get = |k: &str| env.iter().find(|(key, _)| key == k).unwrap().1.clone();
        assert_eq!(
            get("CARGO_TARGET_ARM_UNKNOWN_LINUX_GNUEABIHF_LINKER"),
            "clang"
        );
        let rustflags = get("CARGO_TARGET_ARM_UNKNOWN_LINUX_GNUEABIHF_RUSTFLAGS");
        assert!(rustflags.contains("--sysroot=/sr") && rustflags.contains("-fuse-ld=lld"));
        assert!(get("CFLAGS_arm_unknown_linux_gnueabihf").contains("-march=armv6"));
        assert_eq!(get("PKG_CONFIG_SYSROOT_DIR"), "/sr");
    }

    #[test]
    fn only_raspbian_armhf_hosts_qualify() {
        let raspbian = "PRETTY_NAME=\"Raspbian GNU/Linux 13 (trixie)\"\nID=raspbian\n";
        assert_eq!(check_host(raspbian, "armhf"), Ok(()));
        assert!(
            check_host("ID=debian\n", "armhf").is_err(),
            "Debian armhf is ARMv7"
        );
        assert!(check_host(raspbian, "arm64").is_err());
        assert_eq!(
            os_release_value(raspbian, "PRETTY_NAME"),
            Some("Raspbian GNU/Linux 13 (trixie)")
        );
    }

    #[test]
    fn usage() {
        assert!(run(&[]).is_err() && run(&["--from"]).is_err());
    }
}
