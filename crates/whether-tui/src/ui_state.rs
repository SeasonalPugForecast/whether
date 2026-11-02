//! Display-local state: the view, the selected location and toggles.

use alloc::string::String;

use crate::config::TuiConfig;

/// The body's content.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum View {
    /// The configured layout.
    #[default]
    Overview,
    /// Full-width chart and table.
    Hourly,
    /// Detailed daily table.
    Daily,
    /// Air quality.
    Air,
    /// The radar loop.
    Radar,
    /// Past days, normals and records.
    History,
}

/// How many colors to use.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorDepth {
    /// No colors (`NO_COLOR`, `TERM=dumb`, `color = "never"`).
    Plain,
    /// The 16 ANSI colors (no `COLORTERM=truecolor`).
    Ansi16,
    /// 24-bit color.
    #[default]
    TrueColor,
}

/// What the user has chosen; never sent to the service.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // independent user toggles, not a state machine
pub struct UiState {
    /// The body's content.
    pub view: View,
    /// The help overlay is open.
    pub help: bool,
    /// The shown location.
    pub active_location: usize,
    /// Units switched between metric and imperial (rebuilds the view model).
    pub units_toggled: bool,
    /// Draw ASCII art.
    pub art: bool,
    /// First row shown in scrollable tables.
    pub scroll: u16,
    /// How many colors the terminal shows.
    pub colors: ColorDepth,
    /// The open location search, if any.
    pub search: Option<SearchInput>,
    /// Show key hints in the footer (off on devices without a keyboard).
    pub key_hints: bool,
    /// Ticks into the radar loop (see [`UiState::radar_frame`]).
    pub radar_step: usize,
    /// The radar loop is paused.
    pub radar_paused: bool,
}

/// Ticks the radar loop holds its latest frame before starting over.
const RADAR_HOLD: usize = 3;

impl UiState {
    /// The initial state for `cfg`.
    pub fn new(cfg: &TuiConfig, active_location: usize, colors: ColorDepth) -> Self {
        Self {
            view: View::Overview,
            help: false,
            active_location,
            units_toggled: false,
            art: cfg.art,
            scroll: 0,
            search: None,
            colors,
            key_hints: true,
            radar_step: 0,
            radar_paused: false,
        }
    }

    /// The radar frame shown, of `frames` (oldest first).
    pub fn radar_frame(&self, frames: usize) -> usize {
        self.radar_step.min(frames.saturating_sub(1))
    }

    /// Advances the radar loop one tick unless it is paused; the latest frame is
    /// held for a few ticks so it reads as "now".
    pub fn radar_tick(&mut self, frames: usize) -> Action {
        if self.radar_paused || frames < 2 {
            return Action::None;
        }
        self.radar_step = (self.radar_step + 1) % (frames + RADAR_HOLD);
        Action::Redraw
    }

    /// Steps the radar loop by hand (pausing it), within `frames`.
    fn radar_seek(&mut self, forward: bool, frames: usize) -> Action {
        let shown = self.radar_frame(frames);
        self.radar_paused = true;
        self.radar_step = if forward {
            (shown + 1).min(frames.saturating_sub(1))
        } else {
            shown.saturating_sub(1)
        };
        Action::Redraw
    }
}

/// A key press, independent of the terminal library.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum Key {
    Char(char),
    Left,
    Right,
    Up,
    Down,
    Tab,
    BackTab,
    Esc,
    Enter,
    Backspace,
    Other,
}

/// The location search being typed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchInput {
    /// What is typed.
    pub query: String,
    /// The highlighted result.
    pub selected: usize,
    /// The query has been sent and has not changed since.
    pub submitted: bool,
}

/// What the host must do after a key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Nothing changed.
    None,
    /// Redraw the same view model.
    Redraw,
    /// Rebuild the view model (location or units changed), then redraw.
    Rebuild,
    /// Ask the service to refresh one location, or all.
    Refresh(Option<usize>),
    /// Search for places.
    Search(String),
    /// Add search result `n` as a location.
    AddResult(usize),
    /// Leave.
    Quit,
}

impl UiState {
    /// Selects location `i` of `locations`.
    pub fn select(&mut self, i: usize, locations: usize) -> Action {
        if locations == 0 || i == self.active_location {
            return Action::None;
        }
        self.active_location = i % locations;
        self.scroll = 0;
        Action::Rebuild
    }

    fn step(&mut self, forward: bool, locations: usize) -> Action {
        if locations < 2 {
            return Action::None;
        }
        let i = if forward {
            (self.active_location + 1) % locations
        } else {
            (self.active_location + locations - 1) % locations
        };
        self.select(i, locations)
    }

    fn set_view(&mut self, view: View) -> Action {
        self.view = view;
        self.scroll = 0;
        Action::Redraw
    }

