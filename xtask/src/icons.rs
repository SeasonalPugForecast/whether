//! `cargo xtask icons fetch | raster`: vendor the GUI's icons and fonts.
//!
//! `fetch` downloads pinned releases (checked by SHA-256), copies only the Meteocons
//! "fill" SVGs that `IconId`/`UiIcon` use plus their MIT license, and the Inter TTFs
//! plus the OFL. `raster` renders the SVGs to 32 and 64 px PNGs for MCU builds. The
//! output is committed; builds never download anything.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use whether_core::icon::{IconId, UiIcon};

const METEOCONS_VERSION: &str = "0.1.0";
const METEOCONS_URL: &str =
    "https://registry.npmjs.org/@meteocons/svg-static/-/svg-static-0.1.0.tgz";
const METEOCONS_SHA256: &str = "7aead065bfd18127392dd39a67f18c30580e45ddb2facef893839caebf0d029b";
const INTER_VERSION: &str = "4.1";
const INTER_URL: &str = "https://github.com/rsms/inter/releases/download/v4.1/Inter-4.1.zip";
const INTER_SHA256: &str = "9883fdd4a49d4fb66bd8177ba6625ef9a64aa45899767dde3d36aa425756b11e";
const INTER_WEIGHTS: [&str; 4] = ["Regular", "Medium", "SemiBold", "Light"];

/// Meteocons file for each `IconId`, in `IconId::ALL` order.
pub(crate) const CONDITION_FILES: &[(IconId, &str)] = &[
    (IconId::ClearDay, "clear-day"),
    (IconId::ClearNight, "clear-night"),
    (IconId::PartlyCloudyDay, "partly-cloudy-day"),
    (IconId::PartlyCloudyNight, "partly-cloudy-night"),
    (IconId::Cloudy, "cloudy"),
    (IconId::OvercastDay, "overcast-day"),
    (IconId::OvercastNight, "overcast-night"),
    (IconId::FogDay, "fog-day"),
    (IconId::FogNight, "fog-night"),
    (IconId::Haze, "haze"),
    (IconId::Smoke, "smoke"),
    (IconId::Dust, "dust"),
    (IconId::Drizzle, "drizzle"),
    (IconId::Rain, "rain"),
    (IconId::ExtremeRain, "extreme-rain"),
    (IconId::ShowersDay, "partly-cloudy-day-rain"),
    (IconId::ShowersNight, "partly-cloudy-night-rain"),
    (IconId::Sleet, "sleet"),
    (IconId::Snow, "snow"),
    (IconId::ExtremeSnow, "extreme-snow"),
    (IconId::SnowShowersDay, "partly-cloudy-day-snow"),
    (IconId::SnowShowersNight, "partly-cloudy-night-snow"),
    (IconId::ThunderstormsDay, "thunderstorms-day"),
    (IconId::ThunderstormsNight, "thunderstorms-night"),
    (IconId::ThunderstormsRain, "thunderstorms-rain"),
    (IconId::Hail, "hail"),
    (IconId::Wind, "wind"),
    (IconId::Tornado, "tornado"),
    (IconId::NotAvailable, "not-available"),
];

/// Meteocons file for each `UiIcon`, in `UiIcon::ALL` order.
pub(crate) const UI_FILES: &[(UiIcon, &str)] = &[
    (UiIcon::Sunrise, "sunrise"),
    (UiIcon::Sunset, "sunset"),
    (UiIcon::Moonrise, "moonrise"),
    (UiIcon::Moonset, "moonset"),
    (UiIcon::Thermometer, "thermometer"),
    (UiIcon::Humidity, "humidity"),
    (UiIcon::Barometer, "barometer"),
    (UiIcon::Wind, "wind"),
    (UiIcon::Raindrop, "raindrop"),
    (UiIcon::UvIndex, "uv-index"),
    (UiIcon::Visibility, "mist"),
    (UiIcon::Cloud, "cloudy"),
    (UiIcon::MoonNew, "moon-new"),
    (UiIcon::MoonWaxingCrescent, "moon-waxing-crescent"),
    (UiIcon::MoonFirstQuarter, "moon-first-quarter"),
    (UiIcon::MoonWaxingGibbous, "moon-waxing-gibbous"),
    (UiIcon::MoonFull, "moon-full"),
    (UiIcon::MoonWaningGibbous, "moon-waning-gibbous"),
    (UiIcon::MoonLastQuarter, "moon-last-quarter"),
    (UiIcon::MoonWaningCrescent, "moon-waning-crescent"),
];

fn gui_assets() -> PathBuf {
    crate::workspace_root().join("crates/whether-gui/assets")
}

