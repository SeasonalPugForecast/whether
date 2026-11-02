import XCTest

@testable import WhetherKit

/// Color themes: every preset complete, Whether equal to the colors
/// hard-coded before themes, contrast and separation thresholds, color vision ramps,
/// saved themes, and the color math behind them.
final class ColorThemeTests: XCTestCase {
    /// ΔE_OK that adjacent steps (and categories) must keep, as each reader sees them.
    private let minimumDelta = 0.06

    /// The presets designed for the color themes (Whether is today's look and predates the rules;
    /// color vision is what fixes it for readers who need it).
    private var designed: [ColorTheme] { ColorPresets.all.filter { $0.id != "whether" } }

    private func isHighContrast(_ t: ColorTheme) -> Bool { t.id == "high_contrast" }

    /// Marks that must stand off their card (WCAG 1.4.11): severity, temperature bands
    /// and lines, precipitation bars.
    private let marks: [ColorRole] =
        ColorRole.levels + ColorRole.temperatures + [.tempRangeLow, .tempRangeHigh, .tempChartCool, .tempChartWarm]
        + ColorRole.precipitation

    /// Every sky's card in the designed presets.
    private var designedCards: [(String, RGBA)] {
        designed.flatMap { t in Self.skies.map { ("\(t.id) \($0)", t.skyCard($0)) } }
    }

    private static let skies = [
        "clear_day", "clear_night", "cloudy_day", "cloudy_night", "rain", "snow", "storm", "fog", "twilight",
    ]

    // MARK: Complete

    func testEveryPresetDefinesEveryRole() {
        XCTAssertEqual(ColorPresets.all.count, 11, "seven presets and four seasons")
        for theme in ColorPresets.all {
            XCTAssertEqual(theme.missing, [], theme.id)
            XCTAssertEqual(theme.colors.count, ColorRole.allCases.count, theme.id)
        }
        for id in ColorPresets.ids {
            XCTAssertNotNil(ColorPresets.preset(id, month: 1), id)
        }
        XCTAssertEqual(Set(ColorPresets.all.map(\.id)).count, ColorPresets.all.count, "ids are unique")
        XCTAssertEqual(ColorPresets.preset(ColorPresets.defaultID, month: 1)?.name, "Whether")
    }

    func testColorVisionReplacesExactlyTheInformationalRoles() {
        let informational = Set(ColorRole.allCases.filter(\.isInformational))
        XCTAssertEqual(informational.count, 49)
        XCTAssertTrue(ColorPresets.ramps(for: .typical).isEmpty)
        for vision in ColorVision.allCases where vision != .typical {
            XCTAssertEqual(Set(ColorPresets.ramps(for: vision).keys), informational, vision.rawValue)
        }
        let aurora = ColorPresets.aurora
        XCTAssertEqual(aurora.applying(.typical), aurora)
        let seen = aurora.applying(.redGreen)
        for role in ColorRole.allCases {
            if role.isInformational {
                XCTAssertEqual(seen[role], ColorPresets.redGreen[role], role.rawValue)
            } else {
                XCTAssertEqual(seen[role], aurora[role], "\(role.rawValue) stays the preset's")
            }
        }
        XCTAssertFalse(ColorRole.skyClearDayTop.isInformational)
        XCTAssertFalse(ColorRole.rain.isInformational)
        XCTAssertFalse(ColorRole.sunDisc.isInformational)
        XCTAssertFalse(ColorRole.graphTemperature.isInformational)
        XCTAssertTrue(ColorRole.graphFog.isInformational)
        XCTAssertTrue(ColorRole.twilightNoon.isInformational)
    }

    // MARK: Whether is today's colors

