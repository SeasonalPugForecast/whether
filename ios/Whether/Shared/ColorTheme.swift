import Foundation

/// A painted thing with its own color in a theme. The raw values match
/// the core's enums (`level.poor`, `temp.cold`, `sky.clear_day.top`) so the tables
/// can move into `whether-core` later. Never rename one: saved themes use them.
nonisolated enum ColorRole: String, CaseIterable, Codable, Sendable {
    // The nine skies (`ViewModel.theme`), top and bottom of the gradient.
    case skyClearDayTop = "sky.clear_day.top", skyClearDayBottom = "sky.clear_day.bottom"
    case skyClearNightTop = "sky.clear_night.top", skyClearNightBottom = "sky.clear_night.bottom"
    case skyCloudyDayTop = "sky.cloudy_day.top", skyCloudyDayBottom = "sky.cloudy_day.bottom"
    case skyCloudyNightTop = "sky.cloudy_night.top", skyCloudyNightBottom = "sky.cloudy_night.bottom"
    case skyRainTop = "sky.rain.top", skyRainBottom = "sky.rain.bottom"
    case skySnowTop = "sky.snow.top", skySnowBottom = "sky.snow.bottom"
    case skyStormTop = "sky.storm.top", skyStormBottom = "sky.storm.bottom"
    case skyFogTop = "sky.fog.top", skyFogBottom = "sky.fog.bottom"
    case skyTwilightTop = "sky.twilight.top", skyTwilightBottom = "sky.twilight.bottom"
    // Condition groups: the conditions timeline (and the hourly graph's wet hours).
    case conditionClear = "condition.clear", conditionPartlyCloudy = "condition.partly_cloudy"
    case conditionCloudy = "condition.cloudy", conditionFog = "condition.fog"
    case conditionDrizzle = "condition.drizzle", conditionRain = "condition.rain"
    case conditionHeavyRain = "condition.heavy_rain", conditionSnow = "condition.snow"
    case conditionStorm = "condition.storm", conditionOther = "condition.other"
    // The hourly graph: dry hours by weather, then each measure's line.
    case graphClearDay = "graph.clear_day", graphClearNight = "graph.clear_night"
    case graphPartlyCloudyDay = "graph.partly_cloudy_day"
    case graphPartlyCloudyNight = "graph.partly_cloudy_night"
    case graphCloudy = "graph.cloudy", graphFog = "graph.fog"
    case graphTemperature = "graph.temperature", graphActual = "graph.actual"
    case graphWind = "graph.wind", graphHumidity = "graph.humidity"
    case graphUV = "graph.uv", graphPressure = "graph.pressure"
    // Condition icons' parts.
    case iconSun = "icon.sun", iconMoon = "icon.moon", iconCloud = "icon.cloud"
    case iconRain = "icon.rain", iconSnow = "icon.snow", iconBolt = "icon.bolt"
    /// The rain accent: chances, the next-hour graph, rain words.
    case rain = "rain"
    // Temperature: the core's `TempLevel` bands, then where the app paints cold and warm.
    case tempFreezing = "temp.freezing", tempCold = "temp.cold", tempCool = "temp.cool"
    case tempMild = "temp.mild", tempWarm = "temp.warm", tempHot = "temp.hot"
    case tempRangeLow = "temp.range_low", tempRangeHigh = "temp.range_high"
    case tempChartCool = "temp.chart_cool", tempChartWarm = "temp.chart_warm"
    case tempBelowNormal = "temp.below_normal", tempAboveNormal = "temp.above_normal"
    // Severity: the core's `Level` (UV, air quality, alerts).
    case levelGood = "level.good", levelFair = "level.fair", levelModerate = "level.moderate"
    case levelPoor = "level.poor", levelVeryPoor = "level.very_poor", levelExtreme = "level.extreme"
    // Precipitation type.
    case precipRain = "precip.rain", precipMixed = "precip.mixed", precipSnow = "precip.snow"
    // Radar intensity, light to heavy.
    case radar1 = "radar.1", radar2 = "radar.2", radar3 = "radar.3", radar4 = "radar.4"
    case radar5 = "radar.5"
    // Sun and moon: the day-light bar's twilight kinds and the sun's own marks.
    case twilightAstronomical = "twilight.astronomical", twilightNautical = "twilight.nautical"
    case twilightCivil = "twilight.civil", twilightGolden = "twilight.golden"
    case twilightDay = "twilight.day", twilightSun = "twilight.sun", twilightNoon = "twilight.noon"
    /// The sun crossing the sky behind the Now tab.
    case sunDisc = "sun.disc"
    /// The Plan tab's buttons.
    case accent = "accent"

    /// The editor's group for this role.
    var group: ColorRoleGroup {
        let family = rawValue.prefix { $0 != "." }
        return switch family {
        case "sky": .sky
        case "condition", "graph", "icon": .weather
        case "temp": .temperature
        case "level": .severity
        case "precip", "rain": .precipitation
        case "radar": .radar
        case "twilight", "sun": .sunAndMoon
        default: .accent
        }
    }

    /// Whether the color carries meaning (severity, temperature, precipitation type,
    /// radar intensity, condition groups, twilight kinds). Color vision replaces
    /// exactly these; the rest are decorative and stay the preset's.
    var isInformational: Bool {
        switch group {
        case .sky, .accent: false
        case .weather:
            rawValue.hasPrefix("condition.")
                || [.graphClearDay, .graphClearNight, .graphPartlyCloudyDay, .graphPartlyCloudyNight,
                    .graphCloudy, .graphFog].contains(self)
        case .precipitation: self != .rain
        case .sunAndMoon: self != .sunDisc
        case .temperature, .severity, .radar: true
        }
    }

    /// The top and bottom of the sky named by the core's `ViewModel.theme`
    /// (`clear_day`, …); unknown skies get the clear day's.
    static func sky(_ theme: String) -> (top: ColorRole, bottom: ColorRole) {
        guard let top = ColorRole(rawValue: "sky.\(theme).top"),
            let bottom = ColorRole(rawValue: "sky.\(theme).bottom")
        else { return (.skyClearDayTop, .skyClearDayBottom) }
        return (top, bottom)
    }

    /// The timeline color for the core's group name (`heavy_rain`, …); unknown
    /// groups are `condition.other`.
    static func condition(_ group: String) -> ColorRole {
        ColorRole(rawValue: "condition.\(group)") ?? .conditionOther
    }

    /// The color for the core's `Level` name (`very_poor`, …), or `nil` for none.
    static func level(_ name: String?) -> ColorRole? {
        name.flatMap { ColorRole(rawValue: "level.\($0)") }
    }

    /// The severity roles, good to extreme.
    static let levels: [ColorRole] = [
        .levelGood, .levelFair, .levelModerate, .levelPoor, .levelVeryPoor, .levelExtreme,
    ]
    /// The `TempLevel` roles, freezing to hot.
    static let temperatures: [ColorRole] = [
        .tempFreezing, .tempCold, .tempCool, .tempMild, .tempWarm, .tempHot,
    ]
    /// Precipitation types.
    static let precipitation: [ColorRole] = [.precipRain, .precipMixed, .precipSnow]
    /// Radar steps, light to heavy.
    static let radar: [ColorRole] = [.radar1, .radar2, .radar3, .radar4, .radar5]
    /// The day-light bar's stretches, from deep night to full day.
    static let twilightBar: [ColorRole] = [
        .twilightAstronomical, .twilightNautical, .twilightCivil, .twilightGolden, .twilightDay,
    ]
    /// Condition groups.
    static let conditions: [ColorRole] = allCases.filter { $0.rawValue.hasPrefix("condition.") }
}

