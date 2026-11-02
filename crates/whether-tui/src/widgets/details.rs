//! Detail cards as a two-column list: title, value, caption.

use alloc::vec::Vec;

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use whether_core::view::DetailVm;

use super::Ctx;

fn line<'a>(ctx: &Ctx<'a>, d: &'a DetailVm, width: u16) -> Line<'a> {
    let value_style = d
        .level
        .map_or_else(ratatui::style::Style::new, |l| ctx.pal.level_style(l))
        .add_modifier(Modifier::BOLD);
    let mut spans = alloc::vec![
        Span::styled(alloc::format!("{:<12}", d.title), ctx.pal.muted_style()),
        Span::styled(d.value.clone(), value_style),
    ];
    if let Some(caption) = &d.caption
        && width > 34
    {
        spans.push(Span::styled(
            alloc::format!("  {caption}"),
            ctx.pal.muted_style(),
        ));
    }
    Line::from(spans)
}

/// The details panel.
pub(crate) fn details(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer) {
    let inner = ctx.panel("Details", area, buf);
    let items = &ctx.vm.details;
    let columns = if inner.width >= 60 { 2 } else { 1 };
    let per_column = items.len().div_ceil(columns);
    let halves = Layout::horizontal((0..columns).map(|_| Constraint::Fill(1))).split(inner);
    for (i, column) in halves.iter().enumerate() {
        let lines: Vec<Line<'_>> = items
            .iter()
            .skip(i * per_column)
            .take(per_column)
            .map(|d| line(ctx, d, column.width))
            .collect();
        Paragraph::new(lines).render(*column, buf);
    }
}
