import Foundation

/// How the radar loop loads and steps through its frames.
enum RadarLoopRules {
    /// How long the map waits for the frame shown before adding the others anyway.
    static let latestWait: Duration = .seconds(2)
    /// How long the loop waits for the other frames' tiles before playing anyway.
    static let loopWait: Duration = .seconds(5)

    /// The frames added once frame `shown` of `count` is in, in order: the oldest first,
    /// as the loop plays them.
    static func deferred(count: Int, shown: Int) -> [Int] {
        (0..<count).filter { $0 != shown }
    }
}
