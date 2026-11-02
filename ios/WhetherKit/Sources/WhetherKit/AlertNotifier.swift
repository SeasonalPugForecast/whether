import Foundation

/// Which alerts deserve a notification (as `whether-notify` does it): a
/// new alert, a stronger level, or an end an hour or more later than the one
/// last notified. Other updates (reworded text, small time shifts) stay quiet.
public struct AlertNotifier: Sendable {
    /// What was last notified about an alert.
    public struct Notified: Codable, Equatable, Sendable {
        public var level: Int
        public var end: Int64
    }

    /// Notified alerts by key (`place|id`, or `place|title|period` without an id).
    public private(set) var notified: [String: Notified]

    public init(notified: [String: Notified] = [:]) {
        self.notified = notified
    }

    /// `Level` names from mild to severe.
    static let levels = ["good", "fair", "moderate", "poor", "very_poor", "extreme"]

    static func rank(_ level: String) -> Int { levels.firstIndex(of: level) ?? 0 }

    /// The key of `alert` at `place`.
    public static func key(_ alert: ViewModel.Alert, place: String) -> String {
        if let id = alert.id { return "\(place)|\(id)" }
        return "\(place)|\(alert.title)|\(alert.period)"
    }

    /// The alerts of `place` to notify about now, remembering them; alerts that
    /// have ended by `now` (Unix seconds) are forgotten.
    public mutating func notices(
        for alerts: [ViewModel.Alert], place: String, now: Int64
    ) -> [ViewModel.Alert] {
        notified = notified.filter { $0.value.end > now }
        var out: [ViewModel.Alert] = []
        for alert in alerts where alert.end > now {
            let key = Self.key(alert, place: place)
            let level = Self.rank(alert.level)
            let changed: Bool
            if let last = notified[key] {
                if alert.isApple {
                    // Apple gives no end: ours moves with every call, so only a
                    // worse level is news. The stored end follows it,
                    // so a lasting alert is not forgotten and told again.
                    changed = level > last.level
                    if !changed { notified[key] = Notified(level: last.level, end: alert.end) }
                } else {
                    changed = level > last.level || alert.end >= last.end + 3_600
                }
            } else {
                changed = true
            }
            if changed {
                out.append(alert)
                notified[key] = Notified(level: level, end: alert.end)
            }
        }
        return out
    }
}
