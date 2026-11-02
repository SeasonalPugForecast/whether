//! `exec:<name>`: an external display plugin run as a child process.
//!
//! The supervisor spawns `command` with piped stdio, sends `hello`, waits for
//! `ready`, then sends a `state` on every change (and once a minute, so relative
//! times stay current). Plugin commands go to the fetch service; plugin logs and
//! stderr go to `tracing`. A failed plugin restarts with a backoff of 1 s doubling
//! to 60 s. When the host stops, the plugin gets `shutdown` and is killed after a
//! grace period.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::process::{Child, ExitStatus, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::thread;
use std::time::{Duration, Instant};

use schemars::JsonSchema;
use serde::Deserialize;
use whether_config::{Issue, Severity};
use whether_display::{
    AppState, Command, DisplayContext, DisplayError, DisplayInfo, DisplayPlugin, RunMode, view_for,
};

use crate::{
    CommandMsg, HostMessage, LogLevel, MAX_LINE, PROTOCOL_VERSION, PluginMessage, ProtocolError,
    encode, parse_plugin_line,
};

/// When to start a plugin again after it exits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Restart {
    /// Never.
    Never,
    /// After a crash or a non-zero exit.
    #[default]
    OnFailure,
    /// Whenever it exits.
    Always,
}

/// `[display.exec.<name>]`.
#[derive(Clone, Debug, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecConfig {
    /// The program and its arguments (no shell).
    pub command: Vec<String>,
    /// Extra environment variables.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Also send the raw snapshots, not only the formatted view.
    #[serde(default)]
    pub send_snapshot: bool,
    /// When to restart the plugin.
    #[serde(default)]
    pub restart: Restart,
    /// Passed verbatim in `hello` (never secrets).
    #[serde(default)]
    #[schemars(with = "BTreeMap<String, serde_json::Value>")]
    pub config: toml::Table,
}

/// The schema of `[display.exec]`: one [`ExecConfig`] per plugin name.
pub fn schema() -> schemars::Schema {
    let mut schema = schemars::schema_for!(ExecConfig);
    let config = schema.as_value().clone();
    let defs = config.get("$defs").cloned();
    let mut item = config;
    if let Some(object) = item.as_object_mut() {
        object.remove("$schema");
        object.remove("$defs");
    }
    schema = schemars::json_schema!({
        "type": "object",
        "description": "External display plugins by name: `exec:<name>`.",
        "properties": {},
        "additionalProperties": item,
    });
    if let Some(defs) = defs {
        schema.insert("$defs".into(), defs);
    }
    schema
}

/// Whether `value` holds a table shaped like a secret (`{ env }`, `{ file }` or
/// `{ command }`), which must never reach a plugin.
fn holds_secret(value: &toml::Value) -> bool {
    match value {
        toml::Value::Table(t) => {
            let secret_shaped = t.len() == 1
                && t.keys()
                    .all(|k| matches!(k.as_str(), "env" | "file" | "command"));
            secret_shaped || t.values().any(holds_secret)
        }
        toml::Value::Array(items) => items.iter().any(holds_secret),
        _ => false,
    }
}

/// `[display.exec.<name>]`, parsed and checked.
pub fn parse(table: &toml::Table) -> Result<ExecConfig, String> {
    let cfg: ExecConfig = toml::Value::Table(table.clone())
        .try_into()
        .map_err(|e: toml::de::Error| e.message().trim().to_string())?;
    if cfg.command.first().is_none_or(String::is_empty) {
        return Err("`command` needs at least the program".into());
    }
    if cfg.config.values().any(holds_secret) {
        return Err(
            "`config` must not hold secrets ({ env }, { file } or { command } tables)".into(),
        );
    }
    Ok(cfg)
}

/// Issues in `[display.exec]` (every plugin table).
pub fn validate(exec: &toml::Table) -> Vec<Issue> {
    let mut issues = Vec::new();
    for (name, value) in exec {
        if name == "presentation" {
            continue;
        }
        let path = format!("display.exec.{name}");
        let problem = match value {
            toml::Value::Table(t) if crate::exec::valid_name(name) => parse(t).err(),
            toml::Value::Table(_) => Some("names use letters, digits, `-` and `_`".into()),
            _ => Some("must be a table".into()),
        };
        if let Some(message) = problem {
            issues.push(Issue {
                severity: Severity::Error,
                path,
                message,
            });
        }
    }
    issues
}

