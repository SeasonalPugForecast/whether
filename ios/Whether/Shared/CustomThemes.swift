import Foundation

/// The reader's own color themes: made from a preset ("Customize…") or from
/// another custom theme ("Duplicate"), and saved small, with only the roles whose
/// color differs from the base preset (`Preferences.customThemes`). The base fills the
/// rest when the theme is resolved (`ColorPresets.completed`), so a role added later,
/// or the Seasons preset's next season, reaches custom themes too.
nonisolated enum CustomThemes {
    /// Custom theme ids start with this, so they never meet a preset's.
    static let idPrefix = "custom-"

    /// A new custom theme from `theme`: "My <Preset>" for a preset, "<Name> 2" for a
    /// custom theme (or the next free number), with the same base, colors and patterns.
    static func duplicate(
        _ theme: ColorTheme, existing: [ColorTheme], id: String = idPrefix + UUID().uuidString.lowercased()
    ) -> ColorTheme {
        let isCustom = existing.contains { $0.id == theme.id }
        let name = isCustom ? theme.name : "My \(theme.name)"
        return ColorTheme(
            id: id, name: uniqueName(name, among: existing), base: theme.base,
            colors: isCustom ? theme.colors : [:], patterns: theme.patterns)
    }

    /// `name`, or with the first free number after it ("My Aurora 2") when another
    /// custom theme has it already. A number it ends with is counted on from.
    static func uniqueName(_ name: String, among existing: [ColorTheme]) -> String {
        let taken = Set(existing.map(\.name))
        guard taken.contains(name) else { return name }
        var stem = name
        var n = 2
        if let space = name.lastIndex(of: " "), let number = Int(name[name.index(after: space)...]) {
            stem = String(name[..<space])
            n = number + 1
        }
        while taken.contains("\(stem) \(n)") { n += 1 }
        return "\(stem) \(n)"
    }

    /// A name typed for a theme, trimmed; `nil` when nothing is left.
    static func cleanName(_ text: String) -> String? {
        let name = text.trimmingCharacters(in: .whitespacesAndNewlines)
        return name.isEmpty ? nil : name
    }

    /// Saved custom themes as loaded: each once (the first of an id), never one with
    /// a preset's id (the preset would win). They stay as saved: Seasons' base changes
    /// with the month, so a role is only dropped when it is edited back to the base.
    static func loaded(_ themes: [ColorTheme]) -> [ColorTheme] {
        var seen = Set<String>()
        return themes.filter { theme in
            !theme.id.isEmpty && !ColorPresets.ids.contains(theme.id)
                && !ColorPresets.all.contains { $0.id == theme.id } && seen.insert(theme.id).inserted
        }
    }
}

nonisolated extension ColorTheme {
    /// Only the roles whose color differs from `base`'s: what a custom theme saves
    /// (for a theme that arrives whole, like a shared one).
    func trimmed(against base: ColorTheme) -> ColorTheme {
        var out = self
        out.colors = colors.filter { role, color in base.colors[role] != color }
        return out
    }

    /// This custom theme with `role` painted `color`; a color equal to the base's
    /// is dropped, so the role follows the base again.
    func setting(_ role: ColorRole, to color: ThemeColor, base: ColorTheme) -> ColorTheme {
        var out = self
        out.colors[role] = base.colors[role] == color ? nil : color
        return out
    }

    /// Whether this custom theme paints `role` differently from `base`.
    func differs(_ role: ColorRole, from base: ColorTheme) -> Bool {
        colors[role].map { $0 != base.colors[role] } ?? false
    }

    /// This theme as a reader with `deficiency` sees it, every color passed through
    /// the simulation (system colors by their hex): the editor's "Preview as".
    func simulated(as deficiency: Deficiency) -> ColorTheme {
        var out = self
        out.colors = colors.mapValues { color in
            ThemeColor(
                light: ColorMath.simulate(color.light, deficiency),
                dark: color.dark.map { ColorMath.simulate($0, deficiency) })
        }
        return out
    }
}

