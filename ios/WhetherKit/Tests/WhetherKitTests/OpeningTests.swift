import XCTest

@testable import WhetherKit

/// What opening the app fetches.
final class OpeningTests: XCTestCase {
    func testOnlyPlacesWithOldOrNoForecastsAreDue() {
        let now: TimeInterval = 1_000_000
        let places: [(id: String, lastSuccess: Int64?)] = [
            ("fresh", 1_000_000 - 300), ("old", 1_000_000 - 700), ("never", nil), ("edge", 1_000_000 - 600),
        ]
        XCTAssertEqual(Freshness.due(places, now: now, after: 600), ["old", "never"])
        XCTAssertEqual(Freshness.due([], now: now, after: 600), [])
    }
}
