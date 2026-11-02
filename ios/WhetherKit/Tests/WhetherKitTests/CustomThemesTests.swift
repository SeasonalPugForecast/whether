import XCTest

@testable import WhetherKit

/// Your own colors: custom themes made from presets, saved with only the
/// roles that differ from the base, loaded and resolved like presets, and the editor's
/// contrast lines.
final class CustomThemesTests: XCTestCase {
    private func decode(_ json: String) throws -> Preferences {
        try JSONDecoder().decode(Preferences.self, from: Data(json.utf8))
    }

    private let pink = ThemeColor(light: RGBA(rgb: 0xFF66AA))

    // MARK: Making them

    func testCustomizeNamesItAfterThePresetAndSavesNothingYet() {
        let mine = CustomThemes.duplicate(ColorPresets.aurora, existing: [], id: "custom-1")
        XCTAssertEqual(mine.id, "custom-1")
        XCTAssertEqual(mine.name, "My Aurora")
        XCTAssertEqual(mine.base, "aurora")
        XCTAssertEqual(mine.colors, [:], "nothing differs from the base yet")
        XCTAssertTrue(CustomThemes.duplicate(ColorPresets.aurora, existing: []).id.hasPrefix(CustomThemes.idPrefix))

        let again = CustomThemes.duplicate(ColorPresets.aurora, existing: [mine], id: "custom-2")
        XCTAssertEqual(again.name, "My Aurora 2", "a second one gets a number")

        // Seasons keeps its base, so the custom theme follows the season too; Graphite
        // keeps its patterns.
        let fall = ColorPresets.preset("seasons", month: 10)!
        XCTAssertEqual(CustomThemes.duplicate(fall, existing: []).base, "seasons")
        XCTAssertEqual(CustomThemes.duplicate(fall, existing: []).name, "My Seasons")
        XCTAssertTrue(CustomThemes.duplicate(ColorPresets.graphite, existing: []).patterns)
    }

    func testDuplicateCopiesACustomThemesColors() {
        var mine = CustomThemes.duplicate(ColorPresets.neon, existing: [], id: "a")
        mine.colors[.accent] = pink
        let copy = CustomThemes.duplicate(mine, existing: [mine], id: "b")
        XCTAssertEqual(copy.name, "My Neon 2")
        XCTAssertEqual(copy.base, "neon")
        XCTAssertEqual(copy.colors, [.accent: pink])
        let third = CustomThemes.duplicate(copy, existing: [mine, copy], id: "c")
        XCTAssertEqual(third.name, "My Neon 3", "counts on from the number")
    }

    func testUniqueNamesAndTypedNames() {
        func theme(_ name: String) -> ColorTheme { ColorTheme(id: name, name: name, colors: [:]) }
        XCTAssertEqual(CustomThemes.uniqueName("Mine", among: []), "Mine")
        XCTAssertEqual(CustomThemes.uniqueName("Mine", among: [theme("Mine"), theme("Mine 2")]), "Mine 3")
        XCTAssertEqual(CustomThemes.uniqueName("Mine 2", among: [theme("Mine 2")]), "Mine 3")
        XCTAssertEqual(CustomThemes.uniqueName("Route 66", among: [theme("Route 66")]), "Route 67")
        XCTAssertEqual(CustomThemes.cleanName("  Dusk \n"), "Dusk")
        XCTAssertNil(CustomThemes.cleanName("   "))
    }

    // MARK: Saving only what differs

    func testOnlyTheRolesThatDifferAreSaved() throws {
        let base = ColorPresets.aurora
        var mine = CustomThemes.duplicate(base, existing: [], id: "custom-1")
        mine = mine.setting(.levelPoor, to: pink, base: base)
        mine = mine.setting(.accent, to: ThemeColor(light: RGBA(rgb: 0x123456)), base: base)
        XCTAssertTrue(mine.differs(.levelPoor, from: base))
        XCTAssertFalse(mine.differs(.levelGood, from: base))

        // Picking the base's color again drops the role: it follows the base once more.
        mine = mine.setting(.accent, to: base[.accent], base: base)
        XCTAssertEqual(Set(mine.colors.keys), [.levelPoor])
        XCTAssertFalse(mine.differs(.accent, from: base))

        let json = try JSONSerialization.jsonObject(with: JSONEncoder().encode(mine)) as? [String: Any]
        let colors = json?["colors"] as? [String: Any]
        XCTAssertEqual(colors?.keys.sorted(), ["level.poor"], "one role saved, not 82")

        // Whole themes trim to the same.
        XCTAssertEqual(mine.completed(from: base).trimmed(against: base).colors, mine.colors)
    }

    func testAPickedColorReplacesASystemColor() {
        // Whether paints severity with iOS's system colors; a pick of any other color
        // is the reader's own, and resetting brings the system color back.
        let base = ColorPresets.whether
        XCTAssertNotNil(base[.levelGood].system)
        let mine = ColorTheme(id: "c", name: "C", base: "whether", colors: [:])
            .setting(.levelGood, to: ThemeColor(light: RGBA(rgb: 0x00AA00)), base: base)
        XCTAssertTrue(mine.differs(.levelGood, from: base))
        XCTAssertNil(mine.completed(from: base)[.levelGood].system)
        var reset = mine
        reset.colors[.levelGood] = nil
        XCTAssertEqual(reset.completed(from: base)[.levelGood], base[.levelGood])
    }

    // MARK: Loading and painting

