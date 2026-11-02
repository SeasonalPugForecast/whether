import Foundation

/// Which places a running refresh still has to fetch. A refresh asked for while
/// another runs is not dropped: its ids join the running one, which fetches them
/// before it ends, even when it already fetched them once (a place that moved,
/// or was just added during a refresh of the others).
nonisolated struct RefreshQueue: Equatable {
    /// The ids to fetch; `nil` for every place.
    private(set) var only: Set<String>?
    private var fetched: Set<String> = []
    private var requested: Set<String> = []

    init(only: Set<String>?) {
        self.only = only
    }

    /// Joins a refresh asked for while this one runs: `ids` are fetched (again, if
    /// need be); `nil` (every place) widens a targeted refresh to the places not
    /// fetched yet.
    mutating func request(_ ids: Set<String>?) {
        if let ids {
            requested.formUnion(ids)
        } else {
            only = nil
        }
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
        return due
    }
}
