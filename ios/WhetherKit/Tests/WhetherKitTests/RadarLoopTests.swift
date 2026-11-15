import XCTest

@testable import WhetherKit

/// The radar loop: which frames load after the one shown, how it steps, and when a
/// frame's tiles count as in.
final class RadarLoopTests: XCTestCase {
    func testTheOtherFramesLoadOldestFirst() {
        XCTAssertEqual(RadarLoopRules.deferred(count: 6, shown: 5), [0, 1, 2, 3, 4])
        XCTAssertEqual(RadarLoopRules.deferred(count: 6, shown: 2), [0, 1, 3, 4, 5], "dragged to")
        XCTAssertEqual(RadarLoopRules.deferred(count: 1, shown: 0), [], "OpenWeather's one frame")
        XCTAssertEqual(RadarLoopRules.deferred(count: 0, shown: 0), [], "no radar")
    }

    func testTheFramesAreAskedForAgainOnceACompositeOld() {
        let loaded = Date(timeIntervalSince1970: 1_800_000_000)
        XCTAssertTrue(RadarLoopRules.isDue(loadedAt: nil, now: loaded), "never asked")
        XCTAssertFalse(RadarLoopRules.isDue(loadedAt: loaded, now: loaded.addingTimeInterval(299)))
        XCTAssertTrue(RadarLoopRules.isDue(loadedAt: loaded, now: loaded.addingTimeInterval(300)))
        XCTAssertTrue(
            RadarLoopRules.isDue(loadedAt: loaded, now: loaded.addingTimeInterval(3_600)), "back after an hour")
    }

    /// Plays `ticks` steps from `step`: the frames shown.
    private func play(from step: Int, count: Int, ticks: Int) -> [Int] {
        var step = step
        var shown: [Int] = []
        for _ in 0..<ticks {
            let next = RadarLoopRules.next(after: step, count: count)
            step = next.step
            shown.append(next.index)
        }
        return shown
    }

    func testTheFirstPlayRunsFromTheOldestFrame() {
        // The latest frame (5) shows while the others load; then the loop starts over.
        let start = RadarLoopRules.start(index: 5, count: 6, firstPlay: true)
        XCTAssertEqual(
            play(from: start, count: 6, ticks: 12), [0, 1, 2, 3, 4, 5, 5, 5, 5, 0, 1, 2],
            "through once, the latest held, and again")
    }

    func testPlayingAgainGoesOnFromTheFrameShown() {
        let start = RadarLoopRules.start(index: 2, count: 6, firstPlay: false)
        XCTAssertEqual(play(from: start, count: 6, ticks: 5), [3, 4, 5, 5, 5])
        // Paused on the latest: it holds before the loop starts over.
        let latest = RadarLoopRules.start(index: 5, count: 6, firstPlay: false)
        XCTAssertEqual(play(from: latest, count: 6, ticks: 4), [5, 5, 5, 0])
        XCTAssertEqual(RadarLoopRules.next(after: 0, count: 0).index, 0, "no frames")
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
