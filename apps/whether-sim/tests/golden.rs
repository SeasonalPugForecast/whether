//! Firmware page goldens per board, display and page. Rendering is deterministic, so
//! the PNGs must match exactly; `UPDATE_GOLDEN=1` rewrites them. Every render is also
//! written to `target/screenshots/sim/` for inspection.

use std::path::PathBuf;

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::Dimensions;
use embedded_graphics_simulator::SimulatorDisplay;
use whether_sim::{Page, Scene, Sky, board, panel, save_png, tui};

fn check(name: &str, display: &SimulatorDisplay<Rgb565>) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    save_png(
        display,
        &root.join(format!("../../target/screenshots/sim/{name}.png")),
    )
    .unwrap();
    let golden = root.join(format!("tests/golden/{name}.png"));
    if std::env::var_os("UPDATE_GOLDEN").is_some() || !golden.exists() {
        save_png(display, &golden).unwrap();
        return;
    }
    let expected = SimulatorDisplay::<Rgb565>::load_png(&golden).unwrap();
    assert_eq!(
        expected.bounding_box(),
        display.bounding_box(),
        "{name}: size changed"
    );
    assert!(
        display.diff(&expected).is_none(),
        "{name} differs from its golden (see target/screenshots/sim/{name}.png)"
    );
}

#[test]
fn tui_goldens() {
    let scene = Scene::sample();
    let vm = scene.view();
    for name in ["esp32-wroom-spi", "cyd"] {
        let board = board::load(&board::default_path(), name).unwrap();
        for page in Page::ALL {
            let display = tui::render(&board, &vm, page).unwrap();
            check(&format!("{name}-tui-{}", page.name()), &display);
        }
    }
}

#[test]
fn panel_goldens() {
    let scene = Scene::sample();
    let boards = [
        ("esp32-wroom-spi", false, "esp32-wroom-spi"),
        ("esp32-wroom-spi", true, "esp32-wroom-spi-portrait"),
        ("cyd", false, "cyd"),
    ];
    for (name, portrait, label) in boards {
        let board = board::load(&board::default_path(), name).unwrap();
        for sky in Sky::ALL {
            let vm = sky.apply(scene.view());
            for page in [Page::Now, Page::Daily, Page::Details] {
                let display = panel::render(&board, &vm, panel::page(page).unwrap(), portrait);
                check(
                    &format!("{label}-panel-{}-{}", page.name(), sky.name()),
                    &display,
                );
            }
        }
    }
    assert!(panel::page(Page::Hourly).is_err());
}

#[test]
fn tui_alert_goldens() {
    let vm = Scene::sample_with_alerts().view();
    for name in ["esp32-wroom-spi", "cyd"] {
        let board = board::load(&board::default_path(), name).unwrap();
        let display = tui::render(&board, &vm, Page::Now).unwrap();
        check(&format!("{name}-tui-now-alerts"), &display);
    }
}

#[test]
fn panel_alert_goldens() {
    let vm = Scene::sample_with_alerts().view();
    assert_eq!(vm.alerts.len(), 2);
    let boards = [
        ("esp32-wroom-spi", false, "esp32-wroom-spi"),
        ("esp32-wroom-spi", true, "esp32-wroom-spi-portrait"),
        ("cyd", false, "cyd"),
    ];
    for (name, portrait, label) in boards {
        let board = board::load(&board::default_path(), name).unwrap();
        let display = panel::render(&board, &vm, panel::page(Page::Now).unwrap(), portrait);
        check(&format!("{label}-panel-now-alerts"), &display);
    }
}
