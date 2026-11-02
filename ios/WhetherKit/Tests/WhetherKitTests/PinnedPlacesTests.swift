import XCTest

@testable import WhetherKit

/// pinned places, the favourite, and the place the app opens on.
final class PinnedPlacesTests: XCTestCase {
    private func place(_ id: String, follows: Bool = false, pinned: Bool = false) -> AppPlace {
        AppPlace(
            id: id, name: id, detail: nil, latitude: 40, longitude: -75, timeZone: "UTC",
            follows: follows, pinned: pinned)
    }

    private func ids(_ places: [AppPlace]) -> [String] { places.map(\.id) }

    // MARK: Order

    func testPinnedPlacesComeAfterTheFollowingOneAndBeforeTheRest() {
        let list = AppPlace.arranged([
            place("a"), place("b", pinned: true), place("f", follows: true), place("c"),
            place("d", pinned: true),
        ])
        XCTAssertEqual(ids(list), ["f", "b", "d", "a", "c"])
    }

    func testEachGroupKeepsItsOrder() {
        let list = AppPlace.arranged([
            place("z"), place("y", pinned: true), place("x"), place("w", pinned: true), place("v"),
        ])
        XCTAssertEqual(ids(list), ["y", "w", "z", "x", "v"])
        XCTAssertEqual(ids(AppPlace.arranged(list)), ids(list), "arranging twice changes nothing")
    }

    func testTheFollowingPlaceIsFirstEvenWhenPinned() {
        let list = AppPlace.arranged([place("a", pinned: true), place("f", follows: true, pinned: true)])
        XCTAssertEqual(ids(list), ["f", "a"])
    }

    func testNothingPinnedIsTheOldRule() {
        XCTAssertEqual(ids(AppPlace.arranged([place("a"), place("b", follows: true), place("c")])), ["b", "a", "c"])
        XCTAssertEqual(AppPlace.arranged([]), [])
    }

    func testPinnedDefaultsToFalseAndRoundTrips() throws {
        let old = #"[{"name":"Brooklyn","latitude":40.650,"longitude":-73.9496,"timeZone":"America/New_York"}]"#
        XCTAssertFalse(try JSONDecoder().decode([AppPlace].self, from: Data(old.utf8))[0].pinned)
        let back = try JSONDecoder().decode(AppPlace.self, from: JSONEncoder().encode(place("p", pinned: true)))
        XCTAssertTrue(back.pinned)
        // An older widget decodes only the keys it knows.
        struct Old: Decodable { var name: String }
        XCTAssertEqual(try JSONDecoder().decode(Old.self, from: JSONEncoder().encode(back)).name, "p")
    }

    // MARK: Dragging

    func testDraggingAnUnpinnedPlaceAboveTheLastPinnedOnePinsIt() {
        let list = [place("p1", pinned: true), place("p2", pinned: true), place("a"), place("b")]
        // `b` dropped between p1 and p2.
        let moved = AppPlace.moving(list, from: IndexSet(integer: 3), to: 1)
        XCTAssertEqual(ids(moved), ["p1", "b", "p2", "a"])
        XCTAssertTrue(moved[1].pinned)
        XCTAssertEqual(moved.filter(\.pinned).count, 3)
    }

    func testDraggingAPinnedPlaceBelowAnUnpinnedOneUnpinsIt() {
        let list = [place("p1", pinned: true), place("p2", pinned: true), place("a"), place("b")]
        // `p1` dropped after `a`.
        let moved = AppPlace.moving(list, from: IndexSet(integer: 0), to: 3)
        XCTAssertEqual(ids(moved), ["p2", "a", "p1", "b"])
        XCTAssertFalse(moved[2].pinned)
        XCTAssertTrue(moved[0].pinned)
    }

