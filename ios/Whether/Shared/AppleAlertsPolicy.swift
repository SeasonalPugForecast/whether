import Foundation
import WhetherKit

/// When Apple Weather's alerts (a metered WeatherKit call) are asked for, and
/// how they become the core's alerts.
nonisolated enum AppleAlertsPolicy {
    /// The least time between two calls for one place, in seconds.
    static let askEvery: TimeInterval = 30 * 60
    /// How long alerts already held stay on screen without a new call, in seconds.
    static let keepFor: TimeInterval = 60 * 60
    /// Ids of Apple's alerts start with this (the core's `APPLE_ALERT_PREFIX`).
    static let idPrefix = "apple:"

    /// Whether to call Apple now: the switch is on, the place is outside the
    /// National Weather Service's coverage (which has its own alerts), and the
    /// last call for the place is at least 30 minutes old.
    static func shouldAsk(enabled: Bool, nwsCovers: Bool, lastAskedAt: Int64?, now: Int64) -> Bool {
        guard enabled, !nwsCovers else { return false }
        guard let last = lastAskedAt else { return true }
        return TimeInterval(now - last) >= askEvery
    }

    /// Whether alerts fetched at `askedAt` may still be shown at `now`.
    static func keeps(askedAt: Int64?, now: Int64) -> Bool {
        guard let askedAt else { return false }
        return TimeInterval(now - askedAt) < keepFor
    }

    /// The core's alert for one of Apple's. WeatherKit gives no id, start, end
    /// or description: the id is the source, event and area (the same alert
    /// keeps its id between calls), the title is the event, the text the area,
    /// it starts when Apple answered and is shown for `keepFor` after that.
    static func alert(
        summary: String, region: String?, severity: AlertSeverity, source: String,
        url: URL?, answeredAt: Int64
    ) -> ExternalAlert {
        let area = region?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        return ExternalAlert(
            id: "\(idPrefix)\(source)|\(summary)|\(area)", title: summary, summary: area,
            severity: severity, start: answeredAt, end: answeredAt + Int64(keepFor), source: source,
            url: url?.absoluteString)
    }
}

/// One of Apple's alerts as the app keeps it, so a refresh that doesn't ask
/// Apple again can put the last alerts back into the new snapshot.
nonisolated struct KeptAlert: Codable, Sendable, Equatable {
    var id: String
    var title: String
    var summary: String
    /// `minor`, `moderate`, `severe`, `extreme` or `unknown`.
    var severity: String
    var start: Int64
    var end: Int64
    var source: String
    var url: String?

    init(_ alert: ExternalAlert) {
        id = alert.id
        title = alert.title
        summary = alert.summary
        severity = Self.name(alert.severity)
        start = alert.start
        end = alert.end
        source = alert.source
        url = alert.url
    }

    /// The alert to merge into a snapshot.
    var external: ExternalAlert {
        ExternalAlert(
            id: id, title: title, summary: summary, severity: Self.severity(severity),
            start: start, end: end, source: source, url: url)
    }

    static func name(_ severity: AlertSeverity) -> String {
        switch severity {
        case .minor: "minor"
        case .moderate: "moderate"
        case .severe: "severe"
        case .extreme: "extreme"
        case .unknown: "unknown"
        }
    }

    static func severity(_ name: String) -> AlertSeverity {
        switch name {
        case "minor": .minor
        case "moderate": .moderate
        case "severe": .severe
        case "extreme": .extreme
        default: .unknown
        }
    }
}
