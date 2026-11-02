import SwiftUI

extension EnvironmentValues {
    /// The Sun & moon card's items (`Preferences.sunItems`), for the card to read
    /// and its menu to change. Set once, by the forecast screen.
    @Entry var sunItems: Binding<[SunItemChoice]> = .constant(SunItemChoice.defaults)
}

/// The Sun & moon card's items: switch one off to hide it, drag the handles to
/// reorder. A sheet from the card's menu (`sheet`), or a page of Settings.
struct SunMoonEditor: View {
    @Binding var items: [SunItemChoice]
    var sheet = false
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        if sheet {
            NavigationStack {
                list.toolbar {
                    ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } }
                }
            }
        } else {
            list
        }
    }

    private var list: some View {
        List {
            Section {
                ForEach($items) { $choice in
                    Toggle(isOn: $choice.shown.animation(.snappy)) {
                        Label(choice.item.title, systemImage: choice.item.symbol)
                    }
                }
                .onMove { items.move(fromOffsets: $0, toOffset: $1) }
            } footer: {
                Text(
                    "Drag the handles to reorder the card; switch an item off to hide it. The twilights, the golden hour and solar noon, when next to each other, share one list in time order."
                )
            }
            Section {
                Button("Restore the defaults") { items = SunItemChoice.defaults }
            }
        }
        .environment(\.editMode, .constant(.active))
        .navigationTitle("Sun & moon")
        .navigationBarTitleDisplayMode(.inline)
    }
}
