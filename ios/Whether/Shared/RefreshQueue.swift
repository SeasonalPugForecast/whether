import Foundation

/// Which places a running refresh still has to fetch. A refresh asked for while
/// another runs is not dropped: its ids join the running one, which fetches them
/// before it ends, even when it already fetched them once (a place that moved,
/// or was just added during a refresh of the others).
nonisolated struct RefreshQueue: Equatable {
    /// The ids to fetch; `nil` for every place.
    private(set) var only: Set<String>?
    /// Fetched, or being fetched (`fetching`).
    private var fetched: Set<String> = []
    /// The batch under way.
    private var fetching: Set<String> = []
    private var requested: Set<String> = []

    init(only: Set<String>?) {
        self.only = only
    }

    /// Joins a refresh asked for while this one runs: `ids` are fetched, again
    /// when this one fetched them already (or is fetching them) and they are in
    /// `again` (`nil`: all of them; a place that moved); `nil` (every place) widens
    /// a targeted refresh to the places not fetched yet.
    mutating func request(_ ids: Set<String>?, again: Set<String>? = nil) {
        guard let ids else {
            only = nil
            return
        }
        let again = again ?? ids
        requested.formUnion(ids.filter { again.contains($0) || !fetched.contains($0) })
    }

    /// The ids to fetch next out of the places there are now, in their order:
    /// those requested since, and those wanted and not fetched yet. Empty when
    /// the refresh is done.
    mutating func next(among places: [String]) -> [String] {
        let due = places.filter { id in
            requested.contains(id) || (!fetched.contains(id) && (only?.contains(id) ?? true))
        }
        requested = []
        fetched.formUnion(due)
        fetching = Set(due)
        return due
    }

    /// Whether the place `id` is being fetched, or will be before the refresh
    /// ends: what it asks of Apple then is asked with its forecast.
    func willFetch(_ id: String) -> Bool {
        fetching.contains(id) || requested.contains(id) || (!fetched.contains(id) && (only?.contains(id) ?? true))
    }
}

/// What opening the app fetches, and when, around the location fix that may
/// move the following place: `due` the places whose data is old, `following`
/// the place that follows the phone, `shown` the one on screen.
nonisolated struct OpeningFetch: Equatable {
    var due: Set<String>
    var following: String?
    var shown: String?

    /// Whether the fetch waits for the fix (briefly) before it starts: the place
    /// shown follows the phone, and is fetched first, where the phone is, rather
    /// than after the others (or where it was, and then again).
    var waitsForFix: Bool { following != nil && following == shown }

    /// Fetched at once, side by side with the location read.
    var atOnce: Set<String> { waitsForFix ? [] : due.subtracting(following.map { [$0] } ?? []) }

    /// Fetched once the fix is in, `moved` saying whether the following place
    /// moved (`nil`: the wait for it ended first, and it is fetched where it was
    /// when due). With `waitsForFix` the others come with it, in one batch.
    func afterFix(moved: Bool?) -> Set<String> {
        var ids = waitsForFix ? due.subtracting(following.map { [$0] } ?? []) : []
        if let following, moved == true || due.contains(following) { ids.insert(following) }
        return ids
    }

    /// Whether opening fetches the place `id` (and asks Apple with its forecast),
    /// the fix in or the wait over.
    func fetches(_ id: String, moved: Bool?) -> Bool {
        atOnce.contains(id) || afterFix(moved: moved).contains(id)
    }

    /// The following place when it `moved`: fetched again where the phone is
    /// when a refresh fetched it where it was (one that runs, or once the fix
    /// came after the wait).
    func moved(_ moved: Bool?) -> Set<String> {
        moved == true ? following.map { [$0] } ?? [] : []
    }
}

/// Which places a refresh on opening the app fetches.
nonisolated enum Freshness {
    /// The ids of `places` (with when each was last fetched, Unix seconds) whose
    /// forecast is more than `after` seconds old at `now`, or was never fetched.
    static func due(
        _ places: [(id: String, lastSuccess: Int64?)], now: TimeInterval, after: TimeInterval
    ) -> Set<String> {
        Set(places.filter { place in
            place.lastSuccess.map { now - Double($0) > after } ?? true
        }.map(\.id))
    }
}
