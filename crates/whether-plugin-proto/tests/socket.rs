//! The daemon socket end to end: one state publisher (one fetch service) feeding
//! two attached clients.

use std::sync::Arc;
use std::sync::mpsc::channel;
use std::time::Duration;

use whether_config::LocationConfig;
use whether_core::presentation::PresentationConfig;
use whether_core::strings::Strings;
use whether_display::{
    AppState, Command, DisplayContext, LocationState, RunMode, StateHandle, StatePublisher,
};
use whether_plugin_proto::socket::{accept_loop, attach, listen};

/// Serves `state` on a fresh socket; returns its path, the publisher (dropping it
/// stops the daemon side) and the commands the service receives.
fn daemon(
    dir: &std::path::Path,
    state: AppState,
) -> (
    std::path::PathBuf,
    StatePublisher,
    std::sync::mpsc::Receiver<Command>,
) {
    let path = dir.join("whether/whether.sock");
    let (publisher, handle) = StatePublisher::new(state);
    let (commands, service) = channel();
    let listener = listen(&path).unwrap();
    std::thread::spawn(move || {
        accept_loop(&listener, || DisplayContext {
            state: handle.clone(),
            commands: commands.clone(),
            display: "exec".into(),
            presentation: Arc::new(PresentationConfig::default()),
            strings: Arc::new(Strings::default()),
            initial_location: 0,
            mode: RunMode::Watch,
        });
    });
    (path, publisher, service)
}

fn wait_for(handle: &StateHandle, epoch_above: u64) {
    let changes = handle.subscribe();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while handle.epoch() <= epoch_above {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        assert!(!left.is_zero(), "no update within 5 s");
        let _ = changes.recv_timeout(left.min(Duration::from_millis(100)));
    }
}

#[test]
fn two_clients_share_one_service() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("whether/whether.sock");
    let mut state = AppState::default();
    for name in ["Home", "Cabin"] {
        let mut location = LocationState::new(LocationConfig::default());
        location.name = name.into();
        location.snapshot = Some(Arc::new(whether_core::testdata::sample_snapshot()));
        state.locations.push(location);
    }
    let (publisher, handle) = StatePublisher::new(state);
    let (commands, service) = channel();
    let listener = listen(&path).unwrap();
    std::thread::spawn(move || {
        accept_loop(&listener, || DisplayContext {
            state: handle.clone(),
            commands: commands.clone(),
            display: "exec".into(),
            presentation: Arc::new(PresentationConfig::default()),
            strings: Arc::new(Strings::default()),
            initial_location: 0,
            mode: RunMode::Watch,
        });
    });

    let a = attach(&path, Duration::from_secs(5)).unwrap();
    let b = attach(&path, Duration::from_secs(5)).unwrap();
    for client in [&a, &b] {
        let local = client.state.current();
        assert_eq!(local.locations.len(), 2);
        assert_eq!(local.locations[1].name, "Cabin");
        assert!(local.locations[1].snapshot.is_some());
    }

    // One fetch on the daemon reaches both clients identically.
    let (epoch_a, epoch_b) = (a.state.epoch(), b.state.epoch());
    publisher.update(|s| {
        let mut snapshot = (*s.locations[0].snapshot.clone().unwrap()).clone();
        snapshot.fetched_at.0 += 600;
        s.locations[0].snapshot = Some(Arc::new(snapshot));
    });
    wait_for(&a.state, epoch_a);
    wait_for(&b.state, epoch_b);
    let (sa, sb) = (a.state.current(), b.state.current());
    assert_eq!(sa.locations[0].snapshot, sb.locations[0].snapshot);
    assert_eq!(
        sa.locations[0].snapshot.as_ref().unwrap().fetched_at,
        publisher.handle().current().locations[0]
            .snapshot
            .as_ref()
            .unwrap()
            .fetched_at
    );

    // A client's refresh reaches the one service once.
    a.commands
        .send(Command::Refresh { location: Some(1) })
        .unwrap();
    assert_eq!(
        service.recv_timeout(Duration::from_secs(5)).unwrap(),
        Command::Refresh { location: Some(1) }
    );
    assert!(service.recv_timeout(Duration::from_millis(300)).is_err());

    // When the daemon stops, attached clients see their state end.
    let changes = a.state.subscribe();
    drop(publisher);
    a.reader.join().unwrap();
    while changes.recv_timeout(Duration::from_secs(5)).is_ok() {}
}

/// A plain protocol client that sends `quit` gets `shutdown` and its connection is
/// closed; the daemon keeps serving others.
#[test]
fn a_client_quit_ends_only_its_session() {
    use std::io::{BufRead, BufReader, Write};

    let dir = tempfile::tempdir().unwrap();
    let mut state = AppState::default();
    state
        .locations
        .push(LocationState::new(LocationConfig::default()));
    let (path, _publisher, _service) = daemon(dir.path(), state);
    let stream = std::os::unix::net::UnixStream::connect(&path).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut writer = stream.try_clone().unwrap();
    let mut lines = BufReader::new(stream).lines();
    let mut next_type = || {
        let line = lines.next().unwrap().unwrap();
        serde_json::from_str::<serde_json::Value>(&line).unwrap()["type"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert_eq!(next_type(), "hello");
    writer
        .write_all(b"{\"type\":\"ready\",\"name\":\"raw\",\"protocol\":1}\n")
        .unwrap();
    assert_eq!(next_type(), "state");
    writer
        .write_all(b"{\"type\":\"command\",\"command\":{\"kind\":\"quit\"}}\n")
        .unwrap();
    assert_eq!(next_type(), "shutdown");
    assert!(lines.next().is_none(), "the daemon closes the connection");
    // Another client is still served.
    assert!(attach(&path, Duration::from_secs(5)).is_ok());
}

/// Searching or editing locations from an attached display reports that it is not
/// available instead of leaving the display waiting.
#[test]
fn attached_searches_fail_visibly() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = AppState::default();
    state
        .locations
        .push(LocationState::new(LocationConfig::default()));
    let (path, _publisher, _service) = daemon(dir.path(), state);
    let a = attach(&path, Duration::from_secs(5)).unwrap();

    let epoch = a.state.epoch();
    a.commands
        .send(Command::Search {
            query: "Paris".into(),
        })
        .unwrap();
    wait_for(&a.state, epoch);
    match &a.state.current().search {
        Some(whether_display::SearchState::Failed { query, error }) => {
            assert_eq!(query, "Paris");
            assert!(error.contains("--attach"), "{error}");
        }
        other => panic!("{other:?}"),
    }

    let epoch = a.state.epoch();
    a.commands.send(Command::RemoveLocation(0)).unwrap();
    wait_for(&a.state, epoch);
    assert!(
        a.state.current().config_issues[0]
            .message
            .contains("not available")
    );
}
