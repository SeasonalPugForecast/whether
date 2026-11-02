import XCTest

@testable import WhetherKit

/// Sharing a custom theme as a `.whethertheme` file: the versioned JSON, what
/// a file may and may not bring in, and how a received theme joins the custom themes.
final class SharedThemeTests: XCTestCase {
    private let pink = ThemeColor(light: RGBA(rgb: 0xFF66AA))
    private let teal = ThemeColor(light: RGBA(rgb: 0x115566), dark: RGBA(rgba: 0x22778899))

    private func mine() -> ColorTheme {
        var theme = CustomThemes.duplicate(ColorPresets.aurora, existing: [], id: "custom-1")
        theme.colors[.accent] = pink
        theme.colors[.skyClearDayTop] = teal
        return theme
    }

    private func decode(_ json: String) throws -> SharedTheme {
        try SharedTheme(data: Data(json.utf8))
    }

    // MARK: The file

    func testARoundTripKeepsNameBaseColorsAndPatterns() throws {
        var theme = mine()
        theme.patterns = true
        let shared = SharedTheme(theme)
        let back = try SharedTheme(data: shared.data())
        XCTAssertEqual(back, shared)
        XCTAssertEqual(back.name, "My Aurora")
        XCTAssertEqual(back.base, "aurora")
        XCTAssertEqual(back.colors, [.accent: pink, .skyClearDayTop: teal])
        XCTAssertTrue(back.patterns)
    }

    func testTheJSONIsVersionedAndCarriesNoId() throws {
        let json = try XCTUnwrap(
            JSONSerialization.jsonObject(with: SharedTheme(mine()).data()) as? [String: Any])
        XCTAssertEqual(json["v"] as? Int, SharedTheme.version)
        XCTAssertNil(json["id"], "each phone gives a theme its own id")
        XCTAssertNil(json["patterns"], "false is left out")
        let colors = try XCTUnwrap(json["colors"] as? [String: Any])
        XCTAssertEqual(Set(colors.keys), ["accent", "sky.clear_day.top"], "roles by their names")
        // The same theme is the same file.
        XCTAssertEqual(try SharedTheme(mine()).data(), try SharedTheme(mine()).data())
    }

