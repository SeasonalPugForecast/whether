//! Lanes: several git worktrees building on the same remote hosts at once.
//!
//! `cargo xtask remote` and `cargo xtask ios remote` copy the working tree into one
//! directory per host, replacing it each time. Two worktrees using one directory
//! would wipe each other's trees, so a linked worktree gets a **lane**: its name
//! (the worktree directory's, lower-cased, reduced to `[a-z0-9-]`) is appended to
//! the host directory (`whether-build-lane-a`), and to the Mac's simulator
//! (`iPhone 17 (lane-a)`). The main worktree has no lane and keeps every directory
//! and simulator it had before. `WHETHER_LANE` overrides the name (an empty value
//! means no lane).
//!
//! This module also holds what lanes need in common: finding the gitignored config
//! files in the main worktree when the lane has none, the lock that makes two runs
//! in one directory queue up instead of corrupting each other, and the commands
//! that clean a lane up.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::ios_remote::quote;
use crate::remote::posix;
use crate::workspace_root;

/// How long to wait between looks at a lock somebody else holds.
const RETRY: Duration = Duration::from_secs(10);
/// A lock older than this many minutes is taken over (3 hours).
const STALE_MINUTES: u32 = 180;

/// `text` lower-cased with everything outside `[a-z0-9-]` replaced by `-`,
/// without leading, trailing or repeated `-`. `None` when nothing is left.
fn sanitize(text: &str) -> Option<String> {
    let mapped: String = text
        .to_ascii_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() {
                c
            } else {
                '-'
            }
        })
        .collect();
    let parts: Vec<&str> = mapped.split('-').filter(|p| !p.is_empty()).collect();
    (!parts.is_empty()).then(|| parts.join("-"))
}

/// The lane for a checkout. `env` reads environment variables; `git` runs
/// `git rev-parse --path-format=absolute <flag>` in the checkout and gives its
/// output; `root` is the checkout.
fn lane_with(
    env: &dyn Fn(&str) -> Option<String>,
    git: &dyn Fn(&str) -> Option<String>,
    root: &Path,
) -> Option<String> {
    if let Some(value) = env("WHETHER_LANE") {
        let value = value.trim();
        // An empty value switches lanes off; a name nothing is left of still
        // means "a lane", so a typo cannot land in the main directory.
        return if value.is_empty() {
            None
        } else {
            Some(sanitize(value).unwrap_or_else(|| "lane".to_string()))
        };
    }
    let (dir, common) = (git("--git-dir")?, git("--git-common-dir")?);
    if dir == common {
        return None;
    }
    let name = root.file_name()?.to_string_lossy();
    Some(sanitize(&name).unwrap_or_else(|| "lane".to_string()))
}

/// The main worktree's root: the parent of the git common directory (`…/.git`).
fn main_root_with(git: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let common = PathBuf::from(git("--git-common-dir")?);
    (common.file_name()? == ".git")
        .then(|| common.parent().map(Path::to_path_buf))
        .flatten()
}

/// `git rev-parse --path-format=absolute <flag>` in the workspace.
fn git_rev_parse(flag: &str) -> Option<String> {
    let out = Command::new("git")
        .args(["rev-parse", "--path-format=absolute", flag])
        .current_dir(workspace_root())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let text = String::from_utf8(out.stdout).ok()?;
    (out.status.success() && !text.trim().is_empty()).then(|| text.trim().to_string())
}

/// This checkout's lane: `None` in the main worktree (and outside git).
pub(crate) fn lane() -> Option<String> {
    lane_with(
        &|k| std::env::var(k).ok(),
        &git_rev_parse,
        &workspace_root(),
    )
}

/// The main worktree's root, when this is a git checkout.
pub(crate) fn main_root() -> Option<PathBuf> {
    main_root_with(&git_rev_parse)
}

/// `name` (relative to the workspace) in `root`, else in `main`.
fn find_config(name: &str, root: &Path, main: Option<&Path>) -> Option<PathBuf> {
    [Some(root), main]
        .into_iter()
        .flatten()
        .map(|dir| dir.join(name))
        .find(|path| path.is_file())
}

/// The gitignored config `name`: this workspace's, else the main worktree's.
pub(crate) fn config_file(name: &str) -> Option<PathBuf> {
    find_config(name, &workspace_root(), main_root().as_deref())
}