/// The editor's groups (Settings → Display → Colors).
nonisolated enum ColorRoleGroup: String, CaseIterable, Sendable {
    case sky, weather, temperature, severity, precipitation, radar, sunAndMoon, accent

    var title: String {
        switch self {
        case .sky: "Sky"
        case .weather: "Weather"
        case .temperature: "Temperature"
        case .severity: "Severity"
        case .precipitation: "Precipitation"
        case .radar: "Radar"
        case .sunAndMoon: "Sun & moon"
        case .accent: "Accent"
        }
    }
}

/// A UIKit system color that a theme paints with when it names one: it follows
/// iOS (each version's tuning, Increase Contrast). Its hex values in the theme are
/// what iOS 27 shows, for the color math.
nonisolated enum SystemColor: String, Codable, Sendable {
    case green, yellow, orange, red, purple
}

/// One role's color: for light backgrounds, and optionally another for dark
/// ones (dark mode, and the skies, which are always dark).
nonisolated struct ThemeColor: Hashable, Sendable, Codable {
    var light: RGBA
    var dark: RGBA?
    /// Paint with this system color instead of the hex values (the Whether preset
    /// only: today's `.green`, `.orange`, …).
    var system: SystemColor?

    init(light: RGBA, dark: RGBA? = nil, system: SystemColor? = nil) {
        self.light = light
        self.dark = dark
        self.system = system
    }

    /// The color on a light (`dark == false`) or dark background.
    func resolved(dark isDark: Bool) -> RGBA {
        isDark ? (dark ?? light) : light
    }

    private enum Keys: String, CodingKey { case light, dark, system }

    /// `{"light": "#RRGGBB", "dark": "#RRGGBBAA", "system": "green"}`, or just
    /// `"#RRGGBB"`. Colors are clamped to sRGB.
    init(from decoder: Decoder) throws {
        func color(_ text: String, _ path: [CodingKey]) throws -> RGBA {
            guard let c = RGBA(hex: text) else {
                throw DecodingError.dataCorrupted(.init(codingPath: path, debugDescription: "not a color: \(text)"))
            }
            return c
        }
        if let text = try? decoder.singleValueContainer().decode(String.self) {
            self.init(light: try color(text, decoder.codingPath))
            return
        }
        let c = try decoder.container(keyedBy: Keys.self)
        let dark = try c.decodeIfPresent(String.self, forKey: .dark)
        self.init(
            light: try color(c.decode(String.self, forKey: .light), c.codingPath),
            dark: try dark.map { try color($0, c.codingPath) },
            system: try? c.decodeIfPresent(SystemColor.self, forKey: .system))
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        try c.encode(light.hex, forKey: .light)
        try c.encodeIfPresent(dark?.hex, forKey: .dark)
        try c.encodeIfPresent(system, forKey: .system)
    }
}

