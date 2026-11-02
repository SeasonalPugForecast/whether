import XCTest

@testable import WhetherKit

/// "Restore all layouts" resets every order and visibility list, and
/// nothing else.
final class LayoutResetTests: XCTestCase {
    /// Preferences with every layout list changed and a few other settings changed too.
    private func rearranged() -> Preferences {
        var p = Preferences()
        // The layout lists.
        p.sections.reverse()
        p.sections[0].shown.toggle()
        p.tabs.reverse()
        p.tabs[0].shown = false
        p.hourlyCards.reverse()
        p.hourlyCards[0].shown = false
        p.daysCards.reverse()
        p.daysCards[0].shown = false
        p.sunItems.reverse()
        p.sunItems[0].shown = false
        p.graphMeasures = ["humidity", "wind"]
        p.hourRows = ["gusts", "humidity"]
        p.hourTiles.reverse()
        p.hourTiles[0].shown = false
        p.detailCards.reverse()
        p.detailCards[0].shown = false
        p.dayRowItems.reverse()
        p.dayRowItems[0].shown.toggle()
        p.dayDetailItems.reverse()
        p.dayDetailItems[0].shown = false
        // Settings that are not layouts.
        p.units = .metric
        p.clock = .twentyFour
        p.dates = .iso
        p.layout = .darkSky
        p.background = .plain
        p.appearance = .dark
        p.appIcon = .winter
        p.days = 5
        p.refreshMinutes = 60
        p.feelsLike = false
        p.alertNotifications = false
        p.favouritePlaceID = "somewhere"
        p.openOn = .favourite
        p.graphOptions.valueEvery = 3
        p.graphOptions.sharedScales = true
        p.radar.style = .hybrid
        p.radar.speed = .fast
        p.radar.opacity = 0.4
        p.unitOverrides.temperature = "celsius"
        p.sources.reverse()
        return p
    }

    /// The saved form of preferences, as a dictionary by key.
    private func saved(_ p: Preferences) throws -> [String: Any] {
        let data = try JSONEncoder().encode(p)
        return try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
    }

    func testEveryLayoutListGoesBackToItsDefault() {
        let p = rearranged()
        XCTAssertNotEqual(p.sections, Preferences.defaultSections)
        let restored = p.restoringLayouts()
        XCTAssertEqual(restored.sections, Preferences.defaultSections)
        XCTAssertEqual(restored.tabs, TabChoice.defaults)
        XCTAssertEqual(restored.hourlyCards, HourlyCardChoice.defaults)
        XCTAssertEqual(restored.daysCards, DaysCardChoice.defaults)
        XCTAssertEqual(restored.sunItems, SunItemChoice.defaults)
        XCTAssertEqual(restored.graphMeasures, Preferences().graphMeasures)
        XCTAssertEqual(restored.hourRows, Preferences().hourRows)
        XCTAssertEqual(restored.hourTiles, Preferences.defaultHourTiles)
        XCTAssertEqual(restored.detailCards, Preferences.defaultDetails)
        XCTAssertEqual(restored.dayRowItems, DayRowChoice.defaults)
        XCTAssertEqual(restored.dayDetailItems, DayDetailChoice.defaults)
    }

    /// The saved keys whose values differ between two preferences.
    private func differing(_ x: Preferences, _ y: Preferences) throws -> Set<String> {
        // A key with no value (the favourite place, when there is none) is not saved.
        let (a, b) = (try saved(x), try saved(y))
        return Set(a.keys).union(b.keys).filter { key in
            let (left, right) = (a[key] as? NSObject ?? NSNull(), b[key] as? NSObject ?? NSNull())
            return !left.isEqual(right)
        }
    }

    func testItTouchesOnlyTheLayoutFields() throws {
        let before = rearranged()
        let after = before.restoringLayouts()
        // Every saved key that changed is a layout key ...
        let changed = try differing(before, after)
        XCTAssertFalse(changed.isEmpty)
        XCTAssertTrue(changed.isSubset(of: Preferences.layoutKeys), "\(changed.subtracting(Preferences.layoutKeys))")
        // ... and the others are as they were, among them those named here.
        XCTAssertEqual(after.units, .metric)
        XCTAssertEqual(after.clock, .twentyFour)
        XCTAssertEqual(after.dates, .iso)
        XCTAssertEqual(after.layout, .darkSky)
        XCTAssertEqual(after.background, .plain)
        XCTAssertEqual(after.appIcon, .winter)
        XCTAssertEqual(after.favouritePlaceID, "somewhere")
        XCTAssertEqual(after.openOn, .favourite)
        XCTAssertEqual(after.graphOptions, GraphOptions(), "the graph's labels and scales are layout too")
        XCTAssertEqual(after.radar, before.radar)
        XCTAssertEqual(after.sources, before.sources)
        XCTAssertEqual(after.unitOverrides, before.unitOverrides)
    }

    func testEveryLayoutKeyIsSavedAndTheTestRearrangedEachOne() throws {
        // A layout key that names nothing saved, or a list the reset forgot, would show here.
        XCTAssertTrue(Preferences.layoutKeys.isSubset(of: Set(try saved(Preferences()).keys)))
        let changed = try differing(rearranged(), Preferences())
        XCTAssertTrue(Preferences.layoutKeys.isSubset(of: changed), "\(Preferences.layoutKeys.subtracting(changed))")
        // The reset puts every one of them back to the defaults.
        XCTAssertEqual(try differing(rearranged().restoringLayouts(), Preferences()).intersection(Preferences.layoutKeys), [])
    }

    func testRestoringTwiceOrOnDefaultsChangesNothing() {
        let once = rearranged().restoringLayouts()
        XCTAssertEqual(once.restoringLayouts(), once)
        XCTAssertEqual(Preferences().restoringLayouts(), Preferences())
    }

    func testTheHubsDetailCardsHaveTitlesAndSymbols() {
        XCTAssertEqual(Preferences.DetailChoice(kind: "dew_point").fallbackTitle, "Dew point")
        XCTAssertEqual(Preferences.DetailChoice(kind: "uv").fallbackTitle, "Uv")
        XCTAssertEqual(Preferences.DetailChoice(kind: "wind").symbol, "wind")
        XCTAssertEqual(Preferences.DetailChoice(kind: "records").symbol, "trophy")
        XCTAssertEqual(Preferences.DetailChoice(kind: "unknown").symbol, "circle")
    }
}
