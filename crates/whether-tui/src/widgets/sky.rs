//! Sun and moon.

use alloc::format;

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::Line;
use ratatui::widgets::{LineGauge, Paragraph, Widget};
use whether_core::icon::UiIcon;

use super::Ctx;
use crate::icons;

/// Sunrise, sunset, daylight and a progress gauge by day.
pub(crate) fn sun(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer) {
    let inner = ctx.panel("Sun", area, buf);
    let Some(sun) = &ctx.vm.sun else { return };
    let style = ctx.cfg.icons;
    let lines = alloc::vec![
        Line::raw(format!(
            "{} {}   {} {}",
            icons::ui(UiIcon::Sunrise, style),
            sun.sunrise,
            icons::ui(UiIcon::Sunset, style),
            sun.sunset
        )),
        Line::styled(sun.daylight.clone(), ctx.pal.muted_style()),
    ];
    let [text, gauge] =
        Layout::vertical([Constraint::Length(2), Constraint::Length(1)]).areas(inner);
    Paragraph::new(lines).render(text, buf);
    match (sun.is_day, sun.progress) {
        (true, Some(progress)) => LineGauge::default()
            .ratio(f64::from(progress.clamp(0.0, 1.0)))
            .filled_style(ctx.pal.accent_style())
            .unfilled_style(ctx.pal.muted_style())
            .render(gauge, buf),
        _ => Line::styled("Night", ctx.pal.muted_style()).render(gauge, buf),
    }
}

/// Moon phase, glyph, moonrise and moonset.
pub(crate) fn moon(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer) {
    let inner = ctx.panel("Moon", area, buf);
    let Some(moon) = &ctx.vm.moon else { return };
    let style = ctx.cfg.icons;
    let mut lines = alloc::vec![Line::raw(format!(
        "{} {}",
        icons::ui(moon.icon, style),
        moon.phase_label
    ))];
    let times: alloc::vec::Vec<_> = [
        (UiIcon::Moonrise, &moon.moonrise),
        (UiIcon::Moonset, &moon.moonset),
    ]
    .into_iter()
    .filter_map(|(icon, t)| {
        t.as_ref()
            .map(|t| format!("{} {t}", icons::ui(icon, style)))
    })
    .collect();
    if !times.is_empty() {
        lines.push(Line::styled(times.join("   "), ctx.pal.muted_style()));
    }
    Paragraph::new(lines).render(inner, buf);
}
