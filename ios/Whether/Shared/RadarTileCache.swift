import Foundation

/// Repainted radar tiles in memory, keyed by ramp and URL.
///
/// The loop's frames name their composite's time (IEM's archive layer), so a tile's URL
/// always holds the same echoes and IEM lets clients keep it for 14 days; a layer
/// relative to now (`nexrad-n0q-900913-m05m`) gets five minutes. A tile is kept as long
/// as the server says, one radar update when it says nothing (`unstatedLifetime`), and
/// never longer than an hour (`maxLifetime`): by then its frame has left the loop.
/// Failed responses are never kept. `NSCache` is thread-safe and empties itself under
/// memory pressure.
nonisolated final class RadarTileCache: @unchecked Sendable {
    /// How long a tile is kept when the server gives no lifetime: IEM's composite
    /// updates every five minutes.
    static let unstatedLifetime: TimeInterval = 300
    /// The longest a tile is kept: the loop spans half an hour.
    static let maxLifetime: TimeInterval = 60 * 60

    private final class Entry {
        let data: Data
        let expires: Date
        init(data: Data, expires: Date) {
            self.data = data
            self.expires = expires
        }
    }

    private let tiles = NSCache<NSString, Entry>()

    init(costLimit: Int = 48 << 20) {
        tiles.totalCostLimit = costLimit
    }

    /// The tile kept for `key`, or `nil` when there is none or it went stale at `now`
    /// (a stale tile is dropped).
    func tile(for key: String, now: Date) -> Data? {
        guard let entry = tiles.object(forKey: key as NSString) else { return nil }
        guard now < entry.expires else {
            tiles.removeObject(forKey: key as NSString)
            return nil
        }
        return entry.data
    }

    /// Keeps `data` for `key` for `lifetime` seconds from `now`; a lifetime of zero
    /// or less keeps nothing.
    func store(_ data: Data, for key: String, lifetime: TimeInterval, now: Date) {
        guard lifetime > 0 else { return }
        tiles.setObject(
            Entry(data: data, expires: now.addingTimeInterval(lifetime)), forKey: key as NSString,
            cost: data.count)
    }

    /// How long a tile fetched with HTTP `status` and `Cache-Control` header may be
    /// kept: nothing for a failed response or `no-store`/`no-cache`, `max-age` when the
    /// server gives one (else `unstatedLifetime`), and never more than `maxLifetime`.
    static func lifetime(status: Int?, cacheControl: String?) -> TimeInterval {
        guard succeeded(status: status) else { return 0 }
        var lifetime = unstatedLifetime
        for directive in (cacheControl ?? "").lowercased().split(separator: ",") {
            let part = directive.trimmingCharacters(in: .whitespaces)
            if part == "no-store" || part == "no-cache" { return 0 }
            if part.hasPrefix("max-age="), let seconds = TimeInterval(part.dropFirst("max-age=".count)) {
                lifetime = max(seconds, 0)
            }
        }
        return min(lifetime, maxLifetime)
    }

    /// Whether a response with HTTP `status` (none for a non-HTTP response) is a tile.
    static func succeeded(status: Int?) -> Bool { status.map { (200..<300).contains($0) } ?? true }
}

/// The tiles in flight for a group of tile overlays, to tell when the ones the map asked
/// for are in: `settled` runs once, on the main actor, when none has been in flight for a
/// moment (the map asks for a view's tiles together), or when `settle()` stops waiting.
/// A failed tile counts as in. Thread-safe: MapKit asks for tiles on its own threads.
nonisolated final class TileLoads: @unchecked Sendable {
    /// How long nothing may be in flight before the group counts as settled.
    static let quiet: Duration = .milliseconds(200)

    private let lock = NSLock()
    private var inFlight = 0
    /// Tiles asked for so far: one asked for during the quiet moment starts it over.
    private var asked = 0
    private var done = false
    private let settled: @MainActor @Sendable () -> Void

    init(settled: @escaping @MainActor @Sendable () -> Void) {
        self.settled = settled
    }

    /// Whether the group has settled.
    var isSettled: Bool { lock.withLock { done } }

    /// A tile was asked for.
    func begin() {
        lock.withLock {
            inFlight += 1
            asked += 1
        }
    }

    /// A tile came in or failed.
    func end() {
        let quietFrom: Int? = lock.withLock {
            inFlight = max(inFlight - 1, 0)
            return inFlight == 0 && !done ? asked : nil
        }
        guard let quietFrom else { return }
        Task { @MainActor in
            try? await Task.sleep(for: Self.quiet)
            if self.finish(quietFrom: quietFrom) { self.settled() }
        }
    }

    /// Settles now, whatever is in flight: the fallback when tiles are slow or the map
    /// asks for none.
    @MainActor func settle() {
        if finish(quietFrom: nil) { settled() }
    }

    /// Marks the group settled, once; with `quietFrom`, only if no tile was asked for
    /// since then.
    private func finish(quietFrom: Int?) -> Bool {
        lock.withLock {
            guard !done else { return false }
            if let quietFrom, inFlight > 0 || asked != quietFrom { return false }
            done = true
            return true
        }
    }
}
