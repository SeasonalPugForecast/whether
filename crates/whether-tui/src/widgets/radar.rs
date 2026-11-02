//! The radar view: the map and the shown frame in half blocks, two
//! pixels a cell. Truecolor terminals see both; with 16 colors only the echoes are
//! drawn, in the usual green–yellow–red ramp; without colors, in shades.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget, Wrap};

use super::Ctx;
use crate::RadarImage;
use crate::config::Background;
use crate::ui_state::ColorDepth;

/// Width over height of the shown part of the map, at most this far from square.
const MAX_ASPECT: f32 = 2.0;

/// An averaged pixel: the map's color, the echo's color and how much of the pixel
/// the echo covers (0–1).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Sample {
    base: [f32; 3],
    echo: [f32; 3],
    cover: f32,
}

/// Echo strength, from the composite's color ramp: 0 none, 1 light (blues),
/// 2 green, 3 yellow–orange, 4 red, 5 extreme (magenta and white).
fn level(rgb: [f32; 3], cover: f32) -> u8 {
    if cover < 0.35 {
        return 0;
    }
    let [r, g, b] = rgb;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    if max - min < 30.0 {
        return if max > 200.0 { 5 } else { 1 };
    }
    let hue = if (max - r).abs() < f32::EPSILON {
        60.0 * ((g - b) / (max - min))
    } else if (max - g).abs() < f32::EPSILON {
        60.0 * ((b - r) / (max - min)) + 120.0
    } else {
        60.0 * ((r - g) / (max - min)) + 240.0
    };
    let hue = if hue < 0.0 { hue + 360.0 } else { hue };
    match hue {
        h if h < 20.0 => 4,
        h if h < 75.0 => 3,
        h if h < 160.0 => 2,
        h if h < 260.0 => 1,
        h if h < 340.0 => 5,
        _ => 4,
    }
}

fn level_color(level: u8) -> Color {
    match level {
        1 => Color::Cyan,
        2 => Color::Green,
        3 => Color::Yellow,
        4 => Color::Red,
        _ => Color::Magenta,
    }
}

/// The part of the map shown in `cols` × `rows` cells: `(x, y, width, height)` in
/// image pixels, as wide as the cells allow and centered on the marker where the
/// map reaches.
fn crop(image: &RadarImage<'_>, cols: u16, rows: u16) -> (f32, f32, f32, f32) {
    let (w, h) = (image.width as f32, image.height as f32);
    let aspect = f32::from(cols) / (f32::from(rows) * 2.0);
    let (cw, ch) = if aspect * h > w {
        (w, w / aspect)
    } else {
        (h * aspect, h)
    };
    let center = |marker: u32, size: f32, full: f32| {
        (marker as f32 - size / 2.0).clamp(0.0, (full - size).max(0.0))
    };
    (
        center(image.marker.0, cw, w),
        center(image.marker.1, ch, h),
        cw,
        ch,
    )
}

/// The averages over image pixels `x0..x1` × `y0..y1`.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn sample(image: &RadarImage<'_>, x0: f32, y0: f32, x1: f32, y1: f32) -> Sample {
    let clamp = |v: f32, max: u32| (v as u32).min(max.saturating_sub(1));
    let (xa, ya) = (clamp(x0, image.width), clamp(y0, image.height));
    let xb = clamp(x1 - 0.001, image.width).max(xa);
    let yb = clamp(y1 - 0.001, image.height).max(ya);
    let mut base = [0.0_f32; 3];
    let mut echo = [0.0_f32; 3];
    let mut alpha = 0.0_f32;
    let mut n = 0.0_f32;
    for y in ya..=yb {
        for x in xa..=xb {
            let i = ((y * image.width + x) * 4) as usize;
            if let Some(p) = image.base.get(i..i + 3) {
                for (c, v) in base.iter_mut().zip(p) {
                    *c += f32::from(*v);
                }
            }
            if let Some(p) = image.frame.and_then(|f| f.get(i..i + 4)) {
                let a = f32::from(p[3]) / 255.0;
                for (c, v) in echo.iter_mut().zip(&p[..3]) {
                    *c += f32::from(*v) * a;
                }
                alpha += a;
            }
            n += 1.0;
        }
    }
    Sample {
        base: base.map(|c| c / n),
        echo: if alpha > 0.0 {
            echo.map(|c| c / alpha)
        } else {
            [0.0; 3]
        },
        cover: alpha / n,
    }
}

