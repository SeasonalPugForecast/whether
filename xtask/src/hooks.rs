//! `cargo xtask hooks install`: a local pre-push gate while there is no hosted CI.

use std::fs;
use std::path::Path;

/// The pre-push hook body.
/// The pre-push hook body: the gate on the build host when one answers, else here.
pub(crate) const PRE_PUSH: &str = "#!/bin/sh\n# Installed by `cargo xtask hooks install`: the gate on a build host when one\n# answers (remote.local.toml), else here.\nexec cargo xtask remote ci --or-local\n";

/// Hooks earlier versions installed, replaced without `--force`.
const OLD_PRE_PUSH: &[&str] =
    &["#!/bin/sh\n# Installed by `cargo xtask hooks install`.\nexec cargo xtask ci\n"];

pub(crate) fn install(force: bool) -> Result<(), String> {
    let hooks = crate::workspace_root().join(".git").join("hooks");
    let path = write_hook(&hooks, force)?;
    println!("installed {}", path.display());
    Ok(())
}

/// Writes the pre-push hook into `hooks_dir`, refusing to replace a different
/// existing hook unless `force` is set.
pub(crate) fn write_hook(hooks_dir: &Path, force: bool) -> Result<std::path::PathBuf, String> {
    fs::create_dir_all(hooks_dir).map_err(|e| format!("{}: {e}", hooks_dir.display()))?;
    let path = hooks_dir.join("pre-push");
    if let Ok(existing) = fs::read_to_string(&path)
        && existing != PRE_PUSH
        && !OLD_PRE_PUSH.contains(&existing.as_str())
        && !force
    {
        return Err(format!(
            "{} already exists; rerun with --force to replace it",
            path.display()
        ));
    }
    fs::write(&path, PRE_PUSH).map_err(|e| format!("{}: {e}", path.display()))?;
    make_executable(&path)?;
    Ok(path)
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("xtask-hooks-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn installs_an_executable_hook() {
        let dir = temp_dir("fresh");
        let path = write_hook(&dir, false).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), PRE_PUSH);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o111,
                0o111
            );
        }
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn refuses_to_replace_a_foreign_hook_without_force() {
        let dir = temp_dir("foreign");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("pre-push"), "#!/bin/sh\necho mine\n").unwrap();
        assert!(write_hook(&dir, false).is_err());
        assert!(write_hook(&dir, true).is_ok());
        // An older hook of ours is replaced without --force.
        fs::write(dir.join("pre-push"), OLD_PRE_PUSH[0]).unwrap();
        assert!(write_hook(&dir, false).is_ok());
        assert!(
            fs::read_to_string(dir.join("pre-push"))
                .unwrap()
                .contains("remote ci --or-local")
        );
        assert_eq!(fs::read_to_string(dir.join("pre-push")).unwrap(), PRE_PUSH);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reinstalling_the_same_hook_is_allowed() {
        let dir = temp_dir("same");
        write_hook(&dir, false).unwrap();
        assert!(write_hook(&dir, false).is_ok());
        fs::remove_dir_all(dir).unwrap();
    }
}
