import XCTest

@testable import WhetherKit

/// which items the Sun & moon card shows, in what order, and how the
/// light bar follows the hidden twilights.
final class SunMoonItemsTests: XCTestCase {
    /// A day's edges in the shape the core gives them (hand-made: times are seconds).
    private let edges: [SunEdge] = [
        SunEdge(at: 100, morning: true, kind: "astronomical", label: "Astronomical dawn", time: "4:50 AM"),
        SunEdge(at: 200, morning: true, kind: "nautical", label: "Nautical dawn", time: "5:20 AM"),
        SunEdge(at: 300, morning: true, kind: "civil", label: "Civil dawn", time: "5:50 AM"),
        SunEdge(at: 400, morning: true, kind: "golden", label: "Golden hour ends", time: "7:00 AM"),
        SunEdge(at: 800, morning: false, kind: "golden", label: "Golden hour begins", time: "6:30 PM"),
        SunEdge(at: 900, morning: false, kind: "civil", label: "Civil dusk", time: "7:40 PM"),
        SunEdge(at: 1000, morning: false, kind: "nautical", label: "Nautical dusk", time: "8:10 PM"),
        SunEdge(at: 1100, morning: false, kind: "astronomical", label: "Astronomical dusk", time: "8:40 PM"),
    ]

    private func decode(_ json: String) throws -> Preferences {
        try JSONDecoder().decode(Preferences.self, from: Data(json.utf8))
    }

    // MARK: Saved choices

    func testEveryItemShowsByDefault() {
        let choices = SunItemChoice.defaults
        XCTAssertEqual(choices.map(\.item), SunItem.allCases)
        XCTAssertTrue(choices.allSatisfy(\.shown))
        XCTAssertEqual(Preferences().sunItems, choices)
    }

    func testCompleteKeepsOrderAndShownDropsRepeatsAndAddsMissingAtTheEnd() {
        let saved = [
            SunItemChoice(item: .moonPhase, shown: false),
            SunItemChoice(item: .sunTimes),
            SunItemChoice(item: .moonPhase),
        ]
        let done = SunItemChoice.complete(saved)
        XCTAssertEqual(Array(done.prefix(2)), [saved[0], saved[1]], "order and the shown flag stay")
        XCTAssertEqual(done.count, SunItem.allCases.count, "each item once")
        XCTAssertEqual(
            done.dropFirst(2).map(\.item), SunItem.allCases.filter { $0 != .moonPhase && $0 != .sunTimes })
        XCTAssertTrue(done.dropFirst(2).allSatisfy(\.shown), "new items show")
    }

