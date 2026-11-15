import XCTest

@testable import WhetherKit

/// The radar loop: which frames load after the one shown, and when a frame's tiles
/// count as in.
final class RadarLoopTests: XCTestCase {
    func testTheOtherFramesLoadOldestFirst() {
        XCTAssertEqual(RadarLoopRules.deferred(count: 6, shown: 5), [0, 1, 2, 3, 4])
        XCTAssertEqual(RadarLoopRules.deferred(count: 6, shown: 2), [0, 1, 3, 4, 5], "dragged to")
        XCTAssertEqual(RadarLoopRules.deferred(count: 1, shown: 0), [], "OpenWeather's one frame")
        XCTAssertEqual(RadarLoopRules.deferred(count: 0, shown: 0), [], "no radar")
    }

    /// Waits until `condition` holds or `seconds` pass.
    @MainActor private func wait(_ seconds: Double = 2, until condition: () -> Bool) async {
        let end = Date().addingTimeInterval(seconds)
        while !condition() && Date() < end { try? await Task.sleep(for: .milliseconds(20)) }
    }

    @MainActor func testAFrameIsInOnceItsTilesAre() async {
        let settled = Count()
        let loads = TileLoads { settled.value += 1 }
        loads.begin()
        loads.begin()
        loads.end()
        try? await Task.sleep(for: .milliseconds(400))
        XCTAssertEqual(settled.value, 0, "a tile still in flight")
        loads.end()
        await wait { settled.value > 0 }
        XCTAssertEqual(settled.value, 1)
        XCTAssertTrue(loads.isSettled)
        loads.begin()
        loads.end()
        try? await Task.sleep(for: .milliseconds(400))
        XCTAssertEqual(settled.value, 1, "once")
    }

    @MainActor func testTilesAskedForTogetherCountAsOneLoad() async {
        let settled = Count()
        let loads = TileLoads { settled.value += 1 }
        // The first tile comes from memory at once; the map asks for the next ones just after.
        loads.begin()
        loads.end()
        try? await Task.sleep(for: .milliseconds(50))
        loads.begin()
        try? await Task.sleep(for: .milliseconds(400))
        XCTAssertEqual(settled.value, 0, "the second tile is still on its way")
        loads.end()
        await wait { settled.value > 0 }
        XCTAssertEqual(settled.value, 1)
    }

    @MainActor func testTheWaitForTilesRunsOut() {
        let settled = Count()
        let loads = TileLoads { settled.value += 1 }
        loads.begin()
        loads.settle()
        XCTAssertEqual(settled.value, 1, "the fallback")
        loads.settle()
        loads.end()
        XCTAssertEqual(settled.value, 1, "once")
    }
}

/// How often a group of tiles settled.
@MainActor private final class Count {
    var value = 0
}
