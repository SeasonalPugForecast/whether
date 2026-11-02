import CoreLocation
import Foundation
import WeatherKit
import WhetherKit

/// Apple Weather's alerts for a place (WeatherKit), where Apple has them: the
/// countries outside the National Weather Service's coverage. Apple
/// receives the coordinates only, rounded to about 1 km (`AppPlace.outgoingDecimals`).
enum AppleAlerts {
    /// The alerts in force at `place`: empty when there are none or Apple has no
    /// alerts there; `nil` when the call failed (offline, without the
    /// capability), so the caller keeps what it had.
    static func alerts(for place: AppPlace, at now: Int64) async -> [ExternalAlert]? {
        let location = place.outgoingLocation
        let found: [WeatherAlert]?
        do {
            found = try await WeatherService.shared.weather(for: location, including: .alerts)
        } catch {
            return nil
        }
        return (found ?? []).map { alert in
            AppleAlertsPolicy.alert(
                summary: alert.summary, region: alert.region, severity: severity(alert.severity),
                source: alert.source, url: alert.detailsURL, answeredAt: now)
        }
    }

    private static func severity(_ severity: WeatherSeverity) -> AlertSeverity {
        switch severity {
        case .minor: .minor
        case .moderate: .moderate
        case .severe: .severe
        case .extreme: .extreme
        default: .unknown
        }
    }
}
