//! The precipitation table: the next hours, each with an intensity bar, the
//! chance and the amount.

use alloc::string::String;
use alloc::vec::Vec;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use super::{Ctx, MISSING};

/// Columns for the time, chance and amount around the bar.
const LABEL: usize = 6;
const CHANCE: usize = 5;
const AMOUNT: usize = 9;

/// A bar `width` cells long, `fill` (0–1) of it drawn.
fn bar(fill: f32, width: usize, ascii: bool) -> (String, String) {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let on = (libm::roundf(fill.clamp(0.0, 1.0) * width as f32) as usize).min(width);
    let (solid, blank) = if ascii { ("#", ".") } else { ("█", "·") };
    (solid.repeat(on), blank.repeat(width - on))
}

/// The precipitation panel.
pub(crate) fn precip(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer) {
    let inner = ctx.panel("Precipitation", area, buf);
    let Some(p) = &ctx.vm.precipitation else {
        Line::styled("No data", ctx.pal.muted_style()).render(inner, buf);
        return;
    };
    let width = usize::from(inner.width)
        .saturating_sub(LABEL + CHANCE + AMOUNT + 3)
        .max(4);
    let rows = usize::from(inner.height).saturating_sub(1);
    let mut lines: Vec<Line<'_>> = p
        .hours
        .iter()
        .take(rows)
        .map(|h| {
            let (on, off) = bar(h.intensity, width, ctx.ascii());
            let style = if h.frozen {
                ctx.pal.muted_style().add_modifier(Modifier::BOLD)
            } else {
                ctx.pal.cold_style()
            };
            Line::from(alloc::vec![
                Span::styled(
                    alloc::format!("{:<LABEL$} ", h.label),
                    ctx.pal.muted_style()
                ),
                Span::styled(on, style),
                Span::styled(off, ctx.pal.muted_style()),
                Span::raw(alloc::format!(
                    " {:>CHANCE$}",
                    h.chance.as_deref().unwrap_or(MISSING)
                )),
                Span::raw(alloc::format!(" {:>AMOUNT$}", h.amount.text)),
            ])
        })
        .collect();
    let footer = p.dry.clone().unwrap_or_else(|| {
        alloc::format!(
            "Total {}  ·  highest chance {}",
            p.total.text,
            p.max_chance.as_deref().unwrap_or(MISSING)
        )
    });
    lines.push(Line::styled(footer, ctx.pal.muted_style()));
    Paragraph::new(lines).render(inner, buf);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bars_fill_by_intensity() {
        assert_eq!(bar(0.5, 4, true), ("##".into(), "..".into()));
        assert_eq!(bar(2.0, 3, false), ("███".into(), String::new()));
        assert_eq!(bar(0.0, 2, true), (String::new(), "..".into()));
    }
}
