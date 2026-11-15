import Foundation

/// Which places' views a rebuild is for: every place's, or those of some places
/// (by id). Asked for while a build runs, requests are merged and built after it.
nonisolated enum ViewRequest: Equatable, Sendable {
    case all
    case places(Set<String>)

    /// This request and `other` together (`nil`: none).
    func merged(with other: ViewRequest?) -> ViewRequest {
        switch (self, other) {
        case (_, nil): self
        case (.all, _), (_, .all?): .all
        case (.places(let a), .places(let b)?): .places(a.union(b))
        }
    }

    /// Whether the view of place `id` is asked for.
    func covers(_ id: String) -> Bool {
        switch self {
        case .all: true
        case .places(let ids): ids.contains(id)
        }
    }

    /// The indices in `ids` (the places' ids, in their order) of the views asked for.
    func indices(in ids: [String]) -> [Int] {
        ids.indices.filter { covers(ids[$0]) }
    }

    /// `views`, built for the places `ids`, arranged for the places `now` (by id,
    /// in that order), with the indices in `now` of the places that have none.
    static func aligned<View>(_ views: [View], ids: [String], to now: [String]) -> (
        views: [View?], missing: [Int]
    ) {
        var byID: [String: View] = [:]
        for (id, view) in zip(ids, views) { byID[id] = view }
        let arranged = now.map { byID[$0] }
        return (arranged, arranged.indices.filter { arranged[$0] == nil })
    }
}
