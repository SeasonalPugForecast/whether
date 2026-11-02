import Foundation

extension Preferences {
    /// Which place the app shows when it opens.
    enum OpenOn: String, Codable, CaseIterable, Identifiable {
        /// The one shown when the app was last used.
        case lastViewed
        /// The favourite (starred) place.
        case favourite
        /// The place that follows the phone.
        case following
        var id: Self { self }

        var title: String {
            switch self {
            case .lastViewed: "The last place viewed"
            case .favourite: "The favourite place"
            case .following: "The place that follows me"
            }
        }
    }
}

/// Which place is shown first: pure rules, tested without the app.
enum PlaceChoice {
    /// The index of the place the app opens on. A favourite or following place
    /// that does not exist (deleted, or none) falls back to `lastShown`, which is
    /// clamped to `places`; 0 with no places.
    static func openIndex(
        _ openOn: Preferences.OpenOn, places: [AppPlace], favouriteID: String?, lastShown: Int
    ) -> Int {
        let last = max(0, min(lastShown, places.count - 1))
        switch openOn {
        case .lastViewed: return last
        case .favourite:
            return favouriteID.flatMap { id in places.firstIndex { $0.id == id } } ?? last
        case .following:
            return places.firstIndex(where: \.follows) ?? last
        }
    }
}
