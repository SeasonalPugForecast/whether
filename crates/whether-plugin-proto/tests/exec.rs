//! The exec supervisor against a real child process (`tests/bin/test_plugin.rs`).

use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use whether_config::LocationConfig;
use whether_core::presentation::PresentationConfig;
use whether_core::strings::Strings;
use whether_display::{
    AppState, Command, DisplayContext, DisplayError, DisplayPlugin, LocationState, RunMode,
    StatePublisher,
};
use whether_plugin_proto::exec::{ExecConfig, ExecDisplay, Restart, Timeouts};

fn config(mode: &str, restart: Restart) -> ExecConfig {
    ExecConfig {
        command: vec![env!("CARGO_BIN_EXE_whether-test-plugin").into()],
        env: [("WHETHER_TEST_PLUGIN".to_string(), mode.to_string())].into(),
        send_snapshot: false,
        restart,
        config: toml::Table::new(),
    }
}

fn timeouts() -> Timeouts {
    Timeouts {
        ready: Duration::from_millis(800),
        grace: Duration::from_millis(500),
        resend: Duration::from_secs(60),
    }
}

struct Run {
    publisher: Option<StatePublisher>,
    commands: Receiver<Command>,
    thread: JoinHandle<Result<(), DisplayError>>,
}

fn start(mode: &str, restart: Restart, run_mode: RunMode) -> Run {
    let mut state = AppState::default();
    let mut location = LocationState::new(LocationConfig::default());
    location.snapshot = Some(Arc::new(whether_core::testdata::sample_snapshot()));
    state.locations.push(location);
    let (publisher, handle) = StatePublisher::new(state);
    let (tx, commands) = channel();
    let ctx = DisplayContext {
        state: handle,
        commands: tx,
        display: "exec".into(),
        presentation: Arc::new(PresentationConfig::default()),
        strings: Arc::new(Strings::default()),
        initial_location: 0,
        mode: run_mode,
    };
    let display = ExecDisplay::new("test", config(mode, restart), false).with_timeouts(timeouts());
    let thread = std::thread::spawn(move || Box::new(display).run(ctx));
    Run {
        publisher: Some(publisher),
        commands,
        thread,
    }
}

impl Run {
    fn next_command(&self) -> Command {
        self.commands
            .recv_timeout(Duration::from_secs(10))
            .expect("a command from the plugin")
    }

    /// Stops the host and returns how long the plugin took to end.
    fn stop(mut self) -> (Duration, Result<(), DisplayError>) {
        let started = Instant::now();
        drop(self.publisher.take());
        let result = self.thread.join().unwrap();
        (started.elapsed(), result)
    }
}

#[test]
fn states_go_out_and_commands_come_back() {
    let run = start("echo", Restart::OnFailure, RunMode::Watch);
    assert_eq!(run.next_command(), Command::Refresh { location: Some(0) });
    // A change sends another state, which the plugin answers again.
    run.publisher
        .as_ref()
        .unwrap()
        .update(|s| s.locations[0].name = "Home".into());
    assert_eq!(run.next_command(), Command::Refresh { location: Some(0) });
    let (took, result) = run.stop();
    assert!(result.is_ok());
    assert!(took < Duration::from_secs(3), "{took:?}");
}

#[test]
fn a_crash_restarts_with_backoff() {
    let run = start("crash", Restart::OnFailure, RunMode::Watch);
    assert_eq!(run.next_command(), Command::Refresh { location: Some(0) });
    let first = Instant::now();
    assert_eq!(run.next_command(), Command::Refresh { location: Some(0) });
    assert!(
        first.elapsed() >= Duration::from_millis(900),
        "restarted after {:?}",
        first.elapsed()
    );
    let (took, result) = run.stop();
    assert!(result.is_ok());
    assert!(took < Duration::from_secs(3), "{took:?}");
}

#[test]
fn a_plugin_that_ignores_shutdown_is_killed() {
    let run = start("stubborn", Restart::OnFailure, RunMode::Watch);
    std::thread::sleep(Duration::from_millis(300));
    let (took, result) = run.stop();
    assert!(result.is_ok());
    assert!(took < Duration::from_secs(3), "{took:?}");
}

#[test]
fn no_ready_fails_without_restarts() {
    let run = start("silent", Restart::Never, RunMode::Watch);
    let result = run.thread.join().unwrap();
    assert!(result.unwrap_err().to_string().contains("exec:test"));
}

#[test]
fn once_mode_sends_one_state_and_stops() {
    let run = start("echo", Restart::Never, RunMode::Once);
    let result = run.thread.join().unwrap();
    assert!(result.is_ok(), "{result:?}");
}