/// The editor's contrast lines: the thresholds of measured on the
/// theme being edited. They warn and never block.
nonisolated enum ThemeChecks {
    /// White text on a sky: WCAG AA for text.
    static let textMinimum = 4.5
    /// Informational marks on their card: WCAG 1.4.11.
    static let markMinimum = 3.0
    /// ΔE_OK that steps and categories keep apart.
    static let apartMinimum = 0.06

    /// The skies, by the core's `ViewModel.theme` names.
    static let skies = [
        "clear_day", "clear_night", "cloudy_day", "cloudy_night", "rain", "snow", "storm", "fog", "twilight",
    ]

    /// Marks that must stand off their card: severity, temperature bands and lines,
    /// precipitation bars.
    static let marks: [ColorRole] =
        ColorRole.levels + ColorRole.temperatures
        + [.tempRangeLow, .tempRangeHigh, .tempChartCool, .tempChartWarm] + ColorRole.precipitation

    /// The background a mark was measured on.
    enum Card: Equatable, Sendable {
        /// The plain light background's card (light colors).
        case light
        /// The plain dark background's card (dark colors).
        case dark
        /// A sky's card (dark colors), by the sky's name.
        case sky(String)
    }

    /// The lowest contrast of white text on a sky stop.
    struct SkyText: Equatable, Sendable {
        var role: ColorRole
        var ratio: Double
        var passes: Bool { ratio >= ThemeChecks.textMinimum }
    }

    /// The lowest contrast of a mark on its card.
    struct Mark: Equatable, Sendable {
        var role: ColorRole
        var card: Card
        var ratio: Double
        var passes: Bool { ratio >= ThemeChecks.markMinimum }
    }

    /// The closest pair of neighbouring steps (or of precipitation types).
    struct Apart: Equatable, Sendable {
        var first: ColorRole
        var second: ColorRole
        var delta: Double
        var passes: Bool { delta >= ThemeChecks.apartMinimum }
    }

    /// White text on every sky's top and bottom: the lowest.
    static func skyText(_ theme: ColorTheme) -> SkyText {
        let all = skies.flatMap { sky -> [SkyText] in
            let (top, bottom) = ColorRole.sky(sky)
            return [top, bottom].map { SkyText(role: $0, ratio: ColorMath.contrast(.white, theme[$0].light)) }
        }
        // INVARIANT: `skies` is not empty.
        return all.min { $0.ratio < $1.ratio }!
    }

    /// Each mark on its cards (light colors on the light card; dark ones on the dark
    /// card and every sky's): the lowest.
    static func mark(_ theme: ColorTheme) -> Mark {
        let cards: [(Card, RGBA)] =
            [(.dark, ColorPresets.darkCard)] + skies.map { (.sky($0), theme.skyCard($0)) }
        var worst: Mark?
        for role in marks {
            let color = theme[role]
            var found = [Mark(role: role, card: .light, ratio: ColorMath.contrast(color.light, ColorPresets.lightCard))]
            for (card, rgb) in cards {
                found.append(Mark(role: role, card: card, ratio: ColorMath.contrast(color.resolved(dark: true), rgb)))
            }
            for m in found where worst.map({ m.ratio < $0.ratio }) ?? true { worst = m }
        }
        // INVARIANT: `marks` is not empty.
        return worst!
    }

    /// Neighbouring steps of severity, radar, temperature and the twilight bar, and
    /// each pair of precipitation types, as a reader with `deficiency` sees them
    /// (typical vision for `nil`), light and dark: the closest.
    static func apart(_ theme: ColorTheme, as deficiency: Deficiency?) -> Apart {
        var pairs: [(ColorRole, ColorRole)] = []
        for ramp in [ColorRole.levels, ColorRole.radar, ColorRole.temperatures, ColorRole.twilightBar] {
            pairs += zip(ramp, ramp.dropFirst()).map { ($0, $1) }
        }
        let p = ColorRole.precipitation
        for i in p.indices { for j in p.indices where j > i { pairs.append((p[i], p[j])) } }
        var closest: Apart?
        for (a, b) in pairs {
            for dark in [false, true] {
                let delta = ColorMath.deltaE(
                    theme[a].resolved(dark: dark), theme[b].resolved(dark: dark), as: deficiency)
                if closest.map({ delta < $0.delta }) ?? true { closest = Apart(first: a, second: b, delta: delta) }
            }
        }
        // INVARIANT: the ramps are not empty.
        return closest!
    }
}

