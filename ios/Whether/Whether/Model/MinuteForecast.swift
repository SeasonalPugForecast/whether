import CoreLocation
import Foundation
import WeatherKit
import WhetherKit

/// Apple's minute-by-minute precipitation for the next hour (WeatherKit), where
/// Apple has it (the US, Canada, the UK, Ireland and a few more).
enum MinuteForecast {
    /// The next hour's minutes at `place`; `nil` where WeatherKit has none, or
    /// when it fails (without the capability, offline).
    static func minutes(for place: AppPlace) async -> [MinuteStep]? {
        // Apple gets the place at about 1 km, not its full coordinates.
        let location = place.outgoingLocation
        guard let forecast = try? await WeatherService.shared.weather(for: location, including: .minute)
        else { return nil }
        return forecast.forecast.map { minute in
            // An intensity is a speed: 1 m/s is 3,600,000 mm an hour.
            let rate = minute.precipitationIntensity.converted(to: .metersPerSecond).value * 3_600_000
            return MinuteStep(time: Int64(minute.date.timeIntervalSince1970), mmPerHour: Float(rate))
        }
    }

    /// Apple's mark and the page that lists its data sources, which its terms
    /// require wherever WeatherKit data is shown.
    static func attribution() async -> WeatherAttribution? {
        try? await WeatherService.shared.attribution
    }
}
