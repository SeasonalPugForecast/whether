//! `whether-sim`: renders the firmware's display pages on the host.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, ValueEnum};
use whether_sim::{Page, Scene, Sky, board, panel, save_png, tui};

/// Which firmware display to render.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum Display {
    /// The device TUI (mousefood).
    Tui,
    /// The panel GUI.
    Panel,
}

impl Display {
    /// The pages this display has, in order (the window steps through them).
    #[cfg_attr(not(feature = "window"), allow(dead_code))]
    fn pages(self) -> &'static [Page] {
        match self {
            Display::Tui => &Page::ALL,
            Display::Panel => &[Page::Now, Page::Daily, Page::Details],
        }
    }

    fn name(self) -> &'static str {
        match self {
            Display::Tui => "tui",
            Display::Panel => "panel",
        }
    }
}

/// Which built-in sample to show.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum Sample {
    /// The sample as it is.
    Plain,
    /// The sample with a flood warning and a wind advisory.
    Alerts,
}

/// Render the firmware's display pages for a board profile.
#[derive(Debug, Parser)]
#[command(version)]
struct Args {
    /// Board profile from firmware/boards.toml.
    #[arg(long)]
    board: String,
    /// The display to render.
    #[arg(long, value_enum, default_value = "tui")]
    display: Display,
    /// The page to render.
    #[arg(long, value_enum, default_value = "now")]
    page: Page,
    /// A cache file written by `whether` (default: the built-in sample).
    #[arg(long, conflicts_with = "sample")]
    fixture: Option<PathBuf>,
    /// Use the built-in sample data (the default), optionally with alerts.
    #[arg(long, value_enum, num_args = 0..=1, default_missing_value = "plain")]
    sample: Option<Sample>,
    /// Output PNG (default: <board>-<display>-<page>.png).
    #[arg(long)]
    out: Option<PathBuf>,
    /// Open a window; the arrow keys change page (needs the `window` feature).
    #[arg(long)]
    window: bool,
    /// Restyle the sample as this sky (previews of the backgrounds and icons).
    #[arg(long, value_enum)]
    sky: Option<Sky>,
    /// Turn the panel upright (320×480 instead of 480×320).
    #[arg(long)]
    portrait: bool,
    /// Board profiles file.
    #[arg(long, default_value_os_t = board::default_path())]
    boards: PathBuf,
}

fn render(
    args: &Args,
    board: &board::Board,
    scene: &Scene,
    page: Page,
) -> anyhow::Result<
    embedded_graphics_simulator::SimulatorDisplay<embedded_graphics::pixelcolor::Rgb565>,
> {
    let vm = match args.sky {
        Some(sky) => sky.apply(scene.view()),
        None => scene.view(),
    };
    match args.display {
        Display::Tui => tui::render(board, &vm, page),
        Display::Panel => Ok(panel::render(board, &vm, panel::page(page)?, args.portrait)),
    }
}

#[cfg(feature = "window")]
fn window(args: &Args, board: &board::Board, scene: &Scene) -> anyhow::Result<()> {
    use embedded_graphics_simulator::sdl2::Keycode;
    use embedded_graphics_simulator::{OutputSettingsBuilder, SimulatorEvent, Window};

    let pages = args.display.pages();
    let mut index = pages.iter().position(|p| *p == args.page).unwrap_or(0);
    let mut display = render(args, board, scene, pages[index])?;
    let mut win = Window::new(
        &format!("whether-sim: {}", args.board),
        &OutputSettingsBuilder::new().build(),
    );
    loop {
        win.update(&display);
        let mut changed = false;
        for event in win.events() {
            match event {
                SimulatorEvent::Quit => return Ok(()),
                SimulatorEvent::KeyDown { keycode, .. } => {
                    if keycode == Keycode::RIGHT || keycode == Keycode::DOWN {
                        index = (index + 1) % pages.len();
                        changed = true;
                    } else if keycode == Keycode::LEFT || keycode == Keycode::UP {
                        index = (index + pages.len() - 1) % pages.len();
                        changed = true;
                    } else if keycode == Keycode::ESCAPE || keycode == Keycode::Q {
                        return Ok(());
                    }
                }
                _ => {}
            }
        }
        if changed {
            display = render(args, board, scene, pages[index])?;
        }
    }
}

#[cfg(not(feature = "window"))]
fn window(_: &Args, _: &board::Board, _: &Scene) -> anyhow::Result<()> {
    anyhow::bail!("--window needs a build with `--features window` (SDL2)")
}

fn run(args: &Args) -> anyhow::Result<()> {
    let board = board::load(&args.boards, &args.board)?;
    let scene = match &args.fixture {
        Some(path) => Scene::from_cache(path)?,
        None if args.sample == Some(Sample::Alerts) => Scene::sample_with_alerts(),
        None => Scene::sample(),
    };
    if args.window {
        return window(args, &board, &scene);
    }
    let out = args.out.clone().unwrap_or_else(|| {
        PathBuf::from(format!(
            "{}-{}-{}.png",
            args.board,
            args.display.name(),
            args.page.name()
        ))
    });
    save_png(&render(args, &board, &scene, args.page)?, &out)?;
    println!("wrote {}", out.display());
    Ok(())
}

fn main() -> ExitCode {
    match run(&Args::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("whether-sim: {e:#}");
            ExitCode::FAILURE
        }
    }
}
