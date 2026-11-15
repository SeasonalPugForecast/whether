import XCTest

@testable import WhetherKit

/// Metered fetches one at a time: each reads the call ledger the one before saved.
@MainActor
final class TaskChainTests: XCTestCase {
    /// A ledger kept as metered fetches keep it: read at the start, saved after a wait.
    private var saved = 0
    /// The work that started, in order.
    private var started: [String] = []

    private func countOneCall() async -> Int {
        let read = saved
        try? await Task.sleep(for: .milliseconds(20))
        saved = read + 1
        return saved
    }

    func testSideBySideTheLastSaveWins() async {
        saved = 0
        async let a = countOneCall()
        async let b = countOneCall()
        async let c = countOneCall()
        _ = await (a, b, c)
        XCTAssertEqual(saved, 1, "three calls, counted once: what the chain prevents")
    }

    func testInTheChainEveryCallCounts() async {
        saved = 0
        let chain = TaskChain()
        async let a = chain.run { await self.countOneCall() }
        async let b = chain.run { await self.countOneCall() }
        async let c = chain.run { await self.countOneCall() }
        let counts = await [a, b, c]
        XCTAssertEqual(saved, 3)
        XCTAssertEqual(counts.compactMap { $0 }.sorted(), [1, 2, 3])
    }

    /// A background refresh out of time cancels its fetches: the one running is
    /// told, and the ones waiting their turn are never made.
    func testCancellingTheCallerReachesTheWorkAndSkipsItsTurn() async {
        let chain = TaskChain()
        started = []
        let running = Task {
            await chain.run { () -> Bool in
                self.started.append("running")
                try? await Task.sleep(for: .seconds(5))
                return Task.isCancelled
            }
        }
        try? await Task.sleep(for: .milliseconds(50))
        let waiting = Task { await chain.run { () -> Bool in self.started.append("waiting"); return true } }
        try? await Task.sleep(for: .milliseconds(20))
        let after = Task { await chain.run { () -> Bool in self.started.append("after"); return true } }
        try? await Task.sleep(for: .milliseconds(50))
        waiting.cancel()
        running.cancel()
        let wasCancelled = await running.value
        XCTAssertEqual(wasCancelled, true, "the work running sees its caller's cancellation")
        let skipped = await waiting.value
        XCTAssertNil(skipped, "cancelled before its turn: never started")
        let ran = await after.value
        XCTAssertEqual(ran, true, "the chain goes on with the rest")
        XCTAssertEqual(started, ["running", "after"])
    }
}