    /// Applies a key (see [`KEYS`]); `radar_frames` is the shown loop's length.
    pub fn on_key(&mut self, key: Key, locations: usize, radar_frames: usize) -> Action {
        match key {
            Key::Esc if self.help => {
                self.help = false;
                Action::Redraw
            }
            Key::Char('q') | Key::Esc => Action::Quit,
            Key::Left | Key::BackTab | Key::Char('h') => self.step(false, locations),
            Key::Right | Key::Tab | Key::Char('l') => self.step(true, locations),
            Key::Char('1') => self.set_view(View::Overview),
            Key::Char('2') => self.set_view(View::Hourly),
            Key::Char('3') => self.set_view(View::Daily),
            Key::Char('4') => self.set_view(View::Air),
            Key::Char('5') => {
                self.radar_step = 0;
                self.radar_paused = false;
                self.set_view(View::Radar)
            }
            Key::Char('6') => self.set_view(View::History),
            Key::Char(' ') if self.view == View::Radar => {
                self.radar_paused = !self.radar_paused;
                Action::Redraw
            }
            Key::Char(',') if self.view == View::Radar => self.radar_seek(false, radar_frames),
            Key::Char('.') if self.view == View::Radar => self.radar_seek(true, radar_frames),
            Key::Char('?') => {
                self.help = !self.help;
                Action::Redraw
            }
            Key::Char('/') => {
                self.search = Some(SearchInput::default());
                Action::Redraw
            }
            Key::Char('r') => Action::Refresh(Some(self.active_location)),
            Key::Char('R') => Action::Refresh(None),
            Key::Char('u') => {
                self.units_toggled = !self.units_toggled;
                Action::Rebuild
            }
            Key::Char('a') => {
                self.art = !self.art;
                Action::Redraw
            }
            Key::Down | Key::Char('j') => {
                self.scroll = self.scroll.saturating_add(1);
                Action::Redraw
            }
            Key::Up | Key::Char('k') => {
                self.scroll = self.scroll.saturating_sub(1);
                Action::Redraw
            }
            Key::Char(_) | Key::Enter | Key::Backspace | Key::Other => Action::None,
        }
    }

    /// Applies a key while the search is open; `results` is the number of results.
    pub fn on_search_key(&mut self, key: Key, results: usize) -> Action {
        let Some(search) = self.search.as_mut() else {
            return Action::None;
        };
        match key {
            Key::Esc => {
                self.search = None;
                Action::Redraw
            }
            Key::Enter if search.submitted && results > 0 => {
                let chosen = search.selected.min(results - 1);
                self.search = None;
                Action::AddResult(chosen)
            }
            Key::Enter if !search.query.trim().is_empty() => {
                search.submitted = true;
                search.selected = 0;
                Action::Search(String::from(search.query.trim()))
            }
            Key::Backspace => {
                search.query.pop();
                search.submitted = false;
                Action::Redraw
            }
            Key::Char(c) => {
                search.query.push(c);
                search.submitted = false;
                Action::Redraw
            }
            Key::Down | Key::Tab => {
                search.selected = (search.selected + 1).min(results.saturating_sub(1));
                Action::Redraw
            }
            Key::Up | Key::BackTab => {
                search.selected = search.selected.saturating_sub(1);
                Action::Redraw
            }
            _ => Action::None,
        }
    }
}

