import SwiftUI
import WhetherKit

/// The days, each opening to its hours, with their history. Which cards show,
/// and in what order, is `Preferences.daysCards`: touch and hold one to hide it
/// or edit the tab.
struct DaysPage: View {
    let view: ViewModel
    let cards: Bool
    @Environment(\.daysCards) private var daysCards
    @State private var editing = false

    /// The cards to draw: the saved ones (screenshots may choose them for one
    /// launch, without saving: `-dayscards history` shows that one; `-editdaystab`
    /// opens the editor).
    private var cardChoices: [DaysCardChoice] {
        #if DEBUG
            if let names = ProcessInfo.processInfo.launchList("-dayscards") {
                return DaysCardChoice.arranged(by: names)
            }
        #endif
        return daysCards.wrappedValue
    }

    /// A card's touch-and-hold entries: edit the tab, hide the card.
    private func menu(for card: DaysCard) -> CardMenu {
        CardMenu(
            editTitle: "Edit the Days tab", edit: { editing = true },
            hideTitle: "Hide \(card.title)",
            hide: { withAnimation(.snappy) { daysCards.wrappedValue.set(card, shown: false) } })
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: cards ? 14 : 28) {
                ForEach(cardChoices.filter(\.shown)) { choice in
                    Group {
                        switch choice.item {
                        case .days:
                            if let daily = view.daily {
                                DailySection(daily: daily).glassCard(cards)
                            }
                        case .history:
                            if let history = view.history {
                                HistorySection(history: history).glassCard(cards).cardContextMenu()
                            }
                        }
                    }
                    .environment(\.cardMenu, menu(for: choice.item))
                }
                if cardChoices.allSatisfy({ !$0.shown }) {
                    Button("Choose the cards", systemImage: "square.grid.2x2") { editing = true }
                        .font(.callout)
                        .frame(maxWidth: .infinity)
                        .padding(.top, 24)
                }
            }
            .padding(.horizontal, cards ? 16 : 20)
            .padding(.top, 12)
            .padding(.bottom, 40)
        }
        .sheet(isPresented: $editing) {
            CardsEditor(items: daysCards, title: "Days tab", sheet: true)
        }
        .onAppear {
            #if DEBUG
                if ProcessInfo.processInfo.arguments.contains("-editdaystab") { editing = true }
            #endif
        }
    }
}

/// The radar for the place shown, full screen; zoom out (or tap the globe) for
/// the Earth.
struct RadarTab: View {
    @Bindable var store: WeatherStore
    @State private var loop = RadarLoop()

    private var place: AppPlace? {
        store.places.indices.contains(store.active) ? store.places[store.active] : store.places.first
    }

    var body: some View {
        if let place {
            // Outside the US: OpenWeather's layer with a key, else the globe and a note.
            let settings = store.preferences.settings()
            RadarScreen(
                place: place, loop: loop, closable: false, source: settings.radarSource(for: place)
            )
            .id(place)
            .task(id: RadarTaskKey(place: place, settings: settings)) {
                await loop.load(place: place, settings: settings)
            }
        }
    }
}