/// A sample's truecolor: the map dimmed on dark terminals so the echoes stand
/// out, and the echo over it where it covers enough.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn rgb(s: &Sample, background: Background) -> Color {
    let dim = match background {
        Background::Dark => 0.45,
        Background::Light => 0.9,
    };
    let t = (s.cover * 2.0).min(1.0);
    let mix = |i: usize| (s.base[i] * dim * (1.0 - t) + s.echo[i] * t).clamp(0.0, 255.0) as u8;
    Color::Rgb(mix(0), mix(1), mix(2))
}

/// One cell's symbol and style from its top and bottom samples.
fn cell(ctx: &Ctx<'_>, top: &Sample, bottom: &Sample) -> (&'static str, Style) {
    let levels = (level(top.echo, top.cover), level(bottom.echo, bottom.cover));
    match ctx.ui.colors {
        ColorDepth::TrueColor if ctx.ascii() => {
            let mean = Sample {
                base: core::array::from_fn(|i| f32::midpoint(top.base[i], bottom.base[i])),
                echo: if levels.0 >= levels.1 {
                    top.echo
                } else {
                    bottom.echo
                },
                cover: top.cover.max(bottom.cover),
            };
            (" ", Style::new().bg(rgb(&mean, ctx.cfg.background)))
        }
        ColorDepth::TrueColor => (
            "▀",
            Style::new()
                .fg(rgb(top, ctx.cfg.background))
                .bg(rgb(bottom, ctx.cfg.background)),
        ),
        ColorDepth::Ansi16 if ctx.ascii() => match levels.0.max(levels.1) {
            0 => (" ", Style::new()),
            l => (" ", Style::new().bg(level_color(l))),
        },
        ColorDepth::Ansi16 => {
            let color = |l: u8| if l == 0 { Color::Reset } else { level_color(l) };
            match levels {
                (0, 0) => (" ", Style::new()),
                (t, b) => ("▀", Style::new().fg(color(t)).bg(color(b))),
            }
        }
        ColorDepth::Plain => {
            let shades: [&str; 6] = if ctx.ascii() {
                [" ", ".", ":", "+", "*", "#"]
            } else {
                [" ", "·", "░", "▒", "▓", "█"]
            };
            (shades[usize::from(levels.0.max(levels.1))], Style::new())
        }
    }
}

/// The map in `area`, as large as fits; returns the cells drawn.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn map(ctx: &Ctx<'_>, image: &RadarImage<'_>, area: Rect, buf: &mut Buffer) -> Rect {
    let rows = area.height;
    let cols = area
        .width
        .min((f32::from(rows) * 2.0 * MAX_ASPECT) as u16)
        .max(1);
    let area = Rect {
        x: area.x + (area.width - cols) / 2,
        width: cols,
        ..area
    };
    let (sx, sy, sw, sh) = crop(image, cols, rows);
    let (px, py) = (sw / f32::from(cols), sh / (f32::from(rows) * 2.0));
    for row in 0..rows {
        for col in 0..cols {
            let x0 = sx + f32::from(col) * px;
            let y0 = sy + f32::from(row) * 2.0 * py;
            let top = sample(image, x0, y0, x0 + px, y0 + py);
            let bottom = sample(image, x0, y0 + py, x0 + px, y0 + 2.0 * py);
            let (symbol, style) = cell(ctx, &top, &bottom);
            buf[(area.x + col, area.y + row)]
                .set_symbol(symbol)
                .set_style(style);
        }
    }
    let (mx, my) = (image.marker.0 as f32 - sx, image.marker.1 as f32 - sy);
    if (0.0..sw).contains(&mx) && (0.0..sh).contains(&my) {
        let col = (mx / px) as u16;
        let row = (my / (2.0 * py)) as u16;
        if col < cols && row < rows {
            let fg = match ctx.cfg.background {
                Background::Dark => Color::White,
                Background::Light => Color::Black,
            };
            let mut style = Style::new().add_modifier(Modifier::BOLD);
            if ctx.ui.colors != ColorDepth::Plain {
                style = style.fg(fg);
            }
            buf[(area.x + col, area.y + row)]
                .set_symbol("+")
                .set_style(style);
        }
    }
    area
}

