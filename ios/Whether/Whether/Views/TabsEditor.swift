import SwiftUI

#if DEBUG
    extension ProcessInfo {
        /// The comma-separated names after a launch argument (screenshots).
        func launchList(_ flag: String) -> [String]? {
            guard let i = arguments.firstIndex(of: flag), i + 1 < arguments.count else { return nil }
            return arguments[i + 1].split(separator: ",").map(String.init)
        }
    }
#endif

/// A list of items to switch off and drag into order, as one section.
struct ItemListSection<Item: ListedItem>: View {
    @Binding var items: [ItemChoice<Item>]
    var header: String?
    var footer: String

    var body: some View {
        Section {
            ForEach($items) { $choice in
                Toggle(isOn: $choice.shown.animation(.snappy)) {
                    Label(choice.item.title, systemImage: choice.item.symbol)
                }
            }
            .onMove { items.move(fromOffsets: $0, toOffset: $1) }
        } header: {
            if let header { Text(header) }
        } footer: {
            Text(footer)
        }
    }
}

/// The tabs after Now: switch one off to hide it, drag the handles to reorder.
/// Now always shows, and first.
struct TabsEditor: View {
    @Binding var tabs: [TabChoice]

    var body: some View {
        List {
            Section {
                HStack {
                    Label("Now", systemImage: "thermometer.sun")
                    Spacer()
                    Image(systemName: "lock.fill").foregroundStyle(.secondary)
                }
                .accessibilityElement(children: .combine)
            }
            ItemListSection(
                items: $tabs,
                footer: "Drag the handles to reorder the tabs; switch a tab off to hide it. The Now tab always shows, and first.")
            Section {
                Button("Restore the defaults") { tabs = TabChoice.defaults }
            }
        }
        .environment(\.editMode, .constant(.active))
        .navigationTitle("Tabs")
        .navigationBarTitleDisplayMode(.inline)
    }
}

/// The cards of the Hourly tab or the Days tab: switch one off to hide it, drag
/// the handles to reorder. A sheet from a card's menu (`sheet`), or a page of
/// Settings.
struct CardsEditor<Item: ListedItem>: View {
    @Binding var items: [ItemChoice<Item>]
    let title: String
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
            ItemListSection(
                items: $items,
                footer: "Drag the handles to reorder the cards; switch a card off to hide it. Touch and hold a card for its menu.")
            Section {
                Button("Restore the defaults") { items = ItemChoice<Item>.defaults }
            }
        }
        .environment(\.editMode, .constant(.active))
        .navigationTitle(title)
        .navigationBarTitleDisplayMode(.inline)
    }
}
