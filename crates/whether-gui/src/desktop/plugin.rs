//! The `gui` display plugin: the Slint event loop on the main thread, fed by a
//! watcher thread that rebuilds the view model off the UI thread.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::RecvTimeoutError;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use slint::{ComponentHandle, LogicalSize, ModelRc, SharedString, Timer, TimerMode, VecModel};
use whether_config::{Issue, LinuxConfig, Severity};
use whether_core::units::UnitPrefs;
use whether_display::{
    AppState, Command, DisplayContext, DisplayError, DisplayFactory, DisplayInfo, DisplayPlugin,
    SearchState, now, presentation_at, view_with,
};

use super::config::{GuiConfig, GuiMode};
use super::glue::{WindowOptions, apply, sky_for};
use super::settings::{self, SettingsForm};
use crate::model::{UiModel, to_ui};
use crate::{AboutWindow, CompactWindow, DesktopWindow};

const DESCRIPTION: &str = "desktop window (Mousam-inspired), compact mode and kiosk";
/// Rebuild at least this often so relative times ("3 min ago") stay current.
const REFRESH_VIEW: Duration = Duration::from_secs(30);

/// What the user changed locally (never sent to the service).
#[derive(Debug)]
struct Local {
    active: usize,
    units_toggled: bool,
    /// Seconds between locations (`cycle_locations`, following reloads; 0: off).
    cycle: u32,
    /// Seconds since the last turn.
    since_cycle: u32,
    /// The sky last shown (a `SkyTheme` index), for the settings preview.
    sky: i32,
}

impl Local {
    fn new(active: usize, cfg: &GuiConfig) -> Self {
        Self {
            active,
            units_toggled: false,
            cycle: cfg.cycle_locations.secs(),
            since_cycle: 0,
            sky: 0,
        }
    }
}

/// Search results and status for the search overlay.
#[derive(Clone, Debug, Default, PartialEq)]
struct SearchView {
    results: Vec<SharedString>,
    status: SharedString,
}

/// What the watcher builds off the UI thread.
#[derive(Clone, Debug)]
struct Frame {
    model: UiModel,
    opts: WindowOptions,
    search: SearchView,
    /// The shown location's radar imagery (converted on the UI thread).
    radar: Option<whether_display::SharedRadar>,
    /// The compact window's page interval in milliseconds (0: off).
    page_ms: i64,
}

/// Radar imagery and its converted base and frames.
type ConvertedRadar = (
    whether_display::SharedRadar,
    (slint::Image, Vec<slint::Image>),
);

thread_local! {
    /// The radar images last converted, so a state change without new imagery does
    /// not convert again.
    static RADAR: RefCell<Option<ConvertedRadar>> = const { RefCell::new(None) };
}

type Build = Arc<dyn Fn() -> Frame + Send + Sync>;

fn parse(section: Option<&toml::Table>) -> Result<GuiConfig, String> {
    match section {
        None => Ok(GuiConfig::default()),
        Some(t) => toml::Value::Table(t.clone())
            .try_into()
            .map_err(|e: toml::de::Error| e.message().trim().to_string()),
    }
}

fn validate(section: &toml::Table) -> Vec<Issue> {
    match parse(Some(section)) {
        Ok(cfg) => cfg.validate(),
        Err(message) => vec![Issue {
            severity: Severity::Error,
            path: "display.gui".into(),
            message,
        }],
    }
}

fn create(
    section: Option<&toml::Table>,
    _: &LinuxConfig,
) -> Result<Box<dyn DisplayPlugin>, DisplayError> {
    let cfg = parse(section)?;
    if let Some(issue) = cfg
        .validate()
        .into_iter()
        .find(|i| i.severity == Severity::Error)
    {
        return Err(issue.to_string().into());
    }
    Ok(Box::new(GuiPlugin { cfg }))
}

/// The registry entry for `gui`.
pub fn factory() -> DisplayFactory {
    DisplayFactory {
        name: "gui",
        description: DESCRIPTION,
        interactive: true,
        create,
        validate,
        schema: || schemars::schema_for!(GuiConfig),
    }
}

