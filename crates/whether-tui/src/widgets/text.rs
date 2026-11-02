//! Summary, nowcast, alerts and the minimal layout.

use alloc::format;
use alloc::string::ToString;
use alloc::vec::Vec;

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Sparkline, Widget, Wrap};

use super::{Ctx, MISSING};
use crate::icons;

/// The summary sentence.
pub(crate) fn summary(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer) {
    let inner = ctx.panel("Summary", area, buf);
    let text = ctx.vm.summary.clone().unwrap_or_default();
    Paragraph::new(text)
        .wrap(Wrap { trim: true })
        .render(inner, buf);
}

/// The next hours' precipitation outlook (`nowcast_hours`).
pub(crate) fn nowcast(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer) {
    let Some(n) = &ctx.vm.nowcast else {
        ctx.panel("Next hours", area, buf);
        return;
    };
    let title = if n.hours == 1 {
        "Next hour".to_string()
    } else {
        format!("Next {} hours", n.hours)
    };
    let inner = ctx.panel(&title, area, buf);
    let [text, spark] = Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(inner);
    Line::raw(n.text.clone()).render(text, buf);
    // Dark Sky's scale (a third is light, two thirds moderate), so light rain
    // still shows.
    let data: Vec<u64> = n
        .intensity
        .iter()
        .map(|i| (i.clamp(0.0, 1.0) * 100.0) as u64)
        .collect();
    Sparkline::default()
        .data(&data)
        .style(ctx.pal.accent_style())
        .render(spark, buf);
}

/// Rows an alert block needs: borders, the period and one line of text.
const ALERT_BLOCK_ROWS: u16 = 4;

/// One red-bordered block per alert, or one line per alert in a single block when
/// the blocks would not have room for their text.
pub(crate) fn alerts(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer) {
    let alerts = &ctx.vm.alerts;
    if alerts.is_empty() {
        return;
    }
    let count = u16::try_from(alerts.len()).unwrap_or(u16::MAX);
    if area.height / count < ALERT_BLOCK_ROWS {
        let inner = Block::default()
            .borders(Borders::ALL)
            .border_style(ctx.pal.error_style())
            .title(Line::styled(
                format!(" {} Alerts ", ctx.warn()),
                ctx.pal.error_style().add_modifier(Modifier::BOLD),
            ));
        let inside = inner.inner(area);
        inner.render(area, buf);
        alert_lines(ctx, inside, buf);
        return;
    }
    let rows = Layout::vertical(alerts.iter().map(|_| Constraint::Fill(1))).split(area);
    for (alert, row) in alerts.iter().zip(rows.iter()) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(ctx.pal.error_style())
            .title(Line::styled(
                format!(" {} {} ", ctx.warn(), alert.title),
                ctx.pal
                    .level_style(alert.level)
                    .add_modifier(Modifier::BOLD),
            ));
        Paragraph::new(alloc::vec![
            Line::styled(alert.period.clone(), ctx.pal.muted_style()),
            Line::raw(alert.body.clone())
        ])
        .wrap(Wrap { trim: true })
        .block(block)
        .render(*row, buf);
    }
}

/// One line per alert without a border: the sign and title in the alert's level
/// color, then the period and as much of the text as fits.
pub(crate) fn alert_lines(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer) {
    let lines: Vec<Line<'_>> = ctx
        .vm
        .alerts
        .iter()
        .map(|a| {
            Line::from(alloc::vec![
                Span::styled(
                    format!("{} {}", ctx.warn(), a.title),
                    ctx.pal.level_style(a.level).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("  {}", a.period), ctx.pal.muted_style()),
                Span::raw(format!("  {}", a.body)),
            ])
        })
        .collect();
    Paragraph::new(lines).render(area, buf);
}

/// Rows [`alert_lines`] needs in a bordered block, at most `max_lines` alerts.
pub(crate) fn alert_strip_rows(ctx: &Ctx<'_>, max_lines: u16) -> u16 {
    match u16::try_from(ctx.vm.alerts.len()).unwrap_or(u16::MAX) {
        0 => 0,
        n => n.min(max_lines) + 2,
    }
}

/// Summary above alerts, sharing one area. The summary keeps two lines; the alerts
/// get the rest.
pub(crate) fn summary_and_alerts(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer) {
    if ctx.vm.alerts.is_empty() {
        summary(ctx, area, buf);
    } else {
        let [top, bottom] =
            Layout::vertical([Constraint::Length(4), Constraint::Fill(1)]).areas(area);
        summary(ctx, top, buf);
        alerts(ctx, bottom, buf);
    }
}

/// Alert lines, then the summary, without borders (compact layout).
pub(crate) fn alerts_and_summary(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer) {
    let count = u16::try_from(ctx.vm.alerts.len()).unwrap_or(u16::MAX);
    let [top, bottom] = Layout::vertical([
        Constraint::Length(count.min(area.height)),
        Constraint::Fill(1),
    ])
    .areas(area);
    alert_lines(ctx, top, buf);
    plain_summary(ctx, bottom, buf);
}

/// The summary without a border (compact layout).
pub(crate) fn plain_summary(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer) {
    let mut lines = Vec::new();
    if let Some(summary) = &ctx.vm.summary {
        lines.push(Line::raw(summary.clone()));
    }
    // The summary sentence usually ends with the nowcast already.
    let in_summary = |text: &str| ctx.vm.summary.as_ref().is_some_and(|s| s.contains(text));
    if let Some(n) = ctx.vm.nowcast.as_ref().filter(|n| !in_summary(&n.text)) {
        lines.push(Line::styled(n.text.clone(), ctx.pal.muted_style()));
    }
    Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .render(area, buf);
}

/// Temperature, condition and summary in as little space as possible.
pub(crate) fn minimal(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer) {
    let mut lines = Vec::new();
    match &ctx.vm.current {
        Some(c) => lines.push(Line::from(alloc::vec![
            Span::styled(c.temperature.text.clone(), ctx.pal.temp_style(c.temp_level)),
            Span::raw(format!(
                " {} {}",
                icons::condition(c.icon, ctx.cfg.icons),
                c.condition_label
            )),
        ])),
        None => lines.push(Line::raw(MISSING)),
    }
    if let Some(a) = ctx.vm.alerts.first() {
        lines.push(Line::styled(
            format!("{} {}", ctx.warn(), a.title),
            ctx.pal.level_style(a.level).add_modifier(Modifier::BOLD),
        ));
    }
    if let Some(summary) = &ctx.vm.summary {
        lines.push(Line::styled(summary.clone(), ctx.pal.muted_style()));
    }
    Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .render(area, buf);
}
