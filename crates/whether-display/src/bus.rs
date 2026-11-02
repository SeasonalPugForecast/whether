//! The state bus: one publisher (the service), many readers (displays).
//!
//! Readers get `Arc<AppState>` snapshots and a dirty flag. Publishing never blocks:
//! each subscriber has a one-slot channel, so many updates coalesce into one pending
//! notification, and subscribers whose receiver was dropped are pruned.

use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::state::AppState;

#[derive(Debug)]
struct Shared {
    state: Mutex<Arc<AppState>>,
    /// `None` once the publisher is gone, so receivers see a disconnect.
    subscribers: Mutex<Option<Vec<SyncSender<()>>>>,
}

/// Locks `m`, recovering the data if a panicking thread poisoned it (the state is
/// replaced whole, so it is never half-written).
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Changes the shared state and notifies subscribers.
fn update(shared: &Shared, f: impl FnOnce(&mut AppState)) {
    {
        let mut current = lock(&shared.state);
        let state = Arc::make_mut(&mut current);
        let epoch = state.epoch;
        f(state);
        state.epoch = epoch + 1;
    }
    if let Some(subscribers) = lock(&shared.subscribers).as_mut() {
        subscribers.retain(|tx| match tx.try_send(()) {
            Ok(()) | Err(TrySendError::Full(())) => true,
            Err(TrySendError::Disconnected(())) => false,
        });
    }
}

/// An extra writer: updates the state like the publisher, without owning the bus.
#[derive(Clone, Debug)]
pub struct StateWriter {
    shared: Arc<Shared>,
}

impl StateWriter {
    /// Changes the state, as [`StatePublisher::update`] does.
    pub fn update(&self, f: impl FnOnce(&mut AppState)) {
        update(&self.shared, f);
    }

    /// The latest state.
    pub fn current(&self) -> Arc<AppState> {
        Arc::clone(&lock(&self.shared.state))
    }
}

/// The writing side, owned by the service.
#[derive(Debug)]
pub struct StatePublisher {
    shared: Arc<Shared>,
}

/// The reading side; cheap to clone.
#[derive(Clone, Debug)]
pub struct StateHandle {
    shared: Arc<Shared>,
}

impl StatePublisher {
    /// A bus holding `initial`.
    pub fn new(initial: AppState) -> (Self, StateHandle) {
        let shared = Arc::new(Shared {
            state: Mutex::new(Arc::new(initial)),
            subscribers: Mutex::new(Some(Vec::new())),
        });
        (
            Self {
                shared: Arc::clone(&shared),
            },
            StateHandle { shared },
        )
    }

    /// Changes the state, bumps `epoch` and notifies subscribers. The state is copied
    /// only while readers still hold the previous snapshot.
    pub fn update(&self, f: impl FnOnce(&mut AppState)) {
        update(&self.shared, f);
    }

    /// A second writer for helper threads (the radar pass). Unlike the publisher it
    /// does not end the bus when dropped.
    pub fn writer(&self) -> StateWriter {
        StateWriter {
            shared: Arc::clone(&self.shared),
        }
    }

    /// A reader for this bus.
    pub fn handle(&self) -> StateHandle {
        StateHandle {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl Drop for StatePublisher {
    /// Disconnects every subscriber: no more changes will come.
    fn drop(&mut self) {
        lock(&self.shared.subscribers).take();
    }
}

impl StateHandle {
    /// The latest state.
    pub fn current(&self) -> Arc<AppState> {
        Arc::clone(&lock(&self.shared.state))
    }

    /// The latest state's epoch.
    pub fn epoch(&self) -> u64 {
        lock(&self.shared.state).epoch
    }

    /// A receiver that gets `()` after changes (at most one pending at a time). It
    /// disconnects when the publisher is dropped.
    pub fn subscribe(&self) -> Receiver<()> {
        let (tx, rx) = sync_channel(1);
        if let Some(subscribers) = lock(&self.shared.subscribers).as_mut() {
            subscribers.push(tx);
        }
        rx
    }

    /// Subscribers currently registered (for tests and diagnostics).
    pub fn subscriber_count(&self) -> usize {
        lock(&self.shared.subscribers).as_ref().map_or(0, Vec::len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn updates_are_visible_and_epochs_increase() {
        let (publisher, handle) = StatePublisher::new(AppState::default());
        assert_eq!(handle.epoch(), 0);
        let mut last = 0;
        for i in 0..5 {
            publisher.update(|s| s.config_issues.clear());
            let epoch = handle.epoch();
            assert!(epoch > last, "{i}");
            last = epoch;
        }
        assert_eq!(handle.current().epoch, 5);
    }

    #[test]
    fn a_slow_subscriber_never_blocks_and_updates_coalesce() {
        let (publisher, handle) = StatePublisher::new(AppState::default());
        let rx = handle.subscribe();
        let started = Instant::now();
        for _ in 0..1000 {
            publisher.update(|s| s.locations.clear());
        }
        assert!(started.elapsed() < Duration::from_secs(1));
        assert_eq!(rx.try_recv(), Ok(()));
        assert!(rx.try_recv().is_err(), "at most one pending notification");
        assert_eq!(handle.epoch(), 1000);
    }

    #[test]
    fn dropped_receivers_are_pruned() {
        let (publisher, handle) = StatePublisher::new(AppState::default());
        let keep = handle.subscribe();
        drop(handle.subscribe());
        assert_eq!(handle.subscriber_count(), 2);
        publisher.update(|_| {});
        assert_eq!(handle.subscriber_count(), 1);
        assert_eq!(keep.try_recv(), Ok(()));
    }

    #[test]
    fn readers_keep_their_snapshot() {
        let (publisher, handle) = StatePublisher::new(AppState::default());
        let before = handle.current();
        publisher.update(|s| {
            s.config_issues.push(whether_config::Issue {
                severity: whether_config::Severity::Warning,
                path: "x".into(),
                message: "m".into(),
            });
        });
        assert!(before.config_issues.is_empty(), "the old Arc is unchanged");
        assert_eq!(handle.current().config_issues.len(), 1);
    }

    #[test]
    fn notifications_cross_threads() {
        let (publisher, handle) = StatePublisher::new(AppState::default());
        let rx = handle.subscribe();
        let writer = std::thread::spawn(move || publisher.update(|_| {}));
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)), Ok(()));
        writer.join().unwrap();
        assert_eq!(handle.epoch(), 1);
    }

    #[test]
    fn dropping_the_publisher_disconnects_subscribers() {
        let (publisher, handle) = StatePublisher::new(AppState::default());
        let before = handle.subscribe();
        drop(publisher);
        assert!(before.recv_timeout(Duration::from_secs(1)).is_err());
        let after = handle.subscribe();
        assert!(after.recv_timeout(Duration::from_secs(1)).is_err());
        assert_eq!(handle.subscriber_count(), 0);
    }
}