    /// The colors as they were hard-coded before themes, copied from the sources (Sky.swift,
    /// Theme.swift, HourlyPage, DetailsSection, PrecipitationSection, RadarSection,
    /// SkyBodies, DailySection, PlanTab, HistorySection) and the desktop GUI's
    /// `theme.slint` for the temperature bands.
    func testWhetherIsTodaysColors() {
        func rgb(_ r: Double, _ g: Double, _ b: Double) -> String { RGBA(red: r, green: g, blue: b).hex }
        func hex(_ v: UInt32) -> String { RGBA(rgb: v).hex }
        /// `Palette.gray(light:dark:)`: a gray, or translucent white in the dark.
        func gray(_ light: Double, _ dark: Double) -> (String, String?) {
            (rgb(light, light, light), RGBA(red: 1, green: 1, blue: 1, alpha: dark * 0.8).hex)
        }
        let rain = (rgb(0.25, 0.50, 0.85), Optional(rgb(0.62, 0.84, 1.0)))
        var expected: [ColorRole: (String, String?)] = [
            .conditionClear: gray(0.92, 0.22), .conditionPartlyCloudy: gray(0.82, 0.32),
            .conditionCloudy: gray(0.68, 0.45), .conditionFog: gray(0.76, 0.38),
            .conditionDrizzle: (rgb(0.64, 0.78, 0.94), nil), .conditionRain: (rgb(0.37, 0.59, 0.88), nil),
            .conditionHeavyRain: (rgb(0.18, 0.39, 0.75), nil), .conditionSnow: (rgb(0.80, 0.87, 0.98), nil),
            .conditionStorm: (rgb(0.47, 0.36, 0.76), nil), .conditionOther: gray(0.85, 0.30),
            .graphClearDay: (rgb(1.0, 0.80, 0.30), nil), .graphClearNight: (rgb(0.55, 0.62, 0.95), nil),
            .graphPartlyCloudyDay: (rgb(0.93, 0.85, 0.62), nil),
            .graphPartlyCloudyNight: (rgb(0.55, 0.60, 0.75), nil),
            .graphCloudy: (rgb(0.60, 0.65, 0.72), nil), .graphFog: (rgb(0.74, 0.71, 0.82), nil),
            .graphTemperature: (rgb(1.0, 0.62, 0.30), nil), .graphActual: (rgb(1.0, 0.85, 0.55), nil),
            .graphWind: (rgb(0.40, 0.88, 0.80), nil), .graphHumidity: (rgb(0.55, 0.85, 0.45), nil),
            .graphUV: (rgb(0.82, 0.58, 1.0), nil), .graphPressure: (rgb(0.78, 0.80, 0.88), nil),
            .iconSun: (rgb(1.0, 0.76, 0.10), nil), .iconMoon: (rgb(0.55, 0.60, 0.75), nil),
            .iconCloud: (rgb(0.6, 0.6, 0.6), nil), .iconRain: rain, .iconSnow: (rgb(0.55, 0.75, 0.95), nil),
            .iconBolt: (rgb(1.0, 0.80, 0.0), nil), .rain: rain,
            .tempFreezing: (hex(0x64B5F6), nil), .tempCold: (hex(0x64B5F6), nil),
            .tempCool: (hex(0x81D4FA), nil), .tempMild: (hex(0xAED581), nil),
            .tempWarm: (hex(0xFFB74D), nil), .tempHot: (hex(0xE57373), nil),
            .tempRangeLow: (rgb(0.45, 0.65, 0.95), nil), .tempChartCool: (rgb(0.45, 0.68, 1.0), nil),
            .tempChartWarm: (rgb(1.0, 0.62, 0.30), nil), .tempBelowNormal: (rgb(0.20, 0.45, 0.80), nil),
            .tempAboveNormal: (rgb(0.85, 0.35, 0.30), nil),
            .levelFair: (rgb(0.75, 0.79, 0.20), nil),
            .precipRain: rain, .precipMixed: (rgb(0.62, 0.75, 0.95), nil),
            .precipSnow: (rgb(0.62, 0.75, 0.95), nil),
            .radar1: (rgb(0.0, 0.78, 0.0), nil), .radar2: (rgb(1.0, 0.85, 0.0), nil),
            .radar3: (rgb(1.0, 0.55, 0.0), nil), .radar4: (rgb(0.9, 0.0, 0.0), nil),
            .radar5: (rgb(0.75, 0.0, 0.75), nil),
            .twilightAstronomical: (rgb(0.30, 0.27, 0.62), nil), .twilightNautical: (rgb(0.25, 0.45, 0.85), nil),
            .twilightCivil: (rgb(0.98, 0.55, 0.40), nil), .twilightGolden: (rgb(1.0, 0.78, 0.30), nil),
            .twilightDay: (rgb(1.0, 0.92, 0.55), nil), .twilightNoon: (rgb(0.93, 0.70, 0.08), nil),
            .sunDisc: (rgb(1.0, 0.80, 0.28), nil), .accent: (rgb(0.25, 0.50, 0.95), nil),
        ]
        let skies: [String: (UInt32, UInt32)] = [
            "clear_day": (0x2F6FD0, 0x6FB3F2), "clear_night": (0x070E24, 0x1C2C55),
            "cloudy_day": (0x56677A, 0x93A3B3), "cloudy_night": (0x161C26, 0x343E4D),
            "rain": (0x2E3B4C, 0x56677B), "snow": (0x6A7F95, 0xA3B4C6), "storm": (0x1C1B30, 0x47466A),
            "fog": (0x707883, 0xA2A8B0), "twilight": (0x2E3A66, 0xE0875A),
        ]
        for (sky, (top, bottom)) in skies {
            let roles = ColorRole.sky(sky)
            expected[roles.top] = (hex(top), nil)
            expected[roles.bottom] = (hex(bottom), nil)
        }
        // SwiftUI's `.green`, `.yellow`, `.orange`, `.red`, `.purple`: still painted by iOS.
        let system: [ColorRole: SystemColor] = [
            .levelGood: .green, .levelModerate: .yellow, .levelPoor: .orange, .levelVeryPoor: .red,
            .levelExtreme: .purple, .tempRangeHigh: .orange, .twilightSun: .orange,
        ]
        XCTAssertEqual(expected.count + system.count, ColorRole.allCases.count, "every role checked")
        let whether = ColorPresets.whether
        for (role, (light, dark)) in expected {
            XCTAssertEqual(whether[role].light.hex, light, role.rawValue)
            XCTAssertEqual(whether[role].dark?.hex, dark, role.rawValue)
            XCTAssertNil(whether[role].system, role.rawValue)
        }
        for (role, color) in system {
            XCTAssertEqual(whether[role].system, color, role.rawValue)
        }
        XCTAssertEqual(whether[.levelGood].light.hex, "#34C759", "iOS 27's system green, for the math")
        XCTAssertFalse(whether.patterns)
    }

