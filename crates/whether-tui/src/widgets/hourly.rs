//! Hourly chart, table and the compact next-hours row.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::symbols::Marker;
use ratatui::text::Line;
use ratatui::widgets::{
    Axis, Cell, Chart, Dataset, GraphType, Paragraph, Row, Sparkline, Table, Widget,
};
use whether_core::view::{HourVm, format::number};

use super::{Ctx, MISSING, percent};
use crate::config::{GraphStyle, HourlyColumn};
use crate::icons;

/// The chart marker; ASCII-only mode draws dots instead of Braille.
fn marker(ctx: &Ctx<'_>) -> Option<Marker> {
    match ctx.cfg.graph {
        GraphStyle::Braille if ctx.ascii() => Some(Marker::Dot),
        GraphStyle::Braille => Some(Marker::Braille),
        GraphStyle::Block => Some(Marker::Block),
        GraphStyle::Dot => Some(Marker::Dot),
        GraphStyle::None => None,
    }
}

/// Temperature chart with a precipitation-probability strip below it.
pub(crate) fn chart(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer) {
    let inner = ctx.panel("Hourly", area, buf);
    let Some(h) = &ctx.vm.hourly else { return };
    if h.hours.is_empty() {
        return;
    }
    let strip_height = if inner.height >= 8 { 2 } else { 1 };
    let [plot, strip] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(strip_height)]).areas(inner);
    if let Some(marker) = marker(ctx) {
        let points: Vec<(f64, f64)> = h
            .hours
            .iter()
            .enumerate()
            .filter_map(|(i, hour)| hour.temperature.value.map(|v| (i as f64, f64::from(v))))
            .collect();
        let last = h.hours.len() - 1;
        let x_labels: Vec<Line<'_>> = [0, last / 2, last]
            .iter()
            .map(|i| Line::raw(h.hours[*i].label.clone()))
            .collect();
        let y_labels: Vec<Line<'_>> = [h.temp_min, h.temp_max]
            .iter()
            .map(|t| Line::raw(number(*t, 0)))
            .collect();
        let dataset = Dataset::default()
            .data(&points)
            .marker(marker)
            .graph_type(GraphType::Line)
            .style(ctx.pal.accent_style());
        Chart::new(alloc::vec![dataset])
            .x_axis(
                Axis::default()
                    .bounds([0.0, last.max(1) as f64])
                    .labels(x_labels)
                    .style(ctx.pal.muted_style()),
            )
            .y_axis(
                Axis::default()
                    .bounds([f64::from(h.temp_min) - 1.0, f64::from(h.temp_max) + 1.0])
                    .labels(y_labels)
                    .style(ctx.pal.muted_style()),
            )
            .render(plot, buf);
    }
    let pops: Vec<u64> = h
        .hours
        .iter()
        .map(|hour| u64::from(hour.precip_probability.unwrap_or(0)))
        .collect();
    Sparkline::default()
        .data(&pops)
        .max(100)
        .style(ctx.pal.cold_style())
        .render(strip, buf);
}

fn cell(ctx: &Ctx<'_>, column: HourlyColumn, hour: &HourVm) -> String {
    match column {
        HourlyColumn::Time => hour.label.clone(),
        HourlyColumn::Icon => icons::condition(hour.icon, ctx.cfg.icons).to_string(),
        HourlyColumn::Temp => hour.temperature.text.clone(),
        HourlyColumn::Feels => hour.feels_like.text.clone(),
        HourlyColumn::PrecipProb => percent(hour.precip_probability),
        HourlyColumn::Precip => hour.precipitation.text.clone(),
        HourlyColumn::Wind => alloc::format!("{} {}", hour.wind.arrow, hour.wind.speed.text),
        HourlyColumn::Humidity => percent(hour.humidity),
        HourlyColumn::Uv => hour
            .uv
            .map_or_else(|| MISSING.to_string(), |uv| number(uv, 0)),
    }
}

fn title(column: HourlyColumn) -> &'static str {
    match column {
        HourlyColumn::Time => "Time",
        HourlyColumn::Icon => "",
        HourlyColumn::Temp => "Temp",
        HourlyColumn::Feels => "Feels",
        HourlyColumn::PrecipProb => "Rain",
        HourlyColumn::Precip => "Precip",
        HourlyColumn::Wind => "Wind",
        HourlyColumn::Humidity => "Hum",
        HourlyColumn::Uv => "UV",
    }
}

fn width(column: HourlyColumn) -> u16 {
    match column {
        HourlyColumn::Icon => 4,
        HourlyColumn::Time
        | HourlyColumn::Temp
        | HourlyColumn::Feels
        | HourlyColumn::PrecipProb
        | HourlyColumn::Humidity => 6,
        HourlyColumn::Precip => 8,
        HourlyColumn::Wind => 11,
        HourlyColumn::Uv => 3,
    }
}

/// The hourly table from the current hour, skipping `scroll` rows.
pub(crate) fn table(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer) {
    let inner = ctx.panel("Next hours", area, buf);
    let Some(h) = &ctx.vm.hourly else { return };
    let columns = &ctx.cfg.hourly_columns;
    let header =
        Row::new(columns.iter().map(|c| Cell::from(title(*c)))).style(ctx.pal.muted_style());
    let rows: Vec<Row<'_>> = h
        .hours
        .iter()
        .skip(h.now_index + usize::from(ctx.ui.scroll))
        .map(|hour| {
            Row::new(columns.iter().map(|c| {
                let text = cell(ctx, *c, hour);
                if *c == HourlyColumn::Temp {
                    Cell::from(text).style(ctx.pal.temp_style(hour.temp_level))
                } else {
                    Cell::from(text)
                }
            }))
        })
        .collect();
    Table::new(rows, columns.iter().map(|c| Constraint::Length(width(*c))))
        .header(header)
        .render(inner, buf);
}

/// Six columns of hour, icon and temperature (compact layout).
pub(crate) fn next_hours(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer) {
    let Some(h) = &ctx.vm.hourly else { return };
    let hours: Vec<&HourVm> = h.hours.iter().skip(h.now_index).take(6).collect();
    if hours.is_empty() {
        return;
    }
    let cells = Layout::horizontal(hours.iter().map(|_| Constraint::Fill(1))).split(area);
    for (hour, cell) in hours.iter().zip(cells.iter()) {
        Paragraph::new(alloc::vec![
            Line::styled(hour.label.clone(), ctx.pal.muted_style()),
            Line::raw(icons::condition(hour.icon, ctx.cfg.icons)),
            Line::styled(
                hour.temperature.text.clone(),
                ctx.pal.temp_style(hour.temp_level)
            ),
        ])
        .alignment(Alignment::Center)
        .render(*cell, buf);
    }
}
