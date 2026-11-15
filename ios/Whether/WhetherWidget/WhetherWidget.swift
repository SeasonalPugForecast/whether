import SwiftUI
import WidgetKit
import WhetherKit

/// A timeline is an entry for now and one for each of the next hours, all from
/// the same forecast, so the hours move on without a reload; it asks for a new
/// one when the forecast turns old (`WidgetData.reloadDate`), and the app reloads
/// it after each refresh. A widget placed before a choice existed gets the
/// intent's default for it (`WeatherWidgetIntent`): the app's place and
/// temperature, the overview, and the sky behind.
struct WeatherProvider: AppIntentTimelineProvider {
    func placeholder(in context: Context) -> WeatherWidgetEntry {
        WeatherWidgetEntry(date: .now, view: nil)
    }

    func snapshot(for configuration: WeatherWidgetIntent, in context: Context) async
        -> WeatherWidgetEntry
    {
        await WidgetData.entry(setup: configuration.setup)
    }

    func timeline(for configuration: WeatherWidgetIntent, in context: Context) async
        -> Timeline<WeatherWidgetEntry>
    {
        let (entries, reload) = await WidgetData.timeline(setup: configuration.setup)
        return Timeline(entries: entries, policy: .after(reload))
    }
}

struct WeatherWidget: Widget {
    var body: some WidgetConfiguration {
        AppIntentConfiguration(kind: "WhetherNow", intent: WeatherWidgetIntent.self, provider: WeatherProvider()) {
            entry in
            WidgetBody(entry: entry)
        }
        .configurationDisplayName("Weather")
        .description("Now, today's high and low, the next hours and the days. Touch and hold to choose the place, what it shows and its background.")
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
            // The system leaves it out in StandBy and on a tinted Home Screen.
            .containerBackground(for: .widget) { WidgetBackground(entry: entry) }
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
