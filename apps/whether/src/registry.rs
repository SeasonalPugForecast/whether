//! The compiled-in display plugins (an explicit list, no link-time magic).

use whether_display::DisplayFactory;

/// Every display compiled into this binary.
pub(crate) fn registry() -> Vec<DisplayFactory> {
    #[allow(unused_mut)]
    let mut displays = Vec::new();
    #[cfg(feature = "tui")]
    displays.push(whether_tui::host::factory());
    #[cfg(feature = "gui")]
    displays.push(whether_gui::desktop::factory());
    #[cfg(feature = "line")]
    {
        displays.push(whether_line::line_factory());
        displays.push(whether_line::json_factory());
    }
    displays
}

/// The factory named `name`.
pub(crate) fn find(name: &str) -> Option<DisplayFactory> {
    registry().into_iter().find(|f| f.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "line")]
    #[test]
    fn line_and_json_are_registered() {
        #[cfg(feature = "tui")]
        assert!(find("tui").is_some());
        assert!(find("line").is_some() && find("json").is_some());
        assert!(find("nope").is_none());
    }
}