fn valid_name(name: &str) -> bool {
    whether_config::is_exec_name(&format!("exec:{name}"))
}

/// The plugin table for `exec:<name>` from `[display.exec]`.
pub fn lookup(exec: Option<&toml::Table>, name: &str) -> Result<ExecConfig, String> {
    let table = exec
        .and_then(|t| t.get(name))
        .and_then(toml::Value::as_table)
        .ok_or_else(|| format!("display `exec:{name}` has no [display.exec.{name}] table"))?;
    parse(table).map_err(|e| format!("display.exec.{name}: {e}"))
}

/// Host variables a plugin keeps: what desktop and terminal programs need to find
/// programs, the session bus and the display. Everything else, notably
/// `OPENWEATHER_API_KEY` and other secrets loaded into whether's environment, is
/// withheld.
const INHERITED: [&str; 12] = [
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "LANG",
    "TZ",
    "TERM",
    "DISPLAY",
    "WAYLAND_DISPLAY",
    "DBUS_SESSION_BUS_ADDRESS",
    "XDG_CURRENT_DESKTOP",
    "XDG_SESSION_TYPE",
];

/// Whether the host variable `name` passes to plugins.
fn inherited(name: &str) -> bool {
    INHERITED.contains(&name)
        || name.starts_with("LC_")
        || (name.starts_with("XDG_") && name.ends_with("_DIR"))
        || (name.starts_with("XDG_") && name.ends_with("_HOME"))
        || (name.starts_with("XDG_") && name.ends_with("_DIRS"))
}

/// A plugin's environment: the allowed host variables, then the plugin's own `env`.
fn plugin_env(
    host: impl Iterator<Item = (std::ffi::OsString, std::ffi::OsString)>,
    own: &BTreeMap<String, String>,
) -> Vec<(std::ffi::OsString, std::ffi::OsString)> {
    let mut env: Vec<_> = host
        .filter(|(name, _)| name.to_str().is_some_and(inherited))
        .collect();
    env.extend(own.iter().map(|(k, v)| (k.into(), v.into())));
    env
}

/// How long to wait for the plugin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timeouts {
    /// From `hello` to `ready`.
    pub ready: Duration,
    /// From `shutdown` to the kill.
    pub grace: Duration,
    /// Resend the state this often without changes (relative times).
    pub resend: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            ready: Duration::from_secs(5),
            grace: Duration::from_secs(2),
            resend: Duration::from_secs(60),
        }
    }
}

/// The first and the longest restart delay.
const BACKOFF: (Duration, Duration) = (Duration::from_secs(1), Duration::from_secs(60));

/// What a session is, which decides what `quit` and a failure mean.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    /// The main display: `quit` ends the app, a failure is an error.
    Main,
    /// Runs beside the main display: `quit` is ignored, failures are logged.
    Sidecar,
    /// A client on the daemon socket: `quit` ends its own session.
    Client,
}

/// An external plugin as a display: the main one, or a sidecar.
#[derive(Debug)]
pub struct ExecDisplay {
    name: String,
    cfg: ExecConfig,
    role: Role,
    timeouts: Timeouts,
}

impl ExecDisplay {
    /// The plugin `exec:<name>`; a `sidecar` ignores `quit` and never ends the app.
    pub fn new(name: &str, cfg: ExecConfig, sidecar: bool) -> Self {
        let role = if sidecar { Role::Sidecar } else { Role::Main };
        Self {
            name: name.into(),
            cfg,
            role,
            timeouts: Timeouts::default(),
        }
    }

    /// Other timeouts (tests).
    #[must_use]
    pub fn with_timeouts(mut self, timeouts: Timeouts) -> Self {
        self.timeouts = timeouts;
        self
    }
}

impl DisplayPlugin for ExecDisplay {
    fn info(&self) -> DisplayInfo {
        DisplayInfo {
            name: "exec",
            description: "an external display plugin (JSON Lines over stdio)",
            interactive: false,
            needs_tty: false,
        }
    }

    fn run(self: Box<Self>, ctx: DisplayContext) -> Result<(), DisplayError> {
        Supervisor::new(*self, ctx).run()
    }
}

