//! SVG path commands for the desktop hourly chart, in a 0..100 viewbox.

use std::fmt::Write as _;

/// A line through `fracs` (0..1, 1 at the top) and the area under it, as
/// `(line, area)` commands. A small margin keeps the stroke inside the box.
pub fn commands(fracs: &[f32]) -> (String, String) {
    if fracs.is_empty() {
        return (String::new(), String::new());
    }
    let last = (fracs.len() - 1).max(1) as f32;
    let y = |f: f32| 92.0 - f.clamp(0.0, 1.0) * 84.0;
    let mut line = String::new();
    for (i, f) in fracs.iter().enumerate() {
        let x = i as f32 / last * 100.0;
        let _ = write!(
            line,
            "{} {x:.2} {:.2} ",
            if i == 0 { "M" } else { "L" },
            y(*f)
        );
    }
    let end_x = if fracs.len() == 1 { 0.0 } else { 100.0 };
    let area = format!("{line}L {end_x:.2} 100 L 0 100 Z");
    (line.trim_end().to_string(), area)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_and_area() {
        let (line, area) = commands(&[0.0, 1.0, 0.5]);
        assert_eq!(line, "M 0.00 92.00 L 50.00 8.00 L 100.00 50.00");
        assert!(area.starts_with("M 0.00 92.00") && area.ends_with("L 100.00 100 L 0 100 Z"));
        assert_eq!(commands(&[]), (String::new(), String::new()));
    }
}