    /// Timeline labels stay as they were: white on rain, heavy rain and storms only.
    func testWhetherTimelineLabelsStayWhiteOnlyOnTheWetBlues() {
        for role in ColorRole.conditions {
            let white = ColorTheme.whiteText(on: ColorPresets.whether[role].light)
            XCTAssertEqual(
                white, [.conditionRain, .conditionHeavyRain, .conditionStorm].contains(role), role.rawValue)
            XCTAssertFalse(ColorTheme.whiteText(on: ColorPresets.whether[role].resolved(dark: true).withAlpha(0.5)))
        }
    }

    /// Timeline labels read on the color each stretch has in that appearance:
    /// No color's storm is #4A4A4A in light mode but #C4C4C4 on the sky, so its label
    /// can't be white there.
    func testTimelineLabelsReadInEachAppearance() {
        let noColor = ColorPresets.resolve(id: "whether", vision: .noColor, month: 10)
        XCTAssertEqual(noColor.timelineInk(.conditionStorm, dark: false), .white)
        XCTAssertEqual(noColor.timelineInk(.conditionStorm, dark: true), .black)
        XCTAssertEqual(noColor.timelineInk(.conditionHeavyRain, dark: true), .black)
        // Whether's translucent grays keep the primary color, which adapts.
        XCTAssertEqual(ColorPresets.whether.timelineInk(.conditionCloudy, dark: true), .primary)
        XCTAssertEqual(ColorPresets.whether.timelineInk(.conditionRain, dark: true), .white)
        for preset in ColorPresets.all {
            for vision in ColorVision.allCases {
                let theme = preset.applying(vision)
                for role in ColorRole.conditions {
                    for dark in [false, true] {
                        let fill = theme[role].resolved(dark: dark)
                        guard fill.alpha >= 1 else { continue }
                        // Black or the primary color (black in light mode) at 70%.
                        let ink: RGBA =
                            switch theme.timelineInk(role, dark: dark) {
                            case .white: .white
                            case .black, .primary: ColorMath.mix(fill, .black, 0.7)
                            }
                        XCTAssertNotEqual(
                            theme.timelineInk(role, dark: dark) == .primary && dark, true,
                            "the primary color is white on an opaque fill in the dark")
                        XCTAssertGreaterThanOrEqual(
                            ColorMath.contrast(ink, fill), 3, "\(preset.id) \(vision) \(role.rawValue) dark: \(dark)")
                    }
                }
            }
        }
    }