    func testPreferencesWithoutCustomThemesDecode() throws {
        // Saved before custom themes: no customThemes key.
        let old = try decode(#"{"colorTheme":"aurora","days":5}"#)
        XCTAssertEqual(old.customThemes, [])
        XCTAssertEqual(old.colorTheme().id, "aurora")
    }

    func testCustomThemesLoadAndPaintCompletedFromTheirBase() throws {
        // A good theme, one that doesn't decode (no id), one with a preset's id, and a
        // second with the first's id: only the good one stays.
        let prefs = try decode(
            """
            {"colorTheme": "custom-a",
             "customThemes": [
               {"id": "custom-a", "name": "Mine", "base": "almanac",
                "colors": {"accent": "#123456", "level.poor": {"light": "#FF66AA"}, "gone.role": "#000000"}},
               {"name": "No id"},
               {"id": "aurora", "name": "Not Aurora", "colors": {}},
               {"id": "custom-a", "name": "Twin", "colors": {}}
             ]}
            """)
        XCTAssertEqual(prefs.customThemes.map(\.name), ["Mine"])
        XCTAssertEqual(Set(prefs.customThemes[0].colors.keys), [.accent, .levelPoor])

        let painted = prefs.colorTheme()
        XCTAssertEqual(painted.id, "custom-a")
        XCTAssertEqual(painted.name, "Mine")
        XCTAssertEqual(painted.missing, [], "completed from its base")
        XCTAssertEqual(painted[.accent].light.hex, "#123456")
        XCTAssertEqual(painted[.skyRainTop], ColorPresets.almanac[.skyRainTop])

        // Color vision still replaces the informational colors, custom ones included.
        var red = prefs
        red.colorVision = .redGreen
        XCTAssertEqual(red.colorTheme()[.levelPoor], ColorPresets.ramps(for: .redGreen)[.levelPoor])
        XCTAssertEqual(red.colorTheme()[.accent].light.hex, "#123456", "decorative colors stay")

        // Saved and loaded again, it is the same.
        let again = try JSONDecoder().decode(Preferences.self, from: JSONEncoder().encode(prefs))
        XCTAssertEqual(again.customThemes, prefs.customThemes)
    }

    // MARK: The editor

    func testSimulationKeepsGraysAndDropsSystemColors() {
        let deutan = ColorPresets.whether.simulated(as: .deutan)
        XCTAssertEqual(deutan.missing, [])
        XCTAssertTrue(ColorRole.allCases.allSatisfy { deutan[$0].system == nil }, "painted by their hex")
        let gray = RGBA(rgb: 0x808080)
        let theme = ColorTheme(id: "g", name: "G", colors: [.accent: ThemeColor(light: gray, dark: .white)])
        XCTAssertEqual(theme.simulated(as: .protan)[.accent].light.hex, gray.hex)
        XCTAssertEqual(theme.simulated(as: .tritan)[.accent].dark?.hex, "#FFFFFF")
    }

    func testContrastLinesFindTheWorstCase() {
        // The designed presets pass every line for typical vision (`ColorThemeTests`).
        let aurora = ColorPresets.aurora
        XCTAssertTrue(ThemeChecks.skyText(aurora).passes)
        XCTAssertTrue(ThemeChecks.mark(aurora).passes)
        XCTAssertTrue(ThemeChecks.apart(aurora, as: nil).passes)

        // Whether's light day skies are known to fail white text.
        let whether = ThemeChecks.skyText(ColorPresets.whether)
        XCTAssertFalse(whether.passes)
        XCTAssertLessThan(whether.ratio, 3)

        // A pale sky stop, a pale mark and two equal steps are each found.
        var mine = aurora
        mine.colors[.skySnowBottom] = ThemeColor(light: RGBA(rgb: 0xEEEEEE))
        mine.colors[.precipSnow] = ThemeColor(light: RGBA(rgb: 0xF0F0F0), dark: RGBA(rgb: 0x202020))
        mine.colors[.levelVeryPoor] = mine[.levelPoor]
        XCTAssertEqual(ThemeChecks.skyText(mine).role, .skySnowBottom)
        XCTAssertFalse(ThemeChecks.skyText(mine).passes)
        let mark = ThemeChecks.mark(mine)
        XCTAssertEqual(mark.role, .precipSnow)
        XCTAssertFalse(mark.passes)
        let apart = ThemeChecks.apart(mine, as: .deutan)
        XCTAssertEqual([apart.first, apart.second], [.levelPoor, .levelVeryPoor])
        XCTAssertEqual(apart.delta, 0, accuracy: 1e-9)
        XCTAssertFalse(apart.passes)
    }

    func testEveryRoleHasANameAndOneGroup() {
        for role in ColorRole.allCases {
            XCTAssertNotEqual(role.title, role.rawValue, "\(role.rawValue) has no name in words")
            XCTAssertFalse(role.title.isEmpty)
        }
        let grouped = ColorRoleGroup.allCases.flatMap { ColorRole.roles(in: $0) }
        XCTAssertEqual(grouped.count, ColorRole.allCases.count)
        XCTAssertEqual(Set(grouped), Set(ColorRole.allCases))
        XCTAssertEqual(ColorRole.skyClearDayTop.title, "Clear day, top")
        XCTAssertEqual(ColorRole.skyCloudyNightBottom.skyName, "cloudy night")
        XCTAssertNil(ColorRole.accent.skyName)
    }
}
