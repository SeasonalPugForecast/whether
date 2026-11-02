//! Board profiles from `firmware/boards.toml`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use mousefood::embedded_graphics::mono_font::MonoFont;
use mousefood::fonts;
use serde::Deserialize;

/// What the sim needs from one profile (the firmware reads the rest).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Board {
    /// Width in pixels (landscape).
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Pixel format: `rgb565`, `mono` or `gray4`.
    pub color: String,
    /// The display the board starts with.
    pub default_display: String,
    /// mousefood font for the TUI, such as `6x12`.
    pub tui_font: String,
}

/// `firmware/boards.toml` in the workspace.
pub fn default_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../firmware/boards.toml")
}

/// Every profile in `text`, by name.
pub fn parse(text: &str) -> anyhow::Result<BTreeMap<String, Board>> {
    Ok(toml::from_str(text)?)
}

/// The profile `name` from the file at `path`.
pub fn load(path: &Path, name: &str) -> anyhow::Result<Board> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut boards = parse(&text).with_context(|| format!("parsing {}", path.display()))?;
    let known: Vec<String> = boards.keys().cloned().collect();
    let Some(board) = boards.remove(name) else {
        bail!("unknown board `{name}`; known: {}", known.join(", "));
    };
    if board.color != "rgb565" {
        bail!("board `{name}`: only rgb565 panels are simulated so far");
    }
    Ok(board)
}

/// The mousefood font called `name` (`WxH`).
pub fn font(name: &str) -> anyhow::Result<MonoFont<'static>> {
    Ok(match name {
        "5x8" => fonts::MONO_5X8,
        "6x10" => fonts::MONO_6X10,
        "6x12" => fonts::MONO_6X12,
        "6x13" => fonts::MONO_6X13,
        "7x13" => fonts::MONO_7X13,
        "7x14" => fonts::MONO_7X14,
        "8x13" => fonts::MONO_8X13,
        "9x15" => fonts::MONO_9X15,
        "9x18" => fonts::MONO_9X18,
        "10x20" => fonts::MONO_10X20,
        other => bail!("unknown tui_font `{other}`"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_firmware_profiles() {
        let boards = parse(&std::fs::read_to_string(default_path()).unwrap()).unwrap();
        let wroom = &boards["esp32-wroom-spi"];
        assert_eq!((wroom.width, wroom.height), (480, 320));
        assert_eq!(wroom.default_display, "panel");
        assert_eq!((boards["cyd"].width, boards["cyd"].height), (320, 240));
        for board in boards.values() {
            assert!(board.width >= board.height, "profiles are landscape");
            font(&board.tui_font).unwrap();
        }
    }

    #[test]
    fn rejects_unknown_boards_and_fonts() {
        let err = load(&default_path(), "esp8266").unwrap_err().to_string();
        assert!(err.contains("known: "), "{err}");
        assert!(font("3x3").is_err());
        assert_eq!(font("6x12").unwrap().character_size.width, 6);
    }
}