    /// The card color matches `Sky.card` (the middle of the gradient, 82%).
    func testSkyCardIsTheDeeperMiddleOfTheGradient() {
        let card = ColorPresets.whether.skyCard("clear_day")
        XCTAssertEqual(card.red, (0x2F + 0x6F) / 2.0 / 255 * 0.82, accuracy: 1e-12)
        XCTAssertEqual(card.green, (0x6F + 0xB3) / 2.0 / 255 * 0.82, accuracy: 1e-12)
        XCTAssertEqual(card.blue, (0xD0 + 0xF2) / 2.0 / 255 * 0.82, accuracy: 1e-12)
        XCTAssertEqual(ColorPresets.whether.skyCard("nowhere"), card, "unknown skies are clear day")
    }

    // MARK: Contrast

    func testSkiesKeepWhiteTextReadable() {
        for theme in designed {
            let minimum = isHighContrast(theme) ? 7.0 : 4.5
            for sky in Self.skies {
                let (top, bottom) = ColorRole.sky(sky)
                for role in [top, bottom] {
                    let ratio = ColorMath.contrast(.white, theme[role].light)
                    XCTAssertGreaterThanOrEqual(ratio, minimum, "\(theme.id) \(role.rawValue)")
                }
            }
        }
    }

    func testMarksStandOffTheirCards() {
        for theme in designed {
            let minimum = isHighContrast(theme) ? 4.5 : 3.0
            let cards = Self.skies.map { ($0, theme.skyCard($0)) } + [("plain dark", ColorPresets.darkCard)]
            checkMarks(theme.colors, cards: cards, minimum: minimum, label: theme.id)
            checkPills(theme.colors, label: theme.id)
        }
    }

    func testColorVisionMarksStandOffEveryDesignedCard() {
        let cards = designedCards + [("plain dark", ColorPresets.darkCard)]
        for vision in ColorVision.allCases where vision != .typical {
            let ramps = ColorPresets.ramps(for: vision)
            checkMarks(ramps, cards: cards, minimum: 3, label: vision.rawValue)
            checkPills(ramps, label: vision.rawValue)
        }
    }

    /// Light variants against the plain light card; dark variants against `cards`.
    private func checkMarks(
        _ colors: [ColorRole: ThemeColor], cards: [(String, RGBA)], minimum: Double, label: String
    ) {
        for role in marks {
            guard let color = colors[role] else { return XCTFail("\(label) lacks \(role.rawValue)") }
            let light = ColorMath.contrast(color.light, ColorPresets.lightCard)
            XCTAssertGreaterThanOrEqual(light, minimum, "\(label) \(role.rawValue) on the light card")
            for (name, card) in cards {
                let dark = ColorMath.contrast(color.resolved(dark: true), card)
                XCTAssertGreaterThanOrEqual(dark, minimum, "\(label) \(role.rawValue) on \(name)")
            }
        }
    }

    /// The departure pills carry their number: black on the light pill, white on the dark.
    private func checkPills(_ colors: [ColorRole: ThemeColor], label: String) {
        for role in [ColorRole.tempBelowNormal, .tempAboveNormal] {
            guard let color = colors[role] else { return XCTFail("\(label) lacks \(role.rawValue)") }
            XCTAssertGreaterThanOrEqual(ColorMath.contrast(.black, color.light), 4.5, "\(label) \(role.rawValue)")
            XCTAssertGreaterThanOrEqual(
                ColorMath.contrast(.white, color.resolved(dark: true)), 4.5, "\(label) \(role.rawValue) dark")
        }
    }

