//! `cargo xtask remote …`: run builds on a faster Linux machine over SSH.
//!
//! The working tree is copied with rsync, honouring `.gitignore` (so build
//! output, `firmware/whether-fw.toml` and `.env` never leave this machine), into
//! a directory on the build host that this command owns: it carries a
//! `.whether-remote` marker, and a directory without one is never overwritten.
//! The host's own `target/` is ignored by the copy, so its builds stay warm
//! between runs. `--fetch` copies results (a screenshot, `target/release/…`)
//! back to the same path here.
//!
//! The host comes from `--host`, else `WHETHER_BUILD_HOST`, else `host` in the
//! gitignored `remote.local.toml`, else the first of its `hosts = [...]` that
//! answers (add machines there; the first reachable one builds); the directory likewise from `--dir`,
//! `WHETHER_BUILD_DIR` or `dir`, relative to the host's home unless absolute
//! (default `whether-build`). In a linked git worktree (a *lane*, see
//! [`crate::lane`]) a directory from the file or the default gets `-<lane>`
//! appended (`whether-build-whether-lane-a`), so worktrees never share a tree;
//! an explicit `--dir` or `WHETHER_BUILD_DIR` is used as given. Each run holds a
//! lock (`<dir>.lock`, made with `mkdir`) while it syncs, builds and fetches, so
//! two runs in one directory queue instead of corrupting each other. The config
//! file is read from the worktree, else from the main worktree. The host needs rustup (the pinned toolchain
//! installs itself from `rust-toolchain.toml`) and the build's system packages.

use std::io::IsTerminal as _;
use std::process::Command;

use crate::ios_remote::quote;
use crate::lane::{self, Which};
use crate::workspace_root;

/// The marker that makes a directory on the host ours to replace.
const MARKER: &str = ".whether-remote";
/// The default directory on the host, under its home.
const DEFAULT_DIR: &str = "whether-build";

const USAGE: &str = "\
usage: cargo xtask remote <step> [--host user@host] [--dir path] [--fetch path]… [--dry-run]
                          [--or-local] [-- command…]
       cargo xtask remote clean (--lane <name> | --all-lanes) [--host user@host] [--dry-run]

steps:
  doctor   show the host's OS, cores, memory, free disk space and toolchains
  sync     copy the working tree to the host
  ci       sync, then `cargo xtask ci` on the host
  run      sync, then the command after `--` in the tree on the host, e.g.
           cargo xtask remote run --fetch out.png -- cargo run -p whether -- gui --screenshot out.png
  clean    delete a lane's directory on the host (--lane whether-lane-a), or every lane's
           (--all-lanes). Never the main checkout's directory

A linked git worktree is a lane: its directory on the host is `<dir>-<lane>`
(lane = the worktree directory's name; WHETHER_LANE overrides it, empty switches it off).

--or-local (ci): when no build host answers, run the gate here instead of failing.
--fetch copies a path (relative to the workspace) back here after the step,
whether or not it succeeded; repeat it for more paths.
The host is --host, else WHETHER_BUILD_HOST, else `host` in remote.local.toml, else
the first of its `hosts = [\"a\", \"b\"]` that answers. `cargo xtask ios remote` builds
the iOS core on that host too when one answers (--local there builds it here).";

/// What to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Doctor,
    Sync,
    Ci,
    Run,
    Clean,
}

/// A parsed command line.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Args {
    step: Step,
    host: Option<String>,
    dir: Option<String>,
    fetch: Vec<String>,
    dry_run: bool,
    /// `ci`: run the gate here when no build host answers.
    or_local: bool,
    /// The command for `run`.
    command: Vec<String>,
    /// `clean`: the lane to delete.
    lane: Option<String>,
    /// `clean`: every lane.
    all_lanes: bool,
}

