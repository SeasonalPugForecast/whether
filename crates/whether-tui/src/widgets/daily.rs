//! The daily table with a temperature range bar.

use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Row, Table, Widget};
use whether_core::view::DayVm;

use super::{Ctx, MISSING, percent};
use crate::icons;

/// `━` across the day's range (positions in 0..=1 of the period's range), colored
/// cold to hot by position, with a dim `─` elsewhere.
fn range_bar<'a>(ctx: &Ctx<'a>, day: &DayVm, width: u16) -> Line<'a> {
    let w = f32::from(width);
    let start = (day.range.0.clamp(0.0, 1.0) * w) as u16;
    let end = (libm::ceilf(day.range.1.clamp(0.0, 1.0) * w) as u16)
        .max(start.saturating_add(1))
        .min(width);
    let mut spans = Vec::new();
    for x in 0..width {
        if (start..end).contains(&x) {
            let t = f32::from(x) / w.max(1.0);
            let color = if t < 0.33 {
                ctx.pal.cold
            } else if t < 0.66 {
                ctx.pal.levels[0]
            } else {
                ctx.pal.hot
            };
            let style = if ctx.pal.colored {
                ratatui::style::Style::new().fg(color)
            } else {
                ratatui::style::Style::new()
            };
            spans.push(Span::styled("━", style));
        } else {
            spans.push(Span::styled("─", ctx.pal.muted_style()));
        }
    }
    Line::from(spans)
}

/// Optional detail columns in the order they are dropped when space runs out
/// (the last one is dropped first), with their widths.
const EXTRAS: [(Extra, u16); 4] = [
    (Extra::Wind, 11),
    (Extra::Uv, 4),
    (Extra::Sunrise, 6),
    (Extra::Sunset, 6),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Extra {
    Wind,
    Uv,
    Sunrise,
    Sunset,
}

/// Icon, precipitation, low and high (each plus one column of spacing), without
/// the day label.
const BASE: u16 = (4 + 1) + (5 + 1) + (6 + 1) + 6;
/// The range bar's minimum width (plus spacing).
const MIN_BAR: u16 = 4 + 1;
/// The condition label's widths: full, and the least worth showing.
const CONDITION: (u16, u16) = (14, 6);

/// Column plan for `width`: the condition label's width (0 = hidden), the detail
/// columns that fit, and the bar's width. Narrow panels (53 columns on a CYD) drop
/// sunset, sunrise, UV and wind first, then shorten the label, rather than
/// truncating every column.
fn plan(width: u16, day: u16, detailed: bool) -> (u16, Vec<(Extra, u16)>, u16) {
    let width = width.saturating_sub(day + 1);
    let mut extras: Vec<(Extra, u16)> = if detailed {
        EXTRAS.to_vec()
    } else {
        Vec::new()
    };
    let extras_width = |e: &[(Extra, u16)]| e.iter().map(|(_, w)| w + 1).sum::<u16>();
    let full = BASE + MIN_BAR + CONDITION.0 + 1;
    while !extras.is_empty() && full + extras_width(&extras) > width {
        extras.pop();
    }
    let room = width.saturating_sub(BASE + MIN_BAR + extras_width(&extras));
    let condition = match room.saturating_sub(1).min(CONDITION.0) {
        w if w >= CONDITION.1 => w,
        _ => 0,
    };
    let used = BASE + extras_width(&extras) + if condition > 0 { condition + 1 } else { 0 };
    let bar = width.saturating_sub(used + 1).max(4);
    (condition, extras, bar)
}

/// The daily table; `detailed` adds wind, UV, sunrise and sunset columns as far as
/// they fit.
pub(crate) fn daily(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer, detailed: bool) {
    let inner = ctx.panel("Days", area, buf);
    let Some(d) = &ctx.vm.daily else { return };
    let day = d
        .days
        .iter()
        .map(|day| u16::try_from(day.label.chars().count()).unwrap_or(u16::MAX))
        .max()
        .unwrap_or(0)
        .clamp(3, 12);
    let (condition, extras, bar) = plan(inner.width, day, detailed);
    let mut widths = alloc::vec![day, 4];
    if condition > 0 {
        widths.push(condition);
    }
    widths.extend_from_slice(&[5, 6, bar, 6]);
    widths.extend(extras.iter().map(|(_, w)| *w));
    let rows: Vec<Row<'_>> = d
        .days
        .iter()
        .map(|day| {
            let mut cells: Vec<Cell<'_>> = alloc::vec![
                Cell::from(day.label.clone()),
                Cell::from(icons::condition(day.icon, ctx.cfg.icons)),
            ];
            if condition > 0 {
                // When the day's precipitation falls, if the column holds it.
                let text = day
                    .precip_text
                    .as_deref()
                    .filter(|t| t.chars().count() <= usize::from(condition))
                    .unwrap_or(&day.condition_label);
                cells.push(Cell::from(text.to_string()));
            }
            cells.extend([
                Cell::from(percent(day.precip_probability)).style(ctx.pal.cold_style()),
                Cell::from(day.low.text.clone()).style(ctx.pal.muted_style()),
                Cell::from(range_bar(ctx, day, bar)),
                Cell::from(day.high.text.clone()).style(ctx.pal.temp_style(day.temp_level_high)),
            ]);
            let or_missing = |s: &Option<String>| s.clone().unwrap_or_else(|| MISSING.into());
            for (extra, _) in &extras {
                cells.push(Cell::from(match extra {
                    Extra::Wind => day.wind_max.text.clone(),
                    Extra::Uv => day.uv_max.map_or_else(
                        || MISSING.into(),
                        |uv| whether_core::view::format::number(uv, 0),
                    ),
                    Extra::Sunrise => or_missing(&day.sunrise),
                    Extra::Sunset => or_missing(&day.sunset),
                }));
            }
            Row::new(cells)
        })
        .collect();
    Table::new(rows, widths.into_iter().map(Constraint::Length)).render(inner, buf);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_tables_keep_every_column() {
        let (condition, extras, bar) = plan(120, 10, true);
        assert_eq!((condition, extras.len()), (14, 4));
        assert!(bar >= 4);
    }

    #[test]
    fn narrow_tables_drop_details_before_truncating() {
        // A CYD at 53 columns: 51 inside the panel border.
        let (condition, extras, bar) = plan(51, 8, true);
        assert!(extras.is_empty());
        assert!(condition == 0 || condition >= CONDITION.1);
        let used = 9 + BASE + if condition > 0 { condition + 1 } else { 0 } + bar + 1;
        assert!(used <= 51, "{used}");
        // Sunset goes first, wind last.
        let (_, extras, _) = plan(9 + BASE + MIN_BAR + 15 + 12 + 5, 8, true);
        assert!(extras.iter().any(|(e, _)| *e == Extra::Wind));
        assert!(!extras.iter().any(|(e, _)| *e == Extra::Sunset));
    }

    #[test]
    fn tiny_widths_still_plan() {
        let (condition, extras, bar) = plan(10, 8, true);
        assert_eq!((condition, extras.len(), bar), (0, 0, 4));
    }
}