fn run(program: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("{program}: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "{program} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Downloads `url` into `dir` unless a file with the right checksum is there.
fn download(url: &str, sha256: &str, dir: &Path) -> Result<PathBuf, String> {
    let name = url.rsplit('/').next().ok_or("bad URL")?;
    let path = dir.join(name);
    let path_str = path.to_str().ok_or("non-UTF-8 path")?;
    let matches = |p: &str| run("sha256sum", &[p]).is_ok_and(|out| out.starts_with(sha256));
    if !matches(path_str) {
        run("curl", &["-sSfL", "-o", path_str, url])?;
        if !matches(path_str) {
            let _ = fs::remove_file(&path);
            return Err(format!("{name}: checksum mismatch (expected {sha256})"));
        }
    }
    Ok(path)
}

/// The file names to vendor, without duplicates.
pub(crate) fn svg_names() -> Vec<&'static str> {
    let mut names: Vec<&str> = CONDITION_FILES
        .iter()
        .map(|(_, n)| *n)
        .chain(UI_FILES.iter().map(|(_, n)| *n))
        .collect();
    names.sort_unstable();
    names.dedup();
    names
}

fn readme() -> String {
    let mut out = format!(
        "# Vendored icons\n\nMeteocons by Bas Milius, MIT license (`meteocons/LICENSE`), from the npm package\n`@meteocons/svg-static` {METEOCONS_VERSION} ({METEOCONS_URL},\nSHA-256 `{METEOCONS_SHA256}`). Only the \"fill\" SVGs the app uses are vendored;\n`png/{{32,64}}/` are rendered from them by `cargo xtask icons raster`. Regenerate\nwith `cargo xtask icons fetch`.\n\n| IconId | File |\n|---|---|\n"
    );
    for (id, name) in CONDITION_FILES {
        let _ = writeln!(out, "| `{id:?}` | `{name}.svg` |");
    }
    out.push_str("\n| UiIcon | File |\n|---|---|\n");
    for (id, name) in UI_FILES {
        let _ = writeln!(out, "| `{id:?}` | `{name}.svg` |");
    }
    out
}

fn fetch() -> Result<(), String> {
    let downloads = crate::workspace_root().join("target/xtask-downloads");
    fs::create_dir_all(&downloads).map_err(|e| e.to_string())?;
    let assets = gui_assets();

    let tarball = download(METEOCONS_URL, METEOCONS_SHA256, &downloads)?;
    let unpacked = downloads.join("meteocons");
    fs::create_dir_all(&unpacked).map_err(|e| e.to_string())?;
    run(
        "tar",
        &[
            "xzf",
            tarball.to_str().ok_or("path")?,
            "-C",
            unpacked.to_str().ok_or("path")?,
        ],
    )?;
    let icons = assets.join("icons/meteocons/fill");
    fs::create_dir_all(&icons).map_err(|e| e.to_string())?;
    for name in svg_names() {
        let from = unpacked.join(format!("package/fill/{name}.svg"));
        fs::copy(&from, icons.join(format!("{name}.svg")))
            .map_err(|e| format!("{}: {e}", from.display()))?;
    }
    fs::copy(
        unpacked.join("package/LICENSE"),
        assets.join("icons/meteocons/LICENSE"),
    )
    .map_err(|e| e.to_string())?;
    fs::write(assets.join("icons/README.md"), readme()).map_err(|e| e.to_string())?;

    let zip = download(INTER_URL, INTER_SHA256, &downloads)?;
    let fonts = assets.join("fonts");
    fs::create_dir_all(&fonts).map_err(|e| e.to_string())?;
    let zip = zip.to_str().ok_or("path")?;
    for weight in INTER_WEIGHTS {
        let member = format!("extras/ttf/Inter-{weight}.ttf");
        run(
            "unzip",
            &[
                "-o",
                "-j",
                zip,
                &member,
                "-d",
                fonts.to_str().ok_or("path")?,
            ],
        )?;
    }
    run(
        "unzip",
        &[
            "-o",
            "-j",
            zip,
            "LICENSE.txt",
            "-d",
            fonts.to_str().ok_or("path")?,
        ],
    )?;
    fs::rename(fonts.join("LICENSE.txt"), fonts.join("OFL.txt")).map_err(|e| e.to_string())?;
    fs::write(
        fonts.join("README.md"),
        format!("# Fonts\n\nInter {INTER_VERSION} by Rasmus Andersson, SIL Open Font License 1.1 (`OFL.txt`), from\n{INTER_URL} (SHA-256 `{INTER_SHA256}`).\n"),
    )
    .map_err(|e| e.to_string())?;
    println!(
        "vendored {} icons and {} fonts into {}",
        svg_names().len(),
        INTER_WEIGHTS.len(),
        assets.display()
    );
    Ok(())
}

/// Renders one SVG to a square RGBA PNG of `size` pixels.
pub(crate) fn rasterize(svg: &[u8], size: u32) -> Result<Vec<u8>, String> {
    let tree = resvg::usvg::Tree::from_data(svg, &resvg::usvg::Options::default())
        .map_err(|e| e.to_string())?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size).ok_or("empty size")?;
    let view = tree.size();
    let scale = size as f32 / view.width().max(view.height());
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap.encode_png().map_err(|e| e.to_string())
}

