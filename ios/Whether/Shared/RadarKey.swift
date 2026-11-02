import Foundation
import WhetherKit

extension Settings {
    /// The OpenWeather key that unlocks radar outside the NEXRAD composite
    /// (trimmed; nil without one). Its tile URLs hold the key, so they are
    /// never logged or shown.
    var radarKey: String? {
        #if DEBUG
            // Screenshots: `-fakeRadarKey` switches the UI as if a key were saved;
            // `RadarLoop` then gives the map a template that loads no tiles.
            if ProcessInfo.processInfo.arguments.contains("-fakeRadarKey") { return Self.fakeRadarKey }
        #endif
        guard let key = openweatherKey?.trimmingCharacters(in: .whitespacesAndNewlines), !key.isEmpty
        else { return nil }
        return key
    }

    #if DEBUG
        static let fakeRadarKey = "debug-fake-key"
    #endif

    /// Where the radar for `place` comes from (the core decides).
    func radarSource(for place: AppPlace) -> RadarSource {
        WhetherKit.radarSource(
            latitude: place.latitude, longitude: place.longitude, hasKey: radarKey != nil)
    }
}

/// What reloads a radar view: the place, where its radar comes from, and the key
/// (so a key saved in Settings takes effect). Only compared, never printed.
struct RadarTaskKey: Equatable {
    let place: AppPlace
    let source: RadarSource
    let key: String?

    init(place: AppPlace, settings: Settings) {
        self.place = place
        source = settings.radarSource(for: place)
        key = settings.radarKey
    }
}
