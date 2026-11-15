import XCTest

@testable import WhetherKit

/// Metered fetches one at a time: each reads the call ledger the one before saved.
@MainActor
final class TaskChainTests: XCTestCase {
    /// A ledger kept as metered fetches keep it: read at the start, saved after a wait.
    private var saved = 0

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
        XCTAssertEqual(counts.sorted(), [1, 2, 3])
    }
}