/// Serves one daemon client on `stream` until it disconnects or the host stops:
/// the same session as an exec plugin, always with snapshots, never restarted.
pub fn serve(stream: UnixStream, name: &str, ctx: DisplayContext) {
    let cfg = ExecConfig {
        command: Vec::new(),
        env: BTreeMap::new(),
        send_snapshot: true,
        restart: Restart::Never,
        config: toml::Table::new(),
    };
    let display = ExecDisplay {
        role: Role::Client,
        ..ExecDisplay::new(name, cfg, true)
    };
    let mut supervisor = Supervisor::new(display, ctx);
    match supervisor.connect(stream) {
        Ok(running) => {
            if let Err(e) = supervisor.session(running) {
                tracing::warn!(client = name, "session ended: {e}");
            }
        }
        Err(e) => tracing::warn!(client = name, "cannot serve: {e}"),
    }
}

/// What the supervisor waits for.
enum Event {
    /// The state changed.
    Changed,
    /// The host is stopping (the state publisher is gone).
    HostStop,
    /// A line from child `generation`.
    Plugin(u64, Result<PluginMessage, ProtocolError>),
    /// Child `generation` closed its stdout.
    Eof(u64),
}

/// The outcome of waiting for `ready`.
enum Wait {
    Ready,
    Failed,
    Stop,
}

/// A connected plugin: a child process, or a client on the daemon socket.
struct Running {
    /// The process, for exec plugins.
    child: Option<Child>,
    /// Where host messages go (the child's stdin or the socket).
    writer: Option<Box<dyn Write + Send>>,
    /// The socket, shut down at the end of a client session.
    socket: Option<UnixStream>,
    generation: u64,
    started: Instant,
}

/// How a plugin session ended.
enum Ended {
    /// The host is stopping, or the main plugin asked to quit.
    Stop,
    /// The process exited; `ready` tells whether it ever answered `hello`.
    Exited {
        status: Option<ExitStatus>,
        ready: bool,
    },
}

struct Supervisor {
    display: ExecDisplay,
    ctx: DisplayContext,
    events: Receiver<Event>,
    sender: Sender<Event>,
    active: usize,
    seq: u64,
    generation: u64,
}

/// Reads one line of at most [`MAX_LINE`] bytes (newline excluded); longer lines
/// are consumed and reported as too long. `None` at the end of input.
pub(crate) fn read_line(reader: &mut impl BufRead) -> Option<Result<Vec<u8>, ProtocolError>> {
    let mut line = Vec::new();
    let mut total = 0usize;
    loop {
        let chunk = match reader.fill_buf() {
            Ok([]) if total == 0 => return None,
            Ok([]) => break,
            Ok(chunk) => chunk,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return None,
        };
        let (take, done) = match chunk.iter().position(|b| *b == b'\n') {
            Some(i) => (i + 1, true),
            None => (chunk.len(), false),
        };
        total += take;
        if total <= MAX_LINE + 1 {
            line.extend_from_slice(&chunk[..take]);
        }
        reader.consume(take);
        if done {
            break;
        }
    }
    if total > MAX_LINE + 1 {
        return Some(Err(ProtocolError::TooLong { len: total }));
    }
    Some(Ok(line))
}

impl Supervisor {
    fn new(display: ExecDisplay, ctx: DisplayContext) -> Self {
        let (sender, events) = channel();
        // Forward state changes; the end of the subscription means the host stops.
        let changes = ctx.state.subscribe();
        let forward = sender.clone();
        let spawned = thread::Builder::new()
            .name(format!("exec-{}-state", display.name))
            .spawn(move || {
                while changes.recv().is_ok() {
                    if forward.send(Event::Changed).is_err() {
                        return;
                    }
                }
                let _ = forward.send(Event::HostStop);
            });
        if spawned.is_err() {
            let _ = sender.send(Event::HostStop);
        }
        let active = ctx.initial_location;
        Self {
            display,
            ctx,
            events,
            sender,
            active,
            seq: 0,
            generation: 0,
        }
    }

    fn name(&self) -> &str {
        &self.display.name
    }