    // MARK: Ordered and apart

    func testOrderedRampsAreMonotonicInLightness() {
        var sets: [(String, [ColorRole: ThemeColor])] = designed.map { ($0.id, $0.colors) }
        sets += ColorVision.allCases.filter { $0 != .typical }.map { ($0.rawValue, ColorPresets.ramps(for: $0)) }
        for (label, colors) in sets {
            for ramp in [ColorRole.levels, ColorRole.radar] {
                for dark in [false, true] {
                    let l = ramp.compactMap { colors[$0]?.resolved(dark: dark) }.map { ColorMath.oklab($0).l }
                    XCTAssertEqual(l.count, ramp.count)
                    let steps = zip(l, l.dropFirst()).map { $1 - $0 }
                    XCTAssertTrue(
                        steps.allSatisfy { $0 > 0 } || steps.allSatisfy { $0 < 0 },
                        "\(label) \(ramp[0].rawValue) dark=\(dark): \(l)")
                }
            }
        }
    }

    func testPresetStepsStayApart() {
        for theme in designed {
            let deficiency: Deficiency? = theme.id == "graphite" ? .achromat : nil
            checkApart(theme.colors, as: deficiency, conditions: false, label: theme.id)
        }
    }

    func testColorVisionRampsStayApartForTheirReaders() {
        for vision in ColorVision.allCases where vision != .typical {
            XCTAssertFalse(vision.deficiencies.isEmpty)
            for deficiency in vision.deficiencies {
                checkApart(
                    ColorPresets.ramps(for: vision), as: deficiency, conditions: true,
                    label: "\(vision.rawValue) as \(deficiency.rawValue)")
            }
        }
        XCTAssertEqual(ColorVision.typical.deficiencies, [])
    }

    /// Adjacent steps of severity, radar, temperature and the twilight bar, and every pair
    /// of precipitation types (and of condition groups), at least `minimumDelta` apart.
    private func checkApart(
        _ colors: [ColorRole: ThemeColor], as deficiency: Deficiency?, conditions: Bool, label: String
    ) {
        func check(_ a: ColorRole, _ b: ColorRole, dark: Bool) {
            guard let x = colors[a]?.resolved(dark: dark), let y = colors[b]?.resolved(dark: dark) else {
                return XCTFail("\(label) lacks \(a.rawValue) or \(b.rawValue)")
            }
            let delta = ColorMath.deltaE(x, y, as: deficiency)
            XCTAssertGreaterThanOrEqual(
                delta, minimumDelta, "\(label) dark=\(dark): \(a.rawValue) ~ \(b.rawValue)")
        }
        for dark in [false, true] {
            for ramp in [ColorRole.levels, ColorRole.radar, ColorRole.temperatures, ColorRole.twilightBar] {
                for (a, b) in zip(ramp, ramp.dropFirst()) { check(a, b, dark: dark) }
            }
            var categories = [ColorRole.precipitation]
            if conditions { categories.append(ColorRole.conditions) }
            for set in categories {
                for i in set.indices { for j in set.indices where j > i { check(set[i], set[j], dark: dark) } }
            }
        }
    }

    func testGraphiteIsGrayWithPatterns() {
        let graphite = ColorPresets.graphite
        XCTAssertTrue(graphite.patterns)
        for role in ColorRole.allCases {
            for dark in [false, true] {
                XCTAssertLessThan(ColorMath.oklab(graphite[role].resolved(dark: dark)).chroma, 0.02, role.rawValue)
            }
        }
        XCTAssertEqual(ColorPresets.all.filter(\.patterns).map(\.id), ["graphite"])
    }

    // MARK: Saved themes

