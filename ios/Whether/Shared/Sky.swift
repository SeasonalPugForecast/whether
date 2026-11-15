import Combine
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

    /// A repeatable pseudo-random number in 0..<1 for `i`.
    static func noise(_ i: Int, _ salt: Double = 0) -> Double {
        let x = sin(Double(i) * 12.9898 + salt * 78.233) * 43758.5453
        return x - x.rounded(.down)
    }

    /// One cloud or fog bank: a soft white ellipse drifting across a sky of `size`.
    struct Bank {
        let width: Double
        let height: Double
        /// The ellipse's top (the blur spreads around it).
        let y: Double
        /// Points a second.
        let speed: Double
        let blur: Double
        let opacity: Double

        /// How far the blur spreads past the ellipse: its image's margin.
        var margin: Double { blur * 3 }

        init(_ i: Int, _ motion: Motion, _ size: CGSize) {
            let fog = motion == .fog
            speed = 6 + noise(i) * 10
            width = size.width * (0.7 + noise(i, 1) * 0.6)
            y = fog
                ? size.height * (0.15 + 0.17 * Double(i))
                : size.height * (0.04 + 0.1 * Double(i)) + noise(i, 3) * 40
            height = fog ? 90 : 70 + noise(i, 4) * 50
            blur = fog ? 30 : 22
            opacity = fog ? 0.16 : 0.13
        }

        /// How many banks drift in a sky of `motion`.
        static func count(_ motion: Motion) -> Int {
            switch motion {
            case .fog: 5
            case .clouds: 6
            default: 0
            }
        }
    }
}