/// `dir` for `lane`: the base with `-<lane>` appended.
pub(crate) fn lane_dir(base: &str, lane: &str) -> String {
    format!("{}-{lane}", base.trim_end_matches('/'))
}

/// A lane name given to `clean --lane`, which must already be in canonical form.
pub(crate) fn clean_lane(name: &str) -> Result<String, String> {
    match sanitize(name) {
        Some(lane) if lane == name => Ok(lane),
        _ => Err(format!(
            "`{name}` is not a lane name (lower-case letters, digits and `-`, e.g. whether-lane-a)"
        )),
    }
}

/// Which lanes `clean` removes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Which {
    One(String),
    All,
}

/// Reads `--lane <name>` / `--all-lanes` into a [`Which`].
pub(crate) fn which(lane: Option<&str>, all: bool) -> Result<Which, String> {
    match (lane, all) {
        (Some(_), true) => Err("--lane and --all-lanes exclude each other".into()),
        (Some(name), false) => clean_lane(name).map(Which::One),
        (None, true) => Ok(Which::All),
        (None, false) => Err("clean needs --lane <name> or --all-lanes".into()),
    }
}

/// An sh line that removes the lane directory `dir` (and its lock) if, and only
/// if, it carries `marker`. `all`: every `<base>-*` directory with the marker.
pub(crate) fn remove_dirs(base: &str, which: &Which, marker: &str) -> String {
    let base = base.trim_end_matches('/');
    let guarded = |d: &str, lock: &str| {
        format!(
            "if [ -f {d}/{marker} ]; then rm -rf {d} {lock} && echo removed {d}; \
             else echo skipped {d}: not a directory made by xtask; fi"
        )
    };
    match which {
        Which::One(lane) => {
            let dir = quote(&lane_dir(base, lane));
            let lock = quote(&lock_dir(&lane_dir(base, lane)));
            guarded(&dir, &lock)
        }
        // The glob sits outside the quotes; only `<base>-…` can match, never `<base>`.
        Which::All => format!(
            "for d in {}-*; do [ -d \"$d\" ] || continue; case \"$d\" in *.lock) continue;; esac; {}; done",
            quote(base),
            guarded("\"$d\"", "\"$d.lock\"")
        ),
    }
}

/// The lock directory for `dir`, beside it.
fn lock_dir(dir: &str) -> String {
    format!("{}.lock", dir.trim_end_matches('/'))
}

/// This machine's name, for the lock's owner file.
fn hostname() -> String {
    ["HOSTNAME", "HOST"]
        .iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
        .or_else(|| {
            let out = Command::new("hostname").output().ok()?;
            let name = String::from_utf8(out.stdout).ok()?;
            Some(name.trim().to_string()).filter(|n| !n.is_empty())
        })
        .unwrap_or_else(|| "unknown".to_string())
}

/// What a lock's owner file says: four lines, host, pid, lane and start time.
fn owner_fields(host: &str, pid: u32, lane: Option<&str>) -> String {
    format!(
        "printf '%s\\n%s\\n%s\\n%s\\n' {} {pid} {} \"$(date '+%Y-%m-%d %H:%M:%S')\"",
        quote(host),
        quote(lane.unwrap_or("main"))
    )
}

/// An sh line that takes the lock for `dir`. Exit 0: taken. Exit 10: somebody
/// holds it (their owner file is printed). Exit 4: `dir`'s parent is missing.
/// A lock more than [`STALE_MINUTES`] old is taken over, with a warning.
fn acquire_line(dir: &str, host: &str, pid: u32, lane: Option<&str>) -> String {
    let lock = quote(&lock_dir(dir));
    let owner = owner_fields(host, pid, lane);
    format!(
        "lock={lock}; \
         if mkdir \"$lock\" 2>/dev/null; then {owner} > \"$lock/owner\"; exit 0; fi; \
         [ -d \"$lock\" ] || {{ echo \"cannot create $lock\" >&2; exit 4; }}; \
         if [ -n \"$(find \"$lock\" -maxdepth 0 -mmin +{STALE_MINUTES} 2>/dev/null)\" ]; then \
         echo \"taking over $lock, held for over 3 hours\" >&2; \
         rm -rf \"$lock\" && mkdir \"$lock\" && {owner} > \"$lock/owner\" && exit 0; fi; \
         cat \"$lock/owner\" 2>/dev/null || echo unknown; exit 10"
    )
}