    func testDroppingBetweenTheGroupsChangesNothing() {
        let list = [place("p1", pinned: true), place("p2", pinned: true), place("a"), place("b")]
        // `a` dropped right after the last pinned place: it was above and stays unpinned.
        let a = AppPlace.moving(list, from: IndexSet(integer: 2), to: 2)
        XCTAssertEqual(ids(a), ids(list))
        XCTAssertEqual(a.map(\.pinned), list.map(\.pinned))
        // `p1` dropped right before the first unpinned place: stays pinned, goes last of the pinned.
        let p = AppPlace.moving(list, from: IndexSet(integer: 0), to: 2)
        XCTAssertEqual(ids(p), ["p2", "p1", "a", "b"])
        XCTAssertEqual(p.map(\.pinned), [true, true, false, false])
    }

    func testDraggingWithinAGroupKeepsThePins() {
        let list = [place("p1", pinned: true), place("p2", pinned: true), place("a"), place("b"), place("c")]
        let moved = AppPlace.moving(list, from: IndexSet(integer: 4), to: 2)
        XCTAssertEqual(ids(moved), ["p1", "p2", "c", "a", "b"])
        XCTAssertEqual(moved.map(\.pinned), [true, true, false, false, false])
        let pins = AppPlace.moving(list, from: IndexSet(integer: 1), to: 0)
        XCTAssertEqual(ids(pins), ["p2", "p1", "a", "b", "c"])
        XCTAssertEqual(pins.map(\.pinned), [true, true, false, false, false])
    }

    func testTheFollowingPlaceStaysFirstAndIsNotCounted() {
        let list = [place("f", follows: true), place("a"), place("b")]
        // Dropped above the following place: it is still first, and nothing is pinned.
        let moved = AppPlace.moving(list, from: IndexSet(integer: 2), to: 0)
        XCTAssertEqual(ids(moved), ["f", "b", "a"])
        XCTAssertEqual(moved.map(\.pinned), [false, false, false])
        // A pinned place below the following one: dropping it above does not unpin it.
        let pinned = [place("f", follows: true), place("p", pinned: true), place("a")]
        let again = AppPlace.moving(pinned, from: IndexSet(integer: 1), to: 0)
        XCTAssertEqual(ids(again), ["f", "p", "a"])
        XCTAssertTrue(again[1].pinned)
    }

    func testMoveToTopAndBottom() {
        let list = [place("f", follows: true), place("p", pinned: true), place("a"), place("b")]
        // To the top: first after the following place, so above the pinned one: pinned.
        let top = AppPlace.moving(list, id: "b", toTop: true)
        XCTAssertEqual(ids(top), ["f", "b", "p", "a"])
        XCTAssertTrue(top[1].pinned)
        // A pinned place to the bottom: below the unpinned ones, so unpinned.
        let bottom = AppPlace.moving(list, id: "p", toTop: false)
        XCTAssertEqual(ids(bottom), ["f", "a", "b", "p"])
        XCTAssertFalse(bottom[3].pinned)
        // With nothing pinned the pins stay as they were.
        let plain = [place("a"), place("b"), place("c")]
        XCTAssertEqual(ids(AppPlace.moving(plain, id: "c", toTop: true)), ["c", "a", "b"])
        XCTAssertEqual(ids(AppPlace.moving(plain, id: "a", toTop: false)), ["b", "c", "a"])
        XCTAssertFalse(AppPlace.moving(plain, id: "a", toTop: false).contains(where: \.pinned))
        // Unknown id: unchanged.
        XCTAssertEqual(ids(AppPlace.moving(plain, id: "zz", toTop: true)), ["a", "b", "c"])
    }

    func testAllPinnedMovesToTheBottomStayPinned() {
        let list = [place("a", pinned: true), place("b", pinned: true)]
        let moved = AppPlace.moving(list, id: "a", toTop: false)
        XCTAssertEqual(ids(moved), ["b", "a"])
        XCTAssertEqual(moved.map(\.pinned), [true, true])
    }

    // MARK: Which place opens

