import XCTest

@testable import WhetherKit

/// which tabs show and in what order, and the cards of the Hourly and
/// Days tabs.
final class TabItemsTests: XCTestCase {
    private func decode(_ json: String) throws -> Preferences {
        try JSONDecoder().decode(Preferences.self, from: Data(json.utf8))
    }

    func testEveryTabButStormsAndEveryCardShowsByDefault() {
        let prefs = Preferences()
        XCTAssertEqual(prefs.tabs.map(\.item), [.hourly, .days, .radar, .plan, .storms])
        XCTAssertEqual(prefs.tabs.map(\.shown), [true, true, true, true, false], "a new install hides Storms")
        XCTAssertEqual(prefs.hourlyCards.map(\.item), [.graph, .hour, .sun])
        XCTAssertTrue(prefs.hourlyCards.allSatisfy(\.shown))
        XCTAssertEqual(prefs.daysCards.map(\.item), [.days, .history])
        XCTAssertTrue(prefs.daysCards.allSatisfy(\.shown))
        XCTAssertEqual(TabLayout.visible(prefs.tabs), [.hourly, .days, .radar, .plan])
    }

    func testPreferencesSavedBeforeTheListsGetTheDefaults() throws {
        for json in [#"{"units":"metric"}"#, "{}", #"{"tabs":"nonsense","hourlyCards":7}"#] {
            let prefs = try decode(json)
            XCTAssertEqual(prefs.tabs, TabChoice.defaults)
            XCTAssertEqual(prefs.hourlyCards, HourlyCardChoice.defaults)
            XCTAssertEqual(prefs.daysCards, DaysCardChoice.defaults)
        }
    }

    func testASavedOrderSurvivesAndUnknownOnesAreDroppedAndNewOnesFollow() throws {
        let json = #"""
            {"tabs":[{"item":"plan","shown":true},{"item":"alerts","shown":true},
                     {"item":"radar","shown":false},{"item":"plan"},{"item":"hourly"}],
             "hourlyCards":[{"item":"sun","shown":false},{"item":"nope"},{"item":"graph"}],
             "daysCards":[{"item":"history"}]}
            """#
        let prefs = try decode(json)
        XCTAssertEqual(prefs.tabs.map(\.item), [.plan, .radar, .hourly, .days, .storms])
        XCTAssertEqual(prefs.tabs.map(\.shown), [true, false, true, true, false], "a new tab shows, Storms not")
        XCTAssertEqual(TabLayout.visible(prefs.tabs), [.plan, .hourly, .days])
        XCTAssertEqual(prefs.hourlyCards.map(\.item), [.sun, .graph, .hour])
        XCTAssertEqual(prefs.hourlyCards.map(\.shown), [false, true, true])
        XCTAssertEqual(prefs.daysCards.map(\.item), [.history, .days])
    }

    func testTheListsRoundTrip() throws {
        var prefs = Preferences()
        prefs.tabs.set(.radar, shown: false)
        prefs.tabs.relocate(fromOffsets: IndexSet(integer: 3), toOffset: 0)
        prefs.hourlyCards.set(.sun, shown: false)
        prefs.daysCards.relocate(fromOffsets: IndexSet(integer: 1), toOffset: 0)
        let back = try decode(String(decoding: try JSONEncoder().encode(prefs), as: UTF8.self))
        XCTAssertEqual(back, prefs)
        XCTAssertEqual(back.tabs.map(\.item), [.plan, .hourly, .days, .radar, .storms])
        XCTAssertFalse(back.tabs.isShown(.radar))
    }

    /// A tab list saved before Storms existed gains it at the end, hidden, and keeps
    /// everything it had; switched on, it stays on.
    func testStormsComesHiddenIntoAnOldTabList() throws {
        let json = #"""
            {"tabs":[{"item":"hourly","shown":true},{"item":"days","shown":false},
                     {"item":"radar","shown":true},{"item":"plan","shown":true}]}
            """#
        var prefs = try decode(json)
        XCTAssertEqual(prefs.tabs.map(\.item), [.hourly, .days, .radar, .plan, .storms])
        XCTAssertEqual(prefs.tabs.map(\.shown), [true, false, true, true, false])
        XCTAssertEqual(TabLayout.visible(prefs.tabs), [.hourly, .radar, .plan])
        prefs.tabs.set(.storms, shown: true)
        let back = try decode(String(decoding: try JSONEncoder().encode(prefs), as: UTF8.self))
        XCTAssertEqual(TabLayout.visible(back.tabs), [.hourly, .radar, .plan, .storms])
        XCTAssertEqual(AppTabItem.storms.title, "Storms")
        XCTAssertEqual(AppTabItem.storms.symbol, "hurricane")
    }

    func testAHiddenTabFallsBackToNow() {
        let visible: [AppTabItem] = [.plan, .hourly]
        XCTAssertEqual(TabLayout.keeping(.hourly, among: visible), .hourly)
        XCTAssertNil(TabLayout.keeping(.radar, among: visible), "nil is the Now tab")
        XCTAssertNil(TabLayout.keeping(nil, among: visible))
        XCTAssertNil(TabLayout.keeping(.days, among: []))
    }

    func testAllTabsCanBeHiddenAndNowRemains() {
        var tabs = TabChoice.defaults
        for item in AppTabItem.allCases { tabs.set(item, shown: false) }
        XCTAssertEqual(TabLayout.visible(tabs), [])
    }

    func testArrangedByNamesShowsThoseInOrderAndHidesTheRest() {
        let cards = HourlyCardChoice.arranged(by: ["sun", "bogus", "graph", "sun"])
        XCTAssertEqual(cards.map(\.item), [.sun, .graph, .hour])
        XCTAssertEqual(cards.map(\.shown), [true, true, false])
        XCTAssertEqual(TabChoice.arranged(by: []).map(\.shown), [false, false, false, false, false])
        XCTAssertEqual(TabChoice.arranged(by: ["storms"]).shownItems() as [AppTabItem], [.storms])
        XCTAssertEqual(
            TabChoice.arranged(by: ["plan", "days"]).shownItems() as [AppTabItem], [.plan, .days])
    }

    func testEveryItemHasATitleAndASymbol() {
        for item in AppTabItem.allCases { XCTAssertFalse(item.title.isEmpty); XCTAssertFalse(item.symbol.isEmpty) }
        for item in HourlyCard.allCases { XCTAssertFalse(item.title.isEmpty); XCTAssertFalse(item.symbol.isEmpty) }
        for item in DaysCard.allCases { XCTAssertFalse(item.title.isEmpty); XCTAssertFalse(item.symbol.isEmpty) }
    }
}
