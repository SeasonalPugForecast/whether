//! Air quality: index, category, gauge and pollutants.

use alloc::format;
use alloc::vec::Vec;

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Gauge, Paragraph, Widget};

use super::Ctx;

/// The air-quality panel; `pollutants` adds the pollutant list.
pub(crate) fn air(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer, pollutants: bool) {
    let inner = ctx.panel("Air quality", area, buf);
    let Some(aq) = &ctx.vm.air_quality else {
        Line::styled("No data", ctx.pal.muted_style()).render(inner, buf);
        return;
    };
    let level = ctx.pal.level_style(aq.level);
    let [head, gauge, rest] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .areas(inner);
    Line::from(alloc::vec![
        Span::styled(format!("{} ", aq.index), level.add_modifier(Modifier::BOLD)),
        Span::styled(aq.category_label.clone(), level),
    ])
    .render(head, buf);
    Gauge::default()
        .ratio(f64::from(aq.gauge.clamp(0.0, 1.0)))
        .label("")
        .gauge_style(level)
        .render(gauge, buf);
    if pollutants {
        let lines: Vec<Line<'_>> = aq
            .pollutants
            .iter()
            .map(|(name, value)| {
                Line::from(alloc::vec![
                    Span::styled(format!("{name:<7}"), ctx.pal.muted_style()),
                    Span::raw(value.clone())
                ])
            })
            .collect();
        Paragraph::new(lines).render(rest, buf);
    }
}