/// An sh line that drops the lock for `dir` if the owner file still names us.
fn release_line(dir: &str, host: &str, pid: u32) -> String {
    let lock = quote(&lock_dir(dir));
    format!(
        "lock={lock}; if [ \"$(sed -n 1p \"$lock/owner\" 2>/dev/null)\" = {} ] \
         && [ \"$(sed -n 2p \"$lock/owner\" 2>/dev/null)\" = {pid} ]; then rm -rf \"$lock\"; fi",
        quote(host)
    )
}

/// The owner file of a held lock: `(host, pid, lane, since)`, as one line for
/// messages.
fn owner_summary(owner: &str) -> String {
    let lines: Vec<&str> = owner.lines().collect();
    match lines.as_slice() {
        [host, pid, lane, since, ..] => format!("{lane} on {host}, pid {pid}, since {since}"),
        _ => owner.trim().replace('\n', " "),
    }
}

/// Whether the lock's owner is a process on this machine that no longer exists
/// (a Ctrl-C or a kill never runs the release). `alive` says whether a pid runs.
fn owner_gone(owner: &str, me: &str, alive: &dyn Fn(u32) -> bool) -> Option<u32> {
    let lines: Vec<&str> = owner.lines().collect();
    match lines.as_slice() {
        [host, pid, ..] if *host == me => {
            let pid: u32 = pid.parse().ok()?;
            (!alive(pid)).then_some(pid)
        }
        _ => None,
    }
}

/// Whether process `pid` runs here; true when that cannot be told (no `/proc`).
fn pid_alive(pid: u32) -> bool {
    !Path::new("/proc/self").exists() || Path::new(&format!("/proc/{pid}")).exists()
}

/// An sh line that drops the lock for `dir` only while it still names the dead
/// owner `host`/`pid`, so a lock taken since is never removed.
fn break_line(dir: &str, host: &str, pid: u32) -> String {
    release_line(dir, host, pid)
}

