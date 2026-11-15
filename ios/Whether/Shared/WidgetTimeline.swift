import CoreLocation
import Foundation
import WhetherKit

/// A forecast the widget fetched for itself (without the App Group, or when the
/// app's is old), kept beside the app's caches so every widget showing the place
/// reuses it until it is old too. The app never reads it.
struct WidgetForecast: Codable, Equatable, Sendable {
    /// The core's snapshot JSON.
    var snapshotJson: String
    /// Unix seconds of the fetch.
    var fetchedAt: Int64
    /// Where it was fetched for: a place that follows the phone moves.
    var latitude: Double
    var longitude: Double

    /// Whether it is still for `place`: the place has not moved 1 km or more since.
    func belongs(to place: AppPlace) -> Bool {
        !AppPlace.shouldMove(
            old: CLLocationCoordinate2D(latitude: latitude, longitude: longitude), new: place.coordinate)
    }
}

/// When the widget fetches, which forecast it shows, and the moments of its
/// timeline: pure logic, tested without WidgetKit.
extension WidgetData {
    /// A forecast younger than this is shown as it is; an older one is fetched again.
    static let freshFor: Int64 = 2 * 3600
    /// Entries in a timeline: now, then the next hours, all from the same forecast.
    static let entryCount = 6
    /// The soonest a timeline asks to be reloaded (the app also reloads it after
    /// each refresh).
    static let soonestReload: TimeInterval = 30 * 60

    /// Whether a forecast from `at` (Unix seconds) is fresh at `now`.
    static func isFresh(_ at: Int64?, now: Date) -> Bool {
        at.map { Int64(now.timeIntervalSince1970) - $0 < freshFor } ?? false
    }

    /// The newest forecast for `place`: the app's cache (`appSnapshot`, from
    /// `appAt`) or the widget's own, which counts only while the place is where it
    /// was fetched. `nil` with neither.
    static func newest(
        appSnapshot: String?, appAt: Int64?, widget: WidgetForecast?, place: AppPlace
    ) -> (json: String, at: Int64?)? {
        let own = widget.flatMap { $0.belongs(to: place) ? $0 : nil }
        switch (appSnapshot, own) {
        case let (app?, own?):
            return own.fetchedAt > (appAt ?? .min) ? (own.snapshotJson, own.fetchedAt) : (app, appAt)
        case let (app?, nil): return (app, appAt)
        case let (nil, own?): return (own.snapshotJson, own.fetchedAt)
        case (nil, nil): return nil
        }
    }

    /// The widget's own forecast for `place`, in `directory/widget`.
    static func forecastURL(for place: AppPlace, in directory: URL) -> URL {
        place.cacheURL(in: directory.appending(path: "widget", directoryHint: .isDirectory))
    }

    /// The timeline's moments: `now`, then the start of each next hour in
    /// `calendar` (the place's zone), `count` in all.
    static func entryDates(now: Date, count: Int = entryCount, calendar: Calendar) -> [Date] {
        guard count > 0 else { return [] }
        let hour = calendar.dateInterval(of: .hour, for: now)?.start ?? now
        return [now] + (1..<count).compactMap { calendar.date(byAdding: .hour, value: $0, to: hour) }
    }

    /// When to ask for a new timeline: when the forecast (from `dataAt`) turns
    /// old, but not before half an hour nor after two hours; the next hour's rain
    /// changes by the minute, so it asks again in half an hour.
    static func reloadDate(now: Date, dataAt: Int64?, content: WidgetContent) -> Date {
        let soonest = now.addingTimeInterval(soonestReload)
        guard content != .nextHour, let dataAt else { return soonest }
        let stale = Date(timeIntervalSince1970: TimeInterval(dataAt + freshFor))
        return min(max(stale, soonest), now.addingTimeInterval(TimeInterval(freshFor)))
    }

    /// The core's settings for a widget's view: the app's, with only what a
    /// widget shows (the next 12 hours, `content`'s days without their hours, no
    /// history), which keeps the view small. Fetches use the app's settings as they are.
    static func viewSettings(_ app: Settings, content: WidgetContent) -> Settings {
        var s = app
        // Now and the 12 hours after it (the large widget's next hours).
        s.hourlyHours = 13
        // Today (its high and low) at least.
        s.dailyDays = UInt8(clamping: max(content.days, 1))
        s.dayHours = false
        s.precipHours = 1
        s.history = false
        return s
    }
}
