//! The state the fetcher publishes and the displays read.

use alloc::string::String;
use alloc::vec::Vec;
use core::cell::RefCell;

use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::signal::Signal;
use whether_core::model::WeatherSnapshot;
use whether_core::view::StatusInfo;

/// The network as the displays describe it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LinkState {
    /// Not associated with the access point.
    #[default]
    Down,
    /// Associated, waiting for an address.
    Associated,
    /// Associated with an IPv4 address.
    Online,
}

/// One location's latest data.
#[derive(Clone, Debug)]
pub struct FwLocationState {
    /// The configured name.
    pub name: String,
    /// The last good snapshot, kept when later fetches fail.
    pub snapshot: Option<WeatherSnapshot>,
    /// How fresh it is.
    pub status: StatusInfo,
}

/// Everything the fetcher publishes.
#[derive(Clone, Debug, Default)]
pub struct FwState {
    /// One entry per configured location, in order.
    pub locations: Vec<FwLocationState>,
    /// The network.
    pub link: LinkState,
    /// Increases with every change.
    pub epoch: u32,
}

/// What the input task asks the fetcher for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FwCommand {
    /// Fetch every location now (at most once a minute).
    Refresh,
}

static STATE: Mutex<CriticalSectionRawMutex, RefCell<FwState>> =
    Mutex::new(RefCell::new(FwState {
        locations: Vec::new(),
        link: LinkState::Down,
        epoch: 0,
    }));

/// Signaled after every change to the state.
pub static CHANGED: Signal<CriticalSectionRawMutex, ()> = Signal::new();

/// Commands for the fetcher.
pub static COMMANDS: Channel<CriticalSectionRawMutex, FwCommand, 4> = Channel::new();

/// Starts the state with the location names.
pub fn init(names: impl Iterator<Item = String>) {
    update(|s| {
        s.locations = names
            .map(|name| FwLocationState {
                name,
                snapshot: None,
                status: StatusInfo::default(),
            })
            .collect();
    });
}

/// Changes the state, then signals [`CHANGED`].
pub fn update<R>(f: impl FnOnce(&mut FwState) -> R) -> R {
    let result = STATE.lock(|cell| {
        let mut state = cell.borrow_mut();
        let result = f(&mut state);
        state.epoch = state.epoch.wrapping_add(1);
        result
    });
    CHANGED.signal(());
    result
}

/// Reads the state.
pub fn read<R>(f: impl FnOnce(&FwState) -> R) -> R {
    STATE.lock(|cell| f(&cell.borrow()))
}
