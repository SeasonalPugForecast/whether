import Foundation
import WidgetKit
import WhetherKit

/// What a widget shows at one moment.
struct WeatherWidgetEntry: TimelineEntry {
    let date: Date
    /// The shown place's view, or `nil` with nothing to show yet.
    let view: ViewModel?
    /// Saved places (a button switches between them when there are several).
    var places = 1
    /// What the medium and large sizes show.
    var content = WidgetContent.overview
    /// Whether the place is the app's (so the button that switches it shows).
    var followsApp = true
    /// The app's color theme, which the widget extension paints with.
    var colorTheme = ColorPresets.whether
}

/// The widget's data: the app's cached forecast for its shown place, or (without
/// the shared App Group, or when the cache is over two hours old) a fresh
/// Open-Meteo fetch that is shown but never written back.
extension WidgetData {
    /// The saved places, in `places.json` order.
    static func places() -> [AppPlace] {
        (try? Data(contentsOf: SharedContainer.directory.appending(path: "places.json")))
            .flatMap { try? JSONDecoder().decode([AppPlace].self, from: $0) } ?? []
    }

    /// The widget's entry. `configuredPlaceID` is the place it was set to show, else
    /// (`nil`, or `WidgetPlaceOption.appsID`) the app's.
    static func entry(
        now: Date = .now, configuredPlaceID: String? = nil, content: WidgetContent = .overview
    ) async -> WeatherWidgetEntry {
        let dir = SharedContainer.directory
        let places = places()
        // Loaded once: the favourite and the settings both come from them.
        let preferences = Preferences.load()
        // No places yet: the widget asks to open the app (nothing is fetched).
        guard
            let index = chosenIndex(
                places: places, defaults: SharedContainer.defaults, configuredID: configuredPlaceID,
                favouriteID: preferences.favouritePlaceID)
        else {
            return WeatherWidgetEntry(
                date: now, view: nil, places: 0, content: content, colorTheme: preferences.colorTheme(now: now))
        }
        let followsApp = configuredIndex(places: places, configuredID: configuredPlaceID) == nil
        let place = places[index]
        // The widget never uses a key (it is not metered): none is read.
        var settings = preferences.settings(withKey: false)
        // The legacy name is read as is: only the app moves cache files.
        let cache = place.existingCacheURL(in: dir)
            .flatMap { try? Data(contentsOf: $0) }
            .flatMap { try? JSONSerialization.jsonObject(with: $0) as? [String: Any] }
        var snapshot = cache?["snapshotJson"] as? String
        var lastSuccess = (cache?["lastSuccess"] as? NSNumber)?.int64Value
        let fresh = lastSuccess.map { now.timeIntervalSince1970 - Double($0) < 2 * 3600 } ?? false
        if snapshot == nil || !fresh {
            // Never metered: the widget does not touch the app's One Call budget.
            settings.oneCall = false
            let ledger = FileManager.default.temporaryDirectory.appending(path: "widget-ledger.json")
            if let outcome = try? await fetch(
                place: place.corePlace(now: now), settings: settings,
                cached: Cached(snapshotJson: nil, lastModified: nil, usageJson: nil),
                now: Int64(now.timeIntervalSince1970), host: URLSessionHost(ledgerURL: ledger))
            {
                snapshot = outcome.snapshotJson
                lastSuccess = Int64(now.timeIntervalSince1970)
            }
        }
        let entry = LocationEntry(
            place: place.corePlace(now: now), snapshotJson: snapshot, lastSuccess: lastSuccess,
            error: nil, refreshing: false)
        let view = try? ViewModel.build(locations: [entry], active: 0, settings: settings, now: now)
        return WeatherWidgetEntry(
            date: now, view: view?.current == nil ? nil : view, places: places.count, content: content,
            followsApp: followsApp, colorTheme: preferences.colorTheme(now: now))
    }
}