    /// A custom theme as an earlier version saved it: no `base` or `patterns`, a role
    /// that no longer exists, a color that doesn't parse and one as a bare string.
    func testOldSavesDecodeAndComplete() throws {
        let json = """
            {"id": "custom-1", "name": "My Aurora",
             "colors": {"accent": {"light": "#123456", "dark": "#ABCDEF"},
                        "level.poor": "#FF8800",
                        "level.extreme": {"light": "#XYZXYZ"},
                        "sky.aurora_borealis.top": {"light": "#00FF00"},
                        "condition.clear": {"light": "#EEEEEE", "dark": "#FFFFFF33", "system": "chartreuse"}}}
            """
        let theme = try JSONDecoder().decode(ColorTheme.self, from: Data(json.utf8))
        XCTAssertEqual(theme.id, "custom-1")
        XCTAssertEqual(theme.name, "My Aurora")
        XCTAssertEqual(theme.base, "whether", "no base: the default preset")
        XCTAssertFalse(theme.patterns)
        XCTAssertEqual(Set(theme.colors.keys), [.accent, .levelPoor, .conditionClear], "unknown and bad dropped")
        XCTAssertEqual(theme[.accent].light.hex, "#123456")
        XCTAssertEqual(theme[.accent].dark?.hex, "#ABCDEF")
        XCTAssertEqual(theme[.levelPoor], ThemeColor(light: RGBA(rgb: 0xFF8800)))
        XCTAssertEqual(theme[.conditionClear].dark?.hex, "#FFFFFF33")
        XCTAssertNil(theme[.conditionClear].system, "an unknown system color is ignored")

        let full = ColorPresets.completed(theme, month: 4)
        XCTAssertEqual(full.missing, [])
        XCTAssertEqual(full[.accent].light.hex, "#123456", "its own colors win")
        XCTAssertEqual(full[.levelExtreme], ColorPresets.whether[.levelExtreme], "the rest from the base")
        XCTAssertEqual(full.id, "custom-1")
    }

    func testThemesRoundTrip() throws {
        for theme in ColorPresets.all + [ColorPresets.aurora.applying(.noColor)] {
            let data = try JSONEncoder().encode(theme)
            XCTAssertEqual(try JSONDecoder().decode(ColorTheme.self, from: data), theme, theme.id)
        }
        let custom = ColorTheme(
            id: "c", name: "Mine", base: "seasons", colors: [.levelPoor: ThemeColor(light: RGBA(rgb: 0x112233))])
        let text = String(decoding: try JSONEncoder().encode(custom), as: UTF8.self)
        XCTAssertFalse(text.contains("patterns"), "false is left out")
        XCTAssertEqual(ColorPresets.completed(custom, month: 7)[.skyClearDayTop], ColorPresets.seasonsSummer[.skyClearDayTop])
        let unknownBase = ColorTheme(id: "d", name: "D", base: "gone", colors: [:])
        XCTAssertEqual(ColorPresets.completed(unknownBase, month: 1).colors, ColorPresets.whether.colors)
    }

    func testSeasonsFollowTheMonth() {
        typealias S = ColorPresets.Season
        XCTAssertEqual((1...12).map { S.of(month: $0) }, [
            .winter, .winter, .spring, .spring, .spring, .summer, .summer, .summer, .fall, .fall, .fall, .winter,
        ])
        XCTAssertEqual(S.of(month: 1, southern: true), .summer)
        XCTAssertEqual(S.of(month: 7, southern: true), .winter)
        XCTAssertEqual(S.of(month: 10, southern: true), .spring)
        XCTAssertEqual(ColorPresets.preset("seasons", month: 7)?.id, "seasons.summer")
        XCTAssertEqual(ColorPresets.preset("seasons", month: 7)?.name, "Seasons")
        XCTAssertEqual(ColorPresets.preset("seasons.fall", month: 1)?.id, "seasons.fall")
        XCTAssertNil(ColorPresets.preset("nope", month: 1))
        XCTAssertNotEqual(ColorPresets.seasonsSpring[.skyClearDayBottom], ColorPresets.seasonsFall[.skyClearDayBottom])
    }

    // MARK: Role helpers