/// How the reader sees color: replaces the informational roles of any theme
/// with ramps that stay apart for them.
nonisolated enum ColorVision: String, CaseIterable, Codable, Sendable {
    case typical
    /// Protanopia and deuteranopia.
    case redGreen = "red_green"
    /// Tritanopia.
    case blueYellow = "blue_yellow"
    /// Achromatopsia: lightness alone, with patterns.
    case noColor = "no_color"

    /// Its name in Settings → Display → Colors.
    var title: String {
        switch self {
        case .typical: "Typical"
        case .redGreen: "Red–green (protan, deutan)"
        case .blueYellow: "Blue–yellow (tritan)"
        case .noColor: "No color"
        }
    }

    /// The deficiencies its ramps are checked against.
    var deficiencies: [Deficiency] {
        switch self {
        case .typical: []
        case .redGreen: [.protan, .deutan]
        case .blueYellow: [.tritan]
        case .noColor: [.achromat]
        }
    }
}

/// The ink for text on a colored fill (`ColorTheme.timelineInk`), at 70% unless white.
nonisolated enum TimelineInk: Sendable, Equatable {
    case white
    /// The primary color (black in light mode, white in dark), dimmed.
    case primary
    /// Black, dimmed: for a light fill in dark mode, where the primary color is white.
    case black
}