    fn run(mut self) -> Result<(), DisplayError> {
        let mut backoff = BACKOFF.0;
        loop {
            let started = Instant::now();
            let ended = match self.spawn() {
                Ok(running) => self.session(running)?,
                Err(e) => {
                    tracing::error!(
                        plugin = self.name(),
                        "cannot start {:?}: {e}",
                        self.display.cfg.command
                    );
                    Ended::Exited {
                        status: None,
                        ready: false,
                    }
                }
            };
            let Ended::Exited { status, ready } = ended else {
                return Ok(());
            };
            // A plugin that never became ready failed, whatever its exit code.
            let success = ready && status.is_some_and(|s| s.success());
            let restart = match self.display.cfg.restart {
                Restart::Never => false,
                Restart::OnFailure => !success,
                Restart::Always => true,
            };
            if self.ctx.mode == RunMode::Once || !restart {
                return if success || self.display.role != Role::Main {
                    if !success {
                        tracing::error!(plugin = self.name(), "stopped ({status:?})");
                    }
                    Ok(())
                } else {
                    Err(format!("plugin exec:{} failed ({status:?})", self.name()).into())
                };
            }
            // A plugin that ran for a while starts over with a short delay.
            if started.elapsed() >= BACKOFF.1 {
                backoff = BACKOFF.0;
            }
            tracing::warn!(
                plugin = self.name(),
                "exited ({status:?}); restarting in {backoff:?}"
            );
            if self.wait_for_stop(backoff) {
                return Ok(());
            }
            backoff = (backoff * 2).min(BACKOFF.1);
        }
    }

