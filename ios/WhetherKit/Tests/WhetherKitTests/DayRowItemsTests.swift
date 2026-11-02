import XCTest

@testable import WhetherKit

/// what a day row shows and what an opened day shows, in what order.
final class DayRowItemsTests: XCTestCase {
    private func decode(_ json: String) throws -> Preferences {
        try JSONDecoder().decode(Preferences.self, from: Data(json.utf8))
    }

    private func rows(_ shown: [DayRowItem]) -> [DayRowChoice] {
        shown.map { DayRowChoice(item: $0, shown: true) }
            + DayRowItem.allCases.filter { !shown.contains($0) }.map { DayRowChoice(item: $0, shown: false) }
    }

    // MARK: Saved choices

    func testTheRowsAreAsTheyWereByDefault() {
        let choices = DayRowChoice.defaults
        XCTAssertEqual(choices.map(\.item), DayRowItem.allCases)
        XCTAssertEqual(choices.filter(\.shown).map(\.item), [.precipChance, .tempBar, .outlook])
        XCTAssertEqual(Preferences().dayRowItems, choices)
        XCTAssertEqual(
            DayRowLayout.make(choices),
            DayRowLayout(chance: true, bar: true, lines: [.outlook]))
    }

    func testAnOpenedDayShowsEverythingByDefault() {
        XCTAssertTrue(DayDetailChoice.defaults.allSatisfy(\.shown))
        XCTAssertEqual(DayDetailChoice.defaults.map(\.item), DayDetailItem.allCases)
        XCTAssertEqual(Preferences().dayDetailItems, DayDetailChoice.defaults)
    }

    func testPreferencesSavedBeforeTheListsGetTheDefaults() throws {
        for json in [#"{"units":"metric"}"#, "{}"] {
            let prefs = try decode(json)
            XCTAssertEqual(prefs.dayRowItems, DayRowChoice.defaults)
            XCTAssertEqual(prefs.dayDetailItems, DayDetailChoice.defaults)
        }
    }

