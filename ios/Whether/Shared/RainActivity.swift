import ActivityKit
import SwiftUI

/// The rain Live Activity (lock screen and Dynamic Island) while rain is near:
/// the place, and the nowcast's sentence and graph as they change.
nonisolated struct RainAttributes: ActivityAttributes {
    nonisolated struct ContentState: Codable, Hashable {
        /// "Rain starting in ~10 min."
        var text: String
        /// Intensity per step, 0…1 (a third is light).
        var bars: [Float]
        /// "Apple Weather" while the minutes are Apple's (WeatherKit asks for its
        /// credit wherever its data shows). Optional, so activities already
        /// running from an older build still decode.
        var credit: String?
    }

    /// The place's name, for display.
    var place: String
    /// The place's id, which says which place the activity is for. Optional, so
    /// activities already running from an older build still decode (they have
    /// none, and are ended and started again; see `RainActivityMatch`).
    var placeID: String?
}

/// The nowcast as small bars.
struct RainBars: View {
    let bars: [Float]
    @Environment(\.colorTheme) private var colors

    var body: some View {
        GeometryReader { geo in
            HStack(alignment: .bottom, spacing: 1) {
                ForEach(bars.indices, id: \.self) { i in
                    Capsule()
                        .fill(colors.rain)
                        .frame(height: max(1.5, geo.size.height * CGFloat(min(max(bars[i], 0), 1))))
                }
            }
            .frame(maxHeight: .infinity, alignment: .bottom)
        }
        .accessibilityHidden(true)
    }
}
