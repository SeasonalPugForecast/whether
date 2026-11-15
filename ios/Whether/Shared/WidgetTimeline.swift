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
    /// The same for "The next hour": its rain changes by the minute, so each of its
    /// half-hourly reloads (`soonestReload`) finds the last forecast old and fetches.
    static let nextHourFreshFor: Int64 = 20 * 60
    /// Entries in a timeline: now, then the next hours, all from the same forecast.
    static let entryCount = 6
    /// The soonest a timeline asks to be reloaded (the app also reloads it after
    /// each refresh).
    static let soonestReload: TimeInterval = 30 * 60

    /// How long a forecast stays fresh for a widget showing `content`.
    static func freshFor(_ content: WidgetContent) -> Int64 {
        content == .nextHour ? nextHourFreshFor : freshFor
    }

    /// Whether a forecast from `at` (Unix seconds) is fresh at `now` for a widget
    /// showing `content`: shown as it is, else fetched again.
    static func isFresh(_ at: Int64?, now: Date, content: WidgetContent = .overview) -> Bool {
        at.map { Int64(now.timeIntervalSince1970) - $0 < freshFor(content) } ?? false
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

    /// When a snapshot's current conditions were observed (Unix seconds), from
    /// the core's snapshot JSON; `nil` without them.
    static func observedAt(snapshotJson: String) -> Int64? {
        struct Snapshot: Decodable {
            struct Current: Decodable { var time: Int64 }
            var current: Current?
        }
        return (try? JSONDecoder().decode(Snapshot.self, from: Data(snapshotJson.utf8)))?.current?.time
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
    /// changes by the minute, so it asks again in half an hour (and fetches then,
    /// `nextHourFreshFor`).
    static func reloadDate(now: Date, dataAt: Int64?, content: WidgetContent) -> Date {
        let soonest = now.addingTimeInterval(soonestReload)
        guard content != .nextHour, let dataAt else { return soonest }
        let stale = Date(timeIntervalSince1970: TimeInterval(dataAt + freshFor))
        return min(max(stale, soonest), now.addingTimeInterval(TimeInterval(freshFor)))
    }

    /// Whether an entry at `date` leads with the hour's forecast: the current
    /// conditions, observed at `observedAt` (else, unknown, fetched then), are an
    /// hour old or more by then. A station's observation is often most of an hour
    /// old when it is fetched, and an unchanged forecast keeps its old one.
    static func leadsWithHour(at date: Date, observedAt: Int64?) -> Bool {
        observedAt.map { date.timeIntervalSince1970 - TimeInterval($0) >= 3600 } ?? false
    }

    /// The Lock Screen gauge's scale: today's low to high, stretched to the
    /// temperature now when it is outside them, never empty.
    static func gaugeRange(value: Float, low: Float, high: Float) -> ClosedRange<Float> {
        let lowest = min(low, high, value), highest = max(low, high, value)
        return lowest < highest ? lowest...highest : lowest...(lowest + 1)
    }

    /// The sky for an hour (a core icon name), as the core chooses one for the
    /// current conditions: storms, snow, rain and fog first, then cloud, day or night.
    static func sky(icon: String, isDay: Bool) -> String {
        switch WeatherSymbols.group(icon) {
        case .storm: "storm"
        case .snow: "snow"
        case .drizzle, .rain, .heavyRain: "rain"
        case .fog: "fog"
        case .cloudy: isDay ? "cloudy_day" : "cloudy_night"
        case .clear, .partlyCloudy, .other: isDay ? "clear_day" : "clear_night"
        }
    }

    /// The core's settings for a widget's view: the app's, leading with the
    /// temperature `temperature` chooses, and only what a widget shows (the
    /// next 12 hours, `content`'s days without their hours, no history), which
    /// keeps the view small. Fetches use the app's settings as they are.
    static func viewSettings(
        _ app: Settings, content: WidgetContent, temperature: WidgetTemperature
    ) -> Settings {
        var s = app
        s.feelsLike = temperature.feelsLike(app: app.feelsLike)
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

/// What a widget leads with at one moment: the current conditions, or (an hour
/// or more after they were observed) the current hour's forecast. The core's
/// texts either way.
struct WidgetNow: Equatable, Sendable {
    var temperature: ViewModel.Quantity
    var conditionLabel: String
    var icon: String
    var high: ViewModel.Quantity
    var low: ViewModel.Quantity
    /// The sky behind (`ViewModel.theme`'s names).
    var sky: String

    /// `nil` without current conditions (nothing to show yet).
    init?(_ view: ViewModel, fromHour: Bool) {
        guard let current = view.current else { return nil }
        high = current.high
        low = current.low
        if fromHour, let hour = view.hourly?.hours.first {
            temperature = hour.temperature
            conditionLabel = hour.conditionLabel
            icon = hour.icon
            sky = WidgetData.sky(icon: hour.icon, isDay: hour.isDay)
        } else {
            temperature = current.temperature
            conditionLabel = current.conditionLabel
            icon = current.icon
            sky = view.theme
        }
    }
}