nonisolated extension ColorRole {
    /// The role's name in the editor, in words.
    var title: String {
        if let sky = skyName {
            return "\(sky.prefix(1).uppercased())\(sky.dropFirst()), \(rawValue.hasSuffix(".top") ? "top" : "bottom")"
        }
        return switch self {
        case .conditionClear: "Timeline: clear"
        case .conditionPartlyCloudy: "Timeline: partly cloudy"
        case .conditionCloudy: "Timeline: cloudy"
        case .conditionFog: "Timeline: fog"
        case .conditionDrizzle: "Timeline: drizzle"
        case .conditionRain: "Timeline: rain"
        case .conditionHeavyRain: "Timeline: heavy rain"
        case .conditionSnow: "Timeline: snow"
        case .conditionStorm: "Timeline: storm"
        case .conditionOther: "Timeline: other"
        case .graphClearDay: "Graph: clear day"
        case .graphClearNight: "Graph: clear night"
        case .graphPartlyCloudyDay: "Graph: partly cloudy day"
        case .graphPartlyCloudyNight: "Graph: partly cloudy night"
        case .graphCloudy: "Graph: cloudy"
        case .graphFog: "Graph: fog"
        case .graphTemperature: "Graph: temperature line"
        case .graphActual: "Graph: what it was"
        case .graphWind: "Graph: wind line"
        case .graphHumidity: "Graph: humidity line"
        case .graphUV: "Graph: UV line"
        case .graphPressure: "Graph: pressure line"
        case .iconSun: "Icon: sun"
        case .iconMoon: "Icon: moon"
        case .iconCloud: "Icon: cloud"
        case .iconRain: "Icon: rain"
        case .iconSnow: "Icon: snow"
        case .iconBolt: "Icon: lightning"
        case .rain: "Rain accent"
        case .tempFreezing: "Freezing"
        case .tempCold: "Cold"
        case .tempCool: "Cool"
        case .tempMild: "Mild"
        case .tempWarm: "Warm"
        case .tempHot: "Hot"
        case .tempRangeLow: "Day range, low end"
        case .tempRangeHigh: "Day range, high end"
        case .tempChartCool: "Plan chart, lows"
        case .tempChartWarm: "Plan chart, highs"
        case .tempBelowNormal: "Below normal"
        case .tempAboveNormal: "Above normal"
        case .levelGood: "Good"
        case .levelFair: "Fair"
        case .levelModerate: "Moderate"
        case .levelPoor: "Poor"
        case .levelVeryPoor: "Very poor"
        case .levelExtreme: "Extreme"
        case .precipRain: "Rain"
        case .precipMixed: "Mixed"
        case .precipSnow: "Snow"
        case .radar1: "Light"
        case .radar2: "Moderate"
        case .radar3: "Heavy"
        case .radar4: "Very heavy"
        case .radar5: "Extreme"
        case .twilightAstronomical: "Astronomical twilight"
        case .twilightNautical: "Nautical twilight"
        case .twilightCivil: "Civil twilight"
        case .twilightGolden: "Golden hour"
        case .twilightDay: "Daylight"
        case .twilightSun: "Sunrise and sunset"
        case .twilightNoon: "Solar noon"
        case .sunDisc: "The sun in the sky"
        case .accent: "Plan tab buttons"
        default: rawValue
        }
    }

    /// For a sky stop, the sky's name in words ("clear day"); `nil` for other roles.
    var skyName: String? {
        guard group == .sky else { return nil }
        let sky = rawValue.dropFirst("sky.".count).prefix { $0 != "." }
        return sky.replacingOccurrences(of: "_", with: " ")
    }

    /// The roles in the editor's `group`, in the order they are declared.
    static func roles(in group: ColorRoleGroup) -> [ColorRole] {
        allCases.filter { $0.group == group }
    }
}
