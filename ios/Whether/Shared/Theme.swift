import SwiftUI
import WhetherKit

extension EnvironmentValues {
    /// The color theme to paint with, resolved once at the root from
    /// `Preferences` (the app's window, the widget's entry, the Live Activity).
    @Entry var colorTheme: ColorTheme = ColorPresets.whether
    /// How the reader sees color (Settings → Display → Colors): the theme above already
    /// has its ramps; the radar repaints NOAA's tiles with them.
    @Entry var colorVision: ColorVision = .typical
    /// Patterns and symbols beside the colors that carry meaning (WCAG 1.4.1): a hatch
    /// and a snowflake on snow, a word on severity, a stepped radar legend, rain and
    /// snow marks on the hourly graph (`Preferences.showsPatterns`).
    @Entry var showsPatterns = false
}

extension Color {
    /// A theme's color: the one place a SwiftUI color is made from components.
    init(_ c: RGBA) {
        self.init(.sRGB, red: c.red, green: c.green, blue: c.blue, opacity: c.alpha)
    }
}

extension UIColor {
    /// A theme's color, in sRGB.
    convenience init(_ c: RGBA) {
        self.init(cgColor: CGColor(srgbRed: c.red, green: c.green, blue: c.blue, alpha: c.alpha))
    }

    /// Its sRGB components (a dynamic color as the current traits resolve it).
    var rgba: RGBA {
        var r: CGFloat = 0, g: CGFloat = 0, b: CGFloat = 0, a: CGFloat = 0
        getRed(&r, green: &g, blue: &b, alpha: &a)
        return RGBA(red: Double(r), green: Double(g), blue: Double(b), alpha: Double(a))
    }
}

extension SystemColor {
    /// SwiftUI's system color, which follows iOS (Increase Contrast, each version's tuning).
    var color: Color {
        switch self {
        case .green: .green
        case .yellow: .yellow
        case .orange: .orange
        case .red: .red
        case .purple: .purple
        }
    }
}

extension ThemeColor {
    /// The color to paint with: the system color it names, or its light color
    /// with the dark one in dark mode.
    var color: Color {
        if let system { return system.color }
        guard let dark, dark != light else { return Color(light) }
        let l = UIColor(light), d = UIColor(dark)
        return Color(uiColor: UIColor { $0.userInterfaceStyle == .dark ? d : l })
    }
}

/// Lookups on the resolved theme: what used to be `Palette`'s statics (Dark Sky's quiet
/// grays for cloud and blues for rain, in the Whether preset).
extension ColorTheme {
    func color(_ role: ColorRole) -> Color { self[role].color }

    /// The role for a timeline group.
    static func role(_ group: WeatherSymbols.Group) -> ColorRole {
        switch group {
        case .clear: .conditionClear
        case .partlyCloudy: .conditionPartlyCloudy
        case .cloudy: .conditionCloudy
        case .fog: .conditionFog
        case .drizzle: .conditionDrizzle
        case .rain: .conditionRain
        case .heavyRain: .conditionHeavyRain
        case .snow: .conditionSnow
        case .storm: .conditionStorm
        case .other: .conditionOther
        }
    }

    /// A condition group's color (a gray in light mode; translucent white of about
    /// the same weight in dark mode, which also reads on the skies).
    func condition(_ group: WeatherSymbols.Group) -> Color { color(Self.role(group)) }

    /// A timeline stretch's color, from the core's group name.
    func timeline(_ group: String) -> Color { color(ColorRole.condition(group)) }

    /// Text on a timeline stretch: white on the deep fills (rain and storms in
    /// Whether), else dark, each decided on the color the stretch has in that
    /// appearance (`timelineInk`).
    func onTimeline(_ group: String) -> Color {
        let role = ColorRole.condition(group)
        let light = timelineInk(role, dark: false), dark = timelineInk(role, dark: true)
        if light == dark { return light.color }
        let l = light.uiColor, d = dark.uiColor
        return Color(uiColor: UIColor { ($0.userInterfaceStyle == .dark ? d : l).resolvedColor(with: $0) })
    }

    /// A `Level` (good … extreme), or the secondary color for none.
    func level(_ name: String?) -> Color {
        ColorRole.level(name).map { color($0) } ?? .secondary
    }

    /// The rain accent: deeper on white, lighter in the dark and on the skies.
    var rain: Color { color(.rain) }

    /// Top and bottom of the gradient for the core's sky (`ViewModel.theme`).
    func sky(_ sky: String) -> (Color, Color) {
        let (top, bottom) = ColorRole.sky(sky)
        return (Color(self[top].light), Color(self[bottom].light))
    }

    /// A card's color on a sky: the middle of the gradient, a little deeper, so
    /// cards stand off it softly.
    func card(_ sky: String) -> Color { Color(skyCard(sky)) }
}