    func testASavedListSurvivesUnknownItemsAreDroppedAndNewOnesFollowAsTheyShowByDefault() throws {
        let json = #"""
            {"dayRowItems":[{"item":"wind","shown":true},{"item":"fromTheFuture","shown":true},
                            {"item":"outlook","shown":false},{"item":"airTemps"},{"item":"wind"}],
             "dayDetailItems":[{"item":"sun","shown":false},{"item":"nope"},{"item":"hours"}]}
            """#
        let prefs = try decode(json)
        XCTAssertEqual(prefs.dayRowItems.prefix(3).map(\.item), [.wind, .outlook, .airTemps])
        XCTAssertTrue(prefs.dayRowItems[0].shown)
        XCTAssertFalse(prefs.dayRowItems[1].shown)
        XCTAssertFalse(prefs.dayRowItems[2].shown, "no flag: as the item shows by default")
        XCTAssertEqual(prefs.dayRowItems.count, DayRowItem.allCases.count, "each item once")
        let added = prefs.dayRowItems.dropFirst(3)
        XCTAssertEqual(
            added.map(\.item), DayRowItem.allCases.filter { ![.wind, .outlook, .airTemps].contains($0) })
        XCTAssertEqual(added.filter(\.shown).map(\.item), [.precipChance, .tempBar], "new items as by default")
        XCTAssertEqual(prefs.dayDetailItems.prefix(2).map(\.item), [.sun, .hours])
        XCTAssertFalse(prefs.dayDetailItems[0].shown)
        XCTAssertEqual(prefs.dayDetailItems.count, DayDetailItem.allCases.count)
        XCTAssertTrue(prefs.dayDetailItems.dropFirst(2).allSatisfy(\.shown))
    }

    func testTheChoicesRoundTrip() throws {
        var prefs = Preferences()
        prefs.dayRowItems.set(.humidity, shown: true)
        prefs.dayRowItems.set(.precipChance, shown: false)
        prefs.dayRowItems.relocate(fromOffsets: [4], toOffset: 0)
        prefs.dayDetailItems.set(.stretches, shown: false)
        let again = try JSONDecoder().decode(Preferences.self, from: JSONEncoder().encode(prefs))
        XCTAssertEqual(again.dayRowItems, prefs.dayRowItems)
        XCTAssertEqual(again.dayDetailItems, prefs.dayDetailItems)
        XCTAssertEqual(again.dayRowItems.first?.item, .humidity)
        XCTAssertFalse(again.dayRowItems.isShown(.precipChance))
        XCTAssertFalse(again.dayDetailItems.isShown(.stretches))
    }

    // MARK: A day's row

    func testTheChanceAndTheBarKeepTheirPlacesAndTheRestFollowInOrder() {
        let layout = DayRowLayout.make(rows([.uv, .precipChance, .outlook, .humidity]))
        XCTAssertTrue(layout.chance)
        XCTAssertFalse(layout.bar)
        XCTAssertEqual(layout.lines, [.facts([.uv]), .outlook, .facts([.humidity])])
    }

    func testNeighbouringFactsShareOneLine() {
        let layout = DayRowLayout.make(rows([.tempBar, .wind, .airTemps, .outlook, .humidity, .uv]))
        XCTAssertEqual(layout.lines, [.facts([.wind, .air]), .outlook, .facts([.humidity, .uv])])
        XCTAssertFalse(layout.chance)
        XCTAssertTrue(layout.bar)
    }

    func testHidingEverythingLeavesTheBareRow() {
        XCTAssertEqual(
            DayRowLayout.make(rows([])), DayRowLayout(chance: false, bar: false, lines: []))
    }

    func testTheFactsLineKeepsTheCoresTextAndSkipsWhatADayLacks() {
        XCTAssertEqual(
            DayFacts.line(
                [.wind, .humidity, .uv, .air], air: "68° / 50°", humidity: "45%–80%", wind: "12 mph", uv: "6"),
            "Wind 12 mph · Humidity 45%–80% · UV 6 · Air 68° / 50°")
        XCTAssertEqual(
            DayFacts.line([.air, .humidity, .uv], air: nil, humidity: "45%", wind: "9 mph", uv: nil),
            "Humidity 45%")
        XCTAssertNil(DayFacts.line([.air, .uv], air: nil, humidity: "45%", wind: nil, uv: nil))
    }

    // MARK: An opened day

    private func details(_ order: [DayDetailItem], hidden: Set<DayDetailItem> = []) -> [DayDetailChoice] {
        order.map { DayDetailChoice(item: $0, shown: !hidden.contains($0)) }
    }

    func testTheDefaultsLayOutAnOpenedDayAsItWas() {
        XCTAssertEqual(
            DayDetailLayout.blocks(DayDetailChoice.defaults),
            [.timeline(stretches: true, hours: true), .lines([.totals, .sun, .airTemps, .normals])])
    }

    func testTheStripSitsWhereTheFirstOfItsTwoItemsIs() {
        XCTAssertEqual(
            DayDetailLayout.blocks(details([.totals, .hours, .sun, .stretches, .airTemps, .normals])),
            [
                .lines([.totals]), .timeline(stretches: true, hours: true),
                .lines([.sun, .airTemps, .normals]),
            ])
    }

    func testOneItemOfTheStripDrawsOnlyItsPart() {
        XCTAssertEqual(
            DayDetailLayout.blocks(details(DayDetailItem.allCases, hidden: [.hours, .totals])),
            [.timeline(stretches: true, hours: false), .lines([.sun, .airTemps, .normals])])
        XCTAssertEqual(
            DayDetailLayout.blocks(details(DayDetailItem.allCases, hidden: [.stretches])),
            [.timeline(stretches: false, hours: true), .lines([.totals, .sun, .airTemps, .normals])])
    }

    func testHiddenItemsLeaveTheLinesAndAStripBetweenSplitsThem() {
        XCTAssertEqual(
            DayDetailLayout.blocks(
                details([.totals, .stretches, .normals, .sun], hidden: [.sun])),
            [.lines([.totals]), .timeline(stretches: true, hours: false), .lines([.normals])])
        XCTAssertEqual(
            DayDetailLayout.blocks(details(DayDetailItem.allCases, hidden: Set(DayDetailItem.allCases))), [])
    }
}
