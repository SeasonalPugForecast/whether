import SwiftUI

/// The weather models' colors on the Storms tab, by the core's role key (`ecmwf`,
/// `gfs`, `gem`, `icon`, `other`). Every model color the tab draws comes from here.
enum StormModelColors {
    static func color(_ role: String) -> Color {
        switch role {
        case "ecmwf": .blue
        case "gfs": .orange
        case "gem": .green
        case "icon": .purple
        default: .gray
        }
    }
}
