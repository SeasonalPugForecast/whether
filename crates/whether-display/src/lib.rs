//! Display plugin API and shared application state for the Linux host.

pub mod bus;
pub mod plugin;
pub mod state;
pub mod view;

pub use bus::{StateHandle, StatePublisher, StateWriter};
pub use plugin::{
    DisplayContext, DisplayError, DisplayFactory, DisplayInfo, DisplayPlugin, LiveSettings, RunMode,
};
pub use state::{
    AppState, Command, FetchStatus, LoadedConfig, LocationState, RadarFrame, RadarImages,
    SearchState, SharedRadar,
};
pub use view::{now, presentation_at, status_info, view_for, view_with};
