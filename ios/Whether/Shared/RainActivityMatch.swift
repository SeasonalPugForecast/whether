import Foundation

/// Which running rain Live Activity is the one for a place. Activities are keyed
/// by the place's id, so two places with the same name, or a place renamed, are
/// not mixed up. Pure logic, apart from ActivityKit, for the tests.
nonisolated enum RainActivityMatch {
    /// What the controller knows of a running activity.
    struct Running: Equatable {
        /// ActivityKit's id for the activity.
        var id: String
        /// The place id it was started for (`nil` when an older build started it).
        var placeID: String?
        /// The place name it shows (attributes cannot change once started).
        var name: String
    }

    /// The activity to keep updating (the first for this place and name) and the
    /// ones to end. An activity from an older build (no `placeID`) never matches,
    /// so it is ended and a fresh one started.
    static func plan(running: [Running], placeID: String, name: String) -> (keep: String?, end: [String]) {
        let keep = running.first { $0.placeID == placeID && $0.name == name }?.id
        return (keep, running.filter { $0.id != keep }.map(\.id))
    }
}