    func testAnotherVersionIsRefused() {
        XCTAssertThrowsError(try decode(#"{"v": 2, "name": "Next", "base": "aurora", "colors": {}}"#)) {
            XCTAssertEqual($0 as? SharedThemeError, .needsNewerVersion)
        }
        XCTAssertThrowsError(try decode(#"{"v": 0, "name": "Old", "colors": {}}"#)) {
            XCTAssertEqual($0 as? SharedThemeError, .needsNewerVersion)
        }
    }

    func testWhatIsntAThemeIsUnreadable() {
        for json in [
            "not json", "[]", #"{"name": "No version"}"#, #"{"v": 1}"#, #"{"v": "1", "name": "x"}"#,
            #"{"v": 1, "name": "x", "colors": []}"#,
        ] {
            XCTAssertThrowsError(try decode(json), json) {
                XCTAssertEqual($0 as? SharedThemeError, .unreadable, json)
            }
        }
        let huge = Data(repeating: 0x20, count: SharedTheme.largestFile + 1)
        XCTAssertThrowsError(try SharedTheme(data: huge)) {
            XCTAssertEqual($0 as? SharedThemeError, .unreadable)
        }
    }

    func testUnknownRolesAndBadColorsAreDropped() throws {
        let theme = try decode(
            #"""
            {"v": 1, "name": "Mixed", "base": "neon", "colors": {
              "accent": "#FF66AA", "level.poor": {"light": "#112233", "dark": "#445566"},
              "sky.mars.top": "#FF0000", "rain": "orange", "icon.sun": {"dark": "#FFFFFF"},
              "precip.snow": "#12345"}}
            """#)
        XCTAssertEqual(theme.base, "neon")
        XCTAssertEqual(
            theme.colors,
            [
                .accent: pink,
                .levelPoor: ThemeColor(light: RGBA(rgb: 0x112233), dark: RGBA(rgb: 0x445566)),
            ])
    }

    func testColorsArePlainSRGBWithoutSystemColors() throws {
        // A file may claim a system color (only the Whether preset paints those): the hex stays.
        let theme = try decode(
            ##"{"v": 1, "name": "System", "colors": {"level.good": {"light": "#34C759", "system": "green"}}}"##)
        XCTAssertEqual(theme.colors[.levelGood], ThemeColor(light: RGBA(rgb: 0x34C759)))
        XCTAssertNil(theme.colors[.levelGood]?.system)
        XCTAssertEqual(theme.base, ColorPresets.defaultID, "no base is Whether")

        // Out of range components are clamped and rounded to the 256 steps a theme saves.
        let wide = SharedTheme(
            name: "Wide", base: "aurora",
            colors: [.accent: ThemeColor(light: RGBA(red: 1.4, green: -0.2, blue: 0.5004))])
        XCTAssertEqual(wide.colors[.accent]?.light, RGBA(hex: "#FF0080"))
        XCTAssertEqual(wide.colors[.accent]?.light.hex, "#FF0080")
    }

    func testAnUnknownBaseBecomesWhether() throws {
        let theme = try decode(#"{"v": 1, "name": "Future", "base": "solarpunk", "colors": {}}"#)
        XCTAssertEqual(theme.base, ColorPresets.defaultID)
        XCTAssertEqual(try decode(#"{"v": 1, "name": "S", "base": "seasons"}"#).base, "seasons")
    }

    func testNamesAreTrimmedAndCapped() throws {
        XCTAssertEqual(try decode(#"{"v": 1, "name": "  Lake\nat dusk  "}"#).name, "Lake at dusk")
        XCTAssertEqual(try decode(#"{"v": 1, "name": "   "}"#).name, "Shared colors")
        let long = String(repeating: "a", count: 200)
        XCTAssertEqual(try decode(#"{"v": 1, "name": "\#(long)"}"#).name.count, SharedTheme.longestName)
    }

    @MainActor
    func testTheFileIsNamedAfterTheTheme() {
        XCTAssertEqual(SharedTheme(mine()).fileName, "My Aurora.whethertheme")
        var slashed = mine()
        slashed.name = "Day/Night"
        XCTAssertEqual(SharedTheme(slashed).fileName, "Day-Night.whethertheme")
        slashed.name = "..."
        XCTAssertEqual(SharedTheme(slashed).fileName, "Colors.whethertheme")
        XCTAssertTrue(SharedTheme.isThemeFile(URL(fileURLWithPath: "/tmp/Inbox/My Aurora.WhetherTheme")))
        XCTAssertFalse(SharedTheme.isThemeFile(URL(fileURLWithPath: "/tmp/Inbox/Shore.whetherplan")))
    }

    // MARK: Receiving

    func testAReceivedThemeIsANewCustomTheme() {
        let shared = SharedTheme(mine())
        let theme = shared.theme(among: [], month: 10, id: "custom-new")
        XCTAssertEqual(theme.id, "custom-new")
        XCTAssertEqual(theme.name, "My Aurora")
        XCTAssertEqual(theme.base, "aurora")
        XCTAssertEqual(theme.colors, [.accent: pink, .skyClearDayTop: teal])
        XCTAssertTrue(shared.theme(among: [], month: 10).id.hasPrefix(CustomThemes.idPrefix))

        // A name already taken gets a number.
        XCTAssertEqual(shared.theme(among: [mine()], month: 10).name, "My Aurora 2")
    }

    func testAColorEqualToTheBaseIsNotKept() throws {
        let base = try XCTUnwrap(ColorPresets.aurora.colors[.accent])
        let shared = SharedTheme(name: "Same", base: "aurora", colors: [.accent: base, .rain: pink])
        XCTAssertEqual(shared.theme(among: [], month: 10).colors, [.rain: pink])
    }

    func testTheSameColorsAlreadyHereAreFoundWhateverTheirName() {
        var here = mine()
        here.id = "custom-here"
        here.name = "Robin's"
        let shared = SharedTheme(mine())
        XCTAssertEqual(shared.match(among: [here], month: 10)?.id, "custom-here")

        var other = here
        other.colors[.accent] = teal
        XCTAssertNil(shared.match(among: [other], month: 10))
        var patterned = here
        patterned.patterns = true
        XCTAssertNil(shared.match(among: [patterned], month: 10))
        XCTAssertNil(SharedTheme(name: "Plain", base: "neon", colors: [:]).match(among: [here], month: 10))
    }

    func testAReceivedThemeIsCompletedFromItsBase() throws {
        let file = try SharedTheme(mine()).data()
        let received = try SharedTheme(data: file)
        let painted = received.theme(among: [], month: 10).completed(from: received.basePreset(month: 10))
        XCTAssertTrue(painted.missing.isEmpty, "the base fills every other role")
        XCTAssertEqual(painted[.accent], pink)
    }
}
