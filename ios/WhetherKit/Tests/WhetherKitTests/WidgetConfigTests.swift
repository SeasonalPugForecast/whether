import XCTest

@testable import WhetherKit

/// the widget configured on the Home Screen (place and content).
final class WidgetConfigTests: XCTestCase {
    private func place(_ id: String, detail: String? = nil) -> AppPlace {
        AppPlace(id: id, name: "Name \(id)", detail: detail, latitude: 40, longitude: -75, timeZone: "UTC")
    }

    private let list = [AppPlace]([
        AppPlace(id: "a", name: "A", detail: "New York, US", latitude: 40, longitude: -75, timeZone: "UTC"),
        AppPlace(id: "b", name: "B", detail: nil, latitude: 41, longitude: -74, timeZone: "UTC"),
        AppPlace(id: "c", name: "C", detail: "Ohio, US", latitude: 39, longitude: -82, timeZone: "UTC"),
    ])

    // MARK: Place

    func testAConfiguredPlaceIsShownWhateverTheAppAndTheButtonChose() {
        XCTAssertEqual(
            WidgetData.chosenIndex(
                places: list, configuredID: "c", chosenID: "b", legacyIndex: 0, active: 1, favouriteID: "a"), 2)
    }

    func testTheAppsPlaceFollowsTheOldRules() {
        func index(configured: String?, chosen: String? = nil, fav: String? = nil, active: Int = 1) -> Int? {
            WidgetData.chosenIndex(
                places: list, configuredID: configured, chosenID: chosen, legacyIndex: nil, active: active,
                favouriteID: fav)
        }
        // "The app's" and no choice at all (a widget placed before it was configurable) agree.
        for configured in [WidgetPlaceOption.appsID, nil] {
            XCTAssertEqual(index(configured: configured), 1, "the place the app shows")
            XCTAssertEqual(index(configured: configured, fav: "c"), 2, "the favourite")
            XCTAssertEqual(index(configured: configured, chosen: "a", fav: "c"), 0, "the button's choice")
        }
    }

    func testAConfiguredPlaceThatWasDeletedFallsBackToTheApps() {
        XCTAssertEqual(
            WidgetData.chosenIndex(
                places: list, configuredID: "gone", chosenID: nil, legacyIndex: nil, active: 1, favouriteID: "c"), 2)
        XCTAssertNil(WidgetData.configuredIndex(places: list, configuredID: "gone"))
        XCTAssertNil(WidgetData.configuredIndex(places: list, configuredID: WidgetPlaceOption.appsID))
        XCTAssertNil(WidgetData.configuredIndex(places: list, configuredID: nil))
        XCTAssertEqual(WidgetData.configuredIndex(places: list, configuredID: "b"), 1)
    }

    func testNoPlacesShowsNothing() {
        XCTAssertNil(
            WidgetData.chosenIndex(places: [], configuredID: "a", chosenID: nil, legacyIndex: nil, active: 0))
    }

    func testAConfiguredWidgetLeavesTheButtonsChoiceAlone() throws {
        let defaults = try XCTUnwrap(UserDefaults(suiteName: "widget-config-\(UUID().uuidString)"))
        defaults.set("b", forKey: WidgetData.placeKey)
        defaults.set(2, forKey: WidgetData.legacyPlaceKey)
        XCTAssertEqual(WidgetData.chosenIndex(places: list, defaults: defaults, configuredID: "c", favouriteID: nil), 2)
        XCTAssertEqual(defaults.string(forKey: WidgetData.placeKey), "b")
        XCTAssertEqual(defaults.object(forKey: WidgetData.legacyPlaceKey) as? Int, 2, "migrated by the app's mode only")
        XCTAssertEqual(WidgetData.chosenIndex(places: list, defaults: defaults, favouriteID: nil), 1, "the app's mode still sees it")
    }

    // MARK: The place picker

    func testThePickerListsTheAppsPlaceFirstThenEverySavedPlaceInOrder() {
        let options = WidgetPlaceOption.options(places: list)
        XCTAssertEqual(options.map(\.id), [WidgetPlaceOption.appsID, "a", "b", "c"])
        XCTAssertEqual(options.map(\.name), ["The app's", "A", "B", "C"])
        XCTAssertEqual(options[1].detail, "New York, US")
        XCTAssertNil(options[2].detail)
        XCTAssertEqual(WidgetPlaceOption.options(places: []).map(\.id), [WidgetPlaceOption.appsID])
    }

    func testNoSavedPlaceCanTakeTheAppsId() {
        XCTAssertFalse(list.contains { $0.id == WidgetPlaceOption.appsID })
        // New ids are UUIDs and old ones "name:<name>": neither can be "@app".
        XCTAssertFalse(UUID().uuidString.hasPrefix("@"))
    }

    func testLookupKeepsTheAskedOrderAndDropsDeletedPlaces() {
        let found = WidgetPlaceOption.options(for: ["c", "gone", WidgetPlaceOption.appsID, "a"], places: list)
        XCTAssertEqual(found.map(\.id), ["c", WidgetPlaceOption.appsID, "a"])
        XCTAssertEqual(WidgetPlaceOption.options(for: [], places: list), [])
    }

    // MARK: Content

    func testTheDefaultContentIsTodaysWidget() {
        // 5 hours in the medium widget, 6 in the large one (and, large, rain and five days).
        XCTAssertEqual(WidgetContent.overview.hours(large: false), 5)
        XCTAssertEqual(WidgetContent.overview.hours(large: true), 6)
    }

    func testEachContentListsItsHours() {
        XCTAssertEqual(WidgetContent.nextHour.hours(large: false), 1)
        XCTAssertEqual(WidgetContent.nextHour.hours(large: true), 1)
        XCTAssertEqual(WidgetContent.nextHours.hours(large: false), 6)
        XCTAssertEqual(WidgetContent.nextHours.hours(large: true), 12)
        XCTAssertEqual(WidgetContent.today.hours(large: true), 0)
        XCTAssertEqual(WidgetContent.nextHours.hours(large: true) % 2, 0, "the large widget shows two equal rows")
    }

    func testTheSavedNamesOfTheContentsStayStable() {
        XCTAssertEqual(WidgetContent.allCases.map(\.rawValue), ["overview", "nextHour", "nextHours", "today"])
        XCTAssertNil(WidgetContent(rawValue: "sideways"))
    }
}
