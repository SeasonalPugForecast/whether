#if DEBUG
    import SwiftUI
    import WidgetKit
    import WhetherKit

    /// Every widget size with the current forecast, for screenshots
    /// (`-show widgets`); debug builds only.
    struct WidgetGallery: View {
        let view: ViewModel?

        /// What the medium and large widgets show: `-widgetcontent nextHour|nextHours|today`
        /// (Edit Widget's "Show"), else the overview.
        private var content: WidgetContent {
            UserDefaults.standard.string(forKey: "widgetcontent").flatMap(WidgetContent.init(rawValue:))
                ?? .overview
        }

        private var entry: WeatherWidgetEntry {
            WeatherWidgetEntry(date: .now, view: view, places: 3, content: content)
        }

        /// The rain Live Activity's lock screen look, with a made-up shower.
        private var activity: some View {
            HStack(spacing: 12) {
                Image(systemName: "cloud.rain.fill")
                    .symbolRenderingMode(.palette)
                    .foregroundStyle(.white, Palette.rain)
                    .font(.title)
                VStack(alignment: .leading, spacing: 4) {
                    Text(view?.location.name ?? "").font(.caption).foregroundStyle(.secondary)
                    Text("Rain starting in ~10 min.").font(.headline)
                    RainBars(bars: [0, 0.05, 0.2, 0.35, 0.5, 0.45, 0.3, 0.1]).frame(height: 26)
                }
            }
            .padding()
            .frame(width: 364, alignment: .leading)
            .foregroundStyle(.white)
            .background(Color.black.opacity(0.75), in: .rect(cornerRadius: 22))
            .environment(\.colorScheme, .dark)
        }

        var body: some View {
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    HStack(spacing: 16) {
                        tile(.systemSmall, width: 170, height: 170)
                        VStack(spacing: 12) {
                            lock(.accessoryCircular, width: 72, height: 72)
                            lock(.accessoryInline, width: 160, height: 24)
                        }
                    }
                    tile(.systemMedium, width: 364, height: 170)
                    lock(.accessoryRectangular, width: 172, height: 76)
                    tile(.systemLarge, width: 364, height: 382)
                    activity
                }
                .padding()
            }
            .background(Color(white: 0.85))
        }

        private func tile(_ family: WidgetFamily, width: CGFloat, height: CGFloat) -> some View {
            WeatherWidgetView(entry: entry, family: family)
                .padding(16)
                .frame(width: width, height: height)
                .background(Color(.systemBackground), in: .rect(cornerRadius: 22))
        }

        /// Lock screen widgets: monochrome on a dark wallpaper.
        private func lock(_ family: WidgetFamily, width: CGFloat, height: CGFloat) -> some View {
            WeatherWidgetView(entry: entry, family: family)
                .foregroundStyle(.white)
                .frame(width: width, height: height)
                .padding(6)
                .background(Color(white: 0.2), in: .rect(cornerRadius: 12))
        }
    }
#endif
