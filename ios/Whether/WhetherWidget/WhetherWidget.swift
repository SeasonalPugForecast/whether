import SwiftUI
import WidgetKit
import WhetherKit

/// Refreshes every half hour; WidgetKit may stretch that. A widget placed before
/// it was configurable has no stored choices: it gets the intent's defaults ("The
/// app's", the overview), which is what it always showed.
struct WeatherProvider: AppIntentTimelineProvider {
    func placeholder(in context: Context) -> WeatherWidgetEntry {
        WeatherWidgetEntry(date: .now, view: nil)
    }

    func snapshot(for configuration: WeatherWidgetIntent, in context: Context) async
        -> WeatherWidgetEntry
    {
        await entry(for: configuration)
    }

    func timeline(for configuration: WeatherWidgetIntent, in context: Context) async
        -> Timeline<WeatherWidgetEntry>
    {
        Timeline(
            entries: [await entry(for: configuration)],
            policy: .after(.now.addingTimeInterval(30 * 60)))
    }

    private func entry(for configuration: WeatherWidgetIntent) async -> WeatherWidgetEntry {
        await WidgetData.entry(
            configuredPlaceID: configuration.place?.id, content: configuration.show.content)
    }
}

struct WeatherWidget: Widget {
    var body: some WidgetConfiguration {
        AppIntentConfiguration(kind: "WhetherNow", intent: WeatherWidgetIntent.self, provider: WeatherProvider()) {
            entry in
            WidgetBody(entry: entry)
        }
        .configurationDisplayName("Weather")
        .description("Now, today's high and low, and the next hours. Touch and hold to choose the place and what it shows.")
        .supportedFamilies([
            .systemSmall, .systemMedium, .systemLarge, .accessoryRectangular, .accessoryCircular,
            .accessoryInline,
        ])
    }
}

/// The view for the widget's current family.
struct WidgetBody: View {
    let entry: WeatherWidgetEntry
    @Environment(\.widgetFamily) private var family

    var body: some View {
        WeatherWidgetView(entry: entry, family: family)
            .containerBackground(for: .widget) { Color(.systemBackground) }
            // The app's colors, from the shared preferences (resolved with the entry).
            .environment(\.colorTheme, entry.colorTheme)
    }
}

@main
struct WhetherWidgetBundle: WidgetBundle {
    var body: some Widget {
        WeatherWidget()
        RainLiveActivity()
    }
}
