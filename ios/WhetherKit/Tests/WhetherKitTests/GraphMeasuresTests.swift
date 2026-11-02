import XCTest

@testable import WhetherKit

/// The graph measures' order rules and options.
final class GraphMeasuresTests: XCTestCase {
    private let known = ["temperature", "actual", "rain", "wind", "gusts", "humidity", "uv", "pressure"]

    func testShownDropsUnknownAndRepeatedAndNeverEmpty() {
        XCTAssertEqual(MeasureList.shown(["wind", "x", "wind", "rain"], known: known), ["wind", "rain"])
        XCTAssertEqual(MeasureList.shown([], known: known), ["temperature"])
        XCTAssertEqual(MeasureList.shown(["x"], known: known), ["temperature"])
    }

    func testFillMakesOneFirstAndKeepsTheOthersInOrder() {
        XCTAssertEqual(MeasureList.fill("rain", in: ["temperature", "wind", "rain", "uv"]), ["rain", "temperature", "wind", "uv"])
        XCTAssertEqual(MeasureList.fill("temperature", in: ["temperature", "wind"]), ["temperature", "wind"])
        XCTAssertEqual(MeasureList.fill("gusts", in: ["temperature"]), ["temperature"], "not shown: unchanged")
    }

    func testMoveStopsAtTheEnds() {
        let list = ["temperature", "wind", "rain"]
        XCTAssertEqual(MeasureList.move("wind", by: -1, in: list), ["wind", "temperature", "rain"])
        XCTAssertEqual(MeasureList.move("wind", by: 1, in: list), ["temperature", "rain", "wind"])
        XCTAssertEqual(MeasureList.move("temperature", by: -1, in: list), list)
        XCTAssertEqual(MeasureList.move("rain", by: 1, in: list), list)
        XCTAssertEqual(MeasureList.move("uv", by: 1, in: list), list)
    }

    func testPlaceTakesTheTargetsSlot() {
        let list = ["temperature", "wind", "rain", "uv"]
        XCTAssertEqual(MeasureList.place("uv", at: "wind", in: list), ["temperature", "uv", "wind", "rain"], "dragged back")
        XCTAssertEqual(MeasureList.place("temperature", at: "rain", in: list), ["wind", "rain", "temperature", "uv"], "dragged forward")
        XCTAssertEqual(MeasureList.place("wind", at: "wind", in: list), list)
        XCTAssertEqual(MeasureList.place("gusts", at: "wind", in: list), list, "not shown")
        XCTAssertEqual(MeasureList.place("wind", at: "gusts", in: list), list, "no such target")
    }

    func testHideKeepsTheLastOne() {
        XCTAssertEqual(MeasureList.hide("wind", in: ["temperature", "wind"]), ["temperature"])
        XCTAssertEqual(MeasureList.hide("temperature", in: ["temperature"]), ["temperature"])
    }

    func testShowAppendsOnce() {
        XCTAssertEqual(MeasureList.show("uv", in: ["temperature"]), ["temperature", "uv"])
        XCTAssertEqual(MeasureList.show("uv", in: ["uv", "temperature"]), ["uv", "temperature"])
    }

    func testEditorRowsListShownThenHidden() {
        let rows = MeasureList.editorRows(shown: ["wind", "temperature"], offered: known)
        XCTAssertEqual(rows.map(\.id), ["wind", "temperature", "actual", "rain", "gusts", "humidity", "uv", "pressure"])
        XCTAssertEqual(rows.map(\.shown), [true, true] + Array(repeating: false, count: 6))
        // The air temperature is not offered unless feels-like leads.
        let plain = known.filter { $0 != "actual" }
        XCTAssertFalse(MeasureList.editorRows(shown: ["temperature"], offered: plain).contains { $0.id == "actual" })
    }

