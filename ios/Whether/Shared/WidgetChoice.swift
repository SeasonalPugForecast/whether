import Foundation

/// The widget's data (`WidgetData.swift`) and, here, which saved place it
/// shows: pure logic, tested without WidgetKit.
enum WidgetData {
    /// The id of the place the widget shows, once its button picked one (else the app's).
    static let placeKey = "widgetPlaceID"
    /// Before ids: the chosen place's index. Read once, converted to `placeKey`, removed.
    static let legacyPlaceKey = "widgetPlace"

    /// The index of the saved place a widget was configured to show (Home Screen,
    /// Edit Widget). `nil` for "The app's" (`WidgetPlaceOption.appsID`), for no choice
    /// and for a place deleted since: those follow the app.
    static func configuredIndex(places: [AppPlace], configuredID: String?) -> Int? {
        guard let configuredID, configuredID != WidgetPlaceOption.appsID else { return nil }
        return places.firstIndex { $0.id == configuredID }
    }

    /// The index of the place to show: the one the widget was configured to
    /// (`configuredID`), else (the app's) `chosenID`'s, else (no id saved yet) the
    /// legacy index, else the favourite place, else the `active` one; `nil` with no
    /// places. A deleted place's id falls back to the app's.
    static func chosenIndex(
        places: [AppPlace], configuredID: String? = nil, chosenID: String?, legacyIndex: Int?,
        active: Int, favouriteID: String? = nil
    ) -> Int? {
        guard !places.isEmpty else { return nil }
        if let i = configuredIndex(places: places, configuredID: configuredID) { return i }
        if let chosenID {
            if let i = places.firstIndex(where: { $0.id == chosenID }) { return i }
        } else if let legacyIndex {
            return max(0, min(legacyIndex, places.count - 1))
        }
        if let favouriteID, let i = places.firstIndex(where: { $0.id == favouriteID }) { return i }
        return max(0, min(active, places.count - 1))
    }

    /// The id after the one at `index`, in `places` order (wrapping); `nil` with no places.
    static func nextID(places: [AppPlace], after index: Int?) -> String? {
        guard !places.isEmpty else { return nil }
        return places[((index ?? -1) + 1) % places.count].id
    }

    /// The chosen place's index from the shared defaults, converting a legacy
    /// index to its place's id (and removing it) on the way. A widget configured
    /// to a saved place (`configuredID`) shows it and leaves the app's choice alone.
    /// `favouriteID` is the app's favourite place (`Preferences.favouritePlaceID`),
    /// from preferences the caller has already loaded.
    static func chosenIndex(
        places: [AppPlace], defaults: UserDefaults, configuredID: String? = nil, favouriteID: String?
    ) -> Int? {
        if let i = configuredIndex(places: places, configuredID: configuredID) { return i }
        let legacy = defaults.object(forKey: legacyPlaceKey) as? Int
        let index = chosenIndex(
            places: places, chosenID: defaults.string(forKey: placeKey), legacyIndex: legacy,
            active: defaults.integer(forKey: "active"), favouriteID: favouriteID)
        if legacy != nil {
            if defaults.string(forKey: placeKey) == nil, let index {
                defaults.set(places[index].id, forKey: placeKey)
            }
            defaults.removeObject(forKey: legacyPlaceKey)
        }
        return index
    }
}

/// What the medium and large widgets show (Edit Widget, "Show"). Small and Lock
/// Screen widgets always show the conditions.
enum WidgetContent: String, CaseIterable, Sendable {
    /// Today's widget: now, the next hours (and, large, rain and five days).
    case overview
    /// The next hour: the rain minute by minute, and the hour after.
    case nextHour
    /// The next hours: more of them, with their chance of rain.
    case nextHours
    /// Today's high and low, with the day's chance of rain, wind and sun.
    case today

    /// Hours the hour strip lists, in a medium or large widget (`nextHour` shows one).
    func hours(large: Bool) -> Int {
        switch self {
        case .overview: large ? 6 : 5
        case .nextHour: 1
        case .nextHours: large ? 12 : 6
        case .today: 0
        }
    }
}

/// One entry of the widget's place picker.
struct WidgetPlaceOption: Equatable, Sendable {
    /// The id of "The app's": the place the app opens on (the favourite, else the
    /// one shown), switched by the widget's button. No saved place has this id.
    static let appsID = "@app"
    var id: String
    var name: String
    var detail: String?

    static let apps = WidgetPlaceOption(
        id: appsID, name: "The app's", detail: "The favourite place, else the one the app shows")

    /// "The app's" first, then every saved place, in the app's order.
    static func options(places: [AppPlace]) -> [WidgetPlaceOption] {
        [apps] + places.map { WidgetPlaceOption(id: $0.id, name: $0.name, detail: $0.detail) }
    }

    /// The options for `ids` that exist, in `ids`' order (an entity lookup).
    static func options(for ids: [String], places: [AppPlace]) -> [WidgetPlaceOption] {
        let all = options(places: places)
        return ids.compactMap { id in all.first { $0.id == id } }
    }
}
