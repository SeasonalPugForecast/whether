import Foundation

/// How the radar loop loads and steps through its frames.
enum RadarLoopRules {
    /// Ticks the latest frame stays on screen after the loop reaches it.
    static let hold = 3
    /// How long the map waits for the frame shown before adding the others anyway.
    static let latestWait: Duration = .seconds(2)
    /// How long the loop waits for the other frames' tiles before playing anyway.
    static let loopWait: Duration = .seconds(5)
    /// How old the frames get before the loop asks for them again: IEM makes a
    /// composite every five minutes.
    static let refreshAfter: TimeInterval = 300

    /// Whether frames asked for at `loadedAt` are due to be asked for again at `now`.
    static func isDue(loadedAt: Date?, now: Date) -> Bool {
        guard let loadedAt else { return true }
        return now.timeIntervalSince(loadedAt) >= refreshAfter
    }

    /// The frames added once frame `shown` of `count` is in, in order: the oldest first,
    /// as the loop plays them.
    static func deferred(count: Int, shown: Int) -> [Int] {
        (0..<count).filter { $0 != shown }
    }

    /// The step after `step` in a loop of `count` frames, and the frame it shows: one
    /// frame a tick, the latest held `hold` ticks longer, then the oldest again.
    static func next(after step: Int, count: Int) -> (step: Int, index: Int) {
        guard count > 0 else { return (0, 0) }
        let next = (step + 1) % (count + hold)
        return (next, min(next, count - 1))
    }

    /// The step a loop of `count` frames starts playing from, frame `index` shown. The
    /// first play goes on to the oldest frame (the latest was shown while the others
    /// loaded), so the loop plays through from the start; later ones go on from `index`,
    /// where it was paused or dragged to.
    static func start(index: Int, count: Int, firstPlay: Bool) -> Int {
        firstPlay ? count + hold - 1 : index
    }
}