/// Every key binding with its description, for the help overlay and the reducer.
pub const KEYS: &[(&str, &str)] = &[
    ("q  Esc", "quit (Esc closes help first)"),
    ("← →  h l  Tab", "previous / next location"),
    ("1 2 3 4 5 6", "overview hourly daily air radar history"),
    ("Space  , .", "radar: pause / step back, forward"),
    ("?", "this help"),
    ("r  R", "refresh this location / all"),
    ("u", "switch metric / imperial"),
    ("a", "ASCII art on / off"),
    ("↑ ↓  j k", "scroll tables"),
    ("/", "search and add a location"),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> UiState {
        UiState::new(&TuiConfig::default(), 0, ColorDepth::TrueColor)
    }

    #[test]
    fn locations_wrap_and_reset_scrolling() {
        let mut ui = state();
        ui.scroll = 5;
        assert_eq!(ui.on_key(Key::Right, 3, 0), Action::Rebuild);
        assert_eq!((ui.active_location, ui.scroll), (1, 0));
        ui.on_key(Key::Char('l'), 3, 0);
        assert_eq!(ui.on_key(Key::Tab, 3, 0), Action::Rebuild);
        assert_eq!(ui.active_location, 0, "wraps forward");
        ui.on_key(Key::BackTab, 3, 0);
        assert_eq!(ui.active_location, 2, "wraps backward");
        assert_eq!(ui.on_key(Key::Left, 1, 0), Action::None, "one location");
        assert_eq!(ui.select(7, 0), Action::None);
    }

    #[test]
    fn views_help_and_quitting() {
        let mut ui = state();
        assert_eq!(ui.on_key(Key::Char('3'), 1, 0), Action::Redraw);
        assert_eq!(ui.view, View::Daily);
        ui.on_key(Key::Char('?'), 1, 0);
        assert!(ui.help);
        assert_eq!(
            ui.on_key(Key::Esc, 1, 0),
            Action::Redraw,
            "Esc closes help first"
        );
        assert!(!ui.help);
        assert_eq!(ui.on_key(Key::Esc, 1, 0), Action::Quit);
        assert_eq!(state().on_key(Key::Char('q'), 1, 0), Action::Quit);
    }

    #[test]
    fn refresh_units_art_and_scroll() {
        let mut ui = state();
        ui.active_location = 1;
        assert_eq!(ui.on_key(Key::Char('r'), 2, 0), Action::Refresh(Some(1)));
        assert_eq!(ui.on_key(Key::Char('R'), 2, 0), Action::Refresh(None));
        assert_eq!(ui.on_key(Key::Char('u'), 2, 0), Action::Rebuild);
        assert!(ui.units_toggled);
        ui.on_key(Key::Char('a'), 2, 0);
        assert!(!ui.art);
        ui.on_key(Key::Up, 2, 0);
        assert_eq!(ui.scroll, 0, "never below zero");
        ui.on_key(Key::Char('j'), 2, 0);
        ui.on_key(Key::Down, 2, 0);
        assert_eq!(ui.scroll, 2);
        assert_eq!(ui.on_key(Key::Char('x'), 2, 0), Action::None);
    }

    #[test]
    fn search_typing_submitting_and_choosing() {
        let mut ui = state();
        assert_eq!(ui.on_key(Key::Char('/'), 1, 0), Action::Redraw);
        for c in "Brookly".chars() {
            ui.on_search_key(Key::Char(c), 0);
        }
        ui.on_search_key(Key::Char('x'), 0);
        ui.on_search_key(Key::Backspace, 0);
        ui.on_search_key(Key::Char('n'), 0);
        assert_eq!(
            ui.on_search_key(Key::Enter, 0),
            Action::Search("Brooklyn".into())
        );
        assert_eq!(ui.on_search_key(Key::Down, 3), Action::Redraw);
        ui.on_search_key(Key::Down, 3);
        ui.on_search_key(Key::Down, 3);
        assert_eq!(
            ui.search.as_ref().unwrap().selected,
            2,
            "bounded by the results"
        );
        assert_eq!(ui.on_search_key(Key::Enter, 3), Action::AddResult(2));
        assert!(ui.search.is_none());
    }

    #[test]
    fn search_escape_and_retyping() {
        let mut ui = state();
        ui.on_key(Key::Char('/'), 1, 0);
        assert_eq!(
            ui.on_search_key(Key::Enter, 0),
            Action::None,
            "nothing typed"
        );
        ui.on_search_key(Key::Char('a'), 0);
        ui.on_search_key(Key::Enter, 0);
        ui.on_search_key(Key::Char('b'), 2);
        assert_eq!(
            ui.on_search_key(Key::Enter, 2),
            Action::Search("ab".into()),
            "edited queries search again"
        );
        assert_eq!(ui.on_search_key(Key::Esc, 2), Action::Redraw);
        assert!(ui.search.is_none());
        assert_eq!(ui.on_search_key(Key::Char('z'), 0), Action::None);
    }

    #[test]
    fn the_radar_loop_plays_holds_and_steps() {
        let mut ui = state();
        assert_eq!(
            ui.on_key(Key::Char(' '), 1, 6),
            Action::None,
            "not in the radar view"
        );
        assert_eq!(ui.on_key(Key::Char('5'), 1, 6), Action::Redraw);
        assert_eq!((ui.view, ui.radar_frame(6)), (View::Radar, 0));
        for _ in 0..5 {
            ui.radar_tick(6);
        }
        assert_eq!(ui.radar_frame(6), 5, "the latest");
        ui.radar_tick(6);
        ui.radar_tick(6);
        assert_eq!(ui.radar_frame(6), 5, "held");
        ui.radar_tick(6);
        ui.radar_tick(6);
        assert_eq!(ui.radar_frame(6), 0, "then over again");
        assert_eq!(ui.on_key(Key::Char(' '), 1, 6), Action::Redraw);
        assert_eq!(ui.radar_tick(6), Action::None, "paused");
        ui.on_key(Key::Char(','), 1, 6);
        assert_eq!(ui.radar_frame(6), 0, "not before the oldest");
        ui.on_key(Key::Char('.'), 1, 6);
        ui.on_key(Key::Char('.'), 1, 6);
        assert_eq!(ui.radar_frame(6), 2);
        ui.on_key(Key::Char(' '), 1, 6);
        assert_eq!(ui.radar_tick(1), Action::None, "one frame does not loop");
    }

    #[test]
    fn starts_on_the_overview() {
        let ui = UiState::new(&TuiConfig::default(), 2, ColorDepth::TrueColor);
        assert_eq!(
            (ui.view, ui.active_location, ui.art, ui.help),
            (View::Overview, 2, true, false)
        );
        assert!(KEYS.len() >= 8);
    }
}
