import XCTest

@testable import WhetherKit

/// Which views a rebuild makes, and how views follow the places when the list changes.
final class ViewRequestTests: XCTestCase {
    func testRequestsMerge() {
        let a = ViewRequest.places(["a"])
        XCTAssertEqual(a.merged(with: nil), a)
        XCTAssertEqual(a.merged(with: .places(["b"])), .places(["a", "b"]))
        XCTAssertEqual(a.merged(with: .all), .all)
        XCTAssertEqual(ViewRequest.all.merged(with: a), .all)
    }

    func testARequestCoversItsPlacesInTheirOrder() {
        let ids = ["a", "b", "c"]
        XCTAssertEqual(ViewRequest.all.indices(in: ids), [0, 1, 2])
        XCTAssertEqual(ViewRequest.places(["c", "a"]).indices(in: ids), [0, 2])
        XCTAssertEqual(ViewRequest.places(["gone"]).indices(in: ids), [])
        XCTAssertTrue(ViewRequest.places(["b"]).covers("b"))
        XCTAssertFalse(ViewRequest.places(["b"]).covers("a"))
    }

    func testViewsFollowTheirPlaces() {
        let views = ["view a", "view b", "view c"]
        let ids = ["a", "b", "c"]
        // Moved: each view goes with its place.
        let moved = ViewRequest.aligned(views, ids: ids, to: ["c", "a", "b"])
        XCTAssertEqual(moved.views, ["view c", "view a", "view b"])
        XCTAssertEqual(moved.missing, [])
        // Removed and added: the new place has none yet.
        let changed = ViewRequest.aligned(views, ids: ids, to: ["a", "new", "c"])
        XCTAssertEqual(changed.views, ["view a", nil, "view c"])
        XCTAssertEqual(changed.missing, [1])
        // The first views of all.
        XCTAssertEqual(ViewRequest.aligned([String](), ids: [], to: ["a", "b"]).missing, [0, 1])
    }
}
