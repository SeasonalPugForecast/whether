//! The daemon socket: `whether daemon` serves the protocol to any number
//! of clients on a Unix socket, and displays started with `--attach` become such
//! clients, rebuilding their state from the daemon's snapshots so one fetch
//! service feeds them all.

use std::fs::{DirBuilder, Permissions};
use std::io::{BufReader, Write};
use std::os::unix::fs::{DirBuilderExt, FileTypeExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Weak};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde::Deserialize;
use whether_config::LocationConfig;
use whether_core::model::WeatherSnapshot;
use whether_display::{
    AppState, Command, DisplayContext, LocationState, SearchState, StateHandle, StatePublisher,
};

use crate::exec::read_line;
use crate::{CommandMsg, PROTOCOL_VERSION, PluginMessage, encode};

/// Why the socket could not be opened.
#[derive(Debug, thiserror::Error)]
pub enum SocketError {
    /// Another daemon answers on the socket.
    #[error("a daemon is already listening on {0}")]
    InUse(PathBuf),
    /// No daemon answers on the socket.
    #[error("no daemon at {path} ({source}); start one with `whether daemon`")]
    NoDaemon {
        /// The socket.
        path: PathBuf,
        /// The error.
        source: std::io::Error,
    },
    /// The daemon did not send its first state in time.
    #[error("the daemon sent no state within {0:?}")]
    NoState(Duration),
    /// Anything else.
    #[error("{path}: {source}")]
    Io {
        /// The socket or its directory.
        path: PathBuf,
        /// The error.
        source: std::io::Error,
    },
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> SocketError + '_ {
    move |source| SocketError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Binds the socket at `path`: its directory is made private (0700, also when it
/// already exists, before anything is bound in it), a stale socket is replaced but
/// any other file is left alone, and the socket is for the owner only (0600).
pub fn listen(path: &Path) -> Result<UnixListener, SocketError> {
    if let Some(dir) = path.parent() {
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .map_err(io(dir))?;
        std::fs::set_permissions(dir, Permissions::from_mode(0o700)).map_err(io(dir))?;
    }
    if let Ok(meta) = std::fs::symlink_metadata(path) {
        if !meta.file_type().is_socket() {
            return Err(SocketError::Io {
                path: path.to_path_buf(),
                source: std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    "exists and is not a socket; move it away",
                ),
            });
        }
        if UnixStream::connect(path).is_ok() {
            return Err(SocketError::InUse(path.to_path_buf()));
        }
        // Left over from a daemon that did not stop cleanly.
        std::fs::remove_file(path).map_err(io(path))?;
    }
    let listener = UnixListener::bind(path).map_err(io(path))?;
    std::fs::set_permissions(path, Permissions::from_mode(0o600)).map_err(io(path))?;
    Ok(listener)
}

/// Accepts clients until the listener fails, serving each on its own thread with
/// a context from `context`.
pub fn accept_loop(listener: &UnixListener, context: impl Fn() -> DisplayContext) {
    for (n, stream) in listener.incoming().enumerate() {
        match stream {
            Ok(stream) => {
                let name = format!("client-{}", n + 1);
                let ctx = context();
                let spawned = thread::Builder::new()
                    .name(name.clone())
                    .spawn(move || crate::exec::serve(stream, &name, ctx));
                if let Err(e) = spawned {
                    tracing::error!("cannot serve a client: {e}");
                }
            }
            Err(e) => {
                tracing::error!("accept: {e}");
                return;
            }
        }
    }
}

/// Host messages as a client reads them (unknown types and fields are ignored).
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Incoming {
    Hello {
        protocol: u32,
    },
    State {
        active: usize,
        view: serde_json::Value,
        #[serde(default)]
        snapshots: Option<Vec<Option<WeatherSnapshot>>>,
    },
    Shutdown,
    #[serde(other)]
    Other,
}

/// The daemon's state as an [`AppState`]: one location per tab, with its snapshot
/// and, for the shown one, the daemon's status.
fn rebuild(
    active: usize,
    view: &serde_json::Value,
    snapshots: Vec<Option<WeatherSnapshot>>,
) -> AppState {
    let names: Vec<String> = view["tabs"]
        .as_array()
        .map(|tabs| {
            tabs.iter()
                .map(|t| t["name"].as_str().unwrap_or_default().to_string())
                .collect()
        })
        .unwrap_or_default();
    let status = &view["status"];
    let locations = snapshots
        .into_iter()
        .enumerate()
        .map(|(i, snapshot)| {
            let name = names.get(i).cloned().unwrap_or_default();
            let mut location = LocationState::new(LocationConfig {
                name: Some(name.clone()),
                ..LocationConfig::default()
            });
            location.name = name;
            location.status.last_success = snapshot.as_ref().map(|s| s.fetched_at);
            if i == active {
                location.status.stale = status["stale"].as_bool().unwrap_or(false);
                location.status.in_flight = status["refreshing"].as_bool().unwrap_or(false);
                location.status.error = status["error"].as_str().map(str::to_string);
            }
            location.snapshot = snapshot.map(Arc::new);
            location
        })
        .collect();
    AppState {
        locations,
        ..AppState::default()
    }
}

/// A display's connection to the daemon.
#[derive(Debug)]
pub struct Attached {
    /// The daemon's state, rebuilt locally.
    pub state: StateHandle,
    /// Requests for the daemon (refreshes); other commands are display-local or
    /// unsupported when attached.
    pub commands: Sender<Command>,
    /// Ends when the daemon goes away or the display quits.
    pub reader: JoinHandle<()>,
}

/// Connects to the daemon at `path` and waits up to `wait` for the first state.
pub fn attach(path: &Path, wait: Duration) -> Result<Attached, SocketError> {
    let stream = UnixStream::connect(path).map_err(|source| SocketError::NoDaemon {
        path: path.to_path_buf(),
        source,
    })?;
    let reader_stream = stream.try_clone().map_err(io(path))?;
    let writer = Arc::new(std::sync::Mutex::new(stream.try_clone().map_err(io(path))?));
    let (publisher, state) = StatePublisher::new(AppState::default());
    // The reader owns the publisher (its end tells the display the daemon is
    // gone); the command thread only reaches it while it lives.
    let publisher = Arc::new(publisher);
    let local = Arc::downgrade(&publisher);
    let (first_tx, first_rx) = channel::<()>();
    let reader = {
        let writer = Arc::clone(&writer);
        thread::Builder::new()
            .name("attach-in".into())
            .spawn(move || read_host(reader_stream, &writer, publisher, &first_tx))
            .map_err(io(path))?
    };
    let (commands, rx) = channel();
    thread::Builder::new()
        .name("attach-out".into())
        .spawn(move || forward_commands(&rx, &writer, &stream, &local))
        .map_err(io(path))?;
    first_rx
        .recv_timeout(wait)
        .map_err(|_| SocketError::NoState(wait))?;
    Ok(Attached {
        state,
        commands,
        reader,
    })
}

fn write(writer: &std::sync::Mutex<UnixStream>, message: &PluginMessage) -> bool {
    let mut stream = writer
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    stream.write_all(encode(message).as_bytes()).is_ok()
}

/// Applies host messages to the local state; the publisher is dropped (and the
/// display's subscriptions end) when the daemon goes away.
fn read_host(
    stream: UnixStream,
    writer: &std::sync::Mutex<UnixStream>,
    publisher: Arc<StatePublisher>,
    first: &Sender<()>,
) {
    apply_host(stream, writer, &publisher, first);
    // Dropping the publisher ends the display's subscriptions.
    drop(publisher);
}

fn apply_host(
    stream: UnixStream,
    writer: &std::sync::Mutex<UnixStream>,
    publisher: &StatePublisher,
    first: &Sender<()>,
) {
    let mut reader = BufReader::new(stream);
    while let Some(line) = read_line(&mut reader) {
        let Ok(line) = line else {
            tracing::warn!("the daemon sent an over-long line");
            continue;
        };
        match serde_json::from_slice::<Incoming>(&line) {
            Ok(Incoming::Hello { protocol }) => {
                if protocol != PROTOCOL_VERSION {
                    tracing::error!("the daemon speaks protocol {protocol}");
                    return;
                }
                let ready = PluginMessage::Ready {
                    name: "whether-attach".into(),
                    protocol: PROTOCOL_VERSION,
                };
                if !write(writer, &ready) {
                    return;
                }
            }
            Ok(Incoming::State {
                active,
                view,
                snapshots,
            }) => {
                let rebuilt = rebuild(active, &view, snapshots.unwrap_or_default());
                publisher.update(|s| {
                    s.locations = rebuilt.locations;
                });
                let _ = first.send(());
            }
            Ok(Incoming::Shutdown) => return,
            Ok(Incoming::Other) => {}
            Err(e) => tracing::warn!("unreadable daemon message: {e}"),
        }
    }
}

/// What attached displays cannot do: the daemon owns the configuration and the
/// place search.
const NOT_ATTACHED: &str =
    "not available when attached to the daemon; run the display without --attach";

/// Sends refreshes to the daemon; `quit` (or a closed channel) closes the socket.
/// Searches and location edits are answered locally as unavailable, so the
/// display shows why instead of waiting.
fn forward_commands(
    rx: &Receiver<Command>,
    writer: &std::sync::Mutex<UnixStream>,
    stream: &UnixStream,
    local: &Weak<StatePublisher>,
) {
    while let Ok(command) = rx.recv() {
        match command {
            Command::Refresh { location } => {
                let message = PluginMessage::Command {
                    command: CommandMsg::Refresh { location },
                };
                if !write(writer, &message) {
                    break;
                }
            }
            Command::Quit => break,
            Command::Search { query } => {
                if let Some(publisher) = local.upgrade() {
                    publisher.update(|s| {
                        s.search = Some(SearchState::Failed {
                            query,
                            error: NOT_ATTACHED.into(),
                        });
                    });
                }
            }
            // The daemon reloads its own configuration.
            Command::ReloadConfig => tracing::debug!("reload ignored when attached"),
            Command::AddLocation(_) | Command::RemoveLocation(_) | Command::SetConfig(_) => {
                if let Some(publisher) = local.upgrade() {
                    publisher.update(|s| {
                        s.config_issues.push(whether_config::Issue {
                            severity: whether_config::Severity::Error,
                            path: "location".into(),
                            message: format!("changing locations is {NOT_ATTACHED}"),
                        });
                    });
                }
            }
        }
    }
    let _ = stream.shutdown(std::net::Shutdown::Both);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_socket_is_private_and_stale_files_are_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("whether/whether.sock");
        let listener = listen(&path).unwrap();
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(path.parent().unwrap()), 0o700);
        assert!(matches!(listen(&path), Err(SocketError::InUse(_))));
        drop(listener);
        // The file stays behind; the next daemon replaces it.
        assert!(path.exists());
        // A process forked by a parallel test (insta runs `cargo`) can hold a copy
        // of the listener until it execs, so the socket may answer briefly.
        let mut result = listen(&path);
        for _ in 0..50 {
            if result.is_ok() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
            result = listen(&path);
        }
        if let Err(e) = result {
            panic!("relisten failed: {e}");
        }
    }

    #[test]
    fn loose_directories_are_tightened_and_other_files_kept() {
        let dir = tempfile::tempdir().unwrap();
        let sock_dir = dir.path().join("whether");
        std::fs::create_dir(&sock_dir).unwrap();
        std::fs::set_permissions(&sock_dir, Permissions::from_mode(0o755)).unwrap();
        let path = sock_dir.join("whether.sock");
        std::fs::write(&path, "not a socket").unwrap();
        let err = listen(&path).unwrap_err().to_string();
        assert!(err.contains("not a socket"), "{err}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "not a socket");
        let mode = std::fs::metadata(&sock_dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
    }

    #[test]
    fn attaching_without_a_daemon_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let err = attach(&dir.path().join("none.sock"), Duration::from_millis(10)).unwrap_err();
        assert!(err.to_string().contains("whether daemon"), "{err}");
    }

    #[test]
    fn states_rebuild_locations() {
        let view = serde_json::json!({
            "tabs": [{ "name": "Home" }, { "name": "Cabin" }],
            "status": { "stale": true, "refreshing": false, "error": "timeout" }
        });
        let snapshot = whether_core::testdata::sample_snapshot();
        let state = rebuild(1, &view, vec![None, Some(snapshot.clone())]);
        assert_eq!(state.locations.len(), 2);
        assert_eq!(state.locations[0].name, "Home");
        assert!(state.locations[0].snapshot.is_none());
        assert_eq!(state.locations[1].snapshot.as_deref(), Some(&snapshot));
        assert!(state.locations[1].status.stale);
        assert_eq!(state.locations[1].status.error.as_deref(), Some("timeout"));
        assert_eq!(
            state.locations[1].status.last_success,
            Some(snapshot.fetched_at)
        );
    }
}
