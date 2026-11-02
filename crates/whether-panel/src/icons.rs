//! Procedural weather icons from embedded-graphics primitives: no bitmaps, any size.
//! Shapes are laid out in fractions of the icon box, so the same art serves 24, 32
//! and 64 px.

use embedded_graphics::Drawable;
use embedded_graphics::Pixel;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::{DrawTarget, Point, Primitive, Size};
use embedded_graphics::primitives::{
    Circle, Line, Polyline, PrimitiveStyle, Rectangle, RoundedRectangle,
};
use whether_core::icon::IconId;

use crate::theme::Palette;

/// Maps fractions of the icon box to pixels.
#[derive(Clone, Copy)]
struct Box {
    x: f32,
    y: f32,
    s: f32,
}

impl Box {
    fn p(self, fx: f32, fy: f32) -> Point {
        Point::new(
            libm::roundf(self.x + fx * self.s) as i32,
            libm::roundf(self.y + fy * self.s) as i32,
        )
    }

    /// A length of `f` box sizes, at least `min` pixels.
    fn len(self, f: f32, min: u32) -> u32 {
        (libm::roundf(f * self.s) as u32).max(min)
    }

    /// The stroke width for lines.
    fn stroke(self) -> u32 {
        self.len(1.0 / 16.0, 1)
    }
}

fn disc<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    b: Box,
    (cx, cy): (f32, f32),
    r: f32,
    color: Rgb565,
) -> Result<(), D::Error> {
    Circle::with_center(b.p(cx, cy), b.len(2.0 * r, 2))
        .into_styled(PrimitiveStyle::with_fill(color))
        .draw(d)
}

fn line<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    b: Box,
    from: (f32, f32),
    to: (f32, f32),
    width: u32,
    color: Rgb565,
) -> Result<(), D::Error> {
    Line::new(b.p(from.0, from.1), b.p(to.0, to.1))
        .into_styled(PrimitiveStyle::with_stroke(color, width))
        .draw(d)
}

/// A horizontal bar with round ends.
fn bar<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    b: Box,
    (x0, x1): (f32, f32),
    y: f32,
    color: Rgb565,
) -> Result<(), D::Error> {
    let h = b.len(0.07, 2);
    let left = b.p(x0, y);
    let width = b.p(x1, y).x - left.x;
    let rect = Rectangle::new(
        Point::new(left.x, left.y - h as i32 / 2),
        Size::new(width.max(1).unsigned_abs(), h),
    );
    RoundedRectangle::with_equal_corners(rect, Size::new(h / 2, h / 2))
        .into_styled(PrimitiveStyle::with_fill(color))
        .draw(d)
}

/// Sun: a disc with eight rays.
fn sun<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    b: Box,
    c: (f32, f32),
    r: f32,
    pal: &Palette,
) -> Result<(), D::Error> {
    let w = b.len(r / 3.5, 1);
    for i in 0..8 {
        let a = i as f32 * core::f32::consts::FRAC_PI_4;
        let (s, co) = (libm::sinf(a), libm::cosf(a));
        line(
            d,
            b,
            (c.0 + co * r * 1.4, c.1 + s * r * 1.4),
            (c.0 + co * r * 1.95, c.1 + s * r * 1.95),
            w,
            pal.sun,
        )?;
    }
    disc(d, b, c, r, pal.sun)
}

/// Moon: a crescent, drawn pixel by pixel as one disc minus another.
fn moon<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    b: Box,
    c: (f32, f32),
    r: f32,
    pal: &Palette,
) -> Result<(), D::Error> {
    let (cx, cy, rr) = (b.x + c.0 * b.s, b.y + c.1 * b.s, r * b.s);
    let (bx, by, br) = (cx + 0.55 * rr, cy - 0.35 * rr, 0.85 * rr);
    let x0 = libm::floorf(cx - rr) as i32;
    let y0 = libm::floorf(cy - rr) as i32;
    let n = libm::ceilf(2.0 * rr) as i32 + 1;
    let color = pal.moon;
    let pixels = (0..n * n).filter_map(move |i| {
        let (x, y) = (x0 + i % n, y0 + i / n);
        let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
        let inside = (px - cx) * (px - cx) + (py - cy) * (py - cy) <= rr * rr;
        let bitten = (px - bx) * (px - bx) + (py - by) * (py - by) <= br * br;
        (inside && !bitten).then_some(Pixel(Point::new(x, y), color))
    });
    d.draw_iter(pixels)
}

