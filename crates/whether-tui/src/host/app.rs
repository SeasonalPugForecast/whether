//! The event loop: draw on change, handle keys, mouse and resizes.

use std::sync::mpsc::TryRecvError;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind, MouseButton, MouseEventKind,
};
use ratatui::crossterm::execute;
use whether_core::presentation::WidgetKind;
use whether_core::view::ViewModel;
use whether_display::{
    AppState, Command, DisplayContext, DisplayError, LiveSettings, SearchState, now,
    presentation_at, view_with,
};

use super::input;
use crate::config::TuiConfig;
use crate::ui_state::{Action, ColorDepth, UiState, View};
use crate::{RadarImage, SearchPanel};

/// Time between radar frames, as in the desktop card.
const RADAR_FRAME: Duration = Duration::from_millis(600);
use crate::widgets::chrome::tab_title;

/// Runs until the user quits; the terminal is restored on every exit path (and on
/// panics, by ratatui's hook).
pub(super) fn run(
    cfg: &TuiConfig,
    ctx: &DisplayContext,
    colors: ColorDepth,
) -> Result<(), DisplayError> {
    let mut terminal = ratatui::try_init()?;
    if cfg.mouse {
        let _ = execute!(std::io::stdout(), EnableMouseCapture);
    }
    let result = event_loop(&mut terminal, cfg, ctx, colors);
    if cfg.mouse {
        let _ = execute!(std::io::stdout(), DisableMouseCapture);
    }
    ratatui::restore();
    result
}

/// The shown radar frame's pixels, when the view has radar labels (the radar
/// widget is on and the location is covered).
fn radar_image<'a>(state: &'a AppState, ui: &UiState, vm: &ViewModel) -> Option<RadarImage<'a>> {
    let labels = vm.radar.as_ref()?;
    let images = &state.locations.get(ui.active_location)?.radar.as_ref()?.0;
    let frame = images.frames.get(ui.radar_frame(labels.frames.len()));
    Some(RadarImage {
        width: images.width,
        height: images.height,
        base: &images.base,
        frame: frame.map(|f| &*f.rgba),
        marker: images.marker,
    })
}

/// The active location's view in the current presentation (units toggled with `u`).
fn build(state: &AppState, ui: &UiState, settings: &LiveSettings) -> ViewModel {
    let (base, strings) = settings.current();
    // The location's own settings first, so `u` toggles the units it shows.
    let mut presentation = presentation_at(state, ui.active_location, &base);
    if ui.units_toggled {
        presentation.units = presentation.units.toggled();
    }
    view_with(state, ui.active_location, &presentation, &strings, now())
}

