import Foundation

/// Runs work one piece at a time, in the order it is asked for: each waits for
/// the one before it to end.
@MainActor final class TaskChain {
    private var last: Task<Void, Never>?

    /// `work`'s value once the work asked for before it has ended. Cancelling the
    /// caller cancels `work`; work whose caller is cancelled before its turn never
    /// starts, and answers `nil`.
    func run<T: Sendable>(_ work: @escaping @MainActor @Sendable () async -> T) async -> T? {
        let previous = last
        let run = Task { () -> T? in
            // Its turn comes in order, cancelled or not: the next waits for this one.
            await previous?.value
            guard !Task.isCancelled else { return nil }
            return await work()
        }
        last = Task { _ = await run.value }
        return await withTaskCancellationHandler {
            await run.value
        } onCancel: {
            run.cancel()
        }
    }
}
