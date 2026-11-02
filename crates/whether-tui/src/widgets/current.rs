//! Current conditions: art, temperature, condition, feels-like, high/low, summary.

use alloc::format;
use alloc::vec::Vec;

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget, Wrap};

use super::Ctx;
use crate::icons;

/// The current panel (with its border).
pub(crate) fn current(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer) {
    let inner = ctx.panel("Now", area, buf);
    body(ctx, inner, buf, true);
}

/// The panel's content; `with_summary` adds the wrapped summary below.
pub(crate) fn body(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer, with_summary: bool) {
    let Some(c) = &ctx.vm.current else { return };
    let text_area = if ctx.ui.art && area.width >= 30 && area.height >= 5 {
        let [art, rest] =
            Layout::horizontal([Constraint::Length(14), Constraint::Fill(1)]).areas(area);
        let lines: Vec<Line<'_>> = icons::art(c.icon)
            .iter()
            .map(|l| Line::styled(*l, ctx.pal.accent_style()))
            .collect();
        Paragraph::new(lines).render(art, buf);
        rest
    } else {
        area
    };
    let glyph = icons::condition(c.icon, ctx.cfg.icons);
    let mut lines = alloc::vec![
        Line::from(alloc::vec![
            Span::styled(c.temperature.text.clone(), ctx.pal.temp_style(c.temp_level)),
            Span::raw(format!("  {glyph} {}", c.condition_label)),
        ]),
        Line::styled(c.feels_like_label.clone(), ctx.pal.muted_style()),
        Line::from(alloc::vec![
            Span::styled("H ", ctx.pal.muted_style()),
            Span::raw(c.high.text.clone()),
            Span::styled("  L ", ctx.pal.muted_style()),
            Span::raw(c.low.text.clone()),
        ]),
    ];
    if with_summary && let Some(summary) = &ctx.vm.summary {
        lines.push(Line::raw(""));
        lines.push(Line::styled(summary.clone(), ctx.pal.muted_style()));
    }
    lines[0].spans[0].style = lines[0].spans[0].style.add_modifier(Modifier::BOLD);
    Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .render(text_area, buf);
}