fn event_loop(
    terminal: &mut ratatui::DefaultTerminal,
    cfg: &TuiConfig,
    ctx: &DisplayContext,
    colors: ColorDepth,
) -> Result<(), DisplayError> {
    let changes = ctx.state.subscribe();
    let mut ui = UiState::new(cfg, ctx.initial_location, colors);
    let redraw_every = Duration::from_secs(u64::from(cfg.redraw.secs().max(1)));
    let mut vm = None;
    let mut rebuild = true;
    let mut redraw = true;
    let mut last_build = Instant::now();
    let mut last_frame = Instant::now();
    let radar_cell = cfg
        .rows
        .iter()
        .any(|r| r.widgets.iter().any(|c| c.kind == WidgetKind::Radar));
    // Presentation and strings follow config reloads.
    let settings = ctx.settings();
    let mut epoch = 0;
    loop {
        let state = ctx.state.current();
        let locations = state.locations.len();
        ui.active_location = ui.active_location.min(locations.saturating_sub(1));
        if rebuild
            || vm.is_none()
            || last_build.elapsed() >= redraw_every
            || state.config_epoch != epoch
        {
            epoch = state.config_epoch;
            vm = Some(build(&state, &ui, &settings));
            last_build = Instant::now();
            rebuild = false;
            redraw = true;
        }
        let Some(view) = vm.as_ref() else { continue };
        let radar_frames = view.radar.as_ref().map_or(0, |r| r.frames.len());
        if (ui.view == View::Radar || radar_cell) && last_frame.elapsed() >= RADAR_FRAME {
            last_frame = Instant::now();
            redraw |= ui.radar_tick(radar_frames) == Action::Redraw;
        }
        if redraw {
            let panel = search_panel(&state);
            let radar = radar_image(&state, &ui, view);
            terminal.draw(|f| {
                crate::draw_with_radar(f, view, &ui, cfg, radar);
                if let Some(input) = &ui.search {
                    crate::draw_search(f, input, &panel, &ui, cfg);
                }
            })?;
            redraw = false;
        }
        let wait = if radar_frames > 1 { 100 } else { 250 };
        if event::poll(Duration::from_millis(wait))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    #[cfg(debug_assertions)]
                    assert!(
                        key.code != event::KeyCode::F(12),
                        "F12: testing that the terminal is restored after a panic"
                    );
                    let key = input::key(key);
                    let action = if ui.search.is_some() {
                        ui.on_search_key(key, search_panel(&state).results.len())
                    } else {
                        ui.on_key(key, locations, radar_frames)
                    };
                    match apply(action, ctx, &state) {
                        Some(Next::Quit) => return Ok(()),
                        Some(Next::Rebuild) => rebuild = true,
                        Some(Next::Redraw) => redraw = true,
                        None => {}
                    }
                }
                Event::Resize(..) => redraw = true,
                Event::Mouse(m)
                    if cfg.mouse
                        && m.row == 0
                        && m.kind == MouseEventKind::Down(MouseButton::Left) =>
                {
                    let titles: Vec<String> = view.tabs.iter().map(tab_title).collect();
                    if let Some(i) = input::tab_at(m.column, &titles) {
                        rebuild |= ui.select(i, locations) == Action::Rebuild;
                    }
                }
                _ => {}
            }
        }
        match changes.try_recv() {
            Ok(()) => rebuild = true,
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => {}
        }
    }
}

/// What the loop does after a key's action.
enum Next {
    Quit,
    Rebuild,
    Redraw,
}

/// Sends the commands an action asks for.
fn apply(action: Action, ctx: &DisplayContext, state: &AppState) -> Option<Next> {
    match action {
        Action::Quit => Some(Next::Quit),
        Action::Refresh(location) => {
            let _ = ctx.commands.send(Command::Refresh { location });
            None
        }
        Action::Search(query) => {
            let _ = ctx.commands.send(Command::Search { query });
            Some(Next::Redraw)
        }
        Action::AddResult(i) => {
            if let Some(SearchState::Results { results, .. }) = &state.search
                && let Some(result) = results.get(i)
            {
                let _ = ctx.commands.send(Command::AddLocation(result.clone()));
            }
            Some(Next::Redraw)
        }
        Action::Rebuild => Some(Next::Rebuild),
        Action::Redraw => Some(Next::Redraw),
        Action::None => None,
    }
}

/// The search popup's status and results from the service's search state.
fn search_panel(state: &AppState) -> SearchPanel {
    match &state.search {
        None => SearchPanel::default(),
        Some(SearchState::Searching { query }) => SearchPanel {
            status: format!("Searching for “{query}”…"),
            results: Vec::new(),
        },
        Some(SearchState::Results { results, .. }) if results.is_empty() => SearchPanel {
            status: "No places found".into(),
            results: Vec::new(),
        },
        Some(SearchState::Results { results, .. }) => SearchPanel {
            status: String::new(),
            results: results
                .iter()
                .map(whether_providers::GeocodeResult::display_name)
                .collect(),
        },
        Some(SearchState::Failed { error, .. }) => SearchPanel {
            status: error.clone(),
            results: Vec::new(),
        },
    }
}
