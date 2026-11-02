//! `[display.panel]` and the per-frame UI state.

use alloc::vec;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

/// A page of the panel display.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PanelPage {
    /// Current conditions and the next hours.
    Now,
    /// The next days.
    Forecast,
    /// Wind, humidity, UV and pressure tiles.
    Details,
}

/// `[display.panel]` (postcard-safe: no untagged or flattened fields).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PanelConfig {
    /// A sky gradient that follows the weather; otherwise a neutral background.
    pub dynamic_background: bool,
    /// Pages in the order buttons and the timer step through them.
    pub pages: Vec<PanelPage>,
}

impl Default for PanelConfig {
    fn default() -> Self {
        Self {
            dynamic_background: true,
            pages: vec![PanelPage::Now, PanelPage::Forecast, PanelPage::Details],
        }
    }
}

/// What the firmware tells the renderer besides the view model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PanelUi {
    /// The page to draw.
    pub page: PanelPage,
    /// The data is older than it should be (shows a badge).
    pub stale: bool,
    /// Certificate checks are off (shows a permanent badge).
    pub insecure_tls: bool,
}

impl PanelUi {
    /// `page` with no badges.
    pub fn new(page: PanelPage) -> Self {
        Self {
            page,
            stale: false,
            insecure_tls: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_example_config() {
        let cfg = PanelConfig::default();
        assert!(cfg.dynamic_background);
        assert_eq!(
            cfg.pages,
            [PanelPage::Now, PanelPage::Forecast, PanelPage::Details]
        );
        let text = r#"{"dynamic_background":false,"pages":["details","now"]}"#;
        let parsed: PanelConfig = serde_json::from_str(text).unwrap();
        assert_eq!(parsed.pages, [PanelPage::Details, PanelPage::Now]);
        assert!(serde_json::from_str::<PanelConfig>(r#"{"pagez":[]}"#).is_err());
    }
}