    /// What the app and the widget paint with.
    func testResolvePicksThePresetOrCustomThemeWithItsVision() {
        XCTAssertEqual(Preferences().colorTheme(), ColorPresets.whether, "nothing changes by default")
        XCTAssertEqual(ColorPresets.resolve(id: "whether", month: 7), ColorPresets.whether)
        XCTAssertEqual(ColorPresets.resolve(id: "aurora", month: 7), ColorPresets.aurora)
        XCTAssertEqual(ColorPresets.resolve(id: "nowhere", month: 7), ColorPresets.whether, "unknown ids are Whether")
        XCTAssertEqual(ColorPresets.resolve(id: "seasons", month: 7).id, "seasons.summer")
        XCTAssertEqual(ColorPresets.resolve(id: "seasons", month: 7, southern: true).id, "seasons.winter")
        XCTAssertEqual(
            ColorPresets.resolve(id: "neon", vision: .redGreen, month: 1), ColorPresets.neon.applying(.redGreen))
        let mine = ColorTheme(
            id: "mine", name: "Mine", base: "almanac", colors: [.accent: ThemeColor(light: RGBA(rgb: 0x123456))])
        let resolved = ColorPresets.resolve(id: "mine", custom: [mine], month: 1)
        XCTAssertEqual(resolved.missing, [], "completed from its base")
        XCTAssertEqual(resolved[.accent].light, RGBA(rgb: 0x123456))
        XCTAssertEqual(resolved[.skyRainTop], ColorPresets.almanac[.skyRainTop])
        XCTAssertEqual(
            ColorPresets.resolve(id: "mine", custom: [mine], vision: .noColor, month: 1)[.levelPoor],
            ColorPresets.noColor[.levelPoor])
    }

    func testRoleHelpersMapTheCoresNames() {
        XCTAssertEqual(ColorRole.sky("snow").top, .skySnowTop)
        XCTAssertEqual(ColorRole.sky("twilight").bottom, .skyTwilightBottom)
        XCTAssertEqual(ColorRole.sky("mystery").top, .skyClearDayTop)
        XCTAssertEqual(ColorRole.condition("heavy_rain"), .conditionHeavyRain)
        XCTAssertEqual(ColorRole.condition("partly_cloudy"), .conditionPartlyCloudy)
        XCTAssertEqual(ColorRole.condition("volcano"), .conditionOther)
        XCTAssertEqual(ColorRole.level("very_poor"), .levelVeryPoor)
        XCTAssertNil(ColorRole.level(nil))
        XCTAssertNil(ColorRole.level("dire"))
        XCTAssertEqual(ColorRole.levelPoor.group, .severity)
        XCTAssertEqual(ColorRole.rain.group, .precipitation)
        XCTAssertEqual(ColorRole.iconBolt.group, .weather)
        XCTAssertEqual(ColorRole.sunDisc.group, .sunAndMoon)
        XCTAssertEqual(ColorRole.accent.group, .accent)
        XCTAssertEqual(ColorRole.radar3.rawValue, "radar.3")
        XCTAssertEqual(Set(ColorRole.allCases.map(\.group)), Set(ColorRoleGroup.allCases))
    }

    // MARK: Color math

    func testHexParsesAndPrints() {
        XCTAssertEqual(RGBA(hex: "#2F6FD0"), RGBA(rgb: 0x2F6FD0))
        XCTAssertEqual(RGBA(hex: "2f6fd0")?.hex, "#2F6FD0")
        XCTAssertEqual(RGBA(hex: "#FFFFFF2D")?.alpha ?? 0, 45.0 / 255, accuracy: 1e-12)
        XCTAssertEqual(RGBA(hex: "#FFFFFF2D")?.hex, "#FFFFFF2D")
        XCTAssertNil(RGBA(hex: "#FFF"))
        XCTAssertNil(RGBA(hex: "#GGGGGG"))
        XCTAssertNil(RGBA(hex: ""))
        XCTAssertEqual(RGBA(red: 1.4, green: -1, blue: .nan).clamped, RGBA(red: 1, green: 0, blue: 0))
        XCTAssertEqual(RGBA(red: 0.5, green: 0.5, blue: 0.5).hex, "#808080")
    }