/// The sky for `theme`, full screen, with its motion unless Reduce Motion is on.
struct SkyBackground: View {
    let theme: String
    var animated = true
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.colorTheme) private var colors

    /// The sky drawn: `theme` (screenshots: `-skyTheme fog` paints that sky, any
    /// `ViewModel.theme`, for the launch).
    private var shown: String {
        #if DEBUG
            let args = ProcessInfo.processInfo.arguments
            if let i = args.firstIndex(of: "-skyTheme"), i + 1 < args.count { return args[i + 1] }
        #endif
        return theme
    }

    var body: some View {
        let theme = shown
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
/// odd lightning flash, drawn each frame from the time alone. It holds still
/// while a page scrolls (`holdsSkyWhileScrolling`), in Low Power Mode and while
/// the app isn't in front, and carries on from where it stopped.
private struct SkyMotionView: View {
    let motion: Sky.Motion
    @Environment(\.scenePhase) private var scenePhase
    @State private var lowPower = ProcessInfo.processInfo.isLowPowerModeEnabled
    @State private var clock = SkyClock()

    private var paused: Bool {
        lowPower || scenePhase != .active || SkyScrolling.shared.scrolling
    }

    /// Slow drifts need few frames; the rain falls fast and keeps more.
    private var interval: Double {
        switch motion {
        case .rain, .storm: 1 / 30
        case .snow: 1 / 20
        case .none, .stars, .clouds, .fog: 1 / 15
        }
    }

    var body: some View {
        if motion == .none {
            Color.clear
        } else {
            let paused = paused
            GeometryReader { geo in
                // The banks are blurred once for the screen's size, then only moved.
                let banks = SkyBanks.images(for: motion, size: geo.size)
                TimelineView(.animation(minimumInterval: interval, paused: paused)) { context in
                    Canvas { ctx, size in
                        draw(&ctx, size, clock.time(at: context.date), banks: banks)
                    }
                }
            }
            .allowsHitTesting(false)
            .onChange(of: paused, initial: true) { _, paused in clock.hold(paused) }
            .onReceive(
                NotificationCenter.default.publisher(for: .NSProcessInfoPowerStateDidChange)
                    .receive(on: RunLoop.main)
            ) { _ in
                lowPower = ProcessInfo.processInfo.isLowPowerModeEnabled
            }
        }
    }

    private func noise(_ i: Int, _ salt: Double = 0) -> Double { Sky.noise(i, salt) }

    private func draw(_ ctx: inout GraphicsContext, _ size: CGSize, _ t: Double, banks: [Image]) {
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
            for (i, image) in banks.enumerated() {
                let bank = Sky.Bank(i, motion, size)
                let x = (noise(i, 2) * (size.width + bank.width) + t * bank.speed)
                    .truncatingRemainder(dividingBy: size.width + bank.width) - bank.width
                var layer = ctx
                layer.opacity = bank.opacity
                layer.draw(
                    image,
                    in: CGRect(
                        x: x - bank.margin, y: bank.y - bank.margin,
                        width: bank.width + 2 * bank.margin, height: bank.height + 2 * bank.margin))
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

/// The cloud and fog banks, each blurred once into an image that is then only
/// moved: a blur every frame, under glass cards that sample the sky, keeps the
/// GPU busy. Kept for the last few sizes (a rotation, the plan's own sky).
@MainActor
private enum SkyBanks {
    private struct Key: Hashable {
        let motion: Sky.Motion
        let width: Double
        let height: Double
    }

    private static var kept: [Key: [Image]] = [:]

    static func images(for motion: Sky.Motion, size: CGSize) -> [Image] {
        let count = Sky.Bank.count(motion)
        guard count > 0, size.width > 0, size.height > 0 else { return [] }
        let key = Key(motion: motion, width: size.width, height: size.height)
        if let images = kept[key] { return images }
        let images = (0..<count).compactMap { image(Sky.Bank($0, motion, size)) }
        if kept.count >= 4 { kept.removeAll() }
        kept[key] = images
        return images
    }

    /// The bank's ellipse in white, blurred, with room for the blur around it (the
    /// canvas gives it its opacity). One pixel a point: blurred this much, more
    /// would not show.
    private static func image(_ bank: Sky.Bank) -> Image? {
        let renderer = ImageRenderer(
            content: Ellipse()
                .fill(.white)
                .frame(width: bank.width, height: bank.height)
                .padding(bank.margin)
                .blur(radius: bank.blur))
        renderer.scale = 1
        return renderer.cgImage.map { Image(decorative: $0, scale: 1) }
    }
}

/// The sky's own time: it stops while the sky holds still and carries on from
/// there, so nothing jumps when it moves again.
private final class SkyClock {
    /// The time spent holding still so far.
    private var held: TimeInterval = 0
    /// Holding still since then.
    private var since: Date?

    func hold(_ paused: Bool, at now: Date = .now) {
        if paused {
            if since == nil { since = now }
        } else if let since {
            held += now.timeIntervalSince(since)
            self.since = nil
        }
    }

    /// The time to draw for a frame at `date`.
    func time(at date: Date) -> TimeInterval {
        let date = since.map { min(date, $0) } ?? date
        return date.timeIntervalSinceReferenceDate - held
    }
}

/// Whether a page is scrolling, so the sky behind it holds still meanwhile: the
/// glass cards moving over it then sample a still sky.
@MainActor
@Observable
final class SkyScrolling {
    static let shared = SkyScrolling()

    /// The scroll views scrolling now.
    private var moving: Set<UUID> = []

    var scrolling: Bool { !moving.isEmpty }

    func set(_ id: UUID, scrolling: Bool) {
        if scrolling {
            moving.insert(id)
        } else {
            moving.remove(id)
        }
    }
}

private struct SkyHold: ViewModifier {
    @State private var id = UUID()

    func body(content: Content) -> some View {
        content
            .onScrollPhaseChange { _, phase in
                SkyScrolling.shared.set(id, scrolling: phase.isScrolling)
            }
            .onDisappear { SkyScrolling.shared.set(id, scrolling: false) }
    }
}

extension View {
    /// The sky behind holds still while this scroll view scrolls.
    func holdsSkyWhileScrolling() -> some View { modifier(SkyHold()) }
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