/// A full set of colors: a preset (`ColorPresets`) or the reader's own, based on one.
nonisolated struct ColorTheme: Identifiable, Hashable, Sendable, Codable {
    /// A preset's id (`whether`, `aurora`, …) or a custom theme's.
    var id: String
    var name: String
    /// The preset that fills roles this theme lacks; a preset's base is itself.
    var base: String
    var colors: [ColorRole: ThemeColor]
    /// Patterns and symbols always, whatever the setting (Graphite).
    var patterns = false

    init(
        id: String, name: String, base: String? = nil, colors: [ColorRole: ThemeColor],
        patterns: Bool = false
    ) {
        self.id = id
        self.name = name
        self.base = base ?? id
        self.colors = colors
        self.patterns = patterns
    }

    /// The color for `role`. Themes are completed when loaded (`completed(from:)`),
    /// so the Whether preset's color only stands in for a theme that wasn't.
    subscript(role: ColorRole) -> ThemeColor {
        colors[role] ?? ColorPresets.whether.colors[role] ?? ThemeColor(light: .black)
    }

    /// The roles this theme has no color for.
    var missing: [ColorRole] { ColorRole.allCases.filter { colors[$0] == nil } }

    /// This theme with every role it lacks taken from `base`: a role added in a
    /// later version never breaks a saved custom theme.
    func completed(from base: ColorTheme) -> ColorTheme {
        var out = self
        out.colors.merge(base.colors) { mine, _ in mine }
        return out
    }

    /// This theme with its informational roles replaced for `vision`.
    func applying(_ vision: ColorVision) -> ColorTheme {
        var out = self
        out.colors.merge(ColorPresets.ramps(for: vision)) { _, ramp in ramp }
        return out
    }

    /// The card's color on `sky` (`clear_day`, …): the middle of the gradient,
    /// a little deeper, so cards stand off it softly (`Sky.card`).
    func skyCard(_ sky: String) -> RGBA {
        let (top, bottom) = ColorRole.sky(sky)
        return Self.card(top: self[top].light, bottom: self[bottom].light)
    }

    /// A card on a gradient from `top` to `bottom`.
    static func card(top: RGBA, bottom: RGBA) -> RGBA {
        let deeper = 0.82
        return RGBA(
            red: (top.red + bottom.red) / 2 * deeper, green: (top.green + bottom.green) / 2 * deeper,
            blue: (top.blue + bottom.blue) / 2 * deeper)
    }

    /// Whether a label on a fill of `color` is white (else the primary color at
    /// 70%): white once it reaches 3:1, which keeps today's white on rain, heavy
    /// rain and storms. Translucent fills take the primary color, which adapts.
    static func whiteText(on color: RGBA) -> Bool {
        color.alpha >= 1 && ColorMath.contrast(.white, color) >= 3
    }

    /// The ink for a label on a timeline stretch of `role`, decided on the
    /// color the stretch is painted with in that appearance: its dark variant in dark
    /// mode (always on the sky). White once it reaches 3:1 (`whiteText`); else black at
    /// 70% on an opaque fill, and the primary color at 70% on a translucent one, which
    /// adapts (Whether's grays in the dark).
    func timelineInk(_ role: ColorRole, dark: Bool) -> TimelineInk {
        let fill = self[role].resolved(dark: dark)
        if Self.whiteText(on: fill) { return .white }
        return dark && fill.alpha >= 1 ? .black : .primary
    }

    private enum Keys: String, CodingKey { case id, name, base, colors, patterns }

    private struct RoleKey: CodingKey {
        var stringValue: String
        var intValue: Int? { nil }
        init(stringValue: String) { self.stringValue = stringValue }
        init?(intValue: Int) { nil }
    }

    /// Unknown roles and colors that don't parse are dropped (the base fills them).
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        let id = try c.decode(String.self, forKey: .id)
        var colors: [ColorRole: ThemeColor] = [:]
        if c.contains(.colors) {
            let roles = try c.nestedContainer(keyedBy: RoleKey.self, forKey: .colors)
            for key in roles.allKeys {
                guard let role = ColorRole(rawValue: key.stringValue),
                    let color = try? roles.decode(ThemeColor.self, forKey: key)
                else { continue }
                colors[role] = color
            }
        }
        self.init(
            id: id, name: try c.decodeIfPresent(String.self, forKey: .name) ?? id,
            base: try c.decodeIfPresent(String.self, forKey: .base) ?? ColorPresets.defaultID,
            colors: colors, patterns: try c.decodeIfPresent(Bool.self, forKey: .patterns) ?? false)
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        try c.encode(id, forKey: .id)
        try c.encode(name, forKey: .name)
        try c.encode(base, forKey: .base)
        var roles = c.nestedContainer(keyedBy: RoleKey.self, forKey: .colors)
        for (role, color) in colors.sorted(by: { $0.key.rawValue < $1.key.rawValue }) {
            try roles.encode(color, forKey: RoleKey(stringValue: role.rawValue))
        }
        if patterns { try c.encode(patterns, forKey: .patterns) }
    }
}