    /// Waits `delay`; true when the host stops meanwhile.
    fn wait_for_stop(&self, delay: Duration) -> bool {
        let until = Instant::now() + delay;
        loop {
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            match self.events.recv_timeout(left) {
                Ok(Event::HostStop) | Err(RecvTimeoutError::Disconnected) => return true,
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout) => return false,
            }
        }
    }

    fn spawn(&mut self) -> std::io::Result<Running> {
        let cfg = &self.display.cfg;
        let mut child = std::process::Command::new(&cfg.command[0])
            .args(&cfg.command[1..])
            .env_clear()
            .envs(plugin_env(std::env::vars_os(), &cfg.env))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        self.generation += 1;
        let generation = self.generation;
        let name = self.display.name.clone();
        if let Some(stdout) = child.stdout.take() {
            let events = self.sender.clone();
            thread::Builder::new()
                .name(format!("exec-{name}-out"))
                .spawn(move || {
                    let mut reader = BufReader::new(stdout);
                    while let Some(line) = read_line(&mut reader) {
                        let message = line.and_then(|l| parse_plugin_line(&l));
                        if events.send(Event::Plugin(generation, message)).is_err() {
                            return;
                        }
                    }
                    let _ = events.send(Event::Eof(generation));
                })?;
        }
        if let Some(stderr) = child.stderr.take() {
            thread::Builder::new()
                .name(format!("exec-{name}-err"))
                .spawn(move || {
                    let reader = BufReader::new(stderr);
                    for line in reader.lines().map_while(Result::ok) {
                        tracing::info!(plugin = name.as_str(), "{line}");
                    }
                })?;
        }
        let writer = child
            .stdin
            .take()
            .map(|stdin| Box::new(stdin) as Box<dyn Write + Send>);
        Ok(Running {
            child: Some(child),
            writer,
            socket: None,
            generation,
            started: Instant::now(),
        })
    }

    /// A client connected to the daemon socket.
    fn connect(&mut self, stream: UnixStream) -> std::io::Result<Running> {
        self.generation += 1;
        let generation = self.generation;
        let reader = stream.try_clone()?;
        let events = self.sender.clone();
        thread::Builder::new()
            .name(format!("socket-{}-in", self.display.name))
            .spawn(move || {
                let mut reader = BufReader::new(reader);
                while let Some(line) = read_line(&mut reader) {
                    let message = line.and_then(|l| parse_plugin_line(&l));
                    if events.send(Event::Plugin(generation, message)).is_err() {
                        return;
                    }
                }
                let _ = events.send(Event::Eof(generation));
            })?;
        Ok(Running {
            child: None,
            writer: Some(Box::new(stream.try_clone()?)),
            socket: Some(stream),
            generation,
            started: Instant::now(),
        })
    }

    /// Writes one message; false when the plugin's input is gone.
    fn send(running: &mut Running, message: &HostMessage) -> bool {
        running.writer.as_mut().is_some_and(|writer| {
            writer
                .write_all(encode(message).as_bytes())
                .and_then(|()| writer.flush())
                .is_ok()
        })
    }

    fn hello(&self) -> HostMessage {
        let config = serde_json::to_value(&self.display.cfg.config).unwrap_or_default();
        HostMessage::Hello {
            protocol: PROTOCOL_VERSION,
            app_version: env!("CARGO_PKG_VERSION").into(),
            plugin_name: self.display.name.clone(),
            config,
        }
    }

    fn state(&mut self) -> HostMessage {
        let state: std::sync::Arc<AppState> = self.ctx.state.current();
        self.active = self.active.min(state.locations.len().saturating_sub(1));
        // Follows config reloads.
        let (presentation, strings) = self.ctx.settings().current();
        let view = view_for(
            &state,
            self.active,
            &presentation,
            &strings,
            whether_display::now(),
        );
        let snapshots = self
            .display
            .cfg
            .send_snapshot
            .then(|| state.locations.iter().map(|l| l.snapshot.clone()).collect());
        self.seq += 1;
        HostMessage::State {
            seq: self.seq,
            active: self.active,
            view: Box::new(view),
            snapshots,
        }
    }

    /// One plugin process from `hello` to its end.
    fn session(&mut self, mut running: Running) -> Result<Ended, DisplayError> {
        let hello = self.hello();
        if !Self::send(&mut running, &hello) {
            return Ok(self.finish(running, Finish::FAILED));
        }
        match self.await_ready(&mut running) {
            Wait::Ready => {}
            Wait::Failed => return Ok(self.finish(running, Finish::FAILED)),
            Wait::Stop => {
                self.finish(running, Finish::STOP);
                return Ok(Ended::Stop);
            }
        }
        let state = self.state();
        if !Self::send(&mut running, &state) {
            return Ok(self.finish(running, Finish::BROKEN));
        }
        if self.ctx.mode == RunMode::Once {
            return Ok(self.finish(running, Finish::DONE));
        }
        loop {
            let event = match self.events.recv_timeout(self.display.timeouts.resend) {
                Ok(event) => event,
                Err(RecvTimeoutError::Timeout) => Event::Changed,
                Err(RecvTimeoutError::Disconnected) => Event::HostStop,
            };
            match event {
                Event::HostStop => {
                    self.finish(running, Finish::STOP);
                    return Ok(Ended::Stop);
                }
                Event::Changed => {
                    let state = self.state();
                    if !Self::send(&mut running, &state) {
                        return Ok(self.finish(running, Finish::BROKEN));
                    }
                }
                Event::Eof(g) if g == running.generation => {
                    return Ok(self.finish(running, Finish::EXITED));
                }
                Event::Plugin(g, message) if g == running.generation => {
                    if self.handle(&mut running, message) {
                        self.finish(running, Finish::STOP);
                        return Ok(Ended::Stop);
                    }
                }
                Event::Eof(_) | Event::Plugin(..) => {}
            }
        }
    }

    /// Waits for `ready`.
    fn await_ready(&mut self, running: &mut Running) -> Wait {
        let until = Instant::now() + self.display.timeouts.ready;
        loop {
            let left = until.saturating_duration_since(Instant::now());
            match self.events.recv_timeout(left) {
                Ok(Event::Plugin(g, Ok(PluginMessage::Ready { protocol, name })))
                    if g == running.generation =>
                {
                    if protocol != PROTOCOL_VERSION {
                        tracing::error!(
                            plugin = self.name(),
                            "`{name}` speaks protocol {protocol}, not {PROTOCOL_VERSION}"
                        );
                        return Wait::Failed;
                    }
                    tracing::info!(plugin = self.name(), "`{name}` is ready");
                    return Wait::Ready;
                }
                Ok(Event::Plugin(g, message)) if g == running.generation => {
                    self.handle(running, message);
                }
                Ok(Event::Eof(g)) if g == running.generation => return Wait::Failed,
                Ok(Event::HostStop) | Err(RecvTimeoutError::Disconnected) => return Wait::Stop,
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout) => {
                    tracing::error!(
                        plugin = self.name(),
                        "no `ready` within {:?}",
                        self.display.timeouts.ready
                    );
                    return Wait::Failed;
                }
            }
        }
    }

    /// Acts on a plugin message; true when the main plugin asks to quit.
    fn handle(
        &mut self,
        running: &mut Running,
        message: Result<PluginMessage, ProtocolError>,
    ) -> bool {
        let name = self.display.name.clone();
        match message {
            Err(e) => tracing::warn!(plugin = name.as_str(), "protocol error: {e}"),
            Ok(PluginMessage::Ready { .. }) => {}
            Ok(PluginMessage::Log { level, message }) => match level {
                LogLevel::Error => tracing::error!(plugin = name.as_str(), "{message}"),
                LogLevel::Warn => tracing::warn!(plugin = name.as_str(), "{message}"),
                LogLevel::Info => tracing::info!(plugin = name.as_str(), "{message}"),
                LogLevel::Debug => tracing::debug!(plugin = name.as_str(), "{message}"),
            },
            Ok(PluginMessage::Command { command }) => match command {
                CommandMsg::Refresh { location } => {
                    let _ = self.ctx.commands.send(Command::Refresh { location });
                }
                CommandMsg::SelectLocation { index } => {
                    let count = self.ctx.state.current().locations.len();
                    if index < count {
                        self.active = index;
                        let state = self.state();
                        Self::send(running, &state);
                    } else {
                        tracing::warn!(plugin = name.as_str(), "no location {index}");
                    }
                }
                CommandMsg::Quit if self.display.role == Role::Sidecar => {
                    tracing::warn!(
                        plugin = name.as_str(),
                        "a sidecar cannot quit the app; ignored"
                    );
                }
                // The main plugin ends the app; a socket client ends its session.
                CommandMsg::Quit => return true,
            },
        }
        false
    }

    /// Ends a plugin process (see [`Finish`]).
    fn finish(&mut self, mut running: Running, how: Finish) -> Ended {
        if how.polite {
            Self::send(&mut running, &HostMessage::Shutdown);
        }
        // Closing its input tells the plugin there is nothing more to read.
        drop(running.writer.take());
        if let Some(socket) = running.socket.take() {
            let _ = socket.shutdown(std::net::Shutdown::Both);
        }
        let grace = if how.polite {
            self.display.timeouts.grace
        } else {
            Duration::from_millis(200)
        };
        let until = Instant::now() + grace;
        let status = running.child.as_mut().and_then(|child| {
            loop {
                match child.try_wait() {
                    Ok(Some(status)) => break Some(status),
                    Ok(None) if Instant::now() < until => thread::sleep(Duration::from_millis(20)),
                    _ => {
                        let _ = child.kill();
                        break child.wait().ok();
                    }
                }
            }
        });
        if how.drain {
            self.drain(&mut running);
        }
        tracing::debug!(
            plugin = self.name(),
            "ended after {:?}: {status:?}",
            running.started.elapsed()
        );
        Ended::Exited {
            status,
            ready: how.ready,
        }
    }

    /// Handles what the plugin wrote before it exited, up to the end of its output.
    /// A host stop seen meanwhile is kept for the run loop.
    fn drain(&mut self, running: &mut Running) {
        let until = Instant::now() + Duration::from_millis(500);
        loop {
            let left = until.saturating_duration_since(Instant::now());
            match self.events.recv_timeout(left) {
                Ok(Event::Eof(g)) if g == running.generation => return,
                Ok(Event::Plugin(g, message)) if g == running.generation => {
                    // The process is gone, so a `quit` changes nothing here.
                    self.handle(running, message);
                }
                Ok(Event::HostStop) => {
                    let _ = self.sender.send(Event::HostStop);
                    return;
                }
                Ok(_) => {}
                Err(_) => return,
            }
        }
    }
}

