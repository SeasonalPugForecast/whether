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

    // MARK: The following place and its fix

    func testAFollowingPlaceShownIsFetchedFirstWithTheOthers() {
        let plan = OpeningFetch(due: ["me", "a", "b"], following: "me", shown: "me")
        XCTAssertTrue(plan.waitsForFix)
        XCTAssertEqual(plan.atOnce, [], "nothing before the fix")
        XCTAssertEqual(plan.afterFix(moved: true), ["me", "a", "b"], "one batch, the moved place in it")
        XCTAssertEqual(plan.afterFix(moved: false), ["me", "a", "b"])
        XCTAssertEqual(plan.afterFix(moved: nil), ["me", "a", "b"], "the wait ended: where it was")
        XCTAssertEqual(plan.moved(true), ["me"])
        XCTAssertEqual(plan.moved(nil), [])
    }

    func testAFollowingPlaceThatMovedIsFetchedEvenWhenItsDataIsFresh() {
        let plan = OpeningFetch(due: ["a"], following: "me", shown: "me")
        XCTAssertEqual(plan.afterFix(moved: true), ["me", "a"])
        XCTAssertEqual(plan.afterFix(moved: false), ["a"])
        XCTAssertEqual(plan.afterFix(moved: nil), ["a"])
    }

    func testAFollowingPlaceOffScreenDoesNotHoldUpTheOthers() {
        let plan = OpeningFetch(due: ["me", "a"], following: "me", shown: "a")
        XCTAssertFalse(plan.waitsForFix)
        XCTAssertEqual(plan.atOnce, ["a"])
        XCTAssertEqual(plan.afterFix(moved: false), ["me"])
        XCTAssertEqual(OpeningFetch(due: ["a"], following: "me", shown: "a").afterFix(moved: false), [])
    }

    func testWithoutAFollowingPlaceTheDuePlacesAreFetchedAtOnce() {
        let plan = OpeningFetch(due: ["a", "b"], following: nil, shown: "a")
        XCTAssertFalse(plan.waitsForFix)
        XCTAssertEqual(plan.atOnce, ["a", "b"])
        XCTAssertEqual(plan.afterFix(moved: false), [])
        XCTAssertEqual(plan.moved(true), [])
    }

    // MARK: Apple's minutes for the place shown

    func testTheShownPlaceAsksAppleItselfWhenOpeningFetchesNothingForIt() {
        let fresh = OpeningFetch(due: ["b"], following: nil, shown: "a")
        XCTAssertFalse(fresh.fetches("a", moved: false), "a's forecast is fresh: minutes on their own")
        XCTAssertTrue(fresh.fetches("b", moved: false))
        let following = OpeningFetch(due: [], following: "me", shown: "me")
        XCTAssertFalse(following.fetches("me", moved: false))
        XCTAssertFalse(following.fetches("me", moved: nil))
        XCTAssertTrue(following.fetches("me", moved: true), "moved: asked with its new forecast")
    }

    func testARunningRefreshSaysWhichPlacesItStillFetches() {
        var queue = RefreshQueue(only: ["a", "b"])
        XCTAssertTrue(queue.willFetch("a"))
        XCTAssertFalse(queue.willFetch("c"))
        XCTAssertEqual(queue.next(among: ["a", "b", "c"]), ["a", "b"])
        XCTAssertTrue(queue.willFetch("a"), "being fetched")
        queue.request(["c"])
        XCTAssertTrue(queue.willFetch("c"), "joined")
        XCTAssertEqual(queue.next(among: ["a", "b", "c"]), ["c"])
        XCTAssertFalse(queue.willFetch("a"), "fetched: a page shown now asks on its own")
        XCTAssertTrue(RefreshQueue(only: nil).willFetch("any"))
    }

    @MainActor
    func testAFixAfterTheWaitStillComes() async {
        let slow = Task { () -> Int in
            try? await Task.sleep(for: .milliseconds(300))
            return 7
        }
        let early = await slow.value(within: 0.05)
        XCTAssertNil(early, "the wait ends first")
        let late = await slow.value
        XCTAssertEqual(late, 7, "the task went on")
        let quick = Task { () -> Int in 3 }
        let answered = await quick.value(within: 5)
        XCTAssertEqual(answered, 3)
    }
}
