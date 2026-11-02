//! Compiles the Slint UI: the desktop file, or the MCU file with glyphs and images
//! embedded for the software renderer. `@icons` points at SVG icons on the desktop
//! and at pre-rendered PNGs on MCUs.

use std::collections::HashMap;
use std::path::PathBuf;

fn main() {
    let mcu = std::env::var_os("CARGO_FEATURE_MCU").is_some();
    let icons = if mcu {
        "ui/icons-png/icons.slint"
    } else {
        "ui/icons-svg/icons.slint"
    };
    let library = HashMap::from([("icons".to_string(), PathBuf::from(icons))]);
    let config = slint_build::CompilerConfiguration::new().with_library_paths(library);
    let result = if mcu {
        let config =
            config.embed_resources(slint_build::EmbedResourcesKind::EmbedForSoftwareRenderer);
        slint_build::compile_with_config("ui/mcu.slint", config)
    } else {
        slint_build::compile_with_config("ui/app.slint", config)
    };
    if let Err(e) = result {
        eprintln!("Slint compile error: {e}");
        std::process::exit(1);
    }
}