fn parse(args: &[&str]) -> Result<Args, String> {
    let mut iter = args.iter();
    let step = match iter.next() {
        Some(&"doctor") => Step::Doctor,
        Some(&"sync") => Step::Sync,
        Some(&"ci") => Step::Ci,
        Some(&"run") => Step::Run,
        Some(&"clean") => Step::Clean,
        _ => return Err(USAGE.into()),
    };
    let mut out = Args {
        step,
        host: None,
        dir: None,
        fetch: Vec::new(),
        dry_run: false,
        or_local: false,
        command: Vec::new(),
        lane: None,
        all_lanes: false,
    };
    while let Some(arg) = iter.next() {
        match *arg {
            "--host" => out.host = iter.next().map(|s| (*s).to_owned()),
            "--dir" => out.dir = iter.next().map(|s| (*s).to_owned()),
            "--dry-run" => out.dry_run = true,
            "--or-local" => out.or_local = true,
            "--all-lanes" => out.all_lanes = true,
            "--lane" => match iter.next() {
                Some(name) => out.lane = Some((*name).to_owned()),
                None => return Err(format!("--lane needs a lane name\n\n{USAGE}")),
            },
            "--fetch" => match iter.next() {
                Some(path) => out.fetch.push(fetchable(path)?),
                None => return Err(format!("--fetch needs a path\n\n{USAGE}")),
            },
            "--" => {
                out.command = iter.by_ref().map(|s| (*s).to_owned()).collect();
            }
            other => return Err(format!("unknown argument `{other}`\n\n{USAGE}")),
        }
    }
    if step != Step::Clean && (out.lane.is_some() || out.all_lanes) {
        return Err(format!("--lane and --all-lanes belong to clean\n\n{USAGE}"));
    }
    if step == Step::Clean {
        lane::which(out.lane.as_deref(), out.all_lanes)?;
    }
    match (step, out.command.is_empty()) {
        (Step::Run, true) => Err(format!("run needs a command after `--`\n\n{USAGE}")),
        (Step::Run, false) | (_, true) => Ok(out),
        (_, false) => Err(format!("only run takes a command\n\n{USAGE}")),
    }
}

/// `path` when it stays inside the workspace, without a trailing slash.
fn fetchable(path: &str) -> Result<String, String> {
    let trimmed = path.trim_end_matches('/');
    let inside = !trimmed.is_empty()
        && !trimmed.starts_with('/')
        && trimmed.split('/').all(|part| part != ".." && part != ".");
    if inside {
        Ok(trimmed.to_owned())
    } else {
        Err(format!(
            "--fetch `{path}`: use a path inside the workspace, relative to its root"
        ))
    }
}

/// `remote.local.toml`: `host`, or `hosts` (tried in order), and `dir`.
#[derive(Debug, Default)]
struct LocalFile {
    host: Option<String>,
    hosts: Vec<String>,
    dir: Option<String>,
    /// The file these values came from, when there is one.
    path: Option<std::path::PathBuf>,
}

impl LocalFile {
    fn parse(text: &str) -> Result<Self, String> {
        let table: toml::Table = text.parse().map_err(|e: toml::de::Error| e.to_string())?;
        let mut out = Self::default();
        for (key, value) in table {
            if key == "hosts" {
                out.hosts = value
                    .as_array()
                    .and_then(|list| list.iter().map(|v| v.as_str().map(str::to_owned)).collect())
                    .ok_or("`hosts` must be a list of strings")?;
                continue;
            }
            let text = value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("`{key}` must be a string"))?;
            match key.as_str() {
                "host" => out.host = Some(text),
                "dir" => out.dir = Some(text),
                _ => return Err(format!("unknown key `{key}` (use host, hosts and dir)")),
            }
        }
        Ok(out)
    }

    /// The hosts to try, in order: `host`, then `hosts`.
    fn candidates(&self) -> Vec<String> {
        self.host
            .iter()
            .chain(&self.hosts)
            .filter(|h| !h.trim().is_empty())
            .cloned()
            .collect()
    }
}

/// `remote.local.toml` of this worktree, else of the main worktree (a lane has
/// none of its own), or nothing when neither exists.
fn local_file() -> Result<LocalFile, String> {
    let Some(path) = lane::config_file("remote.local.toml") else {
        return Ok(LocalFile::default());
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(LocalFile::default());
    };
    LocalFile::parse(&text)
        .map(|file| LocalFile {
            path: Some(path.clone()),
            ..file
        })
        .map_err(|e| {
            format!(
                "{}: {e}\n(values are quoted: host = \"user@host\")",
                path.display()
            )
        })
}