/// How [`Supervisor::finish`] ends a plugin.
#[derive(Clone, Copy)]
struct Finish {
    /// Send `shutdown` and allow the grace period.
    polite: bool,
    /// The plugin had answered `hello`.
    ready: bool,
    /// Handle its remaining output first (unless its end was already seen).
    drain: bool,
}

impl Finish {
    /// Never became ready.
    const FAILED: Self = Self {
        polite: false,
        ready: false,
        drain: true,
    };
    /// The host stops, or the main plugin quit.
    const STOP: Self = Self {
        polite: true,
        ready: true,
        drain: false,
    };
    /// Its stdin broke.
    const BROKEN: Self = Self {
        polite: false,
        ready: true,
        drain: true,
    };
    /// Its stdout ended.
    const EXITED: Self = Self {
        polite: false,
        ready: true,
        drain: false,
    };
    /// One-shot run complete.
    const DONE: Self = Self {
        polite: true,
        ready: true,
        drain: true,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(text: &str) -> toml::Table {
        toml::from_str(text).unwrap()
    }

    #[test]
    fn configs_parse_with_defaults() {
        let cfg = parse(&table(
            "command = [\"whether-notify\", \"-v\"]\nconfig = { lead_minutes = 60 }",
        ))
        .unwrap();
        assert_eq!(cfg.command, ["whether-notify", "-v"]);
        assert_eq!(cfg.restart, Restart::OnFailure);
        assert!(!cfg.send_snapshot);
        assert_eq!(cfg.config["lead_minutes"].as_integer(), Some(60));
    }

    #[test]
    fn bad_configs_are_rejected() {
        assert!(parse(&table("command = []")).is_err());
        assert!(parse(&table("command = [\"\"]")).is_err());
        assert!(parse(&table("command = [\"x\"]\nrestart = \"sometimes\"")).is_err());
        assert!(parse(&table("command = [\"x\"]\nshell = true")).is_err());
        let secret = parse(&table(
            "command = [\"x\"]\nconfig = { token = { env = \"TOKEN\" } }",
        ));
        assert!(secret.unwrap_err().contains("secrets"));
        let nested = table("command = [\"x\"]\n[config]\nlist = [{ file = \"~/.key\" }]");
        assert!(parse(&nested).is_err());
    }

    #[test]
    fn validation_covers_every_plugin() {
        let exec = table(
            "[ok]\ncommand = [\"a\"]\n[broken]\ncommand = []\n[\"bad name\"]\ncommand = [\"b\"]\n[presentation]\nhourly_hours = 12",
        );
        let issues = validate(&exec);
        let paths: Vec<&str> = issues.iter().map(|i| i.path.as_str()).collect();
        assert_eq!(paths, ["display.exec.bad name", "display.exec.broken"]);
        assert!(lookup(Some(&exec), "ok").is_ok());
        assert!(
            lookup(Some(&exec), "missing")
                .unwrap_err()
                .contains("[display.exec.missing]")
        );
        assert!(lookup(None, "ok").is_err());
    }

    #[test]
    fn plugins_get_a_minimal_environment_without_secrets() {
        let host = [
            ("PATH", "/usr/bin"),
            ("HOME", "/home/u"),
            ("XDG_RUNTIME_DIR", "/run/user/1000"),
            ("XDG_CONFIG_HOME", "/home/u/.config"),
            ("LC_TIME", "en_GB.UTF-8"),
            ("DBUS_SESSION_BUS_ADDRESS", "unix:path=/run/user/1000/bus"),
            ("OPENWEATHER_API_KEY", "hunter2"),
            ("WHETHER_MET_CONTACT", "me@example.com"),
            ("AWS_SECRET_ACCESS_KEY", "x"),
        ]
        .map(|(k, v)| (k.into(), v.into()));
        let own = BTreeMap::from([("MODE".to_string(), "quiet".to_string())]);
        let env = plugin_env(host.into_iter(), &own);
        let names: Vec<&str> = env.iter().map(|(k, _)| k.to_str().unwrap()).collect();
        assert_eq!(
            names,
            [
                "PATH",
                "HOME",
                "XDG_RUNTIME_DIR",
                "XDG_CONFIG_HOME",
                "LC_TIME",
                "DBUS_SESSION_BUS_ADDRESS",
                "MODE"
            ]
        );
        assert!(!env.iter().any(|(_, v)| v == "hunter2"));
    }

    #[test]
    fn long_lines_are_cut_off_and_reading_continues() {
        let mut input = vec![b'x'; MAX_LINE + 10];
        input.extend_from_slice(b"\n{\"type\":\"log\"}\n");
        let mut reader = BufReader::new(&input[..]);
        assert!(matches!(
            read_line(&mut reader),
            Some(Err(ProtocolError::TooLong { .. }))
        ));
        assert_eq!(
            read_line(&mut reader).unwrap().unwrap(),
            b"{\"type\":\"log\"}\n"
        );
        assert!(read_line(&mut reader).is_none());
    }

    #[test]
    fn the_schema_maps_names_to_configs() {
        let schema = serde_json::to_value(schema()).unwrap();
        assert_eq!(schema["type"], "object");
        assert!(
            schema
                .pointer("/additionalProperties/properties/command")
                .is_some()
        );
    }
}
