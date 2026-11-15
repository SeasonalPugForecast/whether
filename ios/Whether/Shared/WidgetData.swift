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
    /// The sky, or the plain background.
    var backdrop = WidgetBackdrop.sky
    /// The place follows the phone (an arrow beside its name).
    var followsPhone = false
    /// The current conditions are an hour old or more by `date`: the big number
    /// is the hour's forecast.
    var leadsWithHour = false

    /// What the widget leads with; `nil` with nothing to show.
    var now: WidgetNow? { view.flatMap { WidgetNow($0, fromHour: leadsWithHour) } }
}

/// How a widget was set up in Edit Widget.
struct WidgetSetup: Sendable {
    /// The place it was set to show, else (`nil`, or `WidgetPlaceOption.appsID`) the app's.
    var placeID: String?
    var content = WidgetContent.overview
    var backdrop = WidgetBackdrop.sky
    var temperature = WidgetTemperature.app
}

/// The widget's data: the app's cached forecast for its shown place, or (without
/// the shared App Group, or when the cache is over two hours old) a forecast the
/// widget fetches itself and keeps for the other widgets (`WidgetForecast`),
/// never in the app's cache.
extension WidgetData {
    /// The saved places, in `places.json` order.
    static func places() -> [AppPlace] {
        (try? Data(contentsOf: SharedContainer.directory.appending(path: "places.json")))
            .flatMap { try? JSONDecoder().decode([AppPlace].self, from: $0) } ?? []
    }

    /// The widget's entry for now (Edit Widget's preview, "Will it rain?").
    static func entry(now: Date = .now, setup: WidgetSetup = WidgetSetup()) async -> WeatherWidgetEntry {
        await timeline(now: now, setup: setup, count: 1).entries.first ?? WeatherWidgetEntry(date: now, view: nil)
    }

    /// The widget's entries, `now` and the next hours from the same forecast, and
    /// when to ask for new ones.
    static func timeline(
        now: Date = .now, setup: WidgetSetup, count: Int = entryCount
    ) async -> (entries: [WeatherWidgetEntry], reload: Date) {
        let dir = SharedContainer.directory
        let places = places()
        // Loaded once: the favourite and the settings both come from them.
        let preferences = Preferences.load()
        let colorTheme = preferences.colorTheme(now: now)
        // No places yet: the widget asks to open the app (nothing is fetched).
        guard
            let index = chosenIndex(
                places: places, defaults: SharedContainer.defaults, configuredID: setup.placeID,
                favouriteID: preferences.favouritePlaceID)
        else {
            let entry = WeatherWidgetEntry(
                date: now, view: nil, places: 0, content: setup.content, colorTheme: colorTheme,
                backdrop: setup.backdrop)
            return ([entry], reloadDate(now: now, dataAt: nil, content: setup.content))
        }
        let followsApp = configuredIndex(places: places, configuredID: setup.placeID) == nil
        let place = places[index]
        // The widget never uses a key (it is not metered): none is read.
        let settings = preferences.settings(withKey: false)
        let forecast = await forecast(for: place, of: places, settings: settings, in: dir, now: now)
        let observedAt = forecast.flatMap { observedAt(snapshotJson: $0.json) } ?? forecast?.at
        let viewSettings = viewSettings(settings, content: setup.content, temperature: setup.temperature)
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: place.timeZone) ?? .current
        let entries = entryDates(now: now, count: count, calendar: calendar).map { date in
            let entry = LocationEntry(
                place: place.corePlace(now: date), snapshotJson: forecast?.json,
                lastSuccess: forecast?.at, error: nil, refreshing: false)
            let view = try? ViewModel.build(locations: [entry], active: 0, settings: viewSettings, now: date)
            return WeatherWidgetEntry(
                date: date, view: view?.current == nil ? nil : view, places: places.count,
                content: setup.content, followsApp: followsApp, colorTheme: colorTheme,
                backdrop: setup.backdrop, followsPhone: place.follows,
                leadsWithHour: leadsWithHour(at: date, observedAt: observedAt))
        }
        return (entries, reloadDate(now: now, dataAt: forecast?.at, content: setup.content))
    }

    /// The newest forecast for `place`: the app's cache or the widget's own while
    /// fresh, else one fetched now (shared by every widget asking at once and kept
    /// for the others), else the newest there is, however old.
    private static func forecast(
        for place: AppPlace, of places: [AppPlace], settings: Settings, in dir: URL, now: Date
    ) async -> (json: String, at: Int64?)? {
        // The legacy name is read as is: only the app moves cache files.
        let cache = place.existingCacheURL(in: dir)
            .flatMap { try? Data(contentsOf: $0) }
            .flatMap { try? JSONSerialization.jsonObject(with: $0) as? [String: Any] }
        let url = forecastURL(for: place, in: dir)
        let own = (try? Data(contentsOf: url)).flatMap { try? JSONDecoder().decode(WidgetForecast.self, from: $0) }
        let newest = newest(
            appSnapshot: cache?["snapshotJson"] as? String,
            appAt: (cache?["lastSuccess"] as? NSNumber)?.int64Value, widget: own, place: place)
        if let newest, isFresh(newest.at, now: now) { return newest }
        guard let fetched = await WidgetFetches.shared.fetch(place, settings: settings, now: now) else {
            return newest
        }
        if let data = try? JSONEncoder().encode(fetched) {
            try? FileManager.default.createDirectory(
                at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            try? data.write(to: url, options: .atomic)
        }
        removeOrphans(in: url.deletingLastPathComponent(), keeping: places, dir: dir)
        return (fetched.snapshotJson, fetched.fetchedAt)
    }

    /// Deletes the widget's forecasts for places no longer saved.
    private static func removeOrphans(in folder: URL, keeping places: [AppPlace], dir: URL) {
        let kept = Set(places.map { forecastURL(for: $0, in: dir).lastPathComponent })
        let files = (try? FileManager.default.contentsOfDirectory(at: folder, includingPropertiesForKeys: nil)) ?? []
        for file in files where !kept.contains(file.lastPathComponent) {
            try? FileManager.default.removeItem(at: file)
        }
    }
}

/// The widget's own fetches: one per place at a time, however many widgets ask
/// for it at once (WidgetKit asks for every widget's timeline together). On the
/// main actor, as the places it fetches for are in the app.
@MainActor final class WidgetFetches {
    static let shared = WidgetFetches()
    private var running: [String: Task<WidgetForecast?, Never>] = [:]

    func fetch(_ place: AppPlace, settings: Settings, now: Date) async -> WidgetForecast? {
        if let task = running[place.id] { return await task.value }
        let task = Task { await Self.fetchNow(place, settings: settings, now: now) }
        running[place.id] = task
        let result = await task.value
        running[place.id] = nil
        return result
    }

    private static func fetchNow(_ place: AppPlace, settings: Settings, now: Date) async -> WidgetForecast? {
        var settings = settings
        // Never metered: the widget does not touch the app's One Call budget.
        settings.oneCall = false
        let ledger = FileManager.default.temporaryDirectory.appending(path: "widget-ledger.json")
        guard
            let outcome = try? await WhetherKit.fetch(
                place: place.corePlace(now: now), settings: settings.forBackground(),
                cached: Cached(snapshotJson: nil, lastModified: nil, usageJson: nil),
                now: Int64(now.timeIntervalSince1970), host: URLSessionHost(ledgerURL: ledger))
        else { return nil }
        return WidgetForecast(
            snapshotJson: outcome.snapshotJson, fetchedAt: Int64(now.timeIntervalSince1970),
            latitude: place.latitude, longitude: place.longitude)
    }
}
