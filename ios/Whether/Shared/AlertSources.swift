import Foundation
import WhetherKit

/// Where weather alerts can come from, to know whether notifications are worth
/// asking for.
enum AlertSources {
    /// Whether alerts can arrive for any of `places`: One Call brings them
    /// everywhere, the National Weather Service for US places, and Apple
    /// Weather where `appleAlerts` is on (`false` until then).
    static func alertsPossible(places: [AppPlace], oneCall: Bool, appleAlerts: Bool) -> Bool {
        oneCall || appleAlerts
            || places.contains { nwsCovers(latitude: $0.latitude, longitude: $0.longitude) }
    }
}
