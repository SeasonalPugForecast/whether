//! `whether displays`.

use std::fmt::Write;

use crate::registry::registry;

/// The table of compiled-in displays.
pub(crate) fn table() -> String {
    let mut out = String::new();
    for f in registry() {
        let kind = if f.interactive {
            "interactive"
        } else {
            "prints"
        };
        let _ = writeln!(out, "{:<8} {:<12} {}", f.name, kind, f.description);
    }
    let _ = writeln!(
        out,
        "{:<8} {:<12} external plugins from [display.exec.<name>] (JSON Lines over stdio)",
        "exec:*", "external"
    );
    out
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "line")]
    #[test]
    fn lists_line_and_json() {
        let t = super::table();
        assert!(t.contains("line ") && t.contains("json ") && t.contains("exec:*"));
    }
}