/// The `gui` plugin.
#[derive(Debug)]
pub struct GuiPlugin {
    cfg: GuiConfig,
}

fn lock(local: &Mutex<Local>) -> MutexGuard<'_, Local> {
    local.lock().unwrap_or_else(PoisonError::into_inner)
}

fn search_view(state: &AppState) -> SearchView {
    match &state.search {
        None => SearchView::default(),
        Some(SearchState::Searching { query }) => SearchView {
            results: Vec::new(),
            status: format!("Searching for “{query}”…").into(),
        },
        Some(SearchState::Results { results, .. }) if results.is_empty() => SearchView {
            results: Vec::new(),
            status: "No places found".into(),
        },
        Some(SearchState::Results { results, .. }) => SearchView {
            results: results.iter().map(|r| r.display_name().into()).collect(),
            status: SharedString::new(),
        },
        Some(SearchState::Failed { error, .. }) => SearchView {
            results: Vec::new(),
            status: error.as_str().into(),
        },
    }
}

/// A closure that builds a frame from the current state.
fn builder(ctx: &DisplayContext, cfg: &GuiConfig, local: &Arc<Mutex<Local>>) -> Build {
    let state = ctx.state.clone();
    // Follows config reloads.
    let settings = ctx.settings();
    let local = Arc::clone(local);
    let startup = cfg.clone();
    Arc::new(move || {
        let (active, toggled) = {
            let l = lock(&local);
            (l.active, l.units_toggled)
        };
        let (presentation, strings) = settings.current();
        let current = state.current();
        // The location's own settings first, so the units button toggles what it shows.
        let mut p = presentation_at(&current, active, &presentation);
        if toggled {
            p.units = p.units.toggled();
        }
        let vm = view_with(&current, active, &p, &strings, now());
        // The settings dialog edits these; a reload brings them here (C1, C3).
        let gui = current
            .config
            .as_ref()
            .filter(|_| current.config_epoch > 0)
            .and_then(|c| super::settings::gui_config(&c.0))
            .unwrap_or_else(|| startup.clone());
        let mut opts = gui.window_options(startup.kiosk);
        let model = to_ui(&vm);
        {
            let mut l = lock(&local);
            l.cycle = gui.cycle_locations.secs();
            l.sky = model.sky;
        }
        opts.units_label = if p.units == UnitPrefs::IMPERIAL {
            "°C".into()
        } else {
            "°F".into()
        };
        Frame {
            model,
            opts,
            search: search_view(&current),
            radar: current
                .locations
                .get(vm.active)
                .and_then(|l| l.radar.clone()),
            page_ms: i64::from(gui.page_interval.secs()) * 1000,
        }
    })
}

/// Applies a frame to the desktop window.
fn apply_desktop(ui: &DesktopWindow, frame: &Frame) {
    apply(ui, &frame.model, &frame.opts);
    ui.set_search_results(ModelRc::new(VecModel::from(frame.search.results.clone())));
    ui.set_search_status(frame.search.status.clone());
    if let Some(radar) = &frame.radar {
        RADAR.with(|cell| {
            let mut cached = cell.borrow_mut();
            if cached.as_ref().is_none_or(|(r, _)| r != radar) {
                *cached = Some((radar.clone(), super::glue::radar_images(&radar.0)));
            }
            if let Some((_, images)) = cached.as_ref() {
                super::glue::apply_radar(ui, &radar.0, images);
            }
        });
    }
}

/// Applies a frame to the compact window.
fn apply_compact(ui: &CompactWindow, frame: &Frame) {
    crate::compact::apply_compact(ui, &frame.model, sky_for(&frame.model, &frame.opts));
    let [r, g, b] = frame.opts.accent;
    ui.global::<crate::Theme>()
        .set_accent(slint::Color::from_rgb_u8(r, g, b));
    // Slint durations are milliseconds; zero stops the window's page timer.
    if ui.get_page_interval() != frame.page_ms {
        ui.set_page_interval(frame.page_ms);
    }
}