    func testOpenOnChoosesThePlace() {
        let list = [place("f", follows: true), place("a"), place("b")]
        func open(_ choice: Preferences.OpenOn, fav: String? = nil, last: Int = 2) -> Int {
            PlaceChoice.openIndex(choice, places: list, favouriteID: fav, lastShown: last)
        }
        XCTAssertEqual(open(.lastViewed), 2)
        XCTAssertEqual(open(.favourite, fav: "a"), 1)
        XCTAssertEqual(open(.following), 0)
    }

    func testOpenOnFallsBackToTheLastPlaceViewed() {
        let list = [place("a"), place("b")]
        XCTAssertEqual(PlaceChoice.openIndex(.favourite, places: list, favouriteID: nil, lastShown: 1), 1)
        XCTAssertEqual(PlaceChoice.openIndex(.favourite, places: list, favouriteID: "gone", lastShown: 1), 1)
        XCTAssertEqual(PlaceChoice.openIndex(.following, places: list, favouriteID: nil, lastShown: 0), 0)
        XCTAssertEqual(PlaceChoice.openIndex(.lastViewed, places: list, favouriteID: nil, lastShown: 9), 1)
        XCTAssertEqual(PlaceChoice.openIndex(.lastViewed, places: list, favouriteID: nil, lastShown: -3), 0)
        XCTAssertEqual(PlaceChoice.openIndex(.favourite, places: [], favouriteID: "a", lastShown: 4), 0)
    }

    // MARK: Preferences and the widget

    func testPreferencesSavedBeforeHaveNoFavouriteAndOpenOnTheLastPlace() throws {
        let old = try JSONDecoder().decode(Preferences.self, from: Data(#"{"units":"metric"}"#.utf8))
        XCTAssertNil(old.favouritePlaceID)
        XCTAssertEqual(old.openOn, .lastViewed)
        XCTAssertEqual(old.units, .metric)
    }

    func testFavouriteAndOpenOnRoundTrip() throws {
        var prefs = Preferences()
        prefs.favouritePlaceID = "abc"
        prefs.openOn = .favourite
        let back = try JSONDecoder().decode(Preferences.self, from: JSONEncoder().encode(prefs))
        XCTAssertEqual(back.favouritePlaceID, "abc")
        XCTAssertEqual(back.openOn, .favourite)
        let unknown = try JSONDecoder().decode(Preferences.self, from: Data(#"{"openOn":"sideways"}"#.utf8))
        XCTAssertEqual(unknown.openOn, .lastViewed, "a choice this version lacks reads as the default")
    }

    func testTheWidgetShowsTheFavouriteWhenItHasNoChoiceOfItsOwn() {
        let list = [place("a"), place("b"), place("c")]
        func index(chosen: String? = nil, fav: String?) -> Int? {
            WidgetData.chosenIndex(places: list, chosenID: chosen, legacyIndex: nil, active: 0, favouriteID: fav)
        }
        XCTAssertEqual(index(fav: "c"), 2)
        XCTAssertEqual(index(chosen: "b", fav: "c"), 1, "its own choice wins")
        XCTAssertEqual(index(fav: nil), 0, "no favourite: the shown place")
        XCTAssertEqual(index(fav: "gone"), 0, "a deleted favourite: the shown place")
        XCTAssertEqual(index(chosen: "gone", fav: "c"), 2, "a deleted choice: the app's")
    }

    func testTheWidgetShowsTheFavouriteItIsGiven() throws {
        // The widget loads the preferences once and passes the favourite in.
        let defaults = try XCTUnwrap(UserDefaults(suiteName: "pinned-places-\(UUID().uuidString)"))
        let list = [place("a"), place("b")]
        XCTAssertEqual(WidgetData.chosenIndex(places: list, defaults: defaults, favouriteID: nil), 0)
        XCTAssertEqual(WidgetData.chosenIndex(places: list, defaults: defaults, favouriteID: "b"), 1)
    }
}
