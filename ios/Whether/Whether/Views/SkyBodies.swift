import SwiftUI
import WhetherKit

/// The sun by day and the moon while it is up, behind the Now tab's top summary,
/// crossing it as they cross the sky: from rising to setting along the width, low
/// at the edges and highest in the middle (`SkyArc`). Below the horizon a body is
/// not drawn. Its path is drawn too, faintly: dotted ahead of it and a little
/// stronger behind it, so how far through its day it is reads at a glance. It is
/// faint, takes no touches and is hidden from accessibility (it says nothing the
/// summary does not). The minute's tick moves it; with Reduce Motion it jumps
/// instead of gliding.
struct SkyBodies: View {
    let sun: ViewModel.Sun?
    let moon: ViewModel.Moon?
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.colorTheme) private var colors

    var body: some View {
        TimelineView(.periodic(from: .now, by: 60)) { context in
            let now = clock(context.date)
            let sunPlace = sun.flatMap { SkyArc.sun(now: now, rise: $0.rise?.at, set: $0.set?.at) }
            let moonPlace = moon.flatMap { SkyArc.moon(now: now, rise: $0.rise?.at, set: $0.set?.at) }
            GeometryReader { geo in
                ZStack {
                    if let sunPlace {
                        SkyPath(travelled: sunPlace.along, color: colors.color(.sunDisc)) {
                            point(SkyArc.place(progress: $0), in: geo.size, radius: 30)
                        }
                    }
                    if moon != nil, let moonPlace {
                        SkyPath(travelled: moonPlace.along, color: .primary) {
                            point(SkyArc.place(progress: $0), in: geo.size, radius: 24)
                        }
                    }
                    if let sunPlace {
                        SunDisc()
                            .opacity(sunPlace.fade)
                            .position(point(sunPlace, in: geo.size, radius: 30))
                    }
                    if let moon, let moonPlace {
                        MoonDisc(symbol: WeatherSymbols.symbol(moon.icon))
                            .opacity(moonPlace.fade)
                            .position(point(moonPlace, in: geo.size, radius: 24))
                    }
                }
                .animation(reduceMotion ? nil : .easeInOut(duration: 1), value: sunPlace)
                .animation(reduceMotion ? nil : .easeInOut(duration: 1), value: moonPlace)
            }
        }
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }

    /// The time the sky shows: now (screenshots: `-skyTime 0.3` is 30 % of the way from
    /// sunrise to sunset, `-0.2` before sunrise and `1.2` after sunset).
    private func clock(_ date: Date) -> Int64 {
        #if DEBUG
            let args = ProcessInfo.processInfo.arguments
            if let i = args.firstIndex(of: "-skyTime"), i + 1 < args.count, let fraction = Double(args[i + 1]),
                let rise = sun?.rise?.at, let set = sun?.set?.at
            {
                return SkyArc.now(fraction: fraction, rise: rise, set: set)
            }
        #endif
        return Int64(date.timeIntervalSince1970)
    }

    /// A body's centre: the horizon a little above the area's foot, the peak about a third
    /// of the way down, so at noon the sun sits behind the big number, clear of the
    /// condition icon above it.
    private func point(_ place: SkyArc.Place, in size: CGSize, radius: Double) -> CGPoint {
        let p = SkyArc.point(
            place, width: size.width, horizon: max(size.height - radius - 4, radius),
            peak: max(size.height * 0.34, radius + 4), inset: radius)
        return CGPoint(x: p.x, y: p.y)
    }
}

/// A soft glow with the sun's symbol, low in contrast: it sits behind text.
private struct SunDisc: View {
    @Environment(\.colorTheme) private var colors
    private var color: Color { colors.color(.sunDisc) }

    var body: some View {
        ZStack {
            Circle()
                .fill(RadialGradient(colors: [color.opacity(0.20), color.opacity(0)], center: .center, startRadius: 4, endRadius: 62))
                .frame(width: 124, height: 124)
            Image(systemName: "sun.max.fill")
                .font(.system(size: 52))
                .foregroundStyle(color)
                .opacity(0.36)
        }
    }
}

/// The moon, as it looks tonight (the phase's symbol), faint, with a pale glow.
private struct MoonDisc: View {
    let symbol: String

    var body: some View {
        ZStack {
            Circle()
                .fill(RadialGradient(colors: [Color.white.opacity(0.18), Color.white.opacity(0)], center: .center, startRadius: 4, endRadius: 60))
                .frame(width: 120, height: 120)
            Image(systemName: symbol)
                .symbolRenderingMode(.hierarchical)
                .font(.system(size: 44))
                .foregroundStyle(Color.primary)
                .opacity(0.32)
        }
    }
}

/// A body's path from horizon to horizon: solid and a little stronger behind it,
/// dotted ahead, with a small mark where it rose and where it will set.
private struct SkyPath: View {
    /// How far along it is, 0…1.
    let travelled: Double
    let color: Color
    /// The point `progress` (0…1) of the way along.
    let point: (Double) -> CGPoint

    var body: some View {
        let done = min(max(travelled, 0), 1)
        ZStack {
            arc(from: done, to: 1)
                .stroke(color.opacity(0.22), style: StrokeStyle(lineWidth: 1.5, lineCap: .round, dash: [2, 6]))
            arc(from: 0, to: done)
                .stroke(color.opacity(0.32), style: StrokeStyle(lineWidth: 1.5, lineCap: .round))
            ForEach([0.0, 1.0], id: \.self) { end in
                Circle()
                    .fill(color.opacity(0.32))
                    .frame(width: 5, height: 5)
                    .position(point(end))
            }
        }
    }

    private func arc(from start: Double, to end: Double) -> Path {
        Path { path in
            guard end > start else { return }
            let steps = max(Int((end - start) * 48), 2)
            path.move(to: point(start))
            for i in 1...steps {
                path.addLine(to: point(start + (end - start) * Double(i) / Double(steps)))
            }
        }
    }
}
