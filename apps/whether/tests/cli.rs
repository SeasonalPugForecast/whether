//! Runs the built binary with XDG directories in a temporary home.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A temporary home with XDG variables pointing inside it.
struct Home {
    dir: tempfile::TempDir,
}

impl Home {
    fn new() -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
        }
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.dir.path().join(rel)
    }

    fn config(&self) -> PathBuf {
        self.path("config/whether/config.toml")
    }

    fn write_config(&self, text: &str) {
        let path = self.config();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_whether"));
        command
            .args(args)
            .env_clear()
            .env("HOME", self.dir.path())
            .env("XDG_CONFIG_HOME", self.path("config"))
            .env("XDG_CONFIG_DIRS", self.path("etc"))
            .env("XDG_CACHE_HOME", self.path("cache"))
            .env("XDG_STATE_HOME", self.path("state"));
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command(args).output().unwrap()
    }

    /// `run` with a private runtime directory (for the daemon socket).
    fn run_with_runtime(&self, args: &[&str]) -> Output {
        self.command(args)
            .env("XDG_RUNTIME_DIR", self.path("run"))
            .output()
            .unwrap()
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

const GOOD: &str = "[[location]]\nname = \"New York\"\nlatitude = 40.7143\nlongitude = -74.00597\n";

#[test]
fn config_check_good_and_bad() {
    let home = Home::new();
    home.write_config(GOOD);
    let ok = home.run(&["config", "check"]);
    assert_eq!(ok.status.code(), Some(0), "{}", text(&ok.stdout));
    assert!(text(&ok.stdout).contains("ok: "));

    home.write_config(&format!("{GOOD}[general]\nrefersh = \"15m\"\n"));
    let bad = home.run(&["config", "check"]);
    assert_eq!(bad.status.code(), Some(2));
    let out = text(&bad.stdout);
    assert!(
        out.contains("config.toml:6:1: unknown field `refersh`"),
        "{out}"
    );

    home.write_config("[general]\nlanguage = \"en\"\n");
    let missing = home.run(&["config", "check"]);
    assert_eq!(missing.status.code(), Some(2));
    assert!(text(&missing.stdout).contains("whether locate"));
}

#[test]
fn config_init_refuses_to_overwrite() {
    let home = Home::new();
    let first = home.run(&["config", "init"]);
    assert_eq!(first.status.code(), Some(0), "{}", text(&first.stderr));
    assert!(Path::new(&home.config()).exists());
    let second = home.run(&["config", "init"]);
    assert_eq!(second.status.code(), Some(1));
    assert!(text(&second.stderr).contains("--force"));
    assert_eq!(
        home.run(&["config", "init", "--force"]).status.code(),
        Some(0)
    );
    let check = home.run(&["config", "check"]);
    assert_eq!(
        check.status.code(),
        Some(0),
        "the example validates: {}",
        text(&check.stdout)
    );
}

#[test]
fn displays_list() {
    let out = Home::new().run(&["displays"]);
    insta::assert_snapshot!(text(&out.stdout));
}

/// Stores the sample snapshot for the GOOD location in the home's cache.
fn seed_cache(home: &Home) {
    use whether_core::model::ProviderId;
    use whether_providers::Coordinates;
    use whether_runtime::{Cache, CachedSnapshot};
    let cache = Cache::at(home.path("cache/whether/v1"));
    let key = Cache::key(
        ProviderId::OpenMeteo,
        Coordinates {
            latitude: 40.7143,
            longitude: -74.005_97,
        },
    );
    let cached = CachedSnapshot {
        schema: whether_runtime::cache::SCHEMA,
        saved_at: whether_core::testdata::SAMPLE_NOW,
        provider: ProviderId::OpenMeteo,
        last_modified: None,
        not_before: None,
        snapshot: whether_core::testdata::sample_snapshot(),
    };
    cache.store(&key, &cached).unwrap();
}

#[test]
fn offline_line_from_the_cache() {
    let home = Home::new();
    home.write_config(GOOD);
    seed_cache(&home);
    let out = home.run(&["--offline", "line"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    insta::assert_snapshot!(text(&out.stdout));
}

#[test]
fn offline_json_is_compact_on_request() {
    let home = Home::new();
    home.write_config(GOOD);
    seed_cache(&home);
    let out = home.run(&["--offline", "json", "--compact"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let stdout = text(&out.stdout);
    assert_eq!(stdout.lines().count(), 1);
    let vm: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(vm["location"]["name"], "New York");
}

#[test]
fn offline_without_a_cache_exits_3() {
    let home = Home::new();
    home.write_config(GOOD);
    let out = home.run(&["--offline", "line"]);
    assert_eq!(out.status.code(), Some(3), "{}", text(&out.stderr));
}

#[test]
fn unknown_display_exits_2() {
    let home = Home::new();
    home.write_config(GOOD);
    let out = home.run(&["--display", "nope"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(text(&out.stderr).contains("not compiled in"));
}

#[test]
fn providers_list() {
    let out = Home::new().run(&["providers"]);
    assert_eq!(out.status.code(), Some(0));
    insta::assert_snapshot!(text(&out.stdout));
}

#[test]
fn locate_zip_needs_a_country() {
    let out = Home::new().run(&["locate", "10001", "--zip"]);
    assert_eq!(out.status.code(), Some(2), "clap usage error");
}

#[test]
fn display_presentation_overrides_are_accepted() {
    let home = Home::new();
    home.write_config(&format!("{GOOD}[display.line]\nformat = \"{{temp}}\"\n[display.line.presentation]\nhourly_hours = 48\n"));
    let out = home.run(&["config", "check"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stdout));
}

#[cfg(feature = "gui")]
#[test]
fn gui_screenshot_from_the_cache() {
    let home = Home::new();
    home.write_config(GOOD);
    seed_cache(&home);
    let out_path = home.path("shot.png");
    let out = home.run(&[
        "gui",
        "--screenshot",
        out_path.to_str().unwrap(),
        "--size",
        "800x600",
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let bytes = std::fs::read(&out_path).unwrap();
    assert_eq!(&bytes[1..4], b"PNG");
}

#[test]
fn attached_displays_use_the_daemon() {
    let home = Home::new();
    home.write_config(GOOD);
    // No runtime directory, no socket.
    let out = home.run(&["daemon"]);
    assert_ne!(out.status.code(), Some(0));
    assert!(
        text(&out.stderr).contains("XDG_RUNTIME_DIR"),
        "{}",
        text(&out.stderr)
    );
    // No daemon yet.
    std::fs::create_dir_all(home.path("run")).unwrap();
    let out = home.run_with_runtime(&["--attach", "line"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr).contains("whether daemon"),
        "{}",
        text(&out.stderr)
    );

    // An offline daemon (no network, no cache): attached displays connect, get its
    // state and report that there is no data yet.
    let mut daemon = home
        .command(&["--offline", "daemon"])
        .env("XDG_RUNTIME_DIR", home.path("run"))
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let socket = home.path("run/whether/whether.sock");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !socket.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let attached = home.run_with_runtime(&["--attach", "line"]);
    let again = home.run_with_runtime(&["daemon"]);
    daemon.kill().unwrap();
    daemon.wait().unwrap();
    assert!(socket.exists(), "the daemon created its socket");
    let stderr = text(&attached.stderr);
    assert!(!stderr.contains("no daemon"), "{stderr}");
    assert!(
        matches!(attached.status.code(), Some(3 | 4)),
        "{:?} {stderr}",
        attached.status
    );
    assert!(
        text(&again.stderr).contains("already listening"),
        "{}",
        text(&again.stderr)
    );
}

/// Sidecars run in the daemon only: attached displays, one-shot or not, start none.
#[test]
fn attached_displays_leave_sidecars_to_the_daemon() {
    let home = Home::new();
    let marks = home.path("marks");
    std::fs::create_dir_all(&marks).unwrap();
    std::fs::create_dir_all(home.path("run")).unwrap();
    // Each start of the sidecar leaves a file named after its process id.
    home.write_config(&format!(
        "{GOOD}[display]\nsidecars = [\"exec:mark\"]\n[display.exec.mark]\ncommand = [\"/bin/sh\", \"-c\", \"/usr/bin/touch \\\"$0/$$\\\"\", \"{}\"]\nrestart = \"never\"\n",
        marks.display()
    ));
    let count = || std::fs::read_dir(&marks).unwrap().count();
    let mut daemon = home
        .command(&["--offline", "daemon"])
        .env("XDG_RUNTIME_DIR", home.path("run"))
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while count() == 0 && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let first = count();
    for _ in 0..2 {
        home.run_with_runtime(&["--attach", "line"]);
    }
    std::thread::sleep(std::time::Duration::from_millis(300));
    let after = count();
    daemon.kill().unwrap();
    daemon.wait().unwrap();
    assert_eq!(first, 1, "the daemon starts the sidecar once");
    assert_eq!(after, 1, "attached runs start no sidecars");
}

#[test]
fn history_offline_without_a_kept_history_exits_3() {
    let home = Home::new();
    home.write_config(GOOD);
    let out = home.run(&["--offline", "history"]);
    assert_eq!(out.status.code(), Some(3), "{}", text(&out.stderr));
    assert!(text(&out.stderr).contains("no history kept for this place yet"));
    let bad = home.run(&["history", "--days", "40"]);
    assert_eq!(bad.status.code(), Some(2), "clap rejects it");
}

#[test]
fn climate_offline_without_a_kept_answer_exits_3() {
    let home = Home::new();
    home.write_config(GOOD);
    let out = home.run(&["--offline", "climate", "--from", "07-12", "--to", "07-19"]);
    assert_eq!(out.status.code(), Some(3), "{}", text(&out.stderr));
    let bad = home.run(&["--offline", "climate", "--from", "07-01", "--to", "08-15"]);
    assert_eq!(bad.status.code(), Some(1));
    assert!(
        text(&bad.stderr).contains("at most 31 days"),
        "{}",
        text(&bad.stderr)
    );
}
