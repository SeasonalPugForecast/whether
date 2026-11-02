import SwiftUI

/// The sky behind the forecast: a gradient for the core's sky (`ViewModel.theme`:
/// clear_day, cloudy_night, rain, …), deep enough for white text and glass
/// cards, with the weather moving in it.
enum Sky {
    // The gradients and the cards on them are the color theme's
    // (`ColorTheme.sky(_:)`, `ColorTheme.card(_:)`).

    /// What moves in the sky for `theme`.
    enum Motion {
        case none, stars, clouds, rain, snow, storm, fog
    }

    static func motion(_ theme: String) -> Motion {
        switch theme {
        case "clear_night": .stars
        case "cloudy_day", "cloudy_night": .clouds
        case "rain": .rain
        case "snow": .snow
        case "storm": .storm
        case "fog": .fog
        default: .none
        }
    }
}

/// The sky for `theme`, full screen, with its motion unless Reduce Motion is on.
struct SkyBackground: View {
    let theme: String
    var animated = true
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.colorTheme) private var colors

    var body: some View {
        let (top, bottom) = colors.sky(theme)
        ZStack {
            LinearGradient(colors: [top, bottom], startPoint: .top, endPoint: .bottom)
            if animated && !reduceMotion {
                SkyMotionView(motion: Sky.motion(theme))
            }
        }
        .ignoresSafeArea()
        .animation(.easeInOut(duration: 1.2), value: theme)
    }
}

/// Rain streaks, snowflakes, drifting clouds, fog banks, twinkling stars and the
/// odd lightning flash, drawn each frame from the time alone (no state).
private struct SkyMotionView: View {
    let motion: Sky.Motion

    var body: some View {
        if motion == .none {
            Color.clear
        } else {
            TimelineView(.animation(minimumInterval: 1 / 30)) { context in
                Canvas { ctx, size in
                    draw(&ctx, size, context.date.timeIntervalSinceReferenceDate)
                }
            }
            .allowsHitTesting(false)
        }
    }

    /// A repeatable pseudo-random number in 0..<1 for `i`.
    private func noise(_ i: Int, _ salt: Double = 0) -> Double {
        let x = sin(Double(i) * 12.9898 + salt * 78.233) * 43758.5453
        return x - x.rounded(.down)
    }

    private func draw(_ ctx: inout GraphicsContext, _ size: CGSize, _ t: Double) {
        switch motion {
        case .none:
            break
        case .stars:
            for i in 0..<70 {
                let x = noise(i) * size.width
                let y = noise(i, 1) * size.height * 0.6
                let twinkle = 0.35 + 0.65 * (0.5 + 0.5 * sin(t * (0.6 + noise(i, 2)) + Double(i)))
                let r = 0.6 + noise(i, 3) * 1.2
                ctx.fill(
                    Path(ellipseIn: CGRect(x: x, y: y, width: r * 2, height: r * 2)),
                    with: .color(.white.opacity(0.7 * twinkle)))
            }
        case .clouds, .fog:
            let banks = motion == .fog ? 5 : 6
            for i in 0..<banks {
                let speed = 6 + noise(i) * 10
                let w = size.width * (0.7 + noise(i, 1) * 0.6)
                let x = (noise(i, 2) * (size.width + w) + t * speed)
                    .truncatingRemainder(dividingBy: size.width + w) - w
                let y = motion == .fog
                    ? size.height * (0.15 + 0.17 * Double(i))
                    : size.height * (0.04 + 0.1 * Double(i)) + noise(i, 3) * 40
                let h = motion == .fog ? 90.0 : 70 + noise(i, 4) * 50
                var layer = ctx
                layer.addFilter(.blur(radius: motion == .fog ? 30 : 22))
                layer.fill(
                    Path(ellipseIn: CGRect(x: x, y: y, width: w, height: h)),
                    with: .color(.white.opacity(motion == .fog ? 0.16 : 0.13)))
            }
        case .rain, .storm:
            let drops = motion == .storm ? 140 : 100
            for i in 0..<drops {
                let speed = 520 + noise(i) * 320
                let len = 14 + noise(i, 1) * 16
                let x0 = noise(i, 2) * (size.width + 80)
                let y = (noise(i, 3) * (size.height + len) + t * speed)
                    .truncatingRemainder(dividingBy: size.height + len) - len
                // Slanted a little, as if blown.
                let x = x0 - y * 0.08
                var path = Path()
                path.move(to: CGPoint(x: x, y: y))
                path.addLine(to: CGPoint(x: x - len * 0.08, y: y + len))
                ctx.stroke(path, with: .color(.white.opacity(0.22)), lineWidth: 1.1)
            }
            if motion == .storm {
                // A flash now and then: a few frames every ~7 seconds.
                let phase = t.truncatingRemainder(dividingBy: 7.3)
                if phase < 0.12 || (phase > 0.22 && phase < 0.3) {
                    ctx.fill(
                        Path(CGRect(origin: .zero, size: size)), with: .color(.white.opacity(0.18)))
                }
            }
        case .snow:
            for i in 0..<90 {
                let speed = 30 + noise(i) * 40
                let r = 1.2 + noise(i, 1) * 2.6
                let y = (noise(i, 2) * (size.height + 10) + t * speed)
                    .truncatingRemainder(dividingBy: size.height + 10) - 10
                let sway = sin(t * (0.5 + noise(i, 3)) + Double(i)) * 18
                let x = noise(i, 4) * size.width + sway
                ctx.fill(
                    Path(ellipseIn: CGRect(x: x, y: y, width: r * 2, height: r * 2)),
                    with: .color(.white.opacity(0.75)))
            }
        }
    }
}

extension EnvironmentValues {
    /// The card color for the sky behind (`ColorTheme.card`), or `nil` on a plain
    /// background.
    @Entry var skyCard: Color? = nil
}

/// A glass card for a section: Liquid Glass on iOS 26 (a thin material before),
/// tinted with a deeper shade of the sky behind (soft, but clear), or a light
/// gray on plain backgrounds, with a hairline edge.
struct GlassCard: ViewModifier {
    var on = true
    var padding: CGFloat = 16
    @Environment(\.colorScheme) private var scheme
    @Environment(\.skyCard) private var skyCard

    func body(content: Content) -> some View {
        if on {
            let shape = RoundedRectangle(cornerRadius: 22, style: .continuous)
            let tint =
                skyCard.map { $0.opacity(0.5) }
                ?? (scheme == .dark ? Color.white.opacity(0.07) : Color.black.opacity(0.035))
            let shaped = content
                .padding(padding)
                .frame(maxWidth: .infinity, alignment: .leading)
            Group {
                if #available(iOS 26.0, *) {
                    shaped.glassEffect(.regular.tint(tint), in: shape)
                } else {
                    shaped.background(tint, in: shape).background(.ultraThinMaterial, in: shape)
                }
            }
            .overlay(
                shape.strokeBorder(
                    .white.opacity(skyCard != nil ? 0.22 : scheme == .dark ? 0.1 : 0.6), lineWidth: 0.6))
        } else {
            content
        }
    }
}

extension View {
    /// A glass card when `on`.
    func glassCard(_ on: Bool = true, padding: CGFloat = 16) -> some View {
        modifier(GlassCard(on: on, padding: padding))
    }
}