/// The build host and the directory there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Remote {
    host: String,
    /// The directory this run uses: `base_dir` with `-<lane>` in a lane, unless
    /// `--dir` or `WHETHER_BUILD_DIR` named it.
    dir: String,
    /// The directory without a lane: the one the main checkout uses.
    base_dir: String,
    /// This worktree's lane, if it is a linked one.
    lane: Option<String>,
}

/// Whether `host` answers over SSH within a few seconds, without a password.
fn answers(host: &str) -> bool {
    Command::new("ssh")
        .args([
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=3",
            host,
            "true",
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn remote(
    args: &Args,
    env: &dyn Fn(&str) -> Option<String>,
    file: LocalFile,
    lane: Option<String>,
) -> Result<Remote, String> {
    remote_with(args, env, file, lane, &answers)
}

/// `remote`, with `reachable` deciding among several hosts in the file.
fn remote_with(
    args: &Args,
    env: &dyn Fn(&str) -> Option<String>,
    file: LocalFile,
    lane: Option<String>,
    reachable: &dyn Fn(&str) -> bool,
) -> Result<Remote, String> {
    let nonempty = |v: Option<String>| v.filter(|s| !s.trim().is_empty());
    let candidates = file.candidates();
    let host = nonempty(args.host.clone())
        .or_else(|| nonempty(env("WHETHER_BUILD_HOST")))
        .or_else(|| match candidates.as_slice() {
            [] => None,
            [only] => Some(only.clone()),
            many => many.iter().find(|h| reachable(h)).cloned(),
        })
        .ok_or_else(|| {
            if candidates.len() > 1 {
                format!(
                    "none of the build hosts answered: {}",
                    candidates.join(", ")
                )
            } else {
                "which build host? pass --host user@host, set WHETHER_BUILD_HOST, or put \
                 `host = \"user@host\"` in remote.local.toml"
                    .to_string()
            }
        })?;
    let explicit = nonempty(args.dir.clone()).or_else(|| nonempty(env("WHETHER_BUILD_DIR")));
    // `clean --dir d` cleans the lanes of `d`; otherwise `d` is not a base at all.
    let named_base = explicit.clone().filter(|_| args.step == Step::Clean);
    let base_dir = named_base
        .or_else(|| nonempty(file.dir))
        .unwrap_or_else(|| DEFAULT_DIR.to_string());
    // A named directory is used as given; the file's or the default one is per lane.
    let dir = match (&explicit, &lane) {
        (Some(dir), _) => dir.clone(),
        (None, Some(lane)) => lane::lane_dir(&base_dir, lane),
        (None, None) => base_dir.clone(),
    };
    for checked in [&dir, &base_dir] {
        if checked.trim_matches('/').is_empty() || checked == "." || checked == "~" {
            return Err(format!(
                "`{checked}` is not a directory this command may replace"
            ));
        }
    }
    Ok(Remote {
        host,
        dir,
        base_dir,
        lane,
    })
}

/// A shell line for the host: rustup's environment (which a non-interactive
/// SSH shell lacks), then `body` in the directory.
fn in_dir(remote: &Remote, body: &str) -> String {
    format!(
        "set -e; if [ -f \"$HOME/.cargo/env\" ]; then . \"$HOME/.cargo/env\"; fi; cd {}; {body}",
        quote(&remote.dir)
    )
}

/// `line` run by `sh` on the host, whatever its login shell (fish, for one,
/// cannot read POSIX shell lines).
pub(crate) fn posix(line: &str) -> String {
    format!("sh -c {}", quote(line))
}

/// What the host has, without changing anything.
const DOCTOR: &str = "if [ -f \"$HOME/.cargo/env\" ]; then . \"$HOME/.cargo/env\"; fi; grep PRETTY_NAME /etc/os-release; \
     echo \"$(nproc) cores\"; free -h | head -2; df -h ~ | tail -1; rustup --version 2>/dev/null; \
     rustup toolchain list; cargo deny --version; cmake --version | head -1; g++ --version | head -1";

/// Refuses a directory without the marker, else creates both.
fn claim(remote: &Remote) -> String {
    let dir = quote(&remote.dir);
    format!(
        "if [ -e {dir} ] && [ ! -e {dir}/{MARKER} ]; then \
         echo {dir} exists and was not made by cargo xtask remote >&2; exit 3; fi; \
         mkdir -p {dir} && touch {dir}/{MARKER}"
    )
}

fn rsync_args(remote: &Remote) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "-az".into(),
        "--delete".into(),
        // Every .gitignore applies, so build output and secrets stay here, and
        // the host's own target/ is neither replaced nor deleted.
        "--filter=:- .gitignore".into(),
        format!("--filter=P /{MARKER}"),
        "--exclude=/.git".into(),
    ];
    args.extend(local_excludes());
    args.extend(["./".into(), format!("{}:{}/", remote.host, remote.dir)]);
    args
}

/// `--exclude-from` the checkout's own ignore file (`info/exclude` in the git
/// directory), when there is one: what only this machine ignores stays here too.
pub(crate) fn local_excludes() -> Option<String> {
    let out = Command::new("git")
        .args([
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "info/exclude",
        ])
        .current_dir(workspace_root())
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    let path = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    std::path::Path::new(&path)
        .is_file()
        .then(|| format!("--exclude-from={path}"))
}

/// Copies `path` from the host to the same place here (`/./` marks where the
/// relative part starts).
fn fetch(remote: &Remote, path: &str) -> Vec<String> {
    vec![
        "-az".into(),
        "--relative".into(),
        format!("{}:{}/./{path}", remote.host, remote.dir),
        "./".into(),
    ]
}

/// The steps for `args`, as `(program, arguments)`; then the fetches, which
/// run whether or not the steps succeed. `tty`: whether this is a terminal, so
/// the build gets one (colours, and Ctrl-C stops it on the host).
type Plan = (Vec<(String, Vec<String>)>, Vec<(String, Vec<String>)>);

fn plan(args: &Args, remote: &Remote, tty: bool) -> Plan {
    let ssh = |line: String| ("ssh".to_string(), vec![remote.host.clone(), posix(&line)]);
    if args.step == Step::Doctor {
        return (vec![ssh(DOCTOR.into())], Vec::new());
    }
    if args.step == Step::Clean {
        // `parse` checked the lane arguments.
        let which = lane::which(args.lane.as_deref(), args.all_lanes).unwrap_or(Which::All);
        return (
            vec![ssh(lane::remove_dirs(&remote.base_dir, &which, MARKER))],
            Vec::new(),
        );
    }
    let mut steps = vec![
        ssh(claim(remote)),
        ("rsync".to_string(), rsync_args(remote)),
    ];
    let body = match args.step {
        Step::Ci => Some("cargo xtask ci".to_string()),
        Step::Run => Some(
            args.command
                .iter()
                .map(|a| quote(a))
                .collect::<Vec<_>>()
                .join(" "),
        ),
        Step::Doctor | Step::Sync | Step::Clean => None,
    };
    if let Some(body) = body {
        let mut line = vec![remote.host.clone(), posix(&in_dir(remote, &body))];
        if tty {
            line.insert(0, "-t".into());
        }
        steps.push(("ssh".to_string(), line));
    }
    let fetches = args
        .fetch
        .iter()
        .map(|path| ("rsync".to_string(), fetch(remote, path)))
        .collect();
    (steps, fetches)
}

fn execute(program: &str, arguments: &[String], dry_run: bool) -> Result<(), String> {
    println!("$ {program} {}", arguments.join(" "));
    if dry_run {
        return Ok(());
    }
    let status = Command::new(program)
        .args(arguments)
        .current_dir(workspace_root())
        .status()
        .map_err(|e| format!("failed to start `{program}`: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("`{program}` failed with {status}"))
    }
}

/// The build host to prefer for heavy builds: from `WHETHER_BUILD_HOST` or
/// `remote.local.toml`, and only when it answers now; `None` builds here.
pub(crate) fn preferred() -> Option<Remote> {
    let file = local_file().ok()?;
    let env = |k: &str| std::env::var(k).ok();
    let bare = Args {
        step: Step::Sync,
        host: None,
        dir: None,
        fetch: Vec::new(),
        dry_run: false,
        or_local: false,
        command: Vec::new(),
        lane: None,
        all_lanes: false,
    };
    let chosen = remote_with(&bare, &env, file, lane::lane(), &answers).ok()?;
    answers(&chosen.host).then_some(chosen)
}

/// The iOS targets the core is built for there.
const IOS_TARGETS: &str = "aarch64-apple-ios aarch64-apple-ios-sim aarch64-apple-darwin";

/// Builds the iOS core (static libraries and Swift bindings) on `remote` and
/// copies its stage back to `target/ios-stage` here.
pub(crate) fn stage_ios(remote: &Remote, release: bool) -> Result<(), String> {
    println!("building the iOS core on {}", remote.host);
    let _lock = lane::lock(&remote.host, &remote.dir, remote.lane.as_deref(), false)?;
    let debug = if release { "" } else { " --debug" };
    let build = in_dir(
        remote,
        &format!(
            "rustup target add {IOS_TARGETS} >/dev/null 2>&1 || true; cargo xtask ios build{debug}"
        ),
    );
    let local = crate::ios::stage_dir();
    std::fs::create_dir_all(&local).map_err(|e| format!("{}: {e}", local.display()))?;
    let steps = [
        (
            "ssh".to_string(),
            vec![remote.host.clone(), posix(&claim(remote))],
        ),
        ("rsync".to_string(), rsync_args(remote)),
        ("ssh".to_string(), vec![remote.host.clone(), posix(&build)]),
        (
            "rsync".to_string(),
            fetch_stage(remote, &local.to_string_lossy()),
        ),
    ];
    steps
        .iter()
        .try_for_each(|(program, arguments)| execute(program, arguments, false))
}

/// Copies the host's iOS stage over the one here.
fn fetch_stage(remote: &Remote, local: &str) -> Vec<String> {
    vec![
        "-az".into(),
        "--delete".into(),
        format!("{}:{}/target/ios-stage/", remote.host, remote.dir),
        format!("{local}/"),
    ]
}

pub(crate) fn run(args: &[&str]) -> Result<(), String> {
    let args = parse(args)?;
    if args.or_local && args.host.is_none() && preferred().is_none() {
        println!("no build host answers; running the gate here");
        return crate::ci::run();
    }
    let file = local_file()?;
    if args.dry_run {
        match &file.path {
            Some(path) => println!("config: {}", path.display()),
            None => println!("config: none (no remote.local.toml here or in the main worktree)"),
        }
    }
    let remote = remote(&args, &|k| std::env::var(k).ok(), file, lane::lane())?;
    if args.dry_run {
        match &remote.lane {
            Some(lane) => println!("lane: {lane} (directory {})", remote.dir),
            None => println!("lane: none (the main checkout; directory {})", remote.dir),
        }
    }
    let tty = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    let (steps, fetches) = plan(&args, &remote, tty);
    // Held until this function returns, whether the steps succeed or not.
    let _lock = if matches!(args.step, Step::Doctor | Step::Clean) {
        None
    } else {
        lane::lock(
            &remote.host,
            &remote.dir,
            remote.lane.as_deref(),
            args.dry_run,
        )?
    };
    let result = steps
        .iter()
        .try_for_each(|(program, arguments)| execute(program, arguments, args.dry_run));
    for (program, arguments) in &fetches {
        if let Err(e) = execute(program, arguments, args.dry_run) {
            eprintln!("xtask: {e}");
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(step: Step) -> Args {
        Args {
            step,
            host: Some("me@box".into()),
            dir: None,
            fetch: Vec::new(),
            dry_run: false,
            or_local: false,
            command: Vec::new(),
            lane: None,
            all_lanes: false,
        }
    }

    fn box_remote() -> Remote {
        Remote {
            host: "me@box".into(),
            dir: "it's here".into(),
            base_dir: "it's here".into(),
            lane: None,
        }
    }

    #[test]
    fn parses_steps_flags_and_the_command() {
        let a = parse(&[
            "run",
            "--host",
            "me@box",
            "--dir",
            "/w",
            "--fetch",
            "out.png",
            "--fetch",
            "target/release/",
            "--dry-run",
            "--",
            "cargo",
            "run",
            "--",
            "gui",
        ])
        .unwrap();
        assert_eq!(a.step, Step::Run);
        assert_eq!(a.host.as_deref(), Some("me@box"));
        assert_eq!(a.dir.as_deref(), Some("/w"));
        assert_eq!(a.fetch, ["out.png", "target/release"]);
        assert!(a.dry_run);
        assert_eq!(a.command, ["cargo", "run", "--", "gui"]);
        assert_eq!(parse(&["ci"]).unwrap().step, Step::Ci);
        assert!(parse(&["ci", "--or-local"]).unwrap().or_local);
        assert!(parse(&["run"]).is_err(), "run needs a command");
        assert!(parse(&["ci", "--", "ls"]).is_err(), "only run takes one");
        assert!(parse(&["build"]).is_err());
        assert!(parse(&["sync", "--fetch"]).is_err());
    }

    #[test]
    fn fetches_stay_inside_the_workspace() {
        assert_eq!(fetchable("out.png").unwrap(), "out.png");
        for path in ["/etc/passwd", "../x", "target/../../x", "./x", "", "/"] {
            assert!(fetchable(path).is_err(), "{path}");
        }
    }

    #[test]
    fn the_host_comes_from_flags_then_the_environment_then_the_file() {
        let env = |k: &str| match k {
            "WHETHER_BUILD_HOST" => Some("env@box".to_string()),
            "WHETHER_BUILD_DIR" => Some("/srv/w".to_string()),
            _ => None,
        };
        let none = |_: &str| None;
        let file = || LocalFile {
            host: Some("file@box".into()),
            hosts: Vec::new(),
            dir: Some("w".into()),
            path: None,
        };
        let r = remote(&args(Step::Sync), &env, file(), None).unwrap();
        assert_eq!((r.host.as_str(), r.dir.as_str()), ("me@box", "/srv/w"));
        let bare = Args {
            host: None,
            ..args(Step::Sync)
        };
        assert_eq!(remote(&bare, &env, file(), None).unwrap().host, "env@box");
        let r = remote(&bare, &none, file(), None).unwrap();
        assert_eq!((r.host.as_str(), r.dir.as_str()), ("file@box", "w"));
        let r = remote(&args(Step::Sync), &none, LocalFile::default(), None).unwrap();
        assert_eq!(r.dir, DEFAULT_DIR);
        assert!(remote(&bare, &none, LocalFile::default(), None).is_err());
        for dir in ["/", "~", "."] {
            let a = Args {
                dir: Some(dir.into()),
                ..args(Step::Sync)
            };
            assert!(
                remote(&a, &none, LocalFile::default(), None).is_err(),
                "{dir}"
            );
        }
    }

    #[test]
    fn the_first_host_that_answers_builds() {
        let none = |_: &str| None;
        let file = || LocalFile {
            host: None,
            hosts: vec!["fast@a".into(), "slow@b".into()],
            dir: None,
            path: None,
        };
        let bare = Args {
            host: None,
            ..args(Step::Sync)
        };
        let up = |h: &str| h == "slow@b";
        assert_eq!(
            remote_with(&bare, &none, file(), None, &up).unwrap().host,
            "slow@b"
        );
        let all = |_: &str| true;
        assert_eq!(
            remote_with(&bare, &none, file(), None, &all).unwrap().host,
            "fast@a"
        );
        let down = |_: &str| false;
        let e = remote_with(&bare, &none, file(), None, &down).unwrap_err();
        assert!(e.contains("fast@a, slow@b"), "{e}");
        // A flag or the environment names the host outright.
        assert_eq!(
            remote_with(&args(Step::Sync), &none, file(), None, &down)
                .unwrap()
                .host,
            "me@box"
        );
    }

    #[test]
    fn the_ios_stage_comes_back_over_the_one_here() {
        let f = fetch_stage(&box_remote(), "/w/target/ios-stage");
        assert_eq!(
            f,
            [
                "-az",
                "--delete",
                "me@box:it's here/target/ios-stage/",
                "/w/target/ios-stage/"
            ]
        );
    }

    #[test]
    fn the_local_file_names_host_and_dir() {
        let f = LocalFile::parse("host = \"me@box\"\ndir = \"w\"\n").unwrap();
        assert_eq!(
            (f.host.as_deref(), f.dir.as_deref()),
            (Some("me@box"), Some("w"))
        );
        let f = LocalFile::parse("hosts = [\"a@x\", \"b@y\"]\n").unwrap();
        assert_eq!(f.candidates(), ["a@x", "b@y"]);
        assert!(LocalFile::parse("hosts = [1]").is_err());
        assert!(LocalFile::parse("xcode = \"x\"").is_err());
        assert!(LocalFile::parse("host = 1").is_err());
    }

    #[test]
    fn plans_keep_secrets_home_and_claim_the_directory() {
        let r = box_remote();
        let (steps, fetches) = plan(&args(Step::Ci), &r, false);
        let programs: Vec<&str> = steps.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(programs, ["ssh", "rsync", "ssh"]);
        assert_eq!(fetches, Vec::new());
        // Every line runs in sh, whatever the host's login shell (fish, say).
        assert_eq!(steps[0].1[1], posix(&claim(&r)));
        assert!(claim(&r).contains(MARKER) && claim(&r).contains(r"'it'\''s here'"));
        let rsync = &steps[1].1;
        assert!(rsync.contains(&"--filter=:- .gitignore".to_string()));
        assert!(rsync.contains(&"--exclude=/.git".to_string()));
        assert!(
            rsync
                .iter()
                .filter(|a| a.starts_with("--exclude-from="))
                .all(|a| std::path::Path::new(&a["--exclude-from=".len()..]).is_file())
        );
        assert_eq!(rsync.last().unwrap(), "me@box:it's here/");
        assert_eq!(steps[2].1[0], "me@box", "no terminal, no -t");
        assert_eq!(
            steps[2].1[1],
            posix(
                r#"set -e; if [ -f "$HOME/.cargo/env" ]; then . "$HOME/.cargo/env"; fi; cd 'it'\''s here'; cargo xtask ci"#
            )
        );
        assert!(steps[2].1[1].starts_with("sh -c '"));
        assert_eq!(plan(&args(Step::Sync), &r, true).0.len(), 2);
    }

    #[test]
    fn run_quotes_the_command_and_fetches_afterwards() {
        let a = Args {
            command: vec!["cargo".into(), "run".into(), "it's".into()],
            fetch: vec!["out.png".into()],
            ..args(Step::Run)
        };
        let (steps, fetches) = plan(&a, &box_remote(), true);
        let ssh = &steps.last().unwrap().1;
        assert_eq!(ssh[0], "-t");
        assert_eq!(
            ssh[2],
            posix(&in_dir(&box_remote(), r"'cargo' 'run' 'it'\''s'")),
            "{}",
            ssh[2]
        );
        assert_eq!(
            fetches,
            [(
                "rsync".to_string(),
                vec![
                    "-az".to_string(),
                    "--relative".into(),
                    "me@box:it's here/./out.png".into(),
                    "./".into()
                ]
            )]
        );
    }

    #[test]
    fn a_lane_gets_its_own_directory_unless_one_is_named() {
        let none = |_: &str| None;
        let lane = || Some("whether-lane-a".to_string());
        let bare = Args {
            host: Some("h".into()),
            ..args(Step::Ci)
        };
        let file = |dir: Option<&str>| LocalFile {
            dir: dir.map(str::to_owned),
            ..LocalFile::default()
        };
        // The default and the file's directory, with and without a lane.
        let main = remote(&bare, &none, file(None), None).unwrap();
        assert_eq!(
            (main.dir.as_str(), main.lane.clone()),
            ("whether-build", None)
        );
        let r = remote(&bare, &none, file(None), lane()).unwrap();
        assert_eq!(r.dir, "whether-build-whether-lane-a");
        assert_eq!(r.base_dir, "whether-build");
        let r = remote(&bare, &none, file(Some("/srv/w/")), lane()).unwrap();
        assert_eq!(r.dir, "/srv/w-whether-lane-a");
        assert_eq!(
            remote(&bare, &none, file(Some("/srv/w")), None)
                .unwrap()
                .dir,
            "/srv/w"
        );
        // --dir and WHETHER_BUILD_DIR are used as given, lane or not.
        let explicit = Args {
            dir: Some("/srv/mine".into()),
            ..bare.clone()
        };
        assert_eq!(
            remote(&explicit, &none, file(None), lane()).unwrap().dir,
            "/srv/mine"
        );
        let env = |k: &str| (k == "WHETHER_BUILD_DIR").then(|| "/srv/env".to_string());
        assert_eq!(
            remote(&bare, &env, file(None), lane()).unwrap().dir,
            "/srv/env"
        );
    }

    #[test]
    fn clean_parses_a_lane_or_all_and_nothing_else_takes_them() {
        let a = parse(&["clean", "--lane", "whether-lane-a", "--dry-run"]).unwrap();
        assert_eq!(
            (a.step, a.lane.as_deref(), a.all_lanes),
            (Step::Clean, Some("whether-lane-a"), false)
        );
        assert!(parse(&["clean", "--all-lanes"]).unwrap().all_lanes);
        assert!(parse(&["clean"]).is_err(), "a lane or --all-lanes");
        assert!(parse(&["clean", "--lane", "../x"]).is_err());
        assert!(parse(&["clean", "--lane", "a", "--all-lanes"]).is_err());
        assert!(parse(&["ci", "--lane", "a"]).is_err());
        assert!(parse(&["clean", "--lane"]).is_err());
    }

    #[test]
    fn clean_removes_lane_directories_and_never_the_main_one() {
        let main = Remote {
            host: "me@box".into(),
            dir: "whether-build".into(),
            base_dir: "whether-build".into(),
            lane: None,
        };
        let one = parse(&["clean", "--lane", "whether-lane-a", "--host", "me@box"]).unwrap();
        let (steps, fetches) = plan(&one, &main, false);
        assert_eq!((steps.len(), fetches.len()), (1, 0));
        let line = &steps[0].1[1];
        assert!(line.starts_with("sh -c "), "{line}");
        assert!(line.contains("rm -rf "), "{line}");
        assert!(line.contains("whether-build-whether-lane-a"), "{line}");
        // Cleaning from inside a lane still targets the named lane under the base.
        let inside = Remote {
            dir: "whether-build-whether-lane-b".into(),
            lane: Some("whether-lane-b".into()),
            ..main.clone()
        };
        let line = &plan(&one, &inside, false).0[0].1[1];
        assert!(line.contains("whether-build-whether-lane-a"), "{line}");
        assert!(!line.contains("lane-b"), "{line}");
        let all = parse(&["clean", "--all-lanes"]).unwrap();
        let line = &plan(&all, &main, false).0[0].1[1];
        assert_eq!(
            line,
            &posix(&lane::remove_dirs("whether-build", &Which::All, MARKER))
        );
        // --dir names the base whose lanes are cleaned.
        let dir = parse(&["clean", "--all-lanes", "--dir", "/srv/w", "--host", "h"]).unwrap();
        let r = remote(&dir, &|_| None, LocalFile::default(), None).unwrap();
        assert_eq!(r.base_dir, "/srv/w");
    }

    #[test]
    fn doctor_only_looks() {
        let (steps, fetches) = plan(&args(Step::Doctor), &box_remote(), false);
        assert_eq!(steps.len(), 1);
        assert_eq!(fetches, Vec::new());
        let line = &steps[0].1[1];
        assert_eq!(line, &posix(DOCTOR));
        assert!(!line.contains(MARKER) && !line.contains("set -e"));
    }
}