/// The celestial body behind a cloud or above bars.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Body {
    Sun,
    Moon,
}

fn body<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    b: Box,
    body: Body,
    c: (f32, f32),
    r: f32,
    pal: &Palette,
) -> Result<(), D::Error> {
    match body {
        Body::Sun => sun(d, b, c, r, pal),
        Body::Moon => moon(d, b, c, r * 1.25, pal),
    }
}

/// A cloud: three discs on a rounded base, outlined. `(ox, oy)` shifts it and `k`
/// scales it about the box center.
fn cloud<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    b: Box,
    (ox, oy): (f32, f32),
    k: f32,
    fill: Rgb565,
    pal: &Palette,
) -> Result<(), D::Error> {
    let t = |x: f32, y: f32| (0.5 + (x - 0.5) * k + ox, 0.5 + (y - 0.5) * k + oy);
    let puffs = [
        (t(0.34, 0.56), 0.14 * k),
        (t(0.53, 0.45), 0.20 * k),
        (t(0.72, 0.58), 0.13 * k),
    ];
    let (bx0, by0) = t(0.20, 0.55);
    let (bx1, by1) = t(0.86, 0.74);
    let o = 1.0 / b.s * b.len(1.0 / 28.0, 1) as f32;
    for (grow, color) in [(o, pal.outline), (0.0, fill)] {
        for (c, r) in puffs {
            disc(d, b, c, r + grow, color)?;
        }
        let tl = b.p(bx0 - grow, by0 - grow);
        let br = b.p(bx1 + grow, by1 + grow);
        let size = Size::new(
            (br.x - tl.x).max(1).unsigned_abs(),
            (br.y - tl.y).max(1).unsigned_abs(),
        );
        let radius = b.len((by1 - by0) / 2.0 * k.min(1.0) + grow, 1);
        RoundedRectangle::with_equal_corners(Rectangle::new(tl, size), Size::new(radius, radius))
            .into_styled(PrimitiveStyle::with_fill(color))
            .draw(d)?;
    }
    Ok(())
}

/// Slanted drops below a cloud at the given x positions.
fn drops<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    b: Box,
    xs: &[f32],
    length: f32,
    width: u32,
    pal: &Palette,
) -> Result<(), D::Error> {
    for &x in xs {
        line(
            d,
            b,
            (x, 0.74),
            (x - length * 0.4, 0.74 + length),
            width,
            pal.rain,
        )?;
    }
    Ok(())
}

/// A six-armed snowflake.
fn flake<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    b: Box,
    c: (f32, f32),
    pal: &Palette,
) -> Result<(), D::Error> {
    let r = 0.07;
    let w = b.len(1.0 / 24.0, 1);
    // Outlined, so white flakes show on light snow skies too.
    for (width, color) in [(w + 2, pal.outline), (w, pal.snow)] {
        for i in 0..3 {
            let a = i as f32 * core::f32::consts::FRAC_PI_3 + core::f32::consts::FRAC_PI_2;
            let (s, co) = (libm::sinf(a) * r, libm::cosf(a) * r);
            line(d, b, (c.0 - co, c.1 - s), (c.0 + co, c.1 + s), width, color)?;
        }
    }
    Ok(())
}

/// A lightning bolt as a thick zigzag.
fn bolt<D: DrawTarget<Color = Rgb565>>(d: &mut D, b: Box, pal: &Palette) -> Result<(), D::Error> {
    let points = [
        b.p(0.60, 0.60),
        b.p(0.46, 0.79),
        b.p(0.58, 0.78),
        b.p(0.47, 0.97),
    ];
    Polyline::new(&points)
        .into_styled(PrimitiveStyle::with_stroke(pal.bolt, b.len(1.0 / 11.0, 2)))
        .draw(d)
}

/// A wavy line (smoke, wind) from `x0` to `x1` at `y`.
fn wave<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    b: Box,
    (x0, x1): (f32, f32),
    y: f32,
    amplitude: f32,
    color: Rgb565,
) -> Result<(), D::Error> {
    let mut points = [Point::zero(); 9];
    for (i, p) in points.iter_mut().enumerate() {
        let t = i as f32 / 8.0;
        let x = x0 + (x1 - x0) * t;
        *p = b.p(x, y + amplitude * libm::sinf(t * core::f32::consts::TAU));
    }
    Polyline::new(&points)
        .into_styled(PrimitiveStyle::with_stroke(color, b.stroke().max(2)))
        .draw(d)
}