    func testContrastFollowsWCAG() {
        XCTAssertEqual(ColorMath.contrast(.white, .black), 21, accuracy: 1e-9)
        XCTAssertEqual(ColorMath.contrast(.black, .white), 21, accuracy: 1e-9)
        XCTAssertEqual(ColorMath.contrast(.white, .white), 1, accuracy: 1e-12)
        // #767676 is the classic 4.5:1 gray on white.
        XCTAssertEqual(ColorMath.contrast(RGBA(rgb: 0x767676), .white), 4.54, accuracy: 0.01)
        let half = ColorMath.over(RGBA(red: 1, green: 1, blue: 1, alpha: 0.5), .black)
        XCTAssertEqual(half.red, 0.5, accuracy: 1e-12)
        XCTAssertEqual(half.alpha, 1)
        XCTAssertEqual(ColorMath.mix(.black, .white, 0.25).green, 0.25, accuracy: 1e-12)
    }

    func testOKLabMatchesTheReference() {
        let white = ColorMath.oklab(.white)
        XCTAssertEqual(white.l, 1, accuracy: 1e-4)
        XCTAssertEqual(white.chroma, 0, accuracy: 1e-4)
        XCTAssertEqual(ColorMath.oklab(.black).l, 0, accuracy: 1e-9)
        // Ottosson's published value for sRGB red: L 0.62796, a 0.22486, b 0.12585.
        let red = ColorMath.oklab(RGBA(red: 1, green: 0, blue: 0))
        XCTAssertEqual(red.l, 0.62796, accuracy: 1e-4)
        XCTAssertEqual(red.a, 0.22486, accuracy: 1e-4)
        XCTAssertEqual(red.b, 0.12585, accuracy: 1e-4)
        for hex: UInt32 in [0x2F6FD0, 0xE0875A, 0x34C759, 0x070E24] {
            let c = RGBA(rgb: hex)
            XCTAssertEqual(ColorMath.rgb(ColorMath.oklab(c)).hex, c.hex, "round trip")
        }
    }

    func testSimulationKeepsGraysAndMergesTheConfusions() {
        let gray = RGBA(rgb: 0x808080)
        for d in Deficiency.allCases {
            XCTAssertEqual(ColorMath.deltaE(gray, ColorMath.simulate(gray, d)), 0, accuracy: 0.003, d.rawValue)
            XCTAssertEqual(ColorMath.simulate(.white, d).hex, "#FFFFFF", d.rawValue)
        }
        let red = RGBA(rgb: 0xD62728), green = RGBA(rgb: 0x2CA02C)
        XCTAssertGreaterThan(ColorMath.deltaE(red, green), 0.25)
        // Red and green of similar lightness: far apart in color, close for deutan readers.
        let olive = RGBA(rgb: 0x9E7B00), leaf = RGBA(rgb: 0x5E8C00)
        XCTAssertLessThan(ColorMath.deltaE(olive, leaf, as: .deutan), ColorMath.deltaE(olive, leaf) / 2)
        XCTAssertLessThan(ColorMath.deltaE(olive, leaf, as: .protan), ColorMath.deltaE(olive, leaf) / 2)
        // Blue and green: apart for deutan, close for tritan readers.
        let blue = RGBA(rgb: 0x3C8DBC), teal = RGBA(rgb: 0x3CA08C)
        XCTAssertLessThan(ColorMath.deltaE(blue, teal, as: .tritan), ColorMath.deltaE(blue, teal, as: .deutan))
        let mono = ColorMath.simulate(RGBA(rgb: 0xFF0000), .achromat)
        XCTAssertEqual(mono.red, mono.green, accuracy: 1e-12)
        XCTAssertEqual(ColorMath.luminance(mono), ColorMath.luminance(RGBA(rgb: 0xFF0000)), accuracy: 1e-9)
    }
}

extension RGBA {
    fileprivate func withAlpha(_ a: Double) -> RGBA { RGBA(red: red, green: green, blue: blue, alpha: a) }
}