extension TimelineInk {
    var color: Color {
        switch self {
        case .white: .white
        case .primary: Color.primary.opacity(0.7)
        case .black: Color.black.opacity(0.7)
        }
    }

    var uiColor: UIColor {
        switch self {
        case .white: .white
        case .primary: UIColor.label.withAlphaComponent(0.7)
        case .black: UIColor.black.withAlphaComponent(0.7)
        }
    }
}

/// The language the core writes words in on this phone (as `Settings.phone` sets it),
/// for the core's words asked for directly (`severityWord`, `radarStepWords`).
enum CoreLanguage {
    static var current: String { Locale.current.language.languageCode?.identifier ?? "en" }
}

/// Diagonal hatching across a rectangle: the pattern on snow and mixed precipitation
///, drawn over the color so it reads without it. Clip it to the bar's shape.
nonisolated struct Hatch: Shape {
    /// The distance between lines.
    var spacing: CGFloat = 4

    func path(in rect: CGRect) -> Path {
        var path = Path()
        var x = rect.minX - rect.height
        while x < rect.maxX {
            path.move(to: CGPoint(x: x, y: rect.maxY))
            path.addLine(to: CGPoint(x: x + rect.height, y: rect.minY))
            x += spacing
        }
        return path
    }
}

extension ColorTheme {
    /// The hatch's color on a fill of `role`: dark lines on a light color, light on a
    /// dark one (`dark`: the color as dark mode resolves it).
    func hatch(on role: ColorRole, dark: Bool) -> Color {
        ColorMath.luminance(self[role].resolved(dark: dark)) > 0.35
            ? .black.opacity(0.45) : .white.opacity(0.7)
    }
}

/// The Whether preset's rain accent, for a view that doesn't read the theme yet (the
/// widget gallery, `WidgetGallery.swift`).
enum Palette {
    static var rain: Color { ColorPresets.whether.rain }
}

/// A condition icon in two tones that read on light and dark backgrounds: gray
/// clouds, with the sun, moon, rain, snow or lightning in its own color.
/// (SF Symbols' multicolor clouds are white and vanish on a white screen.)
struct ConditionIcon: View {
    let icon: String
    var size: CGFloat = 22
    /// Moves gently, as Dark Sky's icons did: rain falls, clouds breathe.
    var animated = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.colorTheme) private var theme

    /// The accent of a symbol: what is drawn with the cloud, or the symbol itself.
    private func accent(_ symbol: String) -> ColorRole {
        if symbol.contains("bolt") { return .iconBolt }
        if symbol.contains("snow") || symbol.contains("sleet") || symbol.contains("hail") {
            return .iconSnow
        }
        if symbol.contains("rain") || symbol.contains("drizzle") { return .iconRain }
        if symbol.contains("sun") { return .iconSun }
        if symbol.contains("moon") { return .iconMoon }
        return .iconCloud
    }

    var body: some View {
        let symbol = WeatherSymbols.symbol(icon)
        let accent = theme.color(accent(symbol))
        // Cloud symbols: the cloud first, then what comes with it.
        let primary = symbol.hasPrefix("cloud") || symbol == "smoke.fill" ? theme.color(.iconCloud) : accent
        let image = Image(systemName: symbol)
            .symbolRenderingMode(.palette)
            .foregroundStyle(primary, accent)
            .font(.system(size: size))
            .accessibilityHidden(true)
        if animated && !reduceMotion {
            image.modifier(SkyMotion(symbol: symbol))
        } else {
            image
        }
    }
}

/// A symbol's motion: falling drops for rain and snow, a flicker for
/// lightning, a sway for wind, and a slow breath for sun, moon and cloud.
private struct SkyMotion: ViewModifier {
    let symbol: String

    func body(content: Content) -> some View {
        let wet = ["rain", "drizzle", "snow", "sleet", "hail"].contains { symbol.contains($0) }
        if symbol.contains("bolt") {
            content.symbolEffect(.pulse, options: .repeat(.continuous))
        } else if wet {
            content.symbolEffect(
                .variableColor.iterative.dimInactiveLayers.nonReversing,
                options: .repeat(.continuous).speed(0.6))
        } else if symbol.contains("wind") {
            content.symbolEffect(.wiggle, options: .repeat(.periodic(delay: 2)))
        } else {
            content.symbolEffect(.breathe.plain, options: .repeat(.continuous).speed(0.4))
        }
    }
}

/// A section title in Dark Sky's small caps style.
struct SectionTitle: View {
    let text: String

    var body: some View {
        Text(text.uppercased())
            .font(.caption.weight(.semibold))
            .foregroundStyle(.secondary)
            .tracking(0.6)
    }
}