/// Rebuilds on every state change and every [`REFRESH_VIEW`], applying on the UI
/// thread. Ends when the service or the window goes away.
fn spawn_watcher<F>(ctx: &DisplayContext, build: Build, apply_on_ui: F)
where
    F: Fn(Frame) -> bool + Send + 'static,
{
    let changes = ctx.state.subscribe();
    std::thread::spawn(move || {
        // A change or a timeout both mean "rebuild"; a disconnect ends the loop.
        while !matches!(
            changes.recv_timeout(REFRESH_VIEW),
            Err(RecvTimeoutError::Disconnected)
        ) {
            if !apply_on_ui(build()) {
                break;
            }
        }
    });
}

fn show_about(about: &Rc<RefCell<Option<AboutWindow>>>, attribution: &str) {
    let mut slot = about.borrow_mut();
    if slot.is_none() {
        let Ok(window) = AboutWindow::new() else {
            return;
        };
        window.set_version(env!("CARGO_PKG_VERSION").into());
        let weak = window.as_weak();
        window.on_close_requested(move || {
            if let Some(w) = weak.upgrade() {
                let _ = w.hide();
            }
        });
        *slot = Some(window);
    }
    if let Some(window) = slot.as_ref() {
        window.set_attribution(attribution.into());
        let _ = window.show();
    }
}

/// Rotates through the locations every `cycle_locations` (off when zero). It ticks
/// every second, so a changed interval applies without a restart.
fn cycle_timer(
    ctx: &DisplayContext,
    local: &Arc<Mutex<Local>>,
    rebuild: impl Fn() + 'static,
) -> Timer {
    let state = ctx.state.clone();
    let local = Arc::clone(local);
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_secs(1), move || {
        let turned = {
            let mut l = lock(&local);
            if l.cycle == 0 {
                l.since_cycle = 0;
                false
            } else {
                l.since_cycle += 1;
                let due = l.since_cycle >= l.cycle;
                if due {
                    l.since_cycle = 0;
                    l.active = (l.active + 1) % state.current().locations.len().max(1);
                }
                due
            }
        };
        if turned {
            rebuild();
        }
    });
    timer
}

fn wire_locations(
    ui: &DesktopWindow,
    ctx: &DisplayContext,
    local: &Arc<Mutex<Local>>,
    rebuild: &Rc<dyn Fn()>,
) {
    {
        let commands = ctx.commands.clone();
        let local = Arc::clone(local);
        ui.on_refresh(move || {
            let active = lock(&local).active;
            let _ = commands.send(Command::Refresh {
                location: Some(active),
            });
        });
    }
    {
        let local = Arc::clone(local);
        let rebuild = Rc::clone(rebuild);
        ui.on_location_selected(move |i| {
            lock(&local).active = usize::try_from(i).unwrap_or(0);
            rebuild();
        });
    }
    {
        let local = Arc::clone(local);
        let rebuild = Rc::clone(rebuild);
        ui.on_toggle_units(move || {
            {
                let mut l = lock(&local);
                l.units_toggled = !l.units_toggled;
            }
            rebuild();
        });
    }
    {
        let commands = ctx.commands.clone();
        ui.on_search(move |query| {
            let query = query.trim().to_string();
            if !query.is_empty() {
                let _ = commands.send(Command::Search { query });
            }
        });
    }
    {
        let commands = ctx.commands.clone();
        let state = ctx.state.clone();
        ui.on_choose_result(move |i| {
            if let Some(SearchState::Results { results, .. }) = &state.current().search
                && let Some(result) = usize::try_from(i).ok().and_then(|i| results.get(i))
            {
                let _ = commands.send(Command::AddLocation(result.clone()));
            }
        });
    }
    {
        let commands = ctx.commands.clone();
        let local = Arc::clone(local);
        ui.on_remove_location(move || {
            let index = {
                let mut l = lock(&local);
                let index = l.active;
                l.active = 0;
                index
            };
            let _ = commands.send(Command::RemoveLocation(index));
        });
    }
}

