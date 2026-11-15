import SwiftUI

/// A day's low–high span on the period's range (both 0…1), with the normal
/// low and high as ticks when history is on.
struct RangeBar: View {
    @Environment(\.colorTheme) private var colors
    let low: Float
    let high: Float
    var normalLow: Float? = nil
    var normalHigh: Float? = nil

    var body: some View {
        GeometryReader { geo in
            let w = geo.size.width
            let start = CGFloat(min(max(low, 0), 1)) * w
            let span = max(CGFloat(min(max(high, 0), 1)) * w - start, 6)
            ZStack(alignment: .leading) {
                Capsule().fill(.quaternary)
                Capsule()
                    .fill(
                        LinearGradient(
                            colors: [colors.color(.tempRangeLow), colors.color(.tempRangeHigh)],
                            startPoint: .leading, endPoint: .trailing)
                    )
                    .frame(width: span)
                    .offset(x: start)
                if let normalLow { tick(normalLow, width: w, height: geo.size.height) }
                if let normalHigh { tick(normalHigh, width: w, height: geo.size.height) }
            }
        }
        .accessibilityHidden(true)
    }

    /// A normal's tick at `f` (0…1) of the bar.
    private func tick(_ f: Float, width w: CGFloat, height: CGFloat) -> some View {
        Capsule()
            .fill(.white.opacity(0.9))
            .frame(width: 2, height: height + 6)
            .offset(x: CGFloat(f) * w - 1)
    }
}