/// A wind stroke ending in a curl.
fn gust<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    b: Box,
    (x0, x1): (f32, f32),
    y: f32,
    color: Rgb565,
) -> Result<(), D::Error> {
    let r = 0.08;
    let mut points = [Point::zero(); 9];
    points[0] = b.p(x0, y);
    for (i, p) in points.iter_mut().enumerate().skip(1) {
        // Three quarters of a circle above the stroke's end.
        let a = core::f32::consts::FRAC_PI_2 - (i - 1) as f32 * core::f32::consts::PI * 1.5 / 7.0;
        *p = b.p(x1 + r * libm::cosf(a), y - r + r * libm::sinf(a));
    }
    Polyline::new(&points)
        .into_styled(PrimitiveStyle::with_stroke(color, b.stroke().max(2)))
        .draw(d)
}

/// Draws `icon` in the square at `top_left` with sides of `size` pixels.
#[allow(
    clippy::too_many_lines,
    reason = "one arm per icon reads best as a table"
)]
pub fn draw_icon<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    icon: IconId,
    top_left: Point,
    size: u32,
    pal: &Palette,
) -> Result<(), D::Error> {
    let b = Box {
        x: top_left.x as f32,
        y: top_left.y as f32,
        s: size as f32,
    };
    let w = b.stroke().max(2);
    // A body peeking out behind a cloud.
    let behind = |d: &mut D, which: Body| body(d, b, which, (0.36, 0.33), 0.14, pal);
    // A cloud raised to leave room for precipitation.
    let high = (0.0, -0.1);
    match icon {
        IconId::ClearDay => sun(d, b, (0.5, 0.5), 0.22, pal),
        IconId::ClearNight => moon(d, b, (0.5, 0.5), 0.3, pal),
        IconId::PartlyCloudyDay | IconId::PartlyCloudyNight => {
            let which = if icon == IconId::PartlyCloudyDay {
                Body::Sun
            } else {
                Body::Moon
            };
            behind(d, which)?;
            cloud(d, b, (0.04, 0.06), 0.9, pal.cloud, pal)
        }
        IconId::Cloudy => {
            cloud(d, b, (-0.1, -0.12), 0.7, pal.cloud_dark, pal)?;
            cloud(d, b, (0.04, 0.06), 0.9, pal.cloud, pal)
        }
        IconId::OvercastDay | IconId::OvercastNight => {
            let which = if icon == IconId::OvercastDay {
                Body::Sun
            } else {
                Body::Moon
            };
            behind(d, which)?;
            cloud(d, b, (0.04, 0.06), 0.9, pal.cloud_dark, pal)
        }
        IconId::FogDay | IconId::FogNight | IconId::Haze => {
            let which = if icon == IconId::FogNight {
                Body::Moon
            } else {
                Body::Sun
            };
            body(d, b, which, (0.5, 0.36), 0.15, pal)?;
            let rows: &[f32] = if icon == IconId::Haze {
                &[0.7, 0.84]
            } else {
                &[0.62, 0.75, 0.88]
            };
            for (i, &y) in rows.iter().enumerate() {
                let inset = if i % 2 == 0 { 0.0 } else { 0.08 };
                bar(d, b, (0.14 + inset, 0.86 - inset), y, pal.fog)?;
            }
            Ok(())
        }
        IconId::Smoke => {
            for (i, y) in [0.35, 0.55, 0.75].into_iter().enumerate() {
                let inset = i as f32 * 0.04;
                wave(d, b, (0.14 + inset, 0.86 - inset), y, 0.04, pal.fog)?;
            }
            Ok(())
        }
        IconId::Dust => {
            sun(d, b, (0.5, 0.36), 0.15, pal)?;
            for (x, y) in [
                (0.22, 0.72),
                (0.4, 0.82),
                (0.58, 0.72),
                (0.76, 0.82),
                (0.3, 0.92),
                (0.68, 0.92),
            ] {
                disc(d, b, (x, y), 0.035, pal.dust)?;
            }
            Ok(())
        }
        IconId::Drizzle => {
            cloud(d, b, high, 1.0, pal.cloud, pal)?;
            drops(d, b, &[0.38, 0.54, 0.70], 0.09, b.stroke(), pal)
        }
        IconId::Rain => {
            cloud(d, b, high, 1.0, pal.cloud, pal)?;
            drops(d, b, &[0.38, 0.54, 0.70], 0.18, w, pal)
        }
        IconId::ExtremeRain => {
            cloud(d, b, high, 1.0, pal.cloud_dark, pal)?;
            drops(
                d,
                b,
                &[0.30, 0.44, 0.58, 0.72],
                0.2,
                w.max(b.len(1.0 / 12.0, 2)),
                pal,
            )
        }
        IconId::ShowersDay | IconId::ShowersNight => {
            let which = if icon == IconId::ShowersDay {
                Body::Sun
            } else {
                Body::Moon
            };
            body(d, b, which, (0.36, 0.25), 0.13, pal)?;
            cloud(d, b, high, 0.95, pal.cloud, pal)?;
            drops(d, b, &[0.46, 0.64], 0.16, w, pal)
        }
        IconId::Sleet => {
            cloud(d, b, high, 1.0, pal.cloud, pal)?;
            drops(d, b, &[0.36, 0.72], 0.16, w, pal)?;
            flake(d, b, (0.54, 0.84), pal)
        }
        IconId::Snow | IconId::ExtremeSnow => {
            cloud(d, b, high, 1.0, pal.cloud, pal)?;
            let flakes: &[(f32, f32)] = if icon == IconId::Snow {
                &[(0.36, 0.82), (0.54, 0.88), (0.72, 0.82)]
            } else {
                &[
                    (0.28, 0.77),
                    (0.46, 0.77),
                    (0.64, 0.77),
                    (0.82, 0.77),
                    (0.37, 0.89),
                    (0.73, 0.89),
                ]
            };
            for &c in flakes {
                flake(d, b, c, pal)?;
            }
            Ok(())
        }
        IconId::SnowShowersDay | IconId::SnowShowersNight => {
            let which = if icon == IconId::SnowShowersDay {
                Body::Sun
            } else {
                Body::Moon
            };
            body(d, b, which, (0.36, 0.25), 0.13, pal)?;
            cloud(d, b, high, 0.95, pal.cloud, pal)?;
            flake(d, b, (0.46, 0.84), pal)?;
            flake(d, b, (0.66, 0.84), pal)
        }
        IconId::ThunderstormsDay | IconId::ThunderstormsNight => {
            let which = if icon == IconId::ThunderstormsDay {
                Body::Sun
            } else {
                Body::Moon
            };
            body(d, b, which, (0.36, 0.25), 0.13, pal)?;
            cloud(d, b, high, 0.95, pal.cloud_dark, pal)?;
            bolt(d, b, pal)
        }
        IconId::ThunderstormsRain => {
            cloud(d, b, high, 1.0, pal.cloud_dark, pal)?;
            drops(d, b, &[0.34, 0.78], 0.16, w, pal)?;
            bolt(d, b, pal)
        }
        IconId::Hail => {
            cloud(d, b, high, 1.0, pal.cloud, pal)?;
            drops(d, b, &[0.54], 0.16, w, pal)?;
            for c in [(0.34, 0.82), (0.72, 0.82), (0.46, 0.94)] {
                disc(d, b, c, 0.05, pal.outline)?;
                disc(d, b, c, 0.035, pal.snow)?;
            }
            Ok(())
        }
        IconId::Wind => {
            gust(d, b, (0.12, 0.62), 0.36, pal.fog)?;
            gust(d, b, (0.12, 0.78), 0.58, pal.fog)?;
            line(d, b, (0.12, 0.78), (0.56, 0.78), b.stroke().max(2), pal.fog)
        }
        IconId::Tornado => {
            for (i, y) in [0.2, 0.34, 0.48, 0.62, 0.76].into_iter().enumerate() {
                let half = 0.34 - i as f32 * 0.06;
                let center = 0.5 + i as f32 * 0.025;
                bar(d, b, (center - half, center + half), y, pal.fog)?;
            }
            Ok(())
        }
        IconId::NotAvailable => {
            Circle::with_center(b.p(0.5, 0.5), b.len(0.6, 4))
                .into_styled(PrimitiveStyle::with_stroke(pal.fog, w))
                .draw(d)?;
            line(d, b, (0.36, 0.5), (0.64, 0.5), w, pal.fog)
        }
    }
}