/// The settings dialog: filled from the loaded configuration when opened, edited
/// here as the user chooses, and saved as `SetConfig` with the changed keys
///.
fn wire_settings(ui: &DesktopWindow, ctx: &DisplayContext, local: &Arc<Mutex<Local>>) {
    // The form as opened, and as edited.
    let forms: Rc<RefCell<Option<(SettingsForm, SettingsForm)>>> = Rc::default();
    {
        let weak = ui.as_weak();
        let state = ctx.state.clone();
        let forms = Rc::clone(&forms);
        let local = Arc::clone(local);
        ui.on_open_settings(move || {
            let current = state.current();
            let (Some(ui), Some(loaded)) = (weak.upgrade(), current.config.as_ref()) else {
                // Attached to a daemon: its configuration is not ours to edit.
                return false;
            };
            let mut form = settings::form(&loaded.0);
            form.sky = lock(&local).sky;
            settings::show(&ui, &form);
            ui.set_settings_tab(0);
            *forms.borrow_mut() = Some((form.clone(), form));
            true
        });
    }
    {
        let forms = Rc::clone(&forms);
        let weak = ui.as_weak();
        ui.on_settings_choose(move |i, option| {
            if let (Some(ui), Some((_, form))) = (weak.upgrade(), forms.borrow_mut().as_mut()) {
                form.choose(index(i), index(option));
                settings::show_preview(&ui, form);
            }
        });
    }
    {
        let weak = ui.as_weak();
        let forms = Rc::clone(&forms);
        ui.on_settings_toggle(move |list, item, on| {
            if let (Some(ui), Some((_, form))) = (weak.upgrade(), forms.borrow_mut().as_mut()) {
                form.toggle(index(list), index(item), on);
                settings::show_lists(&ui, form);
            }
        });
    }
    {
        let weak = ui.as_weak();
        let forms = Rc::clone(&forms);
        ui.on_settings_move(move |list, item, up| {
            if let (Some(ui), Some((_, form))) = (weak.upgrade(), forms.borrow_mut().as_mut()) {
                form.move_item(index(list), index(item), up);
                settings::show_lists(&ui, form);
            }
        });
    }
    let commands = ctx.commands.clone();
    ui.on_save_settings(move || {
        let Some((before, after)) = forms.borrow_mut().take() else {
            return;
        };
        let edits = settings::edits(&before, &after);
        if !edits.is_empty() {
            let _ = commands.send(Command::SetConfig(edits));
        }
    });
}

/// A Slint index as a `usize` (negative ones become 0).
fn index(i: i32) -> usize {
    usize::try_from(i).unwrap_or(0)
}

fn run_desktop(cfg: &GuiConfig, ctx: &DisplayContext) -> Result<(), DisplayError> {
    let ui = DesktopWindow::new()?;
    let local = Arc::new(Mutex::new(Local::new(ctx.initial_location, cfg)));
    let build = builder(ctx, cfg, &local);
    apply_desktop(&ui, &build());
    if cfg.kiosk {
        ui.window().set_fullscreen(true);
    } else {
        #[allow(clippy::cast_precision_loss)]
        ui.window()
            .set_size(LogicalSize::new(cfg.size[0] as f32, cfg.size[1] as f32));
    }
    let rebuild: Rc<dyn Fn()> = {
        let weak = ui.as_weak();
        let build = Arc::clone(&build);
        Rc::new(move || {
            if let Some(ui) = weak.upgrade() {
                apply_desktop(&ui, &build());
            }
        })
    };
    wire_locations(&ui, ctx, &local, &rebuild);
    wire_settings(&ui, ctx, &local);
    let about = Rc::new(RefCell::new(None));
    {
        let weak = ui.as_weak();
        let about = Rc::clone(&about);
        ui.on_about(move || {
            let attribution = weak
                .upgrade()
                .map(|ui| ui.get_attribution().to_string())
                .unwrap_or_default();
            show_about(&about, &attribution);
        });
    }
    let _cycle = cycle_timer(ctx, &local, move || rebuild());
    let weak = ui.as_weak();
    spawn_watcher(ctx, build, move |frame| {
        weak.upgrade_in_event_loop(move |ui| apply_desktop(&ui, &frame))
            .is_ok()
    });
    ui.run()?;
    Ok(())
}

