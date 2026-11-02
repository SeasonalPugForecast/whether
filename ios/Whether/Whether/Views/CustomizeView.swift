import SwiftUI
import WhetherKit

/// Every editor in one place: the tabs and the cards on them, what the cards
/// show, the radar, the places, and "Restore all layouts". Each row opens the
/// editor that touching and holding the card opens. Settings → Display →
/// Customize, and the sheet of a card's "Customize…" when it has no editor of its own.
struct CustomizeView: View {
    @Bindable var store: WeatherStore
    @State private var showPlaces = false
    @State private var confirmingRestore = false
    /// Screenshots: `-customize hourtiles` and `-customize detailcards` open those editors.
    @State private var showHourTiles = false
    @State private var showDetailCards = false

    private var feelsLeads: Bool { store.view?.feelsLikeLeads ?? false }

    /// "3 of 4": how many of a list show.
    private func count<Choice>(_ list: [Choice], _ shown: (Choice) -> Bool) -> String {
        "\(list.filter(shown).count) of \(list.count)"
    }

    var body: some View {
        Form {
            Section {
                link("Tabs", "rectangle.split.3x1", count(store.preferences.tabs, \.shown)) {
                    TabsEditor(tabs: $store.preferences.tabs)
                }
            } header: {
                Text("The tabs")
            }
            Section {
                link("Now tab", "thermometer.sun", count(store.preferences.sections, \.shown)) {
                    SectionsEditor(store: store)
                }
                link("24-hour rows", "clock", nil) {
                    MeasuresEditor(
                        kind: .rows, measures: $store.preferences.hourRows, feelsLeads: feelsLeads)
                }
                link("Detail cards", "square.grid.2x2", count(store.preferences.detailCards, \.shown)) {
                    detailCardsEditor
                }
            } header: {
                Text("Now")
            }
            Section {
                link("Hourly tab", "clock.fill", count(store.preferences.hourlyCards, \.shown)) {
                    CardsEditor(items: $store.preferences.hourlyCards, title: "Hourly tab")
                }
                link("Hourly graph", "chart.xyaxis.line", nil) {
                    MeasuresEditor(
                        kind: .graph, measures: $store.preferences.graphMeasures,
                        options: $store.preferences.graphOptions, feelsLeads: feelsLeads)
                }
                link("Hour tiles", "square.grid.3x3", count(store.preferences.hourTiles, \.shown)) {
                    HourTilesEditor(tiles: $store.preferences.hourTiles, feelsLeads: feelsLeads)
                }
                link("Sun & moon", "sun.horizon", count(store.preferences.sunItems, \.shown)) {
                    SunMoonEditor(items: $store.preferences.sunItems)
                }
            } header: {
                Text("Hourly")
            }
            Section {
                link("Days tab", "calendar", count(store.preferences.daysCards, \.shown)) {
                    CardsEditor(items: $store.preferences.daysCards, title: "Days tab")
                }
                link("Day rows", "list.bullet.rectangle", nil) {
                    DayRowsEditor(
                        rows: $store.preferences.dayRowItems, details: $store.preferences.dayDetailItems)
                }
            } header: {
                Text("Days")
            }
            Section {
                link("Radar", "globe.americas", nil) { RadarOptionsEditor(options: $store.preferences.radar) }
            } header: {
                Text("Radar")
            }
            Section {
                Button {
                    showPlaces = true
                } label: {
                    HStack {
                        Label("Places", systemImage: "mappin.and.ellipse").foregroundStyle(.primary)
                        Spacer()
                        Text("\(store.places.count)").foregroundStyle(.secondary)
                        Image(systemName: "chevron.right")
                            .font(.footnote.weight(.semibold))
                            .foregroundStyle(.tertiary)
                    }
                }
                Picker(selection: $store.preferences.openOn) {
                    ForEach(Preferences.OpenOn.allCases) { Text($0.title).tag($0) }
                } label: {
                    Label("Open on", systemImage: "arrow.up.forward.app")
                }
            } header: {
                Text("Places")
            } footer: {
                Text("Pin, favourite and order the places in the list; the app opens on the one Open on says.")
            }
            Section {
                Button("Restore all layouts", role: .destructive) { confirmingRestore = true }
            } footer: {
                Text("Puts the tabs, the cards, what they show, their order and the graph's labels and scales back to the defaults. Units, places, sources, the radar's options and the other settings stay as they are.")
            }
        }
        .navigationTitle("Customize")
        .navigationBarTitleDisplayMode(.inline)
        .sheet(isPresented: $showPlaces) { PlacesView(store: store) }
        .navigationDestination(isPresented: $showHourTiles) {
            HourTilesEditor(tiles: $store.preferences.hourTiles, feelsLeads: feelsLeads)
        }
        .navigationDestination(isPresented: $showDetailCards) { detailCardsEditor }
        .confirmationDialog(
            "Restore all layouts?", isPresented: $confirmingRestore, titleVisibility: .visible
        ) {
            Button("Restore all layouts", role: .destructive) {
                withAnimation(.snappy) { store.preferences = store.preferences.restoringLayouts() }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("The order and visibility of every tab, card, row and tile go back to the defaults. Your other settings stay.")
        }
        .onAppear {
            #if DEBUG
                // Screenshots: `-restorelayouts` asks the question.
                let args = ProcessInfo.processInfo.arguments
                if args.contains("-restorelayouts") { confirmingRestore = true }
                if let i = args.firstIndex(of: "-customize"), i + 1 < args.count {
                    showHourTiles = args[i + 1] == "hourtiles"
                    showDetailCards = args[i + 1] == "detailcards"
                }
            #endif
        }
    }

    private var detailCardsEditor: some View {
        DetailCardsEditor(
            cards: $store.preferences.detailCards,
            titles: Dictionary(
                (store.view?.details ?? []).map { ($0.kind, $0.title) }, uniquingKeysWith: { a, _ in a }))
    }

    /// A row that opens an editor, with how much of it shows at the right.
    private func link<Editor: View>(
        _ title: String, _ symbol: String, _ detail: String?, @ViewBuilder _ editor: @escaping () -> Editor
    ) -> some View {
        NavigationLink {
            editor()
        } label: {
            LabeledContent {
                if let detail { Text(detail) }
            } label: {
                Label(title, systemImage: symbol)
            }
        }
    }
}
