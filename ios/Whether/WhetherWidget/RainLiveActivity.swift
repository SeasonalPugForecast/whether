import ActivityKit
import SwiftUI
import WidgetKit

/// "Rain starting in ~10 min." on the lock screen and in the Dynamic Island, with
/// the next hours' graph; the app starts it when rain is near and ends it after.
struct RainLiveActivity: Widget {
    /// Apple's credit under the graph while its minutes are shown.
    @ViewBuilder
    private func credit(_ state: RainAttributes.ContentState) -> some View {
        if let credit = state.credit {
            Text(credit).font(.caption2).foregroundStyle(.secondary)
        }
    }

    /// The app's color theme, from the shared preferences, read each time a view is made
    /// so a change in the app shows on the next update.
    private static func colors() -> ColorTheme { Preferences.load().colorTheme() }

    var body: some WidgetConfiguration {
        ActivityConfiguration(for: RainAttributes.self) { context in
            let colors = Self.colors()
            HStack(alignment: .center, spacing: 12) {
                Image(systemName: "cloud.rain.fill")
                    .symbolRenderingMode(.palette)
                    .foregroundStyle(.white, colors.rain)
                    .font(.title)
                VStack(alignment: .leading, spacing: 4) {
                    Text(context.attributes.place).font(.caption).foregroundStyle(.secondary)
                    Text(context.state.text).font(.headline)
                    RainBars(bars: context.state.bars).frame(height: 26)
                    credit(context.state)
                }
            }
            .padding()
            .activityBackgroundTint(Color.black.opacity(0.45))
            .activitySystemActionForegroundColor(.white)
            .environment(\.colorTheme, colors)
        } dynamicIsland: { context in
            let colors = Self.colors()
            return DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    Image(systemName: "cloud.rain.fill").font(.title2).foregroundStyle(colors.rain)
                }
                DynamicIslandExpandedRegion(.trailing) {
                    Text(context.attributes.place).font(.caption).foregroundStyle(.secondary)
                }
                DynamicIslandExpandedRegion(.bottom) {
                    VStack(alignment: .leading, spacing: 6) {
                        Text(context.state.text).font(.headline)
                        RainBars(bars: context.state.bars).frame(height: 30)
                        credit(context.state)
                    }
                    .environment(\.colorTheme, colors)
                }
            } compactLeading: {
                Image(systemName: "cloud.rain.fill").foregroundStyle(colors.rain)
            } compactTrailing: {
                RainBars(bars: Array(context.state.bars.prefix(8))).frame(width: 30, height: 14)
                    .environment(\.colorTheme, colors)
            } minimal: {
                Image(systemName: "cloud.rain.fill").foregroundStyle(colors.rain)
            }
        }
    }
}
