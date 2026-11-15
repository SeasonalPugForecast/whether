import Foundation

/// Runs work one piece at a time, in the order it is asked for: each waits for
/// the one before it to end.
@MainActor final class TaskChain {
    private var last: Task<Void, Never>?

    func run<T: Sendable>(_ work: @escaping @MainActor @Sendable () async -> T) async -> T {
        let previous = last
        let run = Task { () -> T in
            await previous?.value
            return await work()
        }
        last = Task { _ = await run.value }
        return await run.value
    }
}