/// The playback line: play or pause, the frame's time and where it is in the loop.
fn caption<'a>(ctx: &Ctx<'a>, labels: &'a [String], shown: usize) -> Line<'a> {
    let (play, pause, done, todo) = if ctx.ascii() {
        (">", "||", "#", ".")
    } else {
        ("▶", "⏸", "●", "○")
    };
    let dots: String = (0..labels.len())
        .map(|i| if i <= shown { done } else { todo })
        .collect::<Vec<_>>()
        .concat();
    let label = labels.get(shown).map_or("", String::as_str);
    Line::from(alloc::vec![
        Span::styled(
            format!("{} ", if ctx.ui.radar_paused { pause } else { play }),
            ctx.pal.accent_style(),
        ),
        Span::styled(label, ctx.pal.accent_style().add_modifier(Modifier::BOLD)),
        Span::styled(format!("  {dots}"), ctx.pal.muted_style()),
    ])
}

/// Lines `text` takes when wrapped at words to `width` columns.
fn wrapped_rows(text: &str, width: u16) -> u16 {
    let width = usize::from(width.max(1));
    let mut rows = 1_u16;
    let mut used = 0;
    for word in text.split(' ') {
        let len = word.chars().count();
        let need = if used == 0 { len } else { used + 1 + len };
        if used > 0 && need > width {
            rows = rows.saturating_add(1);
            used = len;
        } else {
            used = need;
        }
        while used > width {
            rows = rows.saturating_add(1);
            used -= width;
        }
    }
    rows
}

/// The radar panel: the map with the frame's time and the required credit.
pub(crate) fn radar(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer) {
    let inner = ctx.panel("Radar", area, buf);
    let (Some(vm), Some(image)) = (ctx.vm.radar.as_ref(), ctx.radar.as_ref()) else {
        Paragraph::new(alloc::vec![
            Line::styled("No radar for this location", ctx.pal.accent_style()),
            Line::styled(
                "Add \"radar\" to [presentation] widgets; it covers the contiguous US.",
                ctx.pal.muted_style(),
            ),
        ])
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true })
        .render(inner, buf);
        return;
    };
    let credit = Paragraph::new(Line::styled(vm.attribution.as_str(), ctx.pal.muted_style()))
        .alignment(Alignment::Right)
        .wrap(Wrap { trim: true });
    let credit_rows = wrapped_rows(&vm.attribution, inner.width);
    let [image_area, info, credit_area] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Length(credit_rows),
    ])
    .areas(inner);
    if image_area.height > 0 && image_area.width > 0 {
        map(ctx, image, image_area, buf);
    }
    let shown = ctx.ui.radar_frame(vm.frames.len());
    caption(ctx, &vm.frames, shown).render(info, buf);
    credit.render(credit_area, buf);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn echo_levels_follow_the_color_ramp() {
        let full = 1.0;
        assert_eq!(level([0.0, 200.0, 0.0], full), 2, "green");
        assert_eq!(level([255.0, 255.0, 0.0], full), 3, "yellow");
        assert_eq!(level([255.0, 140.0, 0.0], full), 3, "orange");
        assert_eq!(level([255.0, 0.0, 0.0], full), 4, "red");
        assert_eq!(level([200.0, 0.0, 200.0], full), 5, "magenta");
        assert_eq!(level([255.0, 255.0, 255.0], full), 5, "white");
        assert_eq!(level([60.0, 120.0, 230.0], full), 1, "blue");
        assert_eq!(level([255.0, 0.0, 0.0], 0.1), 0, "a speck is not drawn");
    }

    #[test]
    fn credits_wrap_at_words() {
        assert_eq!(wrapped_rows("Radar: NOAA NEXRAD", 40), 1);
        assert_eq!(wrapped_rows("Radar: NOAA NEXRAD", 12), 2);
        assert_eq!(wrapped_rows("abcdefghij", 4), 3, "long words break");
    }

    #[test]
    fn crops_are_centered_on_the_marker_inside_the_map() {
        let base = [0_u8; 4];
        let image = RadarImage {
            width: 768,
            height: 768,
            base: &base,
            frame: None,
            marker: (700, 384),
        };
        let (x, y, w, h) = crop(&image, 80, 20);
        assert_eq!((w, h), (768.0, 384.0), "wide areas show the full width");
        assert_eq!((x, y), (0.0, 192.0));
        let (x, _, w, h) = crop(&image, 20, 20);
        assert_eq!((w, h), (384.0, 768.0));
        assert_eq!(x, 384.0, "clamped to the map's edge");
    }
}
