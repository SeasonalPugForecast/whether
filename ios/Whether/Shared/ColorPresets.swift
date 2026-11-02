import Foundation

/// The color presets and the color vision ramps. Presets are data: each is
/// a full `ColorTheme`, every role defined, light and dark where they differ.
/// `ColorThemeTests` checks them: every role, white text on every sky, marks against
/// their cards, ordered ramps, and the ramps under simulated color vision.
nonisolated enum ColorPresets {
    /// The preset everyone starts with.
    static let defaultID = "whether"
    /// The preset offered first when iOS's Increase Contrast is on.
    static let highContrastID = "high_contrast"

    /// The gallery's presets, in order. `seasons` follows the month.
    static let ids = [
        "whether", "aurora", "almanac", "sorbet", "neon", "seasons", "graphite", "high_contrast",
    ]

    /// A card on the plain light background (glass on white) and on the plain dark one,
    /// for contrast checks.
    static let lightCard = RGBA(rgb: 0xF2F2F7)
    static let darkCard = RGBA(rgb: 0x1C1C1E)

    /// The Seasons preset's four looks: meteorological seasons, as the pug icon.
    enum Season: String, CaseIterable, Sendable {
        case spring, summer, fall, winter

        /// The season of `month` (1…12), north of the equator unless `southern`.
        static func of(month: Int, southern: Bool = false) -> Season {
            let m = ((month - 1 + (southern ? 6 : 0)) % 12 + 12) % 12 + 1
            return switch m {
            case 3...5: .spring
            case 6...8: .summer
            case 9...11: .fall
            default: .winter
            }
        }
    }

    /// The preset with `id` (`seasons` is the month's look), or `nil`.
    static func preset(_ id: String, month: Int, southern: Bool = false) -> ColorTheme? {
        if id == "seasons" { return seasons[Season.of(month: month, southern: southern)] }
        return all.first { $0.id == id }
    }

    /// The gallery (Settings → Display → Colors): every preset in `ids` order, Seasons
    /// as `month`'s look. A preset's `base` is the id the settings save.
    static func gallery(month: Int, southern: Bool = false) -> [ColorTheme] {
        ids.compactMap { preset($0, month: month, southern: southern) }
    }

    /// `theme` with every role it lacks taken from its base preset (Whether when the
    /// base is unknown): how a saved custom theme is loaded.
    static func completed(_ theme: ColorTheme, month: Int, southern: Bool = false) -> ColorTheme {
        theme.completed(from: preset(theme.base, month: month, southern: southern) ?? whether)
    }

    /// The theme the app and the widget paint with: the preset or custom theme `id`
    /// (Whether when it is neither), completed from its base, with `vision`'s ramps.
    static func resolve(
        id: String, custom: [ColorTheme] = [], vision: ColorVision = .typical, month: Int,
        southern: Bool = false
    ) -> ColorTheme {
        let theme =
            preset(id, month: month, southern: southern)
            ?? custom.first(where: { $0.id == id }).map { completed($0, month: month, southern: southern) }
            ?? whether
        return theme.applying(vision)
    }

    /// Every concrete preset: the gallery's, with each season's look.
    static let all: [ColorTheme] =
        [whether, aurora, almanac, sorbet, neon] + Season.allCases.compactMap { seasons[$0] }
        + [graphite, highContrast]

    /// Seasons, by season.
    static let seasons: [Season: ColorTheme] = [
        .spring: seasonsSpring, .summer: seasonsSummer, .fall: seasonsFall, .winter: seasonsWinter,
    ]

    /// The informational colors for `vision` (empty for typical vision).
    static func ramps(for vision: ColorVision) -> [ColorRole: ThemeColor] {
        switch vision {
        case .typical: [:]
        case .redGreen: redGreen
        case .blueYellow: blueYellow
        case .noColor: noColor
        }
    }

    private static func c(_ light: UInt32, _ dark: UInt32? = nil) -> ThemeColor {
        ThemeColor(light: RGBA(rgb: light), dark: dark.map { RGBA(rgb: $0) })
    }

    /// Opaque on light backgrounds, a translucent `0xRRGGBBAA` on dark ones.
    private static func translucent(_ light: UInt32, dark: UInt32) -> ThemeColor {
        ThemeColor(light: RGBA(rgb: light), dark: RGBA(rgba: dark))
    }

    /// A UIKit system color, with the values iOS 27 resolves it to.
    private static func system(_ color: SystemColor, _ light: UInt32, _ dark: UInt32) -> ThemeColor {
        ThemeColor(light: RGBA(rgb: light), dark: RGBA(rgb: dark), system: color)
    }

    // MARK: - Presets

    /// Today's colors, exactly: Dark Sky's quiet grays and blues, NEXRAD's radar, iOS's
    /// system colors for severity. `temp.*` bands are the desktop GUI's (unused on the phone before).
    static let whether = ColorTheme(
        id: "whether", name: "Whether",
        colors: [
            // Skies
            .skyClearDayTop: c(0x2F6FD0), .skyClearDayBottom: c(0x6FB3F2), .skyClearNightTop: c(0x070E24),
            .skyClearNightBottom: c(0x1C2C55), .skyCloudyDayTop: c(0x56677A),
            .skyCloudyDayBottom: c(0x93A3B3), .skyCloudyNightTop: c(0x161C26),
            .skyCloudyNightBottom: c(0x343E4D), .skyRainTop: c(0x2E3B4C), .skyRainBottom: c(0x56677B),
            .skySnowTop: c(0x6A7F95), .skySnowBottom: c(0xA3B4C6), .skyStormTop: c(0x1C1B30),
            .skyStormBottom: c(0x47466A), .skyFogTop: c(0x707883), .skyFogBottom: c(0xA2A8B0),
            .skyTwilightTop: c(0x2E3A66), .skyTwilightBottom: c(0xE0875A),
            // Weather
            .conditionClear: translucent(0xEBEBEB, dark: 0xFFFFFF2D),
            .conditionPartlyCloudy: translucent(0xD1D1D1, dark: 0xFFFFFF41),
            .conditionCloudy: translucent(0xADADAD, dark: 0xFFFFFF5C),
            .conditionFog: translucent(0xC2C2C2, dark: 0xFFFFFF4E), .conditionDrizzle: c(0xA3C7F0),
            .conditionRain: c(0x5E96E0), .conditionHeavyRain: c(0x2E63BF), .conditionSnow: c(0xCCDEFA),
            .conditionStorm: c(0x785CC2), .conditionOther: translucent(0xD9D9D9, dark: 0xFFFFFF3D),
            .graphClearDay: c(0xFFCC4D), .graphClearNight: c(0x8C9EF2), .graphPartlyCloudyDay: c(0xEDD99E),
            .graphPartlyCloudyNight: c(0x8C99BF), .graphCloudy: c(0x99A6B8), .graphFog: c(0xBDB5D1),
            .graphTemperature: c(0xFF9E4D), .graphActual: c(0xFFD98C), .graphWind: c(0x66E0CC),
            .graphHumidity: c(0x8CD973), .graphUV: c(0xD194FF), .graphPressure: c(0xC7CCE0),
            .iconSun: c(0xFFC21A), .iconMoon: c(0x8C99BF), .iconCloud: c(0x999999),
            .iconRain: c(0x4080D9, 0x9ED6FF), .iconSnow: c(0x8CBFF2), .iconBolt: c(0xFFCC00),
            .rain: c(0x4080D9, 0x9ED6FF),
            // Temperature
            .tempFreezing: c(0x64B5F6), .tempCold: c(0x64B5F6), .tempCool: c(0x81D4FA),
            .tempMild: c(0xAED581), .tempWarm: c(0xFFB74D), .tempHot: c(0xE57373),
            .tempRangeLow: c(0x73A6F2), .tempRangeHigh: system(.orange, 0xFF8D28, 0xFF9230),
            .tempChartCool: c(0x73ADFF), .tempChartWarm: c(0xFF9E4D), .tempBelowNormal: c(0x3373CC),
            .tempAboveNormal: c(0xD9594D),
            // Severity
            .levelGood: system(.green, 0x34C759, 0x30D158), .levelFair: c(0xBFC933),
            .levelModerate: system(.yellow, 0xFFCC00, 0xFFD600),
            .levelPoor: system(.orange, 0xFF8D28, 0xFF9230),
            .levelVeryPoor: system(.red, 0xFF383C, 0xFF4245),
            .levelExtreme: system(.purple, 0xCB30E0, 0xDB34F2),
            // Precipitation
            .precipRain: c(0x4080D9, 0x9ED6FF), .precipMixed: c(0x9EBFF2), .precipSnow: c(0x9EBFF2),
            // Radar
            .radar1: c(0x00C700), .radar2: c(0xFFD900), .radar3: c(0xFF8C00), .radar4: c(0xE60000),
            .radar5: c(0xBF00BF),
            // Sun and moon
            .twilightAstronomical: c(0x4D459E), .twilightNautical: c(0x4073D9), .twilightCivil: c(0xFA8C66),
            .twilightGolden: c(0xFFC74D), .twilightDay: c(0xFFEB8C),
            .twilightSun: system(.orange, 0xFF8D28, 0xFF9230), .twilightNoon: c(0xEDB314),
            .sunDisc: c(0xFFCC47),
            // Accent
            .accent: c(0x4080F2),
        ])

    /// Deep teal days and violet nights with an aurora glow on the horizon; green-to-magenta accents.
    static let aurora = ColorTheme(
        id: "aurora", name: "Aurora",
        colors: [
            // Skies
            .skyClearDayTop: c(0x025062), .skyClearDayBottom: c(0x117878), .skyClearNightTop: c(0x120E2D),
            .skyClearNightBottom: c(0x024636), .skyCloudyDayTop: c(0x324C55),
            .skyCloudyDayBottom: c(0x547275), .skyCloudyNightTop: c(0x111826),
            .skyCloudyNightBottom: c(0x233845), .skyRainTop: c(0x133240), .skyRainBottom: c(0x2F5A63),
            .skySnowTop: c(0x3A4C60), .skySnowBottom: c(0x51707C), .skyStormTop: c(0x1B0C30),
            .skyStormBottom: c(0x52295E), .skyFogTop: c(0x3B4F50), .skyFogBottom: c(0x5B6F6C),
            .skyTwilightTop: c(0x1B1244), .skyTwilightBottom: c(0x9A3B71),
            // Weather
            .conditionClear: translucent(0xE2EEEE, dark: 0xF0F7F72E),
            .conditionPartlyCloudy: translucent(0xC8D3D4, dark: 0xF0F7F742),
            .conditionCloudy: translucent(0xA3ADAE, dark: 0xF0F7F75C),
            .conditionFog: translucent(0xB8C3C4, dark: 0xF0F7F74F), .conditionDrizzle: c(0x90D5DD),
            .conditionRain: c(0x36A3C8), .conditionHeavyRain: c(0x2863AB), .conditionSnow: c(0xC3EAEC),
            .conditionStorm: c(0x8E49AA), .conditionOther: translucent(0xD2DDDE, dark: 0xF0F7F73D),
            .graphClearDay: c(0xF5D068), .graphClearNight: c(0x9BA1E6), .graphPartlyCloudyDay: c(0xE4DCAF),
            .graphPartlyCloudyNight: c(0x8DA4C1), .graphCloudy: c(0x91A9B3), .graphFog: c(0xB7B0C8),
            .graphTemperature: c(0xFFA15C), .graphActual: c(0xFAD596), .graphWind: c(0x6BE7C9),
            .graphHumidity: c(0x8ADF8D), .graphUV: c(0xD592F2), .graphPressure: c(0xBFCBDF),
            .iconSun: c(0xFDCA3A), .iconMoon: c(0x9FA8D1), .iconCloud: c(0x8F9B9F),
            .iconRain: c(0x00729B, 0x7ED7EC), .iconSnow: c(0x8FD0E1), .iconBolt: c(0xECE040),
            .rain: c(0x00729B, 0x7ED7EC),
            // Temperature
            .tempFreezing: c(0x5A40A9, 0xB0A6ED), .tempCold: c(0x1762B6, 0x91C1FF),
            .tempCool: c(0x067F94, 0x7DE2FA), .tempMild: c(0x3A8F42, 0xABF3AD),
            .tempWarm: c(0x996103, 0xFFBB69), .tempHot: c(0xAC2722, 0xFE9184),
            .tempRangeLow: c(0x1762B6, 0x91C1FF), .tempRangeHigh: c(0x996103, 0xFFBB69),
            .tempChartCool: c(0x1762B6, 0x91C1FF), .tempChartWarm: c(0x996103, 0xFFBB69),
            .tempBelowNormal: c(0x6197CD, 0x164781), .tempAboveNormal: c(0xDD7577, 0x902731),
            // Severity
            .levelGood: c(0x2E9E52, 0xB4FFC3), .levelFair: c(0x6C8505, 0xCAEA73),
            .levelModerate: c(0x816C06, 0xEFCC36), .levelPoor: c(0x8F5000, 0xFEAC5F),
            .levelVeryPoor: c(0xA40C0F, 0xFE8E81), .levelExtreme: c(0x83006D, 0xE379C7),
            // Precipitation
            .precipRain: c(0x0568A4, 0x78BFF9), .precipMixed: c(0x6C4494, 0xD8BCFA),
            .precipSnow: c(0x3F8C9E, 0xD2F2FA),
            // Radar
            .radar1: c(0x71F3CA), .radar2: c(0x9BD25B), .radar3: c(0xCE9C09), .radar4: c(0xE54533),
            .radar5: c(0xB61492),
            // Sun and moon
            .twilightAstronomical: c(0x3E2F72), .twilightNautical: c(0x1666AA), .twilightCivil: c(0xDB6EA5),
            .twilightGolden: c(0xFEB354), .twilightDay: c(0xF7EDAF), .twilightSun: c(0xFF885D),
            .twilightNoon: c(0xEDB417), .sunDisc: c(0xF8CA65),
            // Accent
            .accent: c(0x059583),
        ])

    /// Paper and ink: Prussian-blue and sepia skies, warm creams and muted print colors;
    /// at home on the plain light background.
    static let almanac = ColorTheme(
        id: "almanac", name: "Almanac",
        colors: [
            // Skies
            .skyClearDayTop: c(0x2E4C65), .skyClearDayBottom: c(0x517182), .skyClearNightTop: c(0x131822),
            .skyClearNightBottom: c(0x2C3742), .skyCloudyDayTop: c(0x50473E),
            .skyCloudyDayBottom: c(0x736A5E), .skyCloudyNightTop: c(0x221B16),
            .skyCloudyNightBottom: c(0x3F362F), .skyRainTop: c(0x243940), .skyRainBottom: c(0x435D62),
            .skySnowTop: c(0x424F56), .skySnowBottom: c(0x5F6B6F), .skyStormTop: c(0x2A1926),
            .skyStormBottom: c(0x503A4E), .skyFogTop: c(0x4F4B43), .skyFogBottom: c(0x6B6961),
            .skyTwilightTop: c(0x292237), .skyTwilightBottom: c(0x935839),
            // Weather
            .conditionClear: translucent(0xF3EADB, dark: 0xFAF4EB2E),
            .conditionPartlyCloudy: translucent(0xD9D0C1, dark: 0xFAF4EB42),
            .conditionCloudy: translucent(0xB2AA9C, dark: 0xFAF4EB5C),
            .conditionFog: translucent(0xC8C0B1, dark: 0xFAF4EB4F), .conditionDrizzle: c(0xA9CAD8),
            .conditionRain: c(0x5F94B0), .conditionHeavyRain: c(0x386189), .conditionSnow: c(0xD0E6ED),
            .conditionStorm: c(0x7D5279), .conditionOther: translucent(0xE2D9CB, dark: 0xFAF4EB3D),
            .graphClearDay: c(0xEDC47D), .graphClearNight: c(0x95A5C5), .graphPartlyCloudyDay: c(0xE3D2B0),
            .graphPartlyCloudyNight: c(0x92A0B1), .graphCloudy: c(0xADA397), .graphFog: c(0xBFB3BE),
            .graphTemperature: c(0xE59E77), .graphActual: c(0xEDD0A0), .graphWind: c(0x92CBC3),
            .graphHumidity: c(0xA8CA96), .graphUV: c(0xC29BCB), .graphPressure: c(0xCCC3B6),
            .iconSun: c(0xEBB353), .iconMoon: c(0x9BA5B8), .iconCloud: c(0x9A9185),
            .iconRain: c(0x2B6991, 0x96C6DD), .iconSnow: c(0xA3C4D1), .iconBolt: c(0xE5C95E),
            .rain: c(0x2B6991, 0x96C6DD),
            // Temperature
            .tempFreezing: c(0x584A93, 0xB0AADC), .tempCold: c(0x35649F, 0x9CC1F1),
            .tempCool: c(0x2F7D8E, 0x9ADEEF), .tempMild: c(0x528A54, 0xBBEEBB),
            .tempWarm: c(0x946322, 0xF5BF80), .tempHot: c(0x9A4037, 0xEE9B90),
            .tempRangeLow: c(0x35649F, 0x9CC1F1), .tempRangeHigh: c(0x946322, 0xF5BF80),
            .tempChartCool: c(0x35649F, 0x9CC1F1), .tempChartWarm: c(0x946322, 0xF5BF80),
            .tempBelowNormal: c(0x6A98B9, 0x295171), .tempAboveNormal: c(0xC8856E, 0x803B2A),
            // Severity
            .levelGood: c(0x5A975C, 0xC3FBC3), .levelFair: c(0x7A7F31, 0xDCE28D),
            .levelModerate: c(0x886723, 0xF3C672), .levelPoor: c(0x904F1A, 0xF7AE7C),
            .levelVeryPoor: c(0x913327, 0xEF988A), .levelExtreme: c(0x7E2256, 0xDB86AF),
            // Precipitation
            .precipRain: c(0x21689C, 0x8DBDE7), .precipMixed: c(0x674C85, 0xD4C0ED),
            .precipSnow: c(0x568996, 0xD9F0F6),
            // Radar
            .radar1: c(0xACE0B6), .radar2: c(0xBEC073), .radar3: c(0xCA974D), .radar4: c(0xC36543),
            .radar5: c(0x9A4873),
            // Sun and moon
            .twilightAstronomical: c(0x3A385B), .twilightNautical: c(0x3F6789), .twilightCivil: c(0xC8856E),
            .twilightGolden: c(0xE7B876), .twilightDay: c(0xF5E7C3), .twilightSun: c(0xE08D5D),
            .twilightNoon: c(0xE0AE57), .sunDisc: c(0xEDC47D),
            // Accent
            .accent: c(0x8E4031, 0xDB997B),
        ])

    /// Soft pastels: periwinkle-to-orchid skies, mango dusk, mint to grape for severity.
    static let sorbet = ColorTheme(
        id: "sorbet", name: "Sorbet",
        colors: [
            // Skies
            .skyClearDayTop: c(0x3E5082), .skyClearDayBottom: c(0x7D5883), .skyClearNightTop: c(0x231B3C),
            .skyClearNightBottom: c(0x523457), .skyCloudyDayTop: c(0x514B5D),
            .skyCloudyDayBottom: c(0x72616F), .skyCloudyNightTop: c(0x26212E),
            .skyCloudyNightBottom: c(0x453847), .skyRainTop: c(0x2E445C), .skyRainBottom: c(0x3E6980),
            .skySnowTop: c(0x425163), .skySnowBottom: c(0x65657B), .skyStormTop: c(0x2E1A46),
            .skyStormBottom: c(0x6C3665), .skyFogTop: c(0x554B54), .skyFogBottom: c(0x776364),
            .skyTwilightTop: c(0x3D285C), .skyTwilightBottom: c(0xA95A41),
            // Weather
            .conditionClear: translucent(0xF1E8F0, dark: 0xF9F3F82E),
            .conditionPartlyCloudy: translucent(0xD7CED6, dark: 0xF9F3F842),
            .conditionCloudy: translucent(0xB1A8AF, dark: 0xF9F3F85C),
            .conditionFog: translucent(0xC7BEC6, dark: 0xF9F3F84F), .conditionDrizzle: c(0xB1D3EC),
            .conditionRain: c(0x73A1DC), .conditionHeavyRain: c(0x576CB7), .conditionSnow: c(0xD9EAFC),
            .conditionStorm: c(0x9E5CAD), .conditionOther: translucent(0xE1D7DF, dark: 0xF9F3F83D),
            .graphClearDay: c(0xF9D280), .graphClearNight: c(0xB0A9E1), .graphPartlyCloudyDay: c(0xF4D9BB),
            .graphPartlyCloudyNight: c(0xAFA4C6), .graphCloudy: c(0xB0AABF), .graphFog: c(0xCEB6C0),
            .graphTemperature: c(0xFFA383), .graphActual: c(0xFDD7AC), .graphWind: c(0x89E5D9),
            .graphHumidity: c(0xA3E4A4), .graphUV: c(0xD4A0EC), .graphPressure: c(0xD0CEE4),
            .iconSun: c(0xFECD5C), .iconMoon: c(0xBDAFD9), .iconCloud: c(0xAAA1AD),
            .iconRain: c(0x4671B7, 0x9FD1FC), .iconSnow: c(0xB1D2F4), .iconBolt: c(0xF5E05D),
            .rain: c(0x4671B7, 0x9FD1FC),
            // Temperature
            .tempFreezing: c(0x654297, 0xB9A5E0), .tempCold: c(0x3F5FAB, 0xA0BDFB),
            .tempCool: c(0x117F94, 0x8DE0F4), .tempMild: c(0x3E8E53, 0xADF2BA),
            .tempWarm: c(0x996000, 0xFCBD70), .tempHot: c(0xA23445, 0xF6949C),
            .tempRangeLow: c(0x3F5FAB, 0xA0BDFB), .tempRangeHigh: c(0x996000, 0xFCBD70),
            .tempChartCool: c(0x3F5FAB, 0xA0BDFB), .tempChartWarm: c(0x996000, 0xFCBD70),
            .tempBelowNormal: c(0x7D97D0, 0x3D4E89), .tempAboveNormal: c(0xDF7E86, 0x913147),
            // Severity
            .levelGood: c(0x0F9E73, 0xBAFFE0), .levelFair: c(0x648631, 0xC7F093),
            .levelModerate: c(0x7C6E02, 0xE7D462), .levelPoor: c(0x8B5204, 0xFFB467),
            .levelVeryPoor: c(0x99233A, 0xFF969F), .levelExtreme: c(0x702180, 0xD28FE2),
            // Precipitation
            .precipRain: c(0x0168A5, 0x84BEF0), .precipMixed: c(0x69498C, 0xD6BEF3),
            .precipSnow: c(0x4D8B9A, 0xD6F1F8),
            // Radar
            .radar1: c(0x8FF1D7), .radar2: c(0xB1D372), .radar3: c(0xD9A02C), .radar4: c(0xE45B63),
            .radar5: c(0xAC43A5),
            // Sun and moon
            .twilightAstronomical: c(0x503975), .twilightNautical: c(0x4F6CAF), .twilightCivil: c(0xE680A1),
            .twilightGolden: c(0xFFBB77), .twilightDay: c(0xFBEFC2), .twilightSun: c(0xF99262),
            .twilightNoon: c(0xEEBC4A), .sunDisc: c(0xFECE8C),
            // Accent
            .accent: c(0xB83B76, 0xEE83AF),
        ])

    /// Saturated colors on near-black skies, made for OLED at night.
    static let neon = ColorTheme(
        id: "neon", name: "Neon",
        colors: [
            // Skies
            .skyClearDayTop: c(0x081248), .skyClearDayBottom: c(0x003D73), .skyClearNightTop: c(0x050411),
            .skyClearNightBottom: c(0x210E39), .skyCloudyDayTop: c(0x0E1624),
            .skyCloudyDayBottom: c(0x213449), .skyCloudyNightTop: c(0x04060A),
            .skyCloudyNightBottom: c(0x141B26), .skyRainTop: c(0x000F1C), .skyRainBottom: c(0x023347),
            .skySnowTop: c(0x011922), .skySnowBottom: c(0x024550), .skyStormTop: c(0x0E001C),
            .skyStormBottom: c(0x48005F), .skyFogTop: c(0x091414), .skyFogBottom: c(0x1F3231),
            .skyTwilightTop: c(0x120027), .skyTwilightBottom: c(0x84004C),
            // Weather
            .conditionClear: translucent(0xEBEBEB, dark: 0xF5F5F52E),
            .conditionPartlyCloudy: translucent(0xD1D1D1, dark: 0xF5F5F542),
            .conditionCloudy: translucent(0xABABAB, dark: 0xF5F5F55C),
            .conditionFog: translucent(0xC1C1C1, dark: 0xF5F5F54F), .conditionDrizzle: c(0x7BE3F5),
            .conditionRain: c(0x05B2F5), .conditionHeavyRain: c(0x3265F3), .conditionSnow: c(0xC8F9FC),
            .conditionStorm: c(0xC82DE7), .conditionOther: translucent(0xDBDBDB, dark: 0xF5F5F53D),
            .graphClearDay: c(0xFFDC4F), .graphClearNight: c(0xA29DFE), .graphPartlyCloudyDay: c(0xF4E798),
            .graphPartlyCloudyNight: c(0x8CA2E4), .graphCloudy: c(0x95AFC2), .graphFog: c(0xC8B5D6),
            .graphTemperature: c(0xFF9A6B), .graphActual: c(0xFFE47E), .graphWind: c(0x16F6ED),
            .graphHumidity: c(0x7EF962), .graphUV: c(0xE573FF), .graphPressure: c(0xBFCFE9),
            .iconSun: c(0xFFDC4F), .iconMoon: c(0xACB1F0), .iconCloud: c(0x979FAB),
            .iconRain: c(0x0177BC, 0x66DCFF), .iconSnow: c(0x8CE9F4), .iconBolt: c(0xFFF12F),
            .rain: c(0x0177BC, 0x66DCFF),
            // Temperature
            .tempFreezing: c(0x5E2CC4, 0xB0A2FE), .tempCold: c(0x0061BE, 0x91C1FF),
            .tempCool: c(0x037F95, 0x70E4FE), .tempMild: c(0x059428, 0x95FA99),
            .tempWarm: c(0x996000, 0xFFBC6A), .tempHot: c(0xB7020C, 0xFF9183),
            .tempRangeLow: c(0x0061BE, 0x91C1FF), .tempRangeHigh: c(0x996000, 0xFFBC6A),
            .tempChartCool: c(0x0061BE, 0x91C1FF), .tempChartWarm: c(0x996000, 0xFFBC6A),
            .tempBelowNormal: c(0x408DEA, 0x2046B4), .tempAboveNormal: c(0xF34E6A, 0xA60035),
            // Severity
            .levelGood: c(0x04A140, 0xB5FFBF), .levelFair: c(0x6F8405, 0xCBED28),
            .levelModerate: c(0x7E6D03, 0xECCD0D), .levelPoor: c(0x924D00, 0xFFAB69),
            .levelVeryPoor: c(0xA40122, 0xFF8D8A), .levelExtreme: c(0x7E017A, 0xEC68E3),
            // Precipitation
            .precipRain: c(0x0468A5, 0x72BFFE), .precipMixed: c(0x7336A7, 0xD9BBFE),
            .precipSnow: c(0x0090A8, 0xCAF4FE),
            // Radar
            .radar1: c(0x5FFFC4), .radar2: c(0x99E51C), .radar3: c(0xEBA002), .radar4: c(0xFF352F),
            .radar5: c(0xC801C2),
            // Sun and moon
            .twilightAstronomical: c(0x4E1B95), .twilightNautical: c(0x2564E0), .twilightCivil: c(0xFB40AD),
            .twilightGolden: c(0xFFBB69), .twilightDay: c(0xFAF392), .twilightSun: c(0xFF8F5A),
            .twilightNoon: c(0xFDCA0E), .sunDisc: c(0xFFD16B),
            // Accent
            .accent: c(0xC11A8E, 0xFE5EC3),
        ])

    /// Seasons, spring: blue skies over fresh green, lilac evenings, blossom-pink dusk.
    static let seasonsSpring = ColorTheme(
        id: "seasons.spring", name: "Seasons", base: "seasons",
        colors: [
            // Skies
            .skyClearDayTop: c(0x0A547A), .skyClearDayBottom: c(0x2F7464), .skyClearNightTop: c(0x0F162F),
            .skyClearNightBottom: c(0x003F4B), .skyCloudyDayTop: c(0x3F4F46),
            .skyCloudyDayBottom: c(0x5D6E60), .skyCloudyNightTop: c(0x0F1D1E),
            .skyCloudyNightBottom: c(0x293A33), .skyRainTop: c(0x1C3A44), .skyRainBottom: c(0x34615E),
            .skySnowTop: c(0x3B4F59), .skySnowBottom: c(0x546D71), .skyStormTop: c(0x1D1737),
            .skyStormBottom: c(0x4F3562), .skyFogTop: c(0x434E45), .skyFogBottom: c(0x626C60),
            .skyTwilightTop: c(0x2F1D4A), .skyTwilightBottom: c(0xA45676),
            // Weather
            .conditionClear: translucent(0xE5EEE6, dark: 0xF1F7F22E),
            .conditionPartlyCloudy: translucent(0xCBD4CC, dark: 0xF1F7F242),
            .conditionCloudy: translucent(0xA5ADA6, dark: 0xF1F7F25C),
            .conditionFog: translucent(0xBBC4BC, dark: 0xF1F7F24F), .conditionDrizzle: c(0x9ED0E5),
            .conditionRain: c(0x4E9ED2), .conditionHeavyRain: c(0x2F62AC), .conditionSnow: c(0xCEEAF8),
            .conditionStorm: c(0x8050B0), .conditionOther: translucent(0xD5DDD6, dark: 0xF1F7F23D),
            .graphClearDay: c(0xEFDD71), .graphClearNight: c(0x9BA1E6), .graphPartlyCloudyDay: c(0xD8E5B8),
            .graphPartlyCloudyNight: c(0x8DA4C1), .graphCloudy: c(0x91A9B3), .graphFog: c(0xB7B0C8),
            .graphTemperature: c(0xFFA15C), .graphActual: c(0xFAD596), .graphWind: c(0x6BE7C9),
            .graphHumidity: c(0x8ADF8D), .graphUV: c(0xD592F2), .graphPressure: c(0xBFCBDF),
            .iconSun: c(0xFDCA3A), .iconMoon: c(0x9FA8D1), .iconCloud: c(0x929B93),
            .iconRain: c(0x00729B, 0x7ED7EC), .iconSnow: c(0x8FD0E1), .iconBolt: c(0xECE040),
            .rain: c(0x00729B, 0x7ED7EC),
            // Temperature
            .tempFreezing: c(0x5A40A9, 0xB0A6ED), .tempCold: c(0x1762B6, 0x91C1FF),
            .tempCool: c(0x067F94, 0x7DE2FA), .tempMild: c(0x3A8F42, 0xABF3AD),
            .tempWarm: c(0x996103, 0xFFBB69), .tempHot: c(0xAC2722, 0xFE9184),
            .tempRangeLow: c(0x1762B6, 0x91C1FF), .tempRangeHigh: c(0x996103, 0xFFBB69),
            .tempChartCool: c(0x1762B6, 0x91C1FF), .tempChartWarm: c(0x996103, 0xFFBB69),
            .tempBelowNormal: c(0x6197CD, 0x1D4D87), .tempAboveNormal: c(0xDD7767, 0x90291E),
            // Severity
            .levelGood: c(0x2E9E52, 0xC1FFCD), .levelFair: c(0x6C8505, 0xCFEF78),
            .levelModerate: c(0x816C06, 0xF4D13D), .levelPoor: c(0x8F5000, 0xFEB36F),
            .levelVeryPoor: c(0xA40C0F, 0xFE998C), .levelExtreme: c(0x83006D, 0xEC81CF),
            // Precipitation
            .precipRain: c(0x0568A4, 0x78BFF9), .precipMixed: c(0x6C4494, 0xD8BCFA),
            .precipSnow: c(0x3F8C9E, 0xD2F2FA),
            // Radar
            .radar1: c(0x94EFB5), .radar2: c(0xB5CF55), .radar3: c(0xDB9901), .radar4: c(0xE54F36),
            .radar5: c(0xBB208A),
            // Sun and moon
            .twilightAstronomical: c(0x39336F), .twilightNautical: c(0x2266A4), .twilightCivil: c(0xE37F65),
            .twilightGolden: c(0xF9BA5F), .twilightDay: c(0xFCEBAE), .twilightSun: c(0xFC8C45),
            .twilightNoon: c(0xF0B21B), .sunDisc: c(0xF0D777),
            // Accent
            .accent: c(0x277539, 0x73C881),
        ])

    /// Seasons, summer: deep azure, gold sun, warm orange dusk.
    static let seasonsSummer = ColorTheme(
        id: "seasons.summer", name: "Seasons", base: "seasons",
        colors: [
            // Skies
            .skyClearDayTop: c(0x004B96), .skyClearDayBottom: c(0x067396), .skyClearNightTop: c(0x071027),
            .skyClearNightBottom: c(0x13335A), .skyCloudyDayTop: c(0x3A4D5C),
            .skyCloudyDayBottom: c(0x586F7D), .skyCloudyNightTop: c(0x101924),
            .skyCloudyNightBottom: c(0x283744), .skyRainTop: c(0x133144), .skyRainBottom: c(0x29596B),
            .skySnowTop: c(0x394D5C), .skySnowBottom: c(0x536E78), .skyStormTop: c(0x161136),
            .skyStormBottom: c(0x48316C), .skyFogTop: c(0x4F4A3E), .skyFogBottom: c(0x716A5A),
            .skyTwilightTop: c(0x171A4C), .skyTwilightBottom: c(0xAD5C15),
            // Weather
            .conditionClear: translucent(0xEFEBE1, dark: 0xF8F5EF2E),
            .conditionPartlyCloudy: translucent(0xD5D0C7, dark: 0xF8F5EF42),
            .conditionCloudy: translucent(0xAFAAA1, dark: 0xF8F5EF5C),
            .conditionFog: translucent(0xC5C0B7, dark: 0xF8F5EF4F), .conditionDrizzle: c(0x9ED0E5),
            .conditionRain: c(0x4E9ED2), .conditionHeavyRain: c(0x2F62AC), .conditionSnow: c(0xCEEAF8),
            .conditionStorm: c(0x8050B0), .conditionOther: translucent(0xDFDAD1, dark: 0xF8F5EF3D),
            .graphClearDay: c(0xFFD16B), .graphClearNight: c(0x9BA1E6), .graphPartlyCloudyDay: c(0xF7DBA1),
            .graphPartlyCloudyNight: c(0x8DA4C1), .graphCloudy: c(0x91A9B3), .graphFog: c(0xB7B0C8),
            .graphTemperature: c(0xFFA15C), .graphActual: c(0xFAD596), .graphWind: c(0x6BE7C9),
            .graphHumidity: c(0x8ADF8D), .graphUV: c(0xD592F2), .graphPressure: c(0xBFCBDF),
            .iconSun: c(0xFDCA3A), .iconMoon: c(0x9FA8D1), .iconCloud: c(0x9C988E),
            .iconRain: c(0x00729B, 0x7ED7EC), .iconSnow: c(0x8FD0E1), .iconBolt: c(0xECE040),
            .rain: c(0x00729B, 0x7ED7EC),
            // Temperature
            .tempFreezing: c(0x5A40A9, 0xB0A6ED), .tempCold: c(0x1762B6, 0x91C1FF),
            .tempCool: c(0x067F94, 0x7DE2FA), .tempMild: c(0x3A8F42, 0xABF3AD),
            .tempWarm: c(0x996103, 0xFFBB69), .tempHot: c(0xAC2722, 0xFE9184),
            .tempRangeLow: c(0x1762B6, 0x91C1FF), .tempRangeHigh: c(0x996103, 0xFFBB69),
            .tempChartCool: c(0x1762B6, 0x91C1FF), .tempChartWarm: c(0x996103, 0xFFBB69),
            .tempBelowNormal: c(0x6197CD, 0x1D4D87), .tempAboveNormal: c(0xDD7767, 0x90291E),
            // Severity
            .levelGood: c(0x2E9E52, 0xC1FFCD), .levelFair: c(0x6C8505, 0xCFEF78),
            .levelModerate: c(0x816C06, 0xF4D13D), .levelPoor: c(0x8F5000, 0xFEB36F),
            .levelVeryPoor: c(0xA40C0F, 0xFE998C), .levelExtreme: c(0x83006D, 0xEC81CF),
            // Precipitation
            .precipRain: c(0x0568A4, 0x78BFF9), .precipMixed: c(0x6C4494, 0xD8BCFA),
            .precipSnow: c(0x3F8C9E, 0xD2F2FA),
            // Radar
            .radar1: c(0x94EFB5), .radar2: c(0xB5CF55), .radar3: c(0xDB9901), .radar4: c(0xE54F36),
            .radar5: c(0xBB208A),
            // Sun and moon
            .twilightAstronomical: c(0x39336F), .twilightNautical: c(0x2266A4), .twilightCivil: c(0xE37F65),
            .twilightGolden: c(0xF9BA5F), .twilightDay: c(0xFCEBAE), .twilightSun: c(0xFC8C45),
            .twilightNoon: c(0xF0B21B), .sunDisc: c(0xFFD16B),
            // Accent
            .accent: c(0x965802, 0xF1BA4B),
        ])

    /// Seasons, fall: amber haze at the horizon, rust and plum, warm grays.
    static let seasonsFall = ColorTheme(
        id: "seasons.fall", name: "Seasons", base: "seasons",
        colors: [
            // Skies
            .skyClearDayTop: c(0x204B70), .skyClearDayBottom: c(0x91633D), .skyClearNightTop: c(0x131428),
            .skyClearNightBottom: c(0x41293F), .skyCloudyDayTop: c(0x55473D),
            .skyCloudyDayBottom: c(0x7A685C), .skyCloudyNightTop: c(0x231814),
            .skyCloudyNightBottom: c(0x41312C), .skyRainTop: c(0x1D343F), .skyRainBottom: c(0x645142),
            .skySnowTop: c(0x3E4C57), .skySnowBottom: c(0x5E6A77), .skyStormTop: c(0x290E28),
            .skyStormBottom: c(0x5E2C46), .skyFogTop: c(0x534A40), .skyFogBottom: c(0x75695F),
            .skyTwilightTop: c(0x34143B), .skyTwilightBottom: c(0xA54A24),
            // Weather
            .conditionClear: translucent(0xF5E9DF, dark: 0xFBF4EE2E),
            .conditionPartlyCloudy: translucent(0xDACFC6, dark: 0xFBF4EE42),
            .conditionCloudy: translucent(0xB4A8A0, dark: 0xFBF4EE5C),
            .conditionFog: translucent(0xCABEB6, dark: 0xFBF4EE4F), .conditionDrizzle: c(0x9ED0E5),
            .conditionRain: c(0x4E9ED2), .conditionHeavyRain: c(0x2F62AC), .conditionSnow: c(0xCEEAF8),
            .conditionStorm: c(0x8050B0), .conditionOther: translucent(0xE4D8CF, dark: 0xFBF4EE3D),
            .graphClearDay: c(0xFFBB69), .graphClearNight: c(0x9BA1E6), .graphPartlyCloudyDay: c(0xF5CBA4),
            .graphPartlyCloudyNight: c(0x8DA4C1), .graphCloudy: c(0x91A9B3), .graphFog: c(0xB7B0C8),
            .graphTemperature: c(0xFFA15C), .graphActual: c(0xFAD596), .graphWind: c(0x6BE7C9),
            .graphHumidity: c(0x8ADF8D), .graphUV: c(0xD592F2), .graphPressure: c(0xBFCBDF),
            .iconSun: c(0xFDCA3A), .iconMoon: c(0x9FA8D1), .iconCloud: c(0xA0968F),
            .iconRain: c(0x00729B, 0x7ED7EC), .iconSnow: c(0x8FD0E1), .iconBolt: c(0xECE040),
            .rain: c(0x00729B, 0x7ED7EC),
            // Temperature
            .tempFreezing: c(0x5A40A9, 0xB0A6ED), .tempCold: c(0x1762B6, 0x91C1FF),
            .tempCool: c(0x067F94, 0x7DE2FA), .tempMild: c(0x3A8F42, 0xABF3AD),
            .tempWarm: c(0x996103, 0xFFBB69), .tempHot: c(0xAC2722, 0xFE9184),
            .tempRangeLow: c(0x1762B6, 0x91C1FF), .tempRangeHigh: c(0x996103, 0xFFBB69),
            .tempChartCool: c(0x1762B6, 0x91C1FF), .tempChartWarm: c(0x996103, 0xFFBB69),
            .tempBelowNormal: c(0x6197CD, 0x1D4D87), .tempAboveNormal: c(0xDD7767, 0x90291E),
            // Severity
            .levelGood: c(0x2E9E52, 0xC1FFCD), .levelFair: c(0x6C8505, 0xCFEF78),
            .levelModerate: c(0x816C06, 0xF4D13D), .levelPoor: c(0x8F5000, 0xFEB36F),
            .levelVeryPoor: c(0xA40C0F, 0xFE998C), .levelExtreme: c(0x83006D, 0xEC81CF),
            // Precipitation
            .precipRain: c(0x0568A4, 0x78BFF9), .precipMixed: c(0x6C4494, 0xD8BCFA),
            .precipSnow: c(0x3F8C9E, 0xD2F2FA),
            // Radar
            .radar1: c(0x94EFB5), .radar2: c(0xB5CF55), .radar3: c(0xDB9901), .radar4: c(0xE54F36),
            .radar5: c(0xBB208A),
            // Sun and moon
            .twilightAstronomical: c(0x39336F), .twilightNautical: c(0x2266A4), .twilightCivil: c(0xE37F65),
            .twilightGolden: c(0xF9BA5F), .twilightDay: c(0xFCEBAE), .twilightSun: c(0xFC8C45),
            .twilightNoon: c(0xF0B21B), .sunDisc: c(0xFFBB69),
            // Accent
            .accent: c(0x9D4616, 0xF0995B),
        ])

    /// Seasons, winter: icy blues and slate, frosty pink twilight.
    static let seasonsWinter = ColorTheme(
        id: "seasons.winter", name: "Seasons", base: "seasons",
        colors: [
            // Skies
            .skyClearDayTop: c(0x224A71), .skyClearDayBottom: c(0x4D7483), .skyClearNightTop: c(0x070E21),
            .skyClearNightBottom: c(0x12334C), .skyCloudyDayTop: c(0x414C54),
            .skyCloudyDayBottom: c(0x606E75), .skyCloudyNightTop: c(0x11161D),
            .skyCloudyNightBottom: c(0x2A343D), .skyRainTop: c(0x1D303E), .skyRainBottom: c(0x3B5765),
            .skySnowTop: c(0x3E505C), .skySnowBottom: c(0x5A6A74), .skyStormTop: c(0x14122B),
            .skyStormBottom: c(0x3D325B), .skyFogTop: c(0x454B50), .skyFogBottom: c(0x656B6F),
            .skyTwilightTop: c(0x1E2343), .skyTwilightBottom: c(0x83587F),
            // Weather
            .conditionClear: translucent(0xE3EDF3, dark: 0xF0F6FA2E),
            .conditionPartlyCloudy: translucent(0xC9D3D9, dark: 0xF0F6FA42),
            .conditionCloudy: translucent(0xA3ACB2, dark: 0xF0F6FA5C),
            .conditionFog: translucent(0xB9C3C9, dark: 0xF0F6FA4F), .conditionDrizzle: c(0x9ED0E5),
            .conditionRain: c(0x4E9ED2), .conditionHeavyRain: c(0x2F62AC), .conditionSnow: c(0xCEEAF8),
            .conditionStorm: c(0x8050B0), .conditionOther: translucent(0xD2DCE3, dark: 0xF0F6FA3D),
            .graphClearDay: c(0xF3DE90), .graphClearNight: c(0x9BA1E6), .graphPartlyCloudyDay: c(0xEAE6C8),
            .graphPartlyCloudyNight: c(0x8DA4C1), .graphCloudy: c(0x91A9B3), .graphFog: c(0xB7B0C8),
            .graphTemperature: c(0xFFA15C), .graphActual: c(0xFAD596), .graphWind: c(0x6BE7C9),
            .graphHumidity: c(0x8ADF8D), .graphUV: c(0xD592F2), .graphPressure: c(0xBFCBDF),
            .iconSun: c(0xFDCA3A), .iconMoon: c(0x9FA8D1), .iconCloud: c(0x909AA0),
            .iconRain: c(0x00729B, 0x7ED7EC), .iconSnow: c(0x8FD0E1), .iconBolt: c(0xECE040),
            .rain: c(0x00729B, 0x7ED7EC),
            // Temperature
            .tempFreezing: c(0x5A40A9, 0xB0A6ED), .tempCold: c(0x1762B6, 0x91C1FF),
            .tempCool: c(0x067F94, 0x7DE2FA), .tempMild: c(0x3A8F42, 0xABF3AD),
            .tempWarm: c(0x996103, 0xFFBB69), .tempHot: c(0xAC2722, 0xFE9184),
            .tempRangeLow: c(0x1762B6, 0x91C1FF), .tempRangeHigh: c(0x996103, 0xFFBB69),
            .tempChartCool: c(0x1762B6, 0x91C1FF), .tempChartWarm: c(0x996103, 0xFFBB69),
            .tempBelowNormal: c(0x6197CD, 0x1D4D87), .tempAboveNormal: c(0xDD7767, 0x90291E),
            // Severity
            .levelGood: c(0x2E9E52, 0xC1FFCD), .levelFair: c(0x6C8505, 0xCFEF78),
            .levelModerate: c(0x816C06, 0xF4D13D), .levelPoor: c(0x8F5000, 0xFEB36F),
            .levelVeryPoor: c(0xA40C0F, 0xFE998C), .levelExtreme: c(0x83006D, 0xEC81CF),
            // Precipitation
            .precipRain: c(0x0568A4, 0x78BFF9), .precipMixed: c(0x6C4494, 0xD8BCFA),
            .precipSnow: c(0x3F8C9E, 0xD2F2FA),
            // Radar
            .radar1: c(0x94EFB5), .radar2: c(0xB5CF55), .radar3: c(0xDB9901), .radar4: c(0xE54F36),
            .radar5: c(0xBB208A),
            // Sun and moon
            .twilightAstronomical: c(0x39336F), .twilightNautical: c(0x2266A4), .twilightCivil: c(0xE37F65),
            .twilightGolden: c(0xF9BA5F), .twilightDay: c(0xFCEBAE), .twilightSun: c(0xFC8C45),
            .twilightNoon: c(0xF0B21B), .sunDisc: c(0xF3DCA1),
            // Accent
            .accent: c(0x0F68A2, 0x82C7F0),
        ])

    /// Grayscale, with patterns and symbols always on: the look for achromatopsia.
    static let graphite = ColorTheme(
        id: "graphite", name: "Graphite",
        colors: [
            // Skies
            .skyClearDayTop: c(0x3D3D3D), .skyClearDayBottom: c(0x636363), .skyClearNightTop: c(0x0D0D0D),
            .skyClearNightBottom: c(0x2B2B2B), .skyCloudyDayTop: c(0x424242),
            .skyCloudyDayBottom: c(0x606060), .skyCloudyNightTop: c(0x161616),
            .skyCloudyNightBottom: c(0x303030), .skyRainTop: c(0x292929), .skyRainBottom: c(0x4D4D4D),
            .skySnowTop: c(0x484848), .skySnowBottom: c(0x666666), .skyStormTop: c(0x0B0B0B),
            .skyStormBottom: c(0x333333), .skyFogTop: c(0x484848), .skyFogBottom: c(0x636363),
            .skyTwilightTop: c(0x1B1B1B), .skyTwilightBottom: c(0x5B5B5B),
            // Weather
            .conditionClear: translucent(0xEBEBEB, dark: 0xF5F5F52E),
            .conditionPartlyCloudy: translucent(0xD1D1D1, dark: 0xF5F5F542),
            .conditionCloudy: translucent(0xABABAB, dark: 0xF5F5F55C),
            .conditionFog: translucent(0xC1C1C1, dark: 0xF5F5F54F), .conditionDrizzle: c(0xCACACA),
            .conditionRain: c(0x929292), .conditionHeavyRain: c(0x5D5D5D), .conditionSnow: c(0xEEEEEE),
            .conditionStorm: c(0x424242), .conditionOther: translucent(0xDBDBDB, dark: 0xF5F5F53D),
            .graphClearDay: c(0xE4E4E4), .graphClearNight: c(0xABABAB), .graphPartlyCloudyDay: c(0xD1D1D1),
            .graphPartlyCloudyNight: c(0x9E9E9E), .graphCloudy: c(0x929292), .graphFog: c(0xB7B7B7),
            .graphTemperature: c(0xEBEBEB), .graphActual: c(0xCACACA), .graphWind: c(0xBEBEBE),
            .graphHumidity: c(0xB1B1B1), .graphUV: c(0xD7D7D7), .graphPressure: c(0xA4A4A4),
            .iconSun: c(0xEBEBEB), .iconMoon: c(0xBEBEBE), .iconCloud: c(0x8C8C8C),
            .iconRain: c(0x555555, 0xD1D1D1), .iconSnow: c(0xDEDEDE), .iconBolt: c(0xF8F8F8),
            .rain: c(0x555555, 0xD1D1D1),
            // Temperature
            .tempFreezing: c(0x313131, 0x959595), .tempCold: c(0x424242, 0xA9A9A9),
            .tempCool: c(0x535353, 0xBDBDBD), .tempMild: c(0x656565, 0xD1D1D1),
            .tempWarm: c(0x787878, 0xE6E6E6), .tempHot: c(0x8A8A8A, 0xFCFCFC),
            .tempRangeLow: c(0x424242, 0xA9A9A9), .tempRangeHigh: c(0x787878, 0xE6E6E6),
            .tempChartCool: c(0x424242, 0xA9A9A9), .tempChartWarm: c(0x787878, 0xE6E6E6),
            .tempBelowNormal: c(0x9E9E9E, 0x4D4D4D), .tempAboveNormal: c(0xBEBEBE, 0x333333),
            // Severity
            .levelGood: c(0x8A8A8A, 0xFCFCFC), .levelFair: c(0x787878, 0xE6E6E6),
            .levelModerate: c(0x656565, 0xD1D1D1), .levelPoor: c(0x535353, 0xBDBDBD),
            .levelVeryPoor: c(0x424242, 0xA9A9A9), .levelExtreme: c(0x313131, 0x959595),
            // Precipitation
            .precipRain: c(0x3D3D3D, 0xA4A4A4), .precipMixed: c(0x585858, 0xCACACA),
            .precipSnow: c(0x7A7A7A, 0xF5F5F5),
            // Radar
            .radar1: c(0xDEDEDE), .radar2: c(0xB7B7B7), .radar3: c(0x929292), .radar4: c(0x6F6F6F),
            .radar5: c(0x484848),
            // Sun and moon
            .twilightAstronomical: c(0x383838), .twilightNautical: c(0x5D5D5D), .twilightCivil: c(0x8C8C8C),
            .twilightGolden: c(0xBEBEBE), .twilightDay: c(0xEEEEEE), .twilightSun: c(0xCACACA),
            .twilightNoon: c(0xDEDEDE), .sunDisc: c(0xE4E4E4),
            // Accent
            .accent: c(0x484848, 0xD7D7D7),
        ], patterns: true)

    /// AAA text on every sky (7:1) and informational colors at 4.5:1 on their cards.
    static let highContrast = ColorTheme(
        id: "high_contrast", name: "High contrast",
        colors: [
            // Skies
            .skyClearDayTop: c(0x0F2A5F), .skyClearDayBottom: c(0x054980), .skyClearNightTop: c(0x030511),
            .skyClearNightBottom: c(0x131E37), .skyCloudyDayTop: c(0x222F3C),
            .skyCloudyDayBottom: c(0x3A4A57), .skyCloudyNightTop: c(0x06090F),
            .skyCloudyNightBottom: c(0x1B222B), .skyRainTop: c(0x091C2C), .skyRainBottom: c(0x1B3C4E),
            .skySnowTop: c(0x253541), .skySnowBottom: c(0x3B4D59), .skyStormTop: c(0x100420),
            .skyStormBottom: c(0x361949), .skyFogTop: c(0x2A3137), .skyFogBottom: c(0x434C51),
            .skyTwilightTop: c(0x130631), .skyTwilightBottom: c(0x7B2813),
            // Weather
            .conditionClear: translucent(0xEBEBEB, dark: 0xF5F5F52E),
            .conditionPartlyCloudy: translucent(0xD1D1D1, dark: 0xF5F5F542),
            .conditionCloudy: translucent(0xABABAB, dark: 0xF5F5F55C),
            .conditionFog: translucent(0xC1C1C1, dark: 0xF5F5F54F), .conditionDrizzle: c(0xA3E3F8),
            .conditionRain: c(0x52B5F4), .conditionHeavyRain: c(0x326AD9), .conditionSnow: c(0xEBF7FE),
            .conditionStorm: c(0xA245D9), .conditionOther: translucent(0xDBDBDB, dark: 0xF5F5F53D),
            .graphClearDay: c(0xFFE47E), .graphClearNight: c(0xB8BEFE), .graphPartlyCloudyDay: c(0xF7EDAF),
            .graphPartlyCloudyNight: c(0xABBEE5), .graphCloudy: c(0xB9C6D0), .graphFog: c(0xD7CCE0),
            .graphTemperature: c(0xFFB98A), .graphActual: c(0xFEE9BC), .graphWind: c(0x63FAE9),
            .graphHumidity: c(0x90FB95), .graphUV: c(0xE4A9FF), .graphPressure: c(0xD3DFF3),
            .iconSun: c(0xFFDB69), .iconMoon: c(0xBECAEC), .iconCloud: c(0xA4A4A4),
            .iconRain: c(0x035798, 0x9FE3FE), .iconSnow: c(0xAAEAFE), .iconBolt: c(0xFFF465),
            .rain: c(0x035798, 0x9FE3FE),
            // Temperature
            .tempFreezing: c(0x481D9A, 0xBCB2FE), .tempCold: c(0x014991, 0xA7CEFF),
            .tempCool: c(0x036070, 0x95EAFF), .tempMild: c(0x00721B, 0xB1FFB3),
            .tempWarm: c(0x734701, 0xFFCD95), .tempHot: c(0x8B0006, 0xFFABA0),
            .tempRangeLow: c(0x014991, 0xA7CEFF), .tempRangeHigh: c(0x734701, 0xFFCD95),
            .tempChartCool: c(0x014991, 0xA7CEFF), .tempChartWarm: c(0x734701, 0xFFCD95),
            .tempBelowNormal: c(0x5D94DA, 0x0E3685), .tempAboveNormal: c(0xE66F62, 0x800709),
            // Severity
            .levelGood: c(0x037835, 0xCEFFD7), .levelFair: c(0x506402, 0xD2F668),
            .levelModerate: c(0x625102, 0xFFD60F), .levelPoor: c(0x6E3C00, 0xFEBF87),
            .levelVeryPoor: c(0x810005, 0xFEA99D), .levelExtreme: c(0x670155, 0xFF85DF),
            // Precipitation
            .precipRain: c(0x004B7A, 0x8ECBFF), .precipMixed: c(0x53207D, 0xE2CCFE),
            .precipSnow: c(0x016F82, 0xE5F9FF),
            // Radar
            .radar1: c(0x94FFC8), .radar2: c(0xA7D91D), .radar3: c(0xDA950B), .radar4: c(0xEA2413),
            .radar5: c(0x9A0196),
            // Sun and moon
            .twilightAstronomical: c(0x361B73), .twilightNautical: c(0x0B58BB), .twilightCivil: c(0xF75F4C),
            .twilightGolden: c(0xFFBC64), .twilightDay: c(0xFFF4B1), .twilightSun: c(0xFF9B5A),
            .twilightNoon: c(0xFEC848), .sunDisc: c(0xFFD988),
            // Accent
            .accent: c(0x004B96, 0x86CDFF),
        ])

    // MARK: - Color vision ramps

    /// Red–green readers (protan, deutan): ordered ramps run blue → gray → yellow, after cividis
    /// (light: blue to dark olive-brown); categories use blue, yellow and orange, after Okabe–Ito.
    static let redGreen: [ColorRole: ThemeColor] = [
        // Weather
        .conditionClear: c(0xF7F7F7, 0x333333), .conditionPartlyCloudy: c(0xB4B4B4, 0x555555),
        .conditionCloudy: c(0x898989, 0x7A7A7A), .conditionFog: c(0x9E9E9E, 0x676767),
        .conditionDrizzle: c(0x8DCCF9, 0x6FAECE), .conditionRain: c(0x4B99D7, 0x2D86C8),
        .conditionHeavyRain: c(0x1D5AB0, 0x2559B0), .conditionSnow: c(0xC9E3FE, 0xA8EBFB),
        .conditionStorm: c(0xB05A02, 0xD6812E), .conditionOther: c(0xCACACA, 0x444444),
        .graphClearDay: c(0xEED059), .graphClearNight: c(0x95B0EB), .graphPartlyCloudyDay: c(0xEDE2B5),
        .graphPartlyCloudyNight: c(0x94A6BE), .graphCloudy: c(0x9E9E9E), .graphFog: c(0xBEBEBE),
        // Temperature
        .tempFreezing: c(0x313E8C, 0x8499E9), .tempCold: c(0x255B9C, 0x82B5EF),
        .tempCool: c(0x31779B, 0x97D1EB), .tempMild: c(0x878562, 0xEFE9C3), .tempWarm: c(0x93640C, 0xF8C970),
        .tempHot: c(0x8B4501, 0xF6A14F), .tempRangeLow: c(0x255B9C, 0x82B5EF),
        .tempRangeHigh: c(0x93640C, 0xF8C970), .tempChartCool: c(0x255B9C, 0x82B5EF),
        .tempChartWarm: c(0x93640C, 0xF8C970), .tempBelowNormal: c(0x6C96C2, 0x2B568B),
        .tempAboveNormal: c(0xCE9042, 0x845011),
        // Severity
        .levelGood: c(0x5788C7, 0x749DD9), .levelFair: c(0x587696, 0x97B2CE),
        .levelModerate: c(0x686147, 0xCAC2A6), .levelPoor: c(0x604D0F, 0xE6D792),
        .levelVeryPoor: c(0x523902, 0xFDEE78), .levelExtreme: c(0x422701, 0xFFFEF1),
        // Precipitation
        .precipRain: c(0x01519F, 0x65B2F1), .precipMixed: c(0x8A6419, 0xEAD070),
        .precipSnow: c(0x5D8EA2, 0xE7F9FF),
        // Radar
        .radar1: c(0xBDE5F9), .radar2: c(0x80C7F8), .radar3: c(0xBEA333), .radar4: c(0xC16F03),
        .radar5: c(0xA63D02),
        // Sun and moon
        .twilightAstronomical: c(0x293871), .twilightNautical: c(0x196EA9), .twilightCivil: c(0xCD8643),
        .twilightGolden: c(0xE8C44E), .twilightDay: c(0xF7F1BB), .twilightSun: c(0xED9E2F),
        .twilightNoon: c(0xF2CF3B),
    ]

    /// Blue–yellow readers (tritan): teal → cream → red/crimson ramps, off the blue–yellow axis.
    static let blueYellow: [ColorRole: ThemeColor] = [
        // Weather
        .conditionClear: c(0xF7F7F7, 0x333333), .conditionPartlyCloudy: c(0xB4B4B4, 0x555555),
        .conditionCloudy: c(0x898989, 0x7A7A7A), .conditionFog: c(0x9E9E9E, 0x676767),
        .conditionDrizzle: c(0x83D5D4, 0x69BBBA), .conditionRain: c(0x10A6AD, 0x0A999F),
        .conditionHeavyRain: c(0x046A77, 0x02707E), .conditionSnow: c(0xFFCFEE, 0xF6D0F1),
        .conditionStorm: c(0xA02D41, 0xDF676B), .conditionOther: c(0xCACACA, 0x444444),
        .graphClearDay: c(0xFDB89A), .graphClearNight: c(0x76C7CC), .graphPartlyCloudyDay: c(0xF6DCC7),
        .graphPartlyCloudyNight: c(0x88ADAE), .graphCloudy: c(0x9E9E9E), .graphFog: c(0xBEBEBE),
        // Temperature
        .tempFreezing: c(0x05515D, 0x43AFC1), .tempCold: c(0x08686C, 0x5AC8CB),
        .tempCool: c(0x287D79, 0x95DAD3), .tempMild: c(0x908168, 0xF0E7D2), .tempWarm: c(0xA25834, 0xFEBC9F),
        .tempHot: c(0x9A2929, 0xFE9088), .tempRangeLow: c(0x08686C, 0x5AC8CB),
        .tempRangeHigh: c(0xA25834, 0xFEBC9F), .tempChartCool: c(0x08686C, 0x5AC8CB),
        .tempChartWarm: c(0xA25834, 0xFEBC9F), .tempBelowNormal: c(0x49AAA9, 0x026266),
        .tempAboveNormal: c(0xDF8071, 0x9A3130),
        // Severity
        .levelGood: c(0x1D9999, 0xF9FFFE), .levelFair: c(0x2E827E, 0xAAFCF6),
        .levelModerate: c(0x775B3A, 0xE8D4B2), .levelPoor: c(0x773F04, 0xF5B193),
        .levelVeryPoor: c(0x790210, 0xF88F7F), .levelExtreme: c(0x5B0019, 0xF66B71),
        // Precipitation
        .precipRain: c(0x02646D, 0x56C4CA), .precipMixed: c(0xA9496B, 0xF8B3D3),
        .precipSnow: c(0x728C8D, 0xE6FAFA),
        // Radar
        .radar1: c(0xB0EBE7), .radar2: c(0x5FCCC8), .radar3: c(0xB7875F), .radar4: c(0xC04637),
        .radar5: c(0x9B1248),
        // Sun and moon
        .twilightAstronomical: c(0x004046), .twilightNautical: c(0x057176), .twilightCivil: c(0xDD766F),
        .twilightGolden: c(0xEFB696), .twilightDay: c(0xF6EED8), .twilightSun: c(0xF5886C),
        .twilightNoon: c(0xF9C59C),
    ]

    /// No color: OKLab lightness alone, at least 0.06 apart (patterns forced on).
    static let noColor: [ColorRole: ThemeColor] = [
        // Weather
        .conditionClear: c(0xF5F5F5, 0x2E2E2E), .conditionPartlyCloudy: c(0xB8B8B8, 0x505050),
        .conditionCloudy: c(0x919191, 0x747474), .conditionFog: c(0xA4A4A4, 0x626262),
        .conditionDrizzle: c(0x7F7F7F, 0x878787), .conditionRain: c(0x6C6C6C, 0x9B9B9B),
        .conditionHeavyRain: c(0x5B5B5B, 0xAFAFAF), .conditionSnow: c(0xE0E0E0, 0xD9D9D9),
        .conditionStorm: c(0x4A4A4A, 0xC4C4C4), .conditionOther: c(0xCCCCCC, 0x3E3E3E),
        .graphClearDay: c(0xE4E4E4), .graphClearNight: c(0xB1B1B1), .graphPartlyCloudyDay: c(0xD1D1D1),
        .graphPartlyCloudyNight: c(0x9E9E9E), .graphCloudy: c(0x8C8C8C), .graphFog: c(0xBEBEBE),
        // Temperature
        .tempFreezing: c(0x313131, 0x989898), .tempCold: c(0x424242, 0xACACAC),
        .tempCool: c(0x535353, 0xBFBFBF), .tempMild: c(0x656565, 0xD4D4D4), .tempWarm: c(0x787878, 0xE8E8E8),
        .tempHot: c(0x8A8A8A, 0xFDFDFD), .tempRangeLow: c(0x424242, 0xACACAC),
        .tempRangeHigh: c(0x787878, 0xE8E8E8), .tempChartCool: c(0x424242, 0xACACAC),
        .tempChartWarm: c(0x787878, 0xE8E8E8), .tempBelowNormal: c(0x808080, 0x5D5D5D),
        .tempAboveNormal: c(0xC4C4C4, 0x2E2E2E),
        // Severity
        .levelGood: c(0x8A8A8A, 0x989898), .levelFair: c(0x787878, 0xACACAC),
        .levelModerate: c(0x656565, 0xBFBFBF), .levelPoor: c(0x535353, 0xD4D4D4),
        .levelVeryPoor: c(0x424242, 0xE8E8E8), .levelExtreme: c(0x313131, 0xFDFDFD),
        // Precipitation
        .precipRain: c(0x353535, 0x9E9E9E), .precipMixed: c(0x5B5B5B, 0xCACACA),
        .precipSnow: c(0x868686, 0xF8F8F8),
        // Radar
        .radar1: c(0xE4E4E4), .radar2: c(0xBABABA), .radar3: c(0x929292), .radar4: c(0x6C6C6C),
        .radar5: c(0x484848),
        // Sun and moon
        .twilightAstronomical: c(0x2E2E2E), .twilightNautical: c(0x555555), .twilightCivil: c(0x868686),
        .twilightGolden: c(0xBEBEBE), .twilightDay: c(0xEEEEEE), .twilightSun: c(0xAEAEAE),
        .twilightNoon: c(0xD7D7D7),
    ]
}