fn raster() -> Result<(), String> {
    let assets = gui_assets();
    for size in [32, 64] {
        let dir = assets.join(format!("icons/png/{size}"));
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        for name in svg_names() {
            let svg_path = assets.join(format!("icons/meteocons/fill/{name}.svg"));
            let svg = fs::read(&svg_path).map_err(|e| {
                format!("{}: {e}; run `cargo xtask icons fetch`", svg_path.display())
            })?;
            fs::write(dir.join(format!("{name}.png")), rasterize(&svg, size)?)
                .map_err(|e| e.to_string())?;
        }
    }
    app_icon(&assets)?;
    println!(
        "rendered {} icons at 32 and 64 px and the app icon at {APP_ICON_SIZES:?} px",
        svg_names().len()
    );
    Ok(())
}

/// The app icon (desktop entry, packages) comes from this Meteocons icon.
const APP_ICON: &str = "partly-cloudy-day";
/// Rendered app icon sizes, written under `packaging/icons/hicolor/`.
pub(crate) const APP_ICON_SIZES: [u32; 2] = [128, 256];

/// Renders the app icon into `packaging/icons/hicolor/` and copies the SVG as the
/// scalable variant.
fn app_icon(assets: &Path) -> Result<(), String> {
    let svg_path = assets.join(format!("icons/meteocons/fill/{APP_ICON}.svg"));
    let svg = fs::read(&svg_path).map_err(|e| format!("{}: {e}", svg_path.display()))?;
    let hicolor = crate::workspace_root().join("packaging/icons/hicolor");
    for size in APP_ICON_SIZES {
        let dir = hicolor.join(format!("{size}x{size}/apps"));
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        fs::write(dir.join("whether.png"), rasterize(&svg, size)?).map_err(|e| e.to_string())?;
    }
    let scalable = hicolor.join("scalable/apps");
    fs::create_dir_all(&scalable).map_err(|e| e.to_string())?;
    fs::write(scalable.join("whether.svg"), &svg).map_err(|e| e.to_string())
}

pub(crate) fn run_command(args: &[&str]) -> Result<(), String> {
    match args {
        ["fetch"] => fetch(),
        ["raster"] => raster(),
        _ => Err("usage: cargo xtask icons fetch|raster".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_follow_the_enum_order() {
        let ids: Vec<IconId> = CONDITION_FILES.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, IconId::ALL);
        let ui: Vec<UiIcon> = UI_FILES.iter().map(|(id, _)| *id).collect();
        assert_eq!(ui, UiIcon::ALL);
    }

    #[test]
    fn desktop_entry_and_app_icon_are_packaged() {
        let packaging = crate::workspace_root().join("packaging");
        let entry = fs::read_to_string(packaging.join("whether.desktop")).unwrap();
        for line in [
            "Exec=whether gui",
            "Icon=whether",
            "Categories=Utility;",
            "Keywords=weather;forecast;",
        ] {
            assert!(entry.lines().any(|l| l == line), "{line}");
        }
        for size in APP_ICON_SIZES {
            let png = packaging.join(format!("icons/hicolor/{size}x{size}/apps/whether.png"));
            let decoder = png::Decoder::new(std::io::BufReader::new(fs::File::open(&png).unwrap()));
            let info = decoder.read_info().unwrap();
            assert_eq!((info.info().width, info.info().height), (size, size));
        }
        assert!(
            packaging
                .join("icons/hicolor/scalable/apps/whether.svg")
                .exists()
        );
    }

    #[test]
    fn rasterizes_a_simple_svg() {
        let svg = br##"<svg viewBox="0 0 10 10" xmlns="http://www.w3.org/2000/svg"><rect width="10" height="10" fill="#f00"/></svg>"##;
        let png = rasterize(svg, 32).unwrap();
        assert_eq!(&png[1..4], b"PNG");
    }

    #[test]
    fn readme_lists_every_icon() {
        let text = readme();
        assert!(
            text.contains("| `ClearDay` | `clear-day.svg` |")
                && text.contains("| `MoonFull` | `moon-full.svg` |")
        );
    }
}
