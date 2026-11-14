import SwiftUI
import WhetherKit

/// The coming storms at the place shown, several weather models side by side: what
/// each brings, how well they agree, the ensembles' spread and how the runs moved.
/// Loads only while the tab shows.
struct StormsTab: View {
    @Bindable var store: WeatherStore
    @State private var storms = StormsStore()
    @Environment(\.colorTheme) private var colors

    private var place: AppPlace? {
        store.places.indices.contains(store.active) ? store.places[store.active] : store.places.first
    }

    /// The core's settings: units, clock and dates (Open-Meteo needs no key).
    private var settings: Settings { store.preferences.settings(withKey: false) }

    var body: some View {
        Group {
            if let view = storms.view {
                ScrollViewReader { proxy in
                    ScrollView {
                        StormsContent(
                            view: view, stale: storms.stale, failure: storms.failure,
                            loading: storms.loading
                        ) { await storms.loadPastRuns(for: $0) }
                        .padding(.horizontal, 16)
                        .padding(.top, 12)
                        .padding(.bottom, 40)
                    }
                    .task {
                        #if DEBUG
                            // Screenshots: `-scrollTo storms-runs` (or `storms-plumes`,
                            // `storms-credits`) scrolls once the storms are laid out.
                            let args = ProcessInfo.processInfo.arguments
                            guard let i = args.firstIndex(of: "-scrollTo"), i + 1 < args.count else { return }
                            try? await Task.sleep(for: .milliseconds(600))
                            proxy.scrollTo(args[i + 1], anchor: .top)
                        #endif
                    }
                }
            } else if let failure = storms.failure {
                ContentUnavailableView {
                    Label(failure, systemImage: "hurricane")
                } description: {
                    Text("The storm outlook couldn't be fetched.")
                } actions: {
                    Button("Try again") { Task { await load() } }
                        .buttonStyle(.bordered)
                    if let detail = storms.failureDetail {
                        DisclosureGroup("Details") { Text(detail).font(.caption) }
                            .padding(.horizontal, 32)
                    }
                }
            } else {
                VStack(spacing: 12) {
                    ProgressView()
                    Text("Fetching the storm outlook…")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .accessibilityElement(children: .combine)
            }
        }
        .background {
            if store.preferences.background == .sky, let view = store.view {
                SkyBackground(theme: view.theme)
            }
        }
        .environment(
            \.skyCard,
            store.preferences.background == .sky ? store.view.map { colors.card($0.theme) } : nil)
        .task(id: place) { await load() }
        .onChange(of: settings) { storms.rebuild(settings: settings) }
        .task {
            // The steps' "Now" and the events' days move on with the clock.
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(300))
                storms.rebuild()
            }
        }
    }

    private func load() async {
        guard let place else { return }
        await storms.load(place: place, settings: settings)
    }
}
