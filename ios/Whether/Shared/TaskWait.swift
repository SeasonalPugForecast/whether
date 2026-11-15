import Foundation

extension Task where Failure == Never, Success: Sendable {
    /// The task's value, or `nil` when it has none after `seconds`: the task goes
    /// on, and its value can still be awaited.
    @MainActor func value(within seconds: Double) async -> Success? {
        let wait = Wait<Success>()
        return await withCheckedContinuation { continuation in
            wait.continuation = continuation
            wait.timer = Task<Void, Never> { @MainActor in
                try? await Task<Never, Never>.sleep(for: .seconds(seconds))
                wait.answer(nil)
            }
            Task<Void, Never> { @MainActor in wait.answer(await self.value) }
        }
    }
}

/// What `value(within:)` waits for: whichever comes first answers.
@MainActor private final class Wait<Value: Sendable> {
    var continuation: CheckedContinuation<Value?, Never>?
    var timer: Task<Void, Never>?

    func answer(_ value: Value?) {
        guard let continuation else { return }
        self.continuation = nil
        timer?.cancel()
        continuation.resume(returning: value)
    }
}