    func testPreferencesSavedBeforeTheListGetTheDefaults() throws {
        XCTAssertEqual(try decode(#"{"units":"metric"}"#).sunItems, SunItemChoice.defaults)
        XCTAssertEqual(try decode("{}").sunItems, SunItemChoice.defaults)
    }

    func testASavedListSurvivesAndUnknownItemsAreDropped() throws {
        let json = #"""
            {"sunItems":[{"item":"moonTimes","shown":false},{"item":"fromTheFuture","shown":true},
                         {"item":"civilTwilight"},{"item":"sunTimes","shown":true}]}
            """#
        let prefs = try decode(json)
        XCTAssertEqual(prefs.sunItems.prefix(3).map(\.item), [.moonTimes, .civilTwilight, .sunTimes])
        XCTAssertFalse(prefs.sunItems[0].shown)
        XCTAssertTrue(prefs.sunItems[1].shown, "no flag: shown")
        XCTAssertEqual(prefs.sunItems.count, SunItem.allCases.count)
        // And it round-trips.
        var prefs2 = Preferences()
        prefs2.sunItems.set(.astronomicalTwilight, shown: false)
        prefs2.sunItems.relocate(fromOffsets: [0], toOffset: 3)
        let again = try JSONDecoder().decode(Preferences.self, from: JSONEncoder().encode(prefs2))
        XCTAssertEqual(again.sunItems, prefs2.sunItems)
        XCTAssertFalse(again.sunItems.isShown(.astronomicalTwilight))
    }

    // MARK: The card's blocks

    func testTheDefaultsLayOutTheCardAsItWas() {
        XCTAssertEqual(
            SunMoonLayout.blocks(SunItemChoice.defaults),
            [
                .header(sunTimes: true, daylight: true), .bar,
                .times(kinds: ["astronomical", "nautical", "civil", "golden"], noon: true),
                .moon(phase: true, illumination: true), .moonTimes,
            ])
    }

    func testHiddenItemsLeaveTheBlocks() {
        var items = SunItemChoice.defaults
        for item in [SunItem.astronomicalTwilight, .nauticalTwilight, .daylight, .moonIllumination] {
            items.set(item, shown: false)
        }
        XCTAssertEqual(
            SunMoonLayout.blocks(items),
            [
                .header(sunTimes: true, daylight: false), .bar,
                .times(kinds: ["civil", "golden"], noon: true),
                .moon(phase: true, illumination: false), .moonTimes,
            ])
        for choice in items.indices { items[choice].shown = false }
        XCTAssertEqual(SunMoonLayout.blocks(items), [])
    }

    func testAnItemOfAnotherKindSplitsTheTimesInTwo() {
        var items = SunItemChoice.defaults
        // Civil twilight first, the bar, then the rest of the times.
        items.relocate(fromOffsets: [5], toOffset: 0)
        let blocks = SunMoonLayout.blocks(items)
        XCTAssertEqual(blocks.first, .times(kinds: ["civil"], noon: false))
        XCTAssertEqual(blocks[1], .header(sunTimes: true, daylight: true))
        XCTAssertEqual(blocks[2], .bar)
        XCTAssertEqual(blocks[3], .times(kinds: ["astronomical", "nautical", "golden"], noon: true))
    }

    func testTheOrderOfTheItemsIsTheOrderOnTheCard() {
        var items = SunItemChoice.defaults
        items.relocate(fromOffsets: [10], toOffset: 0)  // moon rise and set first
        items.relocate(fromOffsets: [4], toOffset: 11)  // astronomical twilight last
        let blocks = SunMoonLayout.blocks(items)
        XCTAssertEqual(blocks.first, .moonTimes)
        XCTAssertEqual(blocks.last, .times(kinds: ["astronomical"], noon: false))
    }

    // MARK: The times list

    private let noon = SunMark(kind: "noon", label: "Solar noon", time: "1:15 PM", at: 600)
    private let sunrise = SunMark(kind: "sunrise", label: "Sunrise", time: "6:20 AM", at: 350)
    private let sunset = SunMark(kind: "sunset", label: "Sunset", time: "7:10 PM", at: 850)

    func testTheTimesAreInTimeOrderWithSolarNoonBetweenTheDayAndTheEvening() {
        let all: Set = ["astronomical", "nautical", "civil", "golden"]
        let rows = SunMoonLayout.rows(edges: edges, kinds: all, noon: noon, showNoon: true)
        XCTAssertEqual(
            rows.map(\.label),
            [
                "Astronomical dawn", "Nautical dawn", "Civil dawn", "Golden hour ends", "Solar noon",
                "Golden hour begins", "Civil dusk", "Nautical dusk", "Astronomical dusk",
            ])
        XCTAssertEqual(rows[4], noon)
    }

    func testSunriseSunsetAndTheMoonSitInTimeOrderAmongTheTwilights() {
        let moonrise = SunMark(kind: "moonrise", label: "Moonrise", time: "9:00 PM", at: 950)
        let rows = SunMoonLayout.rows(
            edges: edges, kinds: ["civil", "golden"], noon: noon, showNoon: true,
            extra: [sunset, moonrise, sunrise])
        XCTAssertEqual(
            rows.map(\.label),
            [
                "Civil dawn", "Sunrise", "Golden hour ends", "Solar noon", "Golden hour begins", "Sunset",
                "Civil dusk", "Moonrise",
            ])
        XCTAssertEqual(rows.map(\.kind).prefix(3), ["civil", "sunrise", "golden"])
    }

    func testOnlyTheKindsAskedForShow() {
        let rows = SunMoonLayout.rows(edges: edges, kinds: ["civil"], noon: nil, showNoon: false)
        XCTAssertEqual(rows.map(\.label), ["Civil dawn", "Civil dusk"])
        XCTAssertEqual(SunMoonLayout.rows(edges: edges, kinds: [], noon: nil, showNoon: true), [])
        XCTAssertEqual(
            SunMoonLayout.rows(edges: edges, kinds: [], noon: noon, showNoon: false), [], "noon off: left out")
    }

    func testEveryKindHasItsOwnSymbolAndTheItemsUseThem() {
        let kinds = [
            "astronomical", "nautical", "civil", "golden", "sunrise", "sunset", "noon", "moonrise", "moonset",
        ]
        let symbols = kinds.map { SunMoonLayout.symbol(kind: $0) }
        XCTAssertEqual(Set(symbols).count, kinds.count, "no two moments share a symbol")
        XCTAssertFalse(symbols.contains("circle.fill"), "none falls back")
        for item in SunItem.allCases {
            if let kind = item.eventKind { XCTAssertEqual(item.symbol, SunMoonLayout.symbol(kind: kind)) }
        }
        XCTAssertEqual(SunItem.solarNoon.symbol, SunMoonLayout.symbol(kind: "noon"))
    }

    // MARK: The marks on the bar

    func testTheBarCarriesTheShownMomentsInsideItsSpan() {
        let span: ClosedRange<Int64> = 200...1000
        let marks = SunMoonLayout.barMarks(
            edges: edges, hiding: ["astronomical"], extra: [sunset, noon, sunrise], span: span)
        // Astronomical dawn and dusk are hidden (and outside the span anyway); nautical dawn
        // is at 200, the bar's start.
        XCTAssertEqual(
            marks.map(\.label),
            [
                "Nautical dawn", "Civil dawn", "Sunrise", "Golden hour ends", "Solar noon",
                "Golden hour begins", "Sunset", "Civil dusk", "Nautical dusk",
            ])
        let outside = SunMoonLayout.barMarks(
            edges: [], hiding: [], extra: [SunMark(kind: "moonrise", label: "Moonrise", time: "x", at: 50)],
            span: span)
        XCTAssertEqual(outside, [], "a moment outside the bar has no mark")
    }

    func testMarksSitAtTheirTimeAlongTheBarSunAboveTwilightBelow() {
        let span: ClosedRange<Int64> = 0...1000
        let placed = SunMoonLayout.placeMarks(
            [
                SunMark(kind: "civil", label: "Civil dawn", time: "", at: 300),
                SunMark(kind: "sunrise", label: "Sunrise", time: "", at: 500),
            ], span: span, width: 400)
        XCTAssertEqual(placed.map(\.x), [120, 200])
        XCTAssertEqual(placed.map(\.above), [false, true])
    }

    func testACrowdMovesToTheOtherStripThenGivesWayToTheSun() {
        let span: ClosedRange<Int64> = 0...1000
        // 400 points: 100 s is 40 points. Three twilights 5 points apart: the first goes below,
        // the second above (the other strip), the third has no room.
        let crowd = [
            SunMark(kind: "astronomical", label: "A", time: "", at: 500),
            SunMark(kind: "nautical", label: "N", time: "", at: 512),
            SunMark(kind: "civil", label: "C", time: "", at: 525),
        ]
        let placed = SunMoonLayout.placeMarks(crowd, span: span, width: 400)
        XCTAssertEqual(placed.map(\.mark.label), ["A", "N"])
        XCTAssertEqual(placed.map(\.above), [false, true])
        // Sunrise on the same spot wins the strip above: the nautical dawn is left out.
        let withSun = SunMoonLayout.placeMarks(
            crowd + [SunMark(kind: "sunrise", label: "S", time: "", at: 510)], span: span, width: 400)
        XCTAssertEqual(withSun.map(\.mark.label), ["A", "S"])
        XCTAssertEqual(withSun.map(\.above), [false, true])
    }

    func testTheMoonGivesWayToTheSunAndSymbolsStayOnTheBar() {
        let span: ClosedRange<Int64> = 0...1000
        let marks = [
            SunMark(kind: "moonset", label: "M", time: "", at: 2),
            SunMark(kind: "sunrise", label: "S", time: "", at: 5),
        ]
        let placed = SunMoonLayout.placeMarks(marks, span: span, width: 300)
        XCTAssertEqual(placed.map(\.mark.label), ["M", "S"], "the moon takes the strip below")
        XCTAssertEqual(placed.map(\.above), [false, true])
        XCTAssertEqual(placed.map(\.x), [7.5, 7.5], "kept inside the bar's end")
        XCTAssertEqual(SunMoonLayout.placeMarks(marks, span: 5...5, width: 300), [])
        XCTAssertEqual(SunMoonLayout.placeMarks(marks, span: span, width: 10), [])
    }

    // MARK: The light bar

    private func bar(hiding hidden: Set<String> = []) -> [LightStretch] {
        SunMoonLayout.lightBar(edges: edges, hiding: hidden)
    }

    func testTheBarHasSevenStretchesByDefault() {
        XCTAssertEqual(
            bar(),
            [
                LightStretch(start: 100, end: 200, kind: "astronomical"),
                LightStretch(start: 200, end: 300, kind: "nautical"),
                LightStretch(start: 300, end: 400, kind: "civil"),
                LightStretch(start: 400, end: 800, kind: "day"),
                LightStretch(start: 800, end: 900, kind: "civil"),
                LightStretch(start: 900, end: 1000, kind: "nautical"),
                LightStretch(start: 1000, end: 1100, kind: "astronomical"),
            ])
    }

    func testWithAstronomicalAndNauticalHiddenTheBarStartsAtCivilDawn() {
        let stretches = bar(hiding: ["astronomical", "nautical"])
        XCTAssertEqual(
            stretches,
            [
                LightStretch(start: 300, end: 400, kind: "civil"),
                LightStretch(start: 400, end: 800, kind: "day"),
                LightStretch(start: 800, end: 900, kind: "civil"),
            ])
    }

    func testAHiddenMiddleTwilightJoinsItsOuterNeighbour() {
        // Nautical hidden: the morning's joins the astronomical stretch before it,
        // the evening's the astronomical one after it. No gap is left.
        XCTAssertEqual(
            bar(hiding: ["nautical"]),
            [
                LightStretch(start: 100, end: 300, kind: "astronomical"),
                LightStretch(start: 300, end: 400, kind: "civil"),
                LightStretch(start: 400, end: 800, kind: "day"),
                LightStretch(start: 800, end: 900, kind: "civil"),
                LightStretch(start: 900, end: 1100, kind: "astronomical"),
            ])
    }

    func testHidingCivilJoinsTheNauticalStretches() {
        XCTAssertEqual(
            bar(hiding: ["civil"]),
            [
                LightStretch(start: 100, end: 200, kind: "astronomical"),
                LightStretch(start: 200, end: 400, kind: "nautical"),
                LightStretch(start: 400, end: 800, kind: "day"),
                LightStretch(start: 800, end: 1000, kind: "nautical"),
                LightStretch(start: 1000, end: 1100, kind: "astronomical"),
            ])
    }

    func testEveryTwilightHiddenLeavesTheBrightDay() {
        XCTAssertEqual(
            bar(hiding: ["astronomical", "nautical", "civil"]),
            [LightStretch(start: 400, end: 800, kind: "day")])
    }

    func testStretchesAlwaysCoverTheBarWithoutGaps() {
        let kinds = ["astronomical", "nautical", "civil"]
        for mask in 0..<8 {
            let hidden = Set(kinds.enumerated().filter { mask & (1 << $0.offset) != 0 }.map(\.element))
            let s = bar(hiding: hidden)
            for (a, b) in zip(s, s.dropFirst()) { XCTAssertEqual(a.end, b.start, "hiding \(hidden)") }
            XCTAssertTrue(s.allSatisfy { !hidden.contains($0.kind) }, "hiding \(hidden)")
        }
    }

    func testHiddenTwilightsFollowTheItemsAndLeaveTheGoldenHourOut() {
        var items = SunItemChoice.defaults
        XCTAssertEqual(SunMoonLayout.hiddenTwilights(items), [])
        items.set(.astronomicalTwilight, shown: false)
        items.set(.goldenHour, shown: false)
        XCTAssertEqual(SunMoonLayout.hiddenTwilights(items), ["astronomical"])
    }
}
