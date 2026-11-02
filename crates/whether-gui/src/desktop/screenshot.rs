//! Headless rendering to PNG with Slint's software renderer, for tests, docs and
//! `whether gui --screenshot`.
//!
//! Slint's platform is global and its objects are not `Send`, so one dedicated thread
//! installs a minimal software platform once and does every render.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Mutex, OnceLock};

use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, PlatformError, WindowAdapter};
use slint::{
    ComponentHandle, ModelRc, PhysicalSize, Rgb8Pixel, SharedPixelBuffer, SharedString, VecModel,
};

use super::glue::{WindowOptions, apply};
use crate::model::UiModel;
use crate::{AboutWindow, CompactWindow, DesktopWindow, Gallery};

/// Which window to render.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Which {
    /// The desktop window.
    Desktop,
    /// The compact window on page 0 (now), 1 (forecast), 2 (details) or 3 (alerts,
    /// while there are any).
    Compact(i32),
    /// The desktop window with the search overlay open, showing these results.
    DesktopSearch(&'static [&'static str]),
    /// The desktop window with the settings dialog open on this tab, showing the
    /// defaults for one location.
    DesktopSettings(i32),
    /// The desktop window with this radar imagery (shown when the model has radar
    /// labels and the radar section is on); the card shows the latest frame.
    DesktopRadar(&'static whether_display::RadarImages),
    /// The About dialog.
    About,
    /// Every component with sample data (development aid).
    Gallery,
}

/// The configuration the settings screenshots show: the defaults for one place.
const SETTINGS_SAMPLE: &str =
    "[[location]]\nname = \"New York\"\nlatitude = 40.7143\nlongitude = -74.00597\n";

/// Why a screenshot failed.
#[derive(Debug, thiserror::Error)]
pub enum ScreenshotError {
    /// Slint could not create or show the window.
    #[error("slint: {0}")]
    Slint(String),
    /// The PNG could not be written.
    #[error("{}: {source}", path.display())]
    Io {
        /// The output file.
        path: PathBuf,
        /// The error.
        source: std::io::Error,
    },
    /// The renderer thread is gone.
    #[error("the render thread stopped")]
    Thread,
}

struct HeadlessPlatform {
    window: Rc<MinimalSoftwareWindow>,
}

impl Platform for HeadlessPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.window.clone())
    }
}

type Job = Box<dyn FnOnce(&Rc<MinimalSoftwareWindow>) + Send>;

/// The render thread's job queue, started on first use.
fn renderer() -> &'static Mutex<Sender<Job>> {
    static SENDER: OnceLock<Mutex<Sender<Job>>> = OnceLock::new();
    SENDER.get_or_init(|| {
        let (tx, rx) = channel::<Job>();
        std::thread::Builder::new()
            .name("whether-screenshots".into())
            .spawn(move || {
                let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
                let installed = slint::platform::set_platform(Box::new(HeadlessPlatform {
                    window: window.clone(),
                }));
                if installed.is_err() {
                    return;
                }
                for job in rx {
                    job(&window);
                }
            })
            .map_or_else(|_| (), |_| ());
        Mutex::new(tx)
    })
}

/// Shows `window`, renders one frame at `size` and returns the pixels.
fn capture(
    window: &Rc<MinimalSoftwareWindow>,
    component: &impl ComponentHandle,
    (width, height): (u32, u32),
) -> Result<SharedPixelBuffer<Rgb8Pixel>, ScreenshotError> {
    window.set_size(PhysicalSize::new(width, height));
    component
        .show()
        .map_err(|e| ScreenshotError::Slint(e.to_string()))?;
    window.request_redraw();
    let mut buffer = SharedPixelBuffer::<Rgb8Pixel>::new(width, height);
    let stride = buffer.width() as usize;
    window.draw_if_needed(|renderer| {
        renderer.render(buffer.make_mut_slice(), stride);
    });
    component
        .hide()
        .map_err(|e| ScreenshotError::Slint(e.to_string()))?;
    Ok(buffer)
}

/// Encodes RGB pixels as a PNG file.
fn write_png(buffer: &SharedPixelBuffer<Rgb8Pixel>, out: &Path) -> Result<(), ScreenshotError> {
    let io = |source| ScreenshotError::Io {
        path: out.to_path_buf(),
        source,
    };
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).map_err(io)?;
    }
    let file = std::fs::File::create(out).map_err(io)?;
    let mut encoder = png::Encoder::new(
        std::io::BufWriter::new(file),
        buffer.width(),
        buffer.height(),
    );
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let to_io = |e: png::EncodingError| io(std::io::Error::other(e));
    let mut writer = encoder.write_header().map_err(to_io)?;
    writer.write_image_data(buffer.as_bytes()).map_err(to_io)?;
    writer.finish().map_err(to_io)
}

fn model<T: Clone + 'static>(items: &[T]) -> ModelRc<T> {
    ModelRc::new(VecModel::from(items.to_vec()))
}