    func testMovedRowsReorderOnlyTheShownOnes() {
        let shown = ["temperature", "wind", "rain"]
        // Drag the first row below the second (a destination of 2, before "rain").
        XCTAssertEqual(
            MeasureList.moved(shown: shown, offered: known, from: IndexSet(integer: 0), to: 2),
            ["wind", "temperature", "rain"])
        // Drag the last up to the top.
        XCTAssertEqual(
            MeasureList.moved(shown: shown, offered: known, from: IndexSet(integer: 2), to: 0),
            ["rain", "temperature", "wind"])
        // A hidden row dragged among them changes nothing.
        let hiddenRow = MeasureList.editorRows(shown: shown, offered: known).firstIndex { $0.id == "uv" } ?? 0
        XCTAssertEqual(
            MeasureList.moved(shown: shown, offered: known, from: IndexSet(integer: hiddenRow), to: 1), shown)
    }

    func testScalesAreSharedForLikeMeasuresUnlessTurnedOff() {
        XCTAssertEqual(MeasureList.scale(of: "gusts", shared: true), MeasureList.scale(of: "wind", shared: true))
        XCTAssertEqual(MeasureList.scale(of: "actual", shared: true), MeasureList.scale(of: "temperature", shared: true))
        XCTAssertNotEqual(MeasureList.scale(of: "gusts", shared: true), MeasureList.scale(of: "uv", shared: true))
        XCTAssertNotEqual(MeasureList.scale(of: "gusts", shared: false), MeasureList.scale(of: "wind", shared: false))
        XCTAssertNotEqual(MeasureList.scale(of: "actual", shared: false), MeasureList.scale(of: "temperature", shared: false))
    }

    func testValueStrideAutomaticMatchesTheOldRule() {
        // Today's rule: never under 3, thinner as the hours narrow.
        XCTAssertEqual(HourWindow.valueStride(every: 0, step: 54, labelWidth: 36), 3)
        XCTAssertEqual(HourWindow.valueStride(every: 0, step: 10, labelWidth: 36), 6)
    }

    func testValueStrideChoiceIsKeptWhileItFits() {
        XCTAssertEqual(HourWindow.valueStride(every: 1, step: 54, labelWidth: 36), 1)
        XCTAssertEqual(HourWindow.valueStride(every: 6, step: 54, labelWidth: 36), 6)
        // Narrow hours: a label needs more than one of them, so it thins.
        XCTAssertEqual(HourWindow.valueStride(every: 1, step: 18, labelWidth: 36), 2)
        XCTAssertEqual(HourWindow.valueStride(every: 3, step: 18, labelWidth: 36), 3, "a coarser choice stays")
    }

    func testOptionsDecodeWithDefaultsAndRejectOddChoices() throws {
        let none = try JSONDecoder().decode(GraphOptions.self, from: Data("{}".utf8))
        XCTAssertEqual(none, GraphOptions())
        XCTAssertEqual(none.valueEvery, 0)
        XCTAssertFalse(none.sharedScales, "one scale per measure, as before this change")
        let saved = try JSONDecoder().decode(GraphOptions.self, from: Data(#"{"valueEvery":6,"sharedScales":true}"#.utf8))
        XCTAssertEqual(saved.valueEvery, 6)
        XCTAssertTrue(saved.sharedScales)
        let odd = try JSONDecoder().decode(GraphOptions.self, from: Data(#"{"valueEvery":5}"#.utf8))
        XCTAssertEqual(odd.valueEvery, 0, "not a choice: automatic")
    }

    func testPreferencesWithoutGraphOptionsKeepTheirMeasuresAndDefaultTheOptions() throws {
        let old = try JSONDecoder().decode(
            Preferences.self, from: Data(#"{"graphMeasures":["wind","gusts"],"hourRows":["humidity"]}"#.utf8))
        XCTAssertEqual(old.graphMeasures, ["wind", "gusts"], "the saved lists are unchanged")
        XCTAssertEqual(old.hourRows, ["humidity"])
        XCTAssertEqual(old.graphOptions, GraphOptions())
        let saved = try JSONDecoder().decode(
            Preferences.self, from: Data(#"{"graphOptions":{"valueEvery":1,"sharedScales":false}}"#.utf8))
        XCTAssertEqual(saved.graphOptions.valueEvery, 1)
        // And round-trips.
        var prefs = Preferences()
        prefs.graphOptions.valueEvery = 3
        let back = try JSONDecoder().decode(Preferences.self, from: JSONEncoder().encode(prefs))
        XCTAssertEqual(back.graphOptions.valueEvery, 3)
    }
}
