//! Layout presets and custom grids.

use alloc::vec::Vec;

use ratatui::layout::{Constraint, Layout, Rect};
use whether_core::presentation::WidgetKind;

use crate::config::{LayoutPreset, RowSpec, Size};

/// A built-in arrangement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    /// ≥ 160×45.
    Wide,
    /// ≥ 100×30.
    Dashboard,
    /// ≥ 50×18.
    Compact,
    /// Anything smaller.
    Minimal,
}

impl Preset {
    /// The smallest terminal (columns, rows) the preset needs.
    pub fn min_size(self) -> (u16, u16) {
        match self {
            Self::Wide => (160, 45),
            Self::Dashboard => (100, 30),
            Self::Compact => (50, 18),
            Self::Minimal => (0, 0),
        }
    }

    fn fits(self, area: Rect) -> bool {
        let (w, h) = self.min_size();
        area.width >= w && area.height >= h
    }

    const ALL: [Self; 4] = [Self::Wide, Self::Dashboard, Self::Compact, Self::Minimal];
}

/// The preset for `layout` in `area`, or `None` for a custom layout. An explicit
/// preset that does not fit falls back to the next smaller one; `auto` picks the
/// largest that fits.
pub fn choose(layout: LayoutPreset, has_rows: bool, area: Rect) -> Option<Preset> {
    let start = match layout {
        LayoutPreset::Custom if has_rows => return None,
        LayoutPreset::Auto | LayoutPreset::Custom | LayoutPreset::Wide => 0,
        LayoutPreset::Dashboard => 1,
        LayoutPreset::Compact => 2,
        LayoutPreset::Minimal => 3,
    };
    Preset::ALL[start..]
        .iter()
        .copied()
        .find(|p| p.fits(area))
        .or(Some(Preset::Minimal))
}

/// A config size as a ratatui constraint.
pub fn constraint(size: Size) -> Constraint {
    match size {
        Size::Cells(n) => Constraint::Length(n),
        Size::Percent(p) => Constraint::Percentage(u16::from(p)),
        Size::Fill => Constraint::Fill(1),
    }
}

/// Places a custom layout's widgets.
pub fn custom_areas(rows: &[RowSpec], area: Rect) -> Vec<(WidgetKind, Rect)> {
    let heights = Layout::vertical(rows.iter().map(|r| constraint(r.height))).split(area);
    let mut out = Vec::new();
    for (row, row_area) in rows.iter().zip(heights.iter()) {
        let widths = Layout::horizontal(
            row.widgets
                .iter()
                .map(|c| c.width.map_or(Constraint::Fill(1), constraint)),
        )
        .split(*row_area);
        for (cell, cell_area) in row.widgets.iter().zip(widths.iter()) {
            out.push((cell.kind, *cell_area));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CellSpec;
    use alloc::vec;

    fn area(w: u16, h: u16) -> Rect {
        Rect::new(0, 0, w, h)
    }

    #[test]
    fn auto_picks_the_largest_that_fits() {
        assert_eq!(
            choose(LayoutPreset::Auto, false, area(190, 55)),
            Some(Preset::Wide)
        );
        assert_eq!(
            choose(LayoutPreset::Auto, false, area(120, 40)),
            Some(Preset::Dashboard)
        );
        assert_eq!(
            choose(LayoutPreset::Auto, false, area(80, 24)),
            Some(Preset::Compact)
        );
        assert_eq!(
            choose(LayoutPreset::Auto, false, area(40, 10)),
            Some(Preset::Minimal)
        );
    }

    #[test]
    fn explicit_presets_fall_back() {
        assert_eq!(
            choose(LayoutPreset::Wide, false, area(120, 40)),
            Some(Preset::Dashboard)
        );
        assert_eq!(
            choose(LayoutPreset::Compact, false, area(190, 55)),
            Some(Preset::Compact)
        );
        assert_eq!(choose(LayoutPreset::Custom, true, area(10, 5)), None);
        assert_eq!(
            choose(LayoutPreset::Custom, false, area(120, 40)),
            Some(Preset::Dashboard)
        );
    }

    #[test]
    fn custom_rows_split_the_area() {
        let rows = vec![
            RowSpec {
                height: Size::Cells(10),
                widgets: vec![
                    CellSpec {
                        kind: WidgetKind::Current,
                        width: Some(Size::Percent(40)),
                    },
                    CellSpec {
                        kind: WidgetKind::Details,
                        width: Some(Size::Fill),
                    },
                ],
            },
            RowSpec {
                height: Size::Fill,
                widgets: vec![CellSpec {
                    kind: WidgetKind::Hourly,
                    width: None,
                }],
            },
        ];
        let placed = custom_areas(&rows, area(100, 30));
        assert_eq!(placed.len(), 3);
        assert_eq!(placed[0], (WidgetKind::Current, Rect::new(0, 0, 40, 10)));
        assert_eq!(placed[1], (WidgetKind::Details, Rect::new(40, 0, 60, 10)));
        assert_eq!(placed[2], (WidgetKind::Hourly, Rect::new(0, 10, 100, 20)));
    }
}
