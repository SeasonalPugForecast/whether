import SwiftUI

/// The hour card's tiles as a list: switch one off to hide it, drag the handles
/// to reorder. The same tiles that touching and holding one in the card edits
/// like the home screen; this is the page for people who never touch and hold.
struct HourTilesEditor: View {
    @Binding var tiles: [Preferences.HourTileChoice]
    let feelsLeads: Bool

    var body: some View {
        List {
            Section {
                ForEach($tiles) { $choice in
                    Toggle(isOn: $choice.shown.animation(.snappy)) {
                        Label(
                            choice.tile == .feelsLike && feelsLeads ? "Actual" : choice.tile.title,
                            systemImage: choice.tile.symbol)
                    }
                }
                .onMove { tiles.move(fromOffsets: $0, toOffset: $1) }
            } footer: {
                Text("Drag the handles to reorder the hour card's tiles; switch a tile off to hide it. Touch and hold a tile in the card to edit them there.")
            }
            Section {
                Button("Restore the defaults") { tiles = Preferences.defaultHourTiles }
                    .disabled(tiles == Preferences.defaultHourTiles)
            }
        }
        // The app's white tint would leave the switches' on state unreadable.
        .tint(.green)
        .environment(\.editMode, .constant(.active))
        .navigationTitle("Hour tiles")
        .navigationBarTitleDisplayMode(.inline)
    }
}

/// The Now tab's detail cards as a list: switch one off to hide it, drag the
/// handles to reorder. Titles are the core's, from the forecast shown.
struct DetailCardsEditor: View {
    @Binding var cards: [Preferences.DetailChoice]
    /// The core's titles by kind, from the forecast shown (empty before there is one).
    let titles: [String: String]

    var body: some View {
        List {
            Section {
                ForEach($cards) { $choice in
                    Toggle(isOn: $choice.shown.animation(.snappy)) {
                        Label(titles[choice.kind] ?? choice.fallbackTitle, systemImage: choice.symbol)
                    }
                }
                .onMove { cards.move(fromOffsets: $0, toOffset: $1) }
            } footer: {
                Text("Drag the handles to reorder the detail cards; switch a card off to hide it. A card shows only when the forecast has it. Touch and hold a card to edit them there.")
            }
            Section {
                Button("Restore the defaults") { cards = Preferences.defaultDetails }
                    .disabled(cards == Preferences.defaultDetails)
            }
        }
        .tint(.green)
        .environment(\.editMode, .constant(.active))
        .navigationTitle("Detail cards")
        .navigationBarTitleDisplayMode(.inline)
    }
}
