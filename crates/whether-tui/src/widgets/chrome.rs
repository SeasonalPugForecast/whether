//! Header (tabs, status), footer (hints, attribution), help overlay, no-data screen.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Tabs, Widget, Wrap};

use super::Ctx;
use crate::ui_state::KEYS;

/// "Updated 3 min ago · Open-Meteo", with "⟳" while refreshing and "⚠ error".
/// Without `provider` it ends after "Updated …" (the footer still credits the source).
fn status_spans<'a>(ctx: &Ctx<'a>, provider: bool) -> Vec<Span<'a>> {
    let status = &ctx.vm.status;
    let mut spans = Vec::new();
    if let Some(error) = &status.error {
        spans.push(Span::styled(
            format!("{} {error} ", ctx.warn()),
            ctx.pal.error_style(),
        ));
    }
    if status.refreshing {
        spans.push(Span::styled(
            format!("{} ", ctx.refreshing()),
            ctx.pal.accent_style(),
        ));
    }
    let style = if status.stale {
        ctx.pal.error_style()
    } else {
        ctx.pal.muted_style()
    };
    let mut text = if provider {
        format!("{} · {}", status.updated, status.provider)
    } else {
        status.updated.clone()
    };
    if let Some(observed) = &status.observed {
        text.push_str(" · ");
        text.push_str(observed);
    }
    spans.push(Span::styled(text, style));
    spans
}

/// A location tab's text.
pub(crate) fn tab_title(tab: &whether_core::view::TabVm) -> String {
    format!("{} {}", tab.name, tab.temp)
}

/// One line: location tabs (or the location) left, status right.
pub(crate) fn header(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer) {
    let mut status = Line::from(status_spans(ctx, true));
    if status.width() > usize::from(area.width / 2) {
        status = Line::from(status_spans(ctx, false));
    }
    let full = u16::try_from(status.width()).unwrap_or(u16::MAX);
    let width = full.min(area.width / 2);
    // When it does not fit, keep the start of the status ("Updated …") visible.
    let status = if full > width {
        status
    } else {
        status.alignment(Alignment::Right)
    };
    let [left, right] = Layout::horizontal([Constraint::Fill(1), Constraint::Length(width)])
        .spacing(1)
        .areas(area);
    let vm = ctx.vm;
    if vm.tabs.len() > 1 {
        let titles: Vec<String> = vm.tabs.iter().map(tab_title).collect();
        Tabs::new(titles)
            .select(vm.active)
            .style(ctx.pal.muted_style())
            .highlight_style(
                ctx.pal
                    .accent_style()
                    .add_modifier(Modifier::BOLD | Modifier::REVERSED),
            )
            .render(left, buf);
    } else {
        let name = Span::styled(
            vm.location.name.clone(),
            ctx.pal.accent_style().add_modifier(Modifier::BOLD),
        );
        let time = Span::styled(
            format!("  {}", vm.location.local_time),
            ctx.pal.muted_style(),
        );
        // The region goes first when space runs out, so the local time stays.
        let region = vm
            .location
            .region
            .as_ref()
            .map(|r| Span::styled(format!(", {r}"), ctx.pal.muted_style()))
            .filter(|r| name.width() + r.width() + time.width() <= usize::from(left.width));
        let spans: Vec<Span<'_>> = [Some(name), region, Some(time)]
            .into_iter()
            .flatten()
            .collect();
        Line::from(spans).render(left, buf);
    }
    status.render(right, buf);
}

/// Key hints left, attribution right; the attribution always wins the space.
pub(crate) fn footer(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer) {
    let attribution = ctx.vm.attribution.as_str();
    // One extra column keeps the hints from running into the attribution.
    let width = u16::try_from(attribution.chars().count() + 1)
        .unwrap_or(u16::MAX)
        .min(area.width);
    let [left, right] =
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(width)]).areas(area);
    if ctx.ui.key_hints {
        Line::styled(
            "q quit · ←→ location · 1-6 views · r refresh · ? help",
            ctx.pal.muted_style(),
        )
        .render(left, buf);
    }
    Line::styled(attribution, ctx.pal.muted_style())
        .alignment(Alignment::Right)
        .render(right, buf);
}

/// A centered rectangle of at most `w`×`h`.
pub(crate) fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}

/// The key bindings over everything else.
pub(crate) fn help(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer) {
    let lines: Vec<Line<'_>> = KEYS
        .iter()
        .map(|(keys, what)| {
            Line::from(alloc::vec![
                Span::styled(
                    format!("{keys:>15}  "),
                    ctx.pal.accent_style().add_modifier(Modifier::BOLD)
                ),
                Span::raw(*what),
            ])
        })
        .collect();
    let height = u16::try_from(lines.len())
        .unwrap_or(u16::MAX)
        .saturating_add(2);
    let popup = centered(area, 58, height);
    Clear.render(popup, buf);
    Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(ctx.pal.accent_style())
                .title(" Keys "),
        )
        .render(popup, buf);
}

/// No snapshot yet: the status, prominently.
pub(crate) fn no_data(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer) {
    let status = &ctx.vm.status;
    let mut lines = alloc::vec![
        Line::styled(
            "No weather data yet",
            ctx.pal.accent_style().add_modifier(Modifier::BOLD)
        ),
        Line::raw("")
    ];
    if let Some(error) = &status.error {
        lines.push(Line::styled(
            format!("{} {error}", ctx.warn()),
            ctx.pal.error_style(),
        ));
    }
    if status.refreshing {
        lines.push(Line::raw("Fetching…"));
    }
    lines.push(Line::styled(
        format!("{} · {}", status.updated, status.provider),
        ctx.pal.muted_style(),
    ));
    lines.push(Line::raw(""));
    lines.push(Line::styled("r retries · q quits", ctx.pal.muted_style()));
    let height = u16::try_from(lines.len())
        .unwrap_or(u16::MAX)
        .saturating_add(2);
    let popup = centered(area, 60, height);
    Paragraph::new(lines)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(ctx.pal.muted_style()),
        )
        .render(popup, buf);
}

/// The location search popup: the query with a cursor, the status and the results.
pub(crate) fn search(
    pal: &crate::theme::Palette,
    input: &crate::SearchInput,
    panel: &crate::SearchPanel,
    area: Rect,
    buf: &mut Buffer,
) {
    let mut lines = alloc::vec![
        Line::from(alloc::vec![
            Span::styled("> ", pal.accent_style().add_modifier(Modifier::BOLD)),
            Span::raw(input.query.clone()),
            Span::styled("_", pal.accent_style()),
        ]),
        Line::raw(""),
    ];
    if !panel.status.is_empty() {
        lines.push(Line::styled(panel.status.clone(), pal.muted_style()));
    }
    for (i, name) in panel.results.iter().enumerate() {
        let selected = input.submitted && i == input.selected;
        let style = if selected {
            pal.accent_style().add_modifier(Modifier::REVERSED)
        } else {
            ratatui::style::Style::new()
        };
        lines.push(Line::styled(format!(" {name} "), style));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        "Enter search / add · ↑↓ choose · Esc close",
        pal.muted_style(),
    ));
    let height = u16::try_from(lines.len())
        .unwrap_or(u16::MAX)
        .saturating_add(2);
    let popup = centered(area, 60, height);
    Clear.render(popup, buf);
    Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(pal.accent_style())
                .title(" Add a location "),
        )
        .render(popup, buf);
}
