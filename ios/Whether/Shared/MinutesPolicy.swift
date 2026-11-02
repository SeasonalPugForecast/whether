import Foundation

/// When Apple's minute nowcast (a metered WeatherKit call) is worth asking for.
nonisolated enum MinutesPolicy {
    /// The least time between two calls for one place, in seconds.
    static let askEvery: TimeInterval = 10 * 60
    /// How long minutes already held stay on screen without a new call, in seconds.
    static let keepFor: TimeInterval = 60 * 60

    /// Whether to call Apple now: only for the place shown (or the only place),
    /// when the forecast hints at rain within two hours (`wanted`, from the
    /// core's `minutesWanted`), and when the last call is at least ten minutes old.
    static func shouldAsk(
        isShown: Bool, onlyPlace: Bool, wanted: Bool, lastAskedAt: Int64?, now: Int64
    ) -> Bool {
        guard isShown || onlyPlace, wanted else { return false }
        guard let last = lastAskedAt else { return true }
        return TimeInterval(now - last) >= askEvery
    }

    /// Whether minutes fetched at `askedAt` may still be shown at `now`.
    static func keeps(askedAt: Int64?, now: Int64) -> Bool {
        guard let askedAt else { return false }
        return TimeInterval(now - askedAt) < keepFor
    }
}

/// One of Apple's minutes as the app keeps it, so a refresh that doesn't ask
/// Apple again can put the last minutes back into the new snapshot.
nonisolated struct KeptMinute: Codable, Sendable, Equatable {
    /// The minute's start, Unix seconds.
    var time: Int64
    /// Millimetres an hour.
    var mmPerHour: Float
}