fn run_compact(cfg: &GuiConfig, ctx: &DisplayContext) -> Result<(), DisplayError> {
    let ui = CompactWindow::new()?;
    let local = Arc::new(Mutex::new(Local::new(ctx.initial_location, cfg)));
    let build = builder(ctx, cfg, &local);
    apply_compact(&ui, &build());
    if cfg.kiosk {
        ui.window().set_fullscreen(true);
    }
    {
        let commands = ctx.commands.clone();
        ui.on_refresh(move || {
            let _ = commands.send(Command::Refresh { location: None });
        });
    }
    let rebuild = {
        let weak = ui.as_weak();
        let build = Arc::clone(&build);
        move || {
            if let Some(ui) = weak.upgrade() {
                apply_compact(&ui, &build());
            }
        }
    };
    let _cycle = cycle_timer(ctx, &local, rebuild);
    let weak = ui.as_weak();
    spawn_watcher(ctx, build, move |frame| {
        weak.upgrade_in_event_loop(move |ui| apply_compact(&ui, &frame))
            .is_ok()
    });
    ui.run()?;
    Ok(())
}

impl DisplayPlugin for GuiPlugin {
    fn info(&self) -> DisplayInfo {
        DisplayInfo {
            name: "gui",
            description: DESCRIPTION,
            interactive: true,
            needs_tty: false,
        }
    }

    fn run(self: Box<Self>, ctx: DisplayContext) -> Result<(), DisplayError> {
        match self.cfg.mode {
            GuiMode::Desktop => run_desktop(&self.cfg, &ctx),
            GuiMode::Compact => run_compact(&self.cfg, &ctx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use whether_core::model::ProviderId;
    use whether_providers::GeocodeResult;

    #[test]
    fn factory_validates() {
        let bad: toml::Table = "font_scale = 9".parse().unwrap();
        assert_eq!((factory().validate)(&bad)[0].path, "display.gui.font_scale");
        let unknown: toml::Table = "colour = 1".parse().unwrap();
        assert_eq!((factory().validate)(&unknown)[0].path, "display.gui");
        let schema = (factory().schema)();
        assert!(schema.get("properties").is_some_and(
            |p| p.get("cycle_locations").is_some() && p.get("page_interval").is_some()
        ));
        assert!(factory().interactive);
    }

    #[test]
    fn search_views() {
        let mut state = AppState::default();
        assert_eq!(search_view(&state), SearchView::default());
        state.search = Some(SearchState::Searching {
            query: "Brooklyn".into(),
        });
        assert!(search_view(&state).status.contains("Brooklyn"));
        let place = GeocodeResult {
            name: "Brooklyn".into(),
            latitude: 40.65,
            longitude: -73.95,
            country_code: Some("US".into()),
            admin1: Some("New York".into()),
            postcode: None,
            timezone: None,
            elevation: None,
            population: None,
            source: ProviderId::OpenMeteo,
        };
        state.search = Some(SearchState::Results {
            query: "Brooklyn".into(),
            results: vec![place],
        });
        let view = search_view(&state);
        assert_eq!(
            view.results,
            vec![SharedString::from("Brooklyn, New York, US")]
        );
        state.search = Some(SearchState::Results {
            query: "zz".into(),
            results: vec![],
        });
        assert_eq!(search_view(&state).status, "No places found");
        state.search = Some(SearchState::Failed {
            query: "x".into(),
            error: "offline".into(),
        });
        assert_eq!(search_view(&state).status, "offline");
    }
}