/// Holds the remote lock for a directory until dropped, including when the
/// command fails.
pub(crate) struct Lock {
    host: String,
    release: String,
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = Command::new("ssh")
            .args([self.host.as_str(), &posix(&self.release)])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// Takes the lock for `dir` on `host`, waiting (and saying so) while another run
/// holds it. `dry_run` only prints the lines.
pub(crate) fn lock(
    host: &str,
    dir: &str,
    lane: Option<&str>,
    dry_run: bool,
) -> Result<Option<Lock>, String> {
    let (me, pid) = (hostname(), std::process::id());
    let acquire = acquire_line(dir, &me, pid, lane);
    let release = release_line(dir, &me, pid);
    if dry_run {
        println!("$ ssh {host} {}", posix(&acquire));
        println!("$ ssh {host} {}   # when the command ends", posix(&release));
        return Ok(None);
    }
    loop {
        let out = Command::new("ssh")
            .args([host, &posix(&acquire)])
            .stdin(Stdio::null())
            .stderr(Stdio::inherit())
            .output()
            .map_err(|e| format!("failed to start `ssh`: {e}"))?;
        match out.status.code() {
            Some(0) => {
                return Ok(Some(Lock {
                    host: host.to_string(),
                    release,
                }));
            }
            Some(10) => {
                let owner = String::from_utf8_lossy(&out.stdout).to_string();
                if let Some(gone) = owner_gone(&owner, &me, &pid_alive) {
                    println!("the lock on {dir} belongs to pid {gone}, which is gone; taking over");
                    let _ = Command::new("ssh")
                        .args([host, &posix(&break_line(dir, &me, gone))])
                        .stdin(Stdio::null())
                        .status();
                    continue;
                }
                println!("waiting for {dir} (held by {})", owner_summary(&owner));
                std::thread::sleep(RETRY);
            }
            // The directory's parent is missing or unwritable: lock nothing, so a
            // setup that worked before locks existed keeps working.
            Some(4) => {
                println!("warning: cannot lock {dir} on {host}; running without a lock");
                return Ok(None);
            }
            other => {
                return Err(format!(
                    "locking {dir} on {host} failed ({})",
                    other.map_or("killed".to_string(), |c| format!("exit {c}"))
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git<'a>(dir: &'a str, common: &'a str) -> impl Fn(&str) -> Option<String> + 'a {
        move |flag| match flag {
            "--git-dir" => Some(dir.to_string()),
            "--git-common-dir" => Some(common.to_string()),
            _ => None,
        }
    }

    fn no_env(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn names_are_lowercase_letters_digits_and_dashes() {
        assert_eq!(
            sanitize("whether-lane-a").as_deref(),
            Some("whether-lane-a")
        );
        assert_eq!(sanitize("P14_B ").as_deref(), Some("p14-b"));
        assert_eq!(sanitize("..//Lane A!!").as_deref(), Some("lane-a"));
        assert_eq!(sanitize("a--b").as_deref(), Some("a-b"));
        assert_eq!(sanitize("---"), None);
        assert_eq!(sanitize(""), None);
    }

    #[test]
    fn the_main_worktree_has_no_lane_and_a_linked_one_is_named_after_its_directory() {
        let main = git("/r/whether/.git", "/r/whether/.git");
        assert_eq!(lane_with(&no_env, &main, Path::new("/r/whether")), None);
        let linked = git("/r/whether/.git/worktrees/lane-a", "/r/whether/.git");
        let root = Path::new("/r/Whether-Lane_A");
        assert_eq!(
            lane_with(&no_env, &linked, root).as_deref(),
            Some("whether-lane-a")
        );
        // Tool-made worktrees sit deeper; only the last component counts.
        let nested = Path::new("/r/whether/.tools/worktrees/topic");
        assert_eq!(
            lane_with(&no_env, &linked, nested).as_deref(),
            Some("topic")
        );
        // Outside git there is no lane.
        let none = |_: &str| None;
        assert_eq!(lane_with(&no_env, &none, root), None);
    }

    #[test]
    fn whether_lane_overrides_and_empty_switches_lanes_off() {
        let linked = git("/r/w/.git/worktrees/x", "/r/w/.git");
        let main = git("/r/w/.git", "/r/w/.git");
        let root = Path::new("/r/lane-x");
        let set = |v: &'static str| move |k: &str| (k == "WHETHER_LANE").then(|| v.to_string());
        assert_eq!(
            lane_with(&set("Lane B"), &main, root).as_deref(),
            Some("lane-b")
        );
        assert_eq!(
            lane_with(&set("lane-b"), &linked, root).as_deref(),
            Some("lane-b")
        );
        assert_eq!(lane_with(&set(""), &linked, root), None);
        assert_eq!(lane_with(&set("  "), &linked, root), None);
        assert_eq!(lane_with(&set("!!"), &main, root).as_deref(), Some("lane"));
    }

    #[test]
    fn the_main_root_is_the_parent_of_the_common_git_dir() {
        let linked = git("/r/w/.git/worktrees/x", "/r/w/.git");
        assert_eq!(main_root_with(&linked), Some(PathBuf::from("/r/w")));
        let bare = git("/r/w.git", "/r/w.git");
        assert_eq!(main_root_with(&bare), None);
        assert_eq!(main_root_with(&|_| None), None);
    }

    #[test]
    fn a_lane_without_a_config_file_reads_the_main_worktrees() {
        let main = tempfile::tempdir().unwrap();
        let lane = tempfile::tempdir().unwrap();
        let none = || find_config("remote.local.toml", lane.path(), Some(main.path()));
        assert_eq!(none(), None);
        std::fs::write(main.path().join("remote.local.toml"), "host = \"a\"").unwrap();
        assert_eq!(none(), Some(main.path().join("remote.local.toml")));
        // The lane's own file wins.
        std::fs::write(lane.path().join("remote.local.toml"), "host = \"b\"").unwrap();
        assert_eq!(none(), Some(lane.path().join("remote.local.toml")));
        // Nothing to fall back to outside git.
        let other = tempfile::tempdir().unwrap();
        assert_eq!(find_config("remote.local.toml", other.path(), None), None);
        // A nested config name (ios/…) works the same.
        std::fs::create_dir(main.path().join("ios")).unwrap();
        std::fs::write(main.path().join("ios/remote.local.toml"), "").unwrap();
        assert!(find_config("ios/remote.local.toml", lane.path(), Some(main.path())).is_some());
    }

    #[test]
    fn lane_directories_append_the_lane() {
        assert_eq!(lane_dir("whether-build", "lane-a"), "whether-build-lane-a");
        assert_eq!(
            lane_dir("/Users/Shared/whether-build/", "p14-b"),
            "/Users/Shared/whether-build-p14-b"
        );
    }

    #[test]
    fn clean_wants_a_canonical_lane_or_all() {
        assert_eq!(
            which(Some("lane-a"), false),
            Ok(Which::One("lane-a".into()))
        );
        assert_eq!(which(None, true), Ok(Which::All));
        assert!(which(None, false).is_err());
        assert!(which(Some("lane-a"), true).is_err());
        for bad in ["", "../x", "Lane-A", "a b", "-a", "a/b"] {
            assert!(which(Some(bad), false).is_err(), "{bad}");
        }
    }

    #[test]
    fn clean_only_removes_lane_directories_that_carry_the_marker() {
        let one = remove_dirs(
            "whether-build",
            &Which::One("lane-a".into()),
            ".whether-remote",
        );
        assert!(
            one.starts_with("if [ -f 'whether-build-lane-a'/.whether-remote ]"),
            "{one}"
        );
        assert!(
            one.contains("rm -rf 'whether-build-lane-a' 'whether-build-lane-a.lock'"),
            "{one}"
        );
        assert!(!one.contains("rm -rf 'whether-build' "), "{one}");
        let all = remove_dirs(
            "/Users/Shared/whether-build/",
            &Which::All,
            ".whether-remote",
        );
        // Only `<base>-…` is globbed: never the base itself, never a lock.
        assert!(
            all.starts_with("for d in '/Users/Shared/whether-build'-*;"),
            "{all}"
        );
        assert!(all.contains("*.lock) continue"), "{all}");
        assert!(all.contains("[ -f \"$d\"/.whether-remote ]"), "{all}");
    }

    #[test]
    fn the_lock_is_a_directory_beside_the_work_directory() {
        let line = acquire_line("/Users/Shared/whether-build-a", "box", 42, Some("a"));
        assert!(
            line.starts_with("lock='/Users/Shared/whether-build-a.lock'; if mkdir \"$lock\""),
            "{line}"
        );
        assert!(
            line.contains("printf '%s\\n%s\\n%s\\n%s\\n' 'box' 42 'a' \"$(date"),
            "{line}"
        );
        assert!(line.contains("-mmin +180"), "{line}");
        assert!(line.contains("exit 10"), "{line}");
        assert!(
            !line.contains("mkdir -p"),
            "a missing parent is an error, not a held lock"
        );
        let main = acquire_line("w", "box", 1, None);
        assert!(main.contains("'main'"), "{main}");
        // The remote shell may be fish or zsh: it all runs under `sh -c`.
        assert!(posix(&line).starts_with("sh -c '"));
    }

    #[test]
    fn the_lock_is_released_only_by_its_owner() {
        let line = release_line("w", "box", 42);
        assert_eq!(
            line,
            "lock='w.lock'; if [ \"$(sed -n 1p \"$lock/owner\" 2>/dev/null)\" = 'box' ] \
             && [ \"$(sed -n 2p \"$lock/owner\" 2>/dev/null)\" = 42 ]; then rm -rf \"$lock\"; fi"
        );
        assert_eq!(break_line("w", "box", 42), line);
    }

    #[test]
    fn a_lock_of_a_process_that_is_gone_is_broken_but_others_wait() {
        let owner = "box\n4242\nlane-a\n2026-10-25 10:00:00\n";
        let dead = |_: u32| false;
        let alive = |_: u32| true;
        assert_eq!(owner_gone(owner, "box", &dead), Some(4242));
        assert_eq!(owner_gone(owner, "box", &alive), None);
        assert_eq!(
            owner_gone(owner, "other", &dead),
            None,
            "another machine's process"
        );
        assert_eq!(owner_gone("unknown\n", "box", &dead), None);
        assert_eq!(owner_gone("box\nx\n", "box", &dead), None);
        assert_eq!(
            owner_summary(owner),
            "lane-a on box, pid 4242, since 2026-10-25 10:00:00"
        );
        assert_eq!(owner_summary("unknown\n"), "unknown");
    }

    /// Runs `line` with `sh`, as the host would, and gives its exit code and output.
    fn sh(line: &str) -> (i32, String) {
        let out = Command::new("sh").args(["-c", line]).output().unwrap();
        (
            out.status.code().unwrap(),
            String::from_utf8_lossy(&out.stdout).to_string(),
        )
    }

    #[test]
    fn the_lock_lines_work_in_sh() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("build-a").to_string_lossy().to_string();
        let lock = tmp.path().join("build-a.lock");
        // Taken, with an owner file; a second run waits and sees who holds it.
        assert_eq!(sh(&acquire_line(&dir, "box", 11, Some("a"))).0, 0);
        let owner = std::fs::read_to_string(lock.join("owner")).unwrap();
        assert_eq!(
            owner.lines().take(3).collect::<Vec<_>>(),
            ["box", "11", "a"]
        );
        let (code, held) = sh(&acquire_line(&dir, "box", 22, Some("b")));
        assert_eq!((code, held.as_str()), (10, owner.as_str()));
        // Somebody else's release leaves it; the owner's drops it.
        sh(&release_line(&dir, "box", 22));
        sh(&release_line(&dir, "other", 11));
        assert!(lock.exists());
        sh(&release_line(&dir, "box", 11));
        assert!(!lock.exists());
        assert_eq!(sh(&acquire_line(&dir, "box", 22, None)).0, 0);
        // A dead owner's lock is broken only while it still names that owner.
        sh(&break_line(&dir, "box", 11));
        assert!(lock.exists(), "now held by 22");
        sh(&break_line(&dir, "box", 22));
        assert!(!lock.exists());
        // A lock over three hours old is taken over.
        assert_eq!(sh(&acquire_line(&dir, "box", 1, Some("a"))).0, 0);
        assert!(
            Command::new("touch")
                .args(["-d", "4 hours ago"])
                .arg(&lock)
                .status()
                .unwrap()
                .success()
        );
        assert_eq!(sh(&acquire_line(&dir, "box", 2, Some("b"))).0, 0);
        let owner = std::fs::read_to_string(lock.join("owner")).unwrap();
        assert_eq!(owner.lines().nth(1), Some("2"));
        // A missing parent directory is an error, not a lock somebody holds.
        let missing = tmp.path().join("nope/build").to_string_lossy().to_string();
        assert_eq!(sh(&acquire_line(&missing, "box", 3, None)).0, 4);
    }

    #[test]
    fn clean_lines_work_in_sh() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().join("whether-build");
        let make = |name: &str, marked: bool| {
            let dir = tmp.path().join(name);
            std::fs::create_dir_all(&dir).unwrap();
            if marked {
                std::fs::write(dir.join(".whether-remote"), "").unwrap();
            }
            std::fs::create_dir_all(tmp.path().join(format!("{name}.lock"))).unwrap();
            dir
        };
        let main = make("whether-build", true);
        let (a, b, foreign) = (
            make("whether-build-lane-a", true),
            make("whether-build-lane-b", true),
            make("whether-build-other", false),
        );
        let base = base.to_string_lossy().to_string();
        let one = remove_dirs(&base, &Which::One("lane-a".into()), ".whether-remote");
        assert_eq!(sh(&one).0, 0);
        assert!(!a.exists() && b.exists() && main.exists());
        assert!(!tmp.path().join("whether-build-lane-a.lock").exists());
        // --all-lanes: every marked lane, never the main directory or an unmarked one.
        assert_eq!(sh(&remove_dirs(&base, &Which::All, ".whether-remote")).0, 0);
        assert!(!b.exists() && main.exists() && foreign.exists());
        assert!(main.join(".whether-remote").exists());
        // A lane that is not there is not an error; an unmarked one is left alone.
        assert_eq!(sh(&one).0, 0);
        let other = remove_dirs(&base, &Which::One("other".into()), ".whether-remote");
        assert_eq!(sh(&other).0, 0);
        assert!(foreign.exists());
    }
}
