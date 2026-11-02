import XCTest

@testable import WhetherKit

/// Live Activity matching, imported source rows, nearby places and overlapping refreshes.
final class PlaceHandlingTests: XCTestCase {
    // MARK: Live Activity keyed by place id

    private typealias Running = RainActivityMatch.Running

    func testTheActivityIsKeptForTheSamePlaceIdAndName() {
        let running = [Running(id: "a1", placeID: "p1", name: "Home")]
        let plan = RainActivityMatch.plan(running: running, placeID: "p1", name: "Home")
        XCTAssertEqual(plan.keep, "a1")
        XCTAssertEqual(plan.end, [])
    }

    func testTwoPlacesWithTheSameNameAreNotMixedUp() {
        let running = [Running(id: "a1", placeID: "p1", name: "Springfield")]
        let plan = RainActivityMatch.plan(running: running, placeID: "p2", name: "Springfield")
        XCTAssertNil(plan.keep, "another place with the same name starts its own")
        XCTAssertEqual(plan.end, ["a1"])
    }

    func testARenamedPlaceStartsAgainToShowItsName() {
        let running = [Running(id: "a1", placeID: "p1", name: "Home")]
        let plan = RainActivityMatch.plan(running: running, placeID: "p1", name: "Cabin")
        XCTAssertNil(plan.keep)
        XCTAssertEqual(plan.end, ["a1"])
    }

    func testAnActivityFromAnOlderBuildIsEnded() {
        let running = [Running(id: "old", placeID: nil, name: "Home")]
        let plan = RainActivityMatch.plan(running: running, placeID: "p1", name: "Home")
        XCTAssertNil(plan.keep)
        XCTAssertEqual(plan.end, ["old"])
    }

    func testExtraActivitiesForThePlaceAreEnded() {
        let running = [
            Running(id: "a1", placeID: "p1", name: "Home"),
            Running(id: "a2", placeID: "p1", name: "Home"),
            Running(id: "a3", placeID: "p9", name: "Work"),
        ]
        let plan = RainActivityMatch.plan(running: running, placeID: "p1", name: "Home")
        XCTAssertEqual(plan.keep, "a1")
        XCTAssertEqual(plan.end, ["a2", "a3"])
    }

    // MARK: Imported models and One Call switch their rows

    func testImportedModelsTurnTheModelsRowOnForTheCore() {
        var p = Preferences()
        p.set(Sources.models, enabled: false)
        p.setModels(["gfs_hrrr", "ecmwf_ifs025"])
        XCTAssertTrue(p.enabled(Sources.models))
        let chain = p.settings(withKey: false).sources
        XCTAssertEqual(chain.first { $0.provider == Sources.models }?.enabled, true)
        XCTAssertEqual(p.settings(withKey: false).models, ["gfs_hrrr", "ecmwf_ifs025"])
    }

    func testAnEmptyModelListTurnsTheRowOff() {
        var p = Preferences()
        p.setModels(["gfs_hrrr"])
        p.setModels([])
        XCTAssertFalse(p.enabled(Sources.models))
        XCTAssertEqual(p.models, [])
    }

    func testImportedOneCallNeedsAKeyAndTurnsOpenWeatherOn() {
        var p = Preferences()
        p.set(Sources.openWeather, enabled: false)
        p.setOneCall(true, hasKey: false)
        XCTAssertFalse(p.oneCall, "the key stays out of the file")
        XCTAssertFalse(p.enabled(Sources.openWeather))
        p.setOneCall(true, hasKey: true)
        XCTAssertTrue(p.oneCall)
        XCTAssertTrue(p.enabled(Sources.openWeather))
        p.setOneCall(false, hasKey: true)
        XCTAssertFalse(p.oneCall)
    }

    func testImportedNwsSwitchesItsRow() {
        var p = Preferences()
        p.nws = true
        XCTAssertTrue(p.enabled(Sources.nws))
        XCTAssertTrue(p.settings(withKey: false).sources.first { $0.provider == Sources.nws }?.enabled == true)
    }

    // MARK: Following place and saved places within 1 km

    private func at(_ id: String, _ latitude: Double, follows: Bool = false) -> AppPlace {
        AppPlace(
            id: id, name: id, detail: nil, latitude: latitude, longitude: -75, timeZone: "UTC",
            follows: follows)
    }