fn slint_error(e: impl std::fmt::Display) -> ScreenshotError {
    ScreenshotError::Slint(e.to_string())
}

fn gallery(m: &UiModel) -> Result<Gallery, ScreenshotError> {
    let ui = Gallery::new().map_err(slint_error)?;
    ui.set_current(m.current.clone());
    ui.set_hours(model(&m.hours));
    ui.set_days(model(&m.days));
    ui.set_details(model(&m.details));
    ui.set_air(m.air.clone());
    ui.set_sun(m.sun.clone());
    ui.set_status(m.status.clone());
    let names: Vec<SharedString> = m.tabs.iter().map(|t| t.name.clone()).collect();
    ui.set_locations(model(&names));
    ui.set_attribution(m.attribution.clone());
    let temps: Vec<f32> = m.hours.iter().take(24).map(|h| h.temp_frac).collect();
    let (line, area) = crate::charts::commands(&temps);
    ui.set_chart_line(line.into());
    ui.set_chart_area(area.into());
    Ok(ui)
}

/// Renders `which` showing `m` at `size` (physical pixels) to `out`.
pub fn render_png(
    which: Which,
    m: &UiModel,
    opts: &WindowOptions,
    size: (u32, u32),
    out: &Path,
) -> Result<(), ScreenshotError> {
    let opts = opts.clone();
    let (done_tx, done_rx) = channel();
    let out_path = out.to_path_buf();
    let m = m.clone();
    let job: Job = Box::new(move |window| {
        let result = (|| {
            let buffer = match which {
                Which::Desktop => {
                    let ui = DesktopWindow::new().map_err(slint_error)?;
                    apply(&ui, &m, &opts);
                    capture(window, &ui, size)?
                }
                Which::Compact(page) => {
                    let ui = CompactWindow::new().map_err(slint_error)?;
                    let sky = super::glue::sky_for(&m, &opts);
                    crate::compact::apply_compact(&ui, &m, sky);
                    ui.set_page(page);
                    capture(window, &ui, size)?
                }
                Which::DesktopSearch(results) => {
                    let ui = DesktopWindow::new().map_err(slint_error)?;
                    apply(&ui, &m, &opts);
                    let names: Vec<SharedString> =
                        results.iter().map(|r| SharedString::from(*r)).collect();
                    ui.set_search_results(model(&names));
                    ui.set_search_open(true);
                    capture(window, &ui, size)?
                }
                Which::DesktopRadar(radar) => {
                    let ui = DesktopWindow::new().map_err(slint_error)?;
                    apply(&ui, &m, &opts);
                    super::glue::apply_radar(&ui, radar, &super::glue::radar_images(radar));
                    capture(window, &ui, size)?
                }
                Which::DesktopSettings(tab) => {
                    let ui = DesktopWindow::new().map_err(slint_error)?;
                    apply(&ui, &m, &opts);
                    let cfg = whether_config::parse(SETTINGS_SAMPLE).map_err(slint_error)?;
                    super::settings::show(&ui, &super::settings::form(&cfg));
                    ui.set_settings_tab(tab);
                    ui.set_settings_open(true);
                    capture(window, &ui, size)?
                }
                Which::About => {
                    let ui = AboutWindow::new().map_err(slint_error)?;
                    ui.set_version(env!("CARGO_PKG_VERSION").into());
                    ui.set_attribution(m.attribution.clone());
                    capture(window, &ui, size)?
                }
                Which::Gallery => capture(window, &gallery(&m)?, size)?,
            };
            write_png(&buffer, &out_path)
        })();
        let _ = done_tx.send(result);
    });
    renderer()
        .lock()
        .map_err(|_| ScreenshotError::Thread)?
        .send(job)
        .map_err(|_| ScreenshotError::Thread)?;
    done_rx.recv().map_err(|_| ScreenshotError::Thread)?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screenshots() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/screenshots")
    }

    #[test]
    fn gallery_screenshot() {
        let m = crate::model::to_ui(&crate::model::tests::sample_vm());
        let out = screenshots().join("gallery.png");
        render_png(
            Which::Gallery,
            &m,
            &WindowOptions::default(),
            (1200, 860),
            &out,
        )
        .unwrap();
        assert!(out.exists());
    }

    #[test]
    fn about_screenshot() {
        let m = crate::model::to_ui(&crate::model::tests::sample_vm());
        let out = screenshots().join("about.png");
        render_png(
            Which::About,
            &m,
            &WindowOptions::default(),
            (520, 560),
            &out,
        )
        .unwrap();
        assert!(out.exists());
    }

    #[test]
    fn stub_screenshot() {
        let out = screenshots().join("stub.png");
        render_png(
            Which::Desktop,
            &UiModel::default(),
            &WindowOptions::default(),
            (800, 600),
            &out,
        )
        .unwrap();
        let decoder =
            png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&out).unwrap()));
        let reader = decoder.read_info().unwrap();
        assert_eq!((reader.info().width, reader.info().height), (800, 600));
    }
}
