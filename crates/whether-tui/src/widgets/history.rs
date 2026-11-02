//! The history panel: today's normals, records and chance of rain or
//! snow, today against yesterday, the last days as observed against normal,
//! the month so far, and where it comes from.

use alloc::string::String;
use alloc::vec::Vec;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget, Wrap};

use super::Ctx;

/// Columns: day, date (at least; wider for the longer date formats), high,
/// low, against normal, precipitation.
const DAY: usize = 10;
const DATE: usize = 7;
const TEMP: usize = 6;
const DEPARTURE: usize = 7;

/// The history panel.
pub(crate) fn history(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer) {
    let inner = ctx.panel("History", area, buf);
    let Some(h) = &ctx.vm.history else {
        Line::styled("No history yet", ctx.pal.muted_style()).render(inner, buf);
        return;
    };
    let mut lines: Vec<Line<'_>> = Vec::new();
    if let Some(today) = &h.today {
        let mut parts: Vec<String> = alloc::vec![today.normal_text.clone()];
        if let (Some(high), Some(low)) = (&today.record_high, &today.record_low) {
            parts.push(alloc::format!("Records {high} · {low}"));
        }
        lines.push(Line::styled(
            parts.join("  ·  "),
            ctx.pal.accent_style().add_modifier(Modifier::BOLD),
        ));
        if let Some(chance) = &today.precip_chance {
            lines.push(Line::styled(chance.clone(), ctx.pal.cold_style()));
        }
    }
    if let Some(delta) = &h.yesterday_delta {
        lines.push(Line::raw(delta.clone()));
    }
    let date = h
        .days
        .iter()
        .map(|d| d.date_short.chars().count() + 1)
        .fold(DATE, usize::max);
    if !h.days.is_empty() {
        lines.push(Line::styled(
            alloc::format!(
                "{:<DAY$}{:<date$}{:>TEMP$}{:>TEMP$}{:>DEPARTURE$}  {}",
                "",
                "",
                "High",
                "Low",
                "Normal",
                "Precip"
            ),
            ctx.pal.muted_style(),
        ));
    }
    for d in &h.days {
        let departure_style = match d.departure.value {
            Some(v) if v > 0.0 => ctx.pal.temp_style(whether_core::scales::TempLevel::Warm),
            Some(v) if v < 0.0 => ctx.pal.temp_style(whether_core::scales::TempLevel::Cool),
            _ => ctx.pal.muted_style(),
        };
        lines.push(Line::from(alloc::vec![
            Span::styled(alloc::format!("{:<DAY$}", d.label), ctx.pal.muted_style()),
            Span::styled(
                alloc::format!("{:<date$}", d.date_short),
                ctx.pal.muted_style()
            ),
            Span::raw(alloc::format!("{:>TEMP$}", d.high.text)),
            Span::raw(alloc::format!("{:>TEMP$}", d.low.text)),
            Span::styled(
                alloc::format!("{:>DEPARTURE$}", d.departure.text),
                departure_style
            ),
            Span::styled(
                alloc::format!("  {}", d.precipitation.text),
                ctx.pal.cold_style()
            ),
        ]));
    }
    if let Some(mtd) = &h.month_to_date {
        lines.push(Line::raw(mtd.text.clone()));
    }
    if !h.credit.is_empty() {
        lines.push(Line::styled(h.credit.clone(), ctx.pal.muted_style()));
    }
    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .render(inner, buf);
}