    func testAFixedPlaceNearTheFollowingOneIsRemoved() {
        // 0.005 degrees of latitude is about 550 m; 0.02 is about 2.2 km.
        let list = [at("me", 40.000, follows: true), at("home", 40.005), at("far", 40.02)]
        let (kept, removed) = AppPlace.removingDuplicates(of: "me", in: list)
        XCTAssertEqual(kept.map(\.id), ["me", "far"])
        XCTAssertEqual(removed.map(\.id), ["home"])
    }

    func testTheFollowingPlaceKeepsItsIdAndOtherFollowersAreNotTouched() {
        let list = [at("me", 40.000, follows: true), at("a", 40.002), at("b", 40.003)]
        let (kept, removed) = AppPlace.removingDuplicates(of: "me", in: list)
        XCTAssertEqual(kept.map(\.id), ["me"])
        XCTAssertEqual(removed.map(\.id), ["a", "b"])
        // An id that does not follow removes nothing.
        let (same, none) = AppPlace.removingDuplicates(of: "a", in: list)
        XCTAssertEqual(same, list)
        XCTAssertTrue(none.isEmpty)
    }

    // MARK: A refresh asked for during a refresh joins it

    func testAFullRefreshFetchesEveryPlaceOnceAndEnds() {
        var queue = RefreshQueue(only: nil)
        XCTAssertEqual(queue.next(among: ["a", "b"]), ["a", "b"])
        XCTAssertEqual(queue.next(among: ["a", "b"]), [])
    }

    func testAPlaceAddedDuringATargetedRefreshIsFetchedByIt() {
        // add() of "b" while the refresh of "a" runs.
        var queue = RefreshQueue(only: ["a"])
        XCTAssertEqual(queue.next(among: ["a", "b"]), ["a"])
        queue.request(["b"])
        XCTAssertEqual(queue.next(among: ["a", "b"]), ["b"], "not dropped, and not a again")
        XCTAssertEqual(queue.next(among: ["a", "b"]), [])
    }

    func testAMovedPlaceIsFetchedAgainDuringAFullRefresh() {
        // followMe() of "a" after the full refresh fetched it with the old coordinates.
        var queue = RefreshQueue(only: nil)
        XCTAssertEqual(queue.next(among: ["a", "b"]), ["a", "b"])
        queue.request(["a"])
        XCTAssertEqual(queue.next(among: ["a", "b"]), ["a"])
        XCTAssertEqual(queue.next(among: ["a", "b"]), [])
    }

    func testAPlaceAddedDuringAFullRefreshIsFetchedAndTheOthersAreNot() {
        var queue = RefreshQueue(only: nil)
        XCTAssertEqual(queue.next(among: ["a"]), ["a"])
        XCTAssertEqual(queue.next(among: ["a", "new"]), ["new"])
    }

    func testAFullRefreshAskedForDuringATargetedOneFetchesTheOthers() {
        var queue = RefreshQueue(only: ["a"])
        XCTAssertEqual(queue.next(among: ["a", "b", "c"]), ["a"])
        queue.request(nil)
        XCTAssertEqual(queue.next(among: ["a", "b", "c"]), ["b", "c"])
    }

    func testARequestForARemovedPlaceIsIgnored() {
        var queue = RefreshQueue(only: ["a"])
        XCTAssertEqual(queue.next(among: ["a"]), ["a"])
        queue.request(["gone"])
        XCTAssertEqual(queue.next(among: ["a"]), [])
    }

    // MARK: A forecast for coordinates the place has left is dropped

    func testAForecastForTheOldCoordinatesIsNotCurrent() {
        let before = at("me", 40.000, follows: true)
        var moved = before
        moved.latitude = 40.1
        XCTAssertTrue(AppPlace.isCurrent(before, in: [before]))
        XCTAssertFalse(AppPlace.isCurrent(before, in: [moved]), "the place moved")
        XCTAssertTrue(AppPlace.isCurrent(moved, in: [moved]))
        var renamed = before
        renamed.name = "Elsewhere"
        XCTAssertTrue(AppPlace.isCurrent(before, in: [renamed]), "a new name keeps the forecast")
        XCTAssertFalse(AppPlace.isCurrent(before, in: [at("other", 40)]), "removed")
    }
}
