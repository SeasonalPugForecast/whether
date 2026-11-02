import SwiftUI

extension EnvironmentValues {
    /// The Hourly tab's cards (`Preferences.hourlyCards`), for the tab to read and
    /// its menus to change. Set once, by the forecast screen.
    @Entry var hourlyCards: Binding<[HourlyCardChoice]> = .constant(HourlyCardChoice.defaults)
    /// The Days tab's cards (`Preferences.daysCards`).
    @Entry var daysCards: Binding<[DaysCardChoice]> = .constant(DaysCardChoice.defaults)
    /// What touching and holding the card around this view offers beyond its own
    /// items: edit the tab, hide the card. `nil` where the card is not on a tab
    /// that can be arranged.
    @Entry var cardMenu: CardMenu?
    /// Opens the Customize page as a sheet: where "Customize…" goes for a card
    /// with no editor of its own.
    @Entry var openCustomize = CustomizeOpener()
}

/// Opens the Customize page. Always equal to another, so that storing a closure in
/// the environment does not redraw every card.
struct CustomizeOpener: Equatable {
    var open: () -> Void = {}
    static func == (a: Self, b: Self) -> Bool { true }
}

/// The entries every card adds to its touch-and-hold menu, in the same words
/// everywhere: Customize…, Edit the … tab, Hide ….
struct CardMenu {
    /// "Edit the Now tab", "Edit the Hourly tab" …
    var editTitle: String
    /// Opens the tab's arrangement: its cards in order, with switches.
    var edit: () -> Void
    var hideTitle: String
    /// `nil` for a card that always shows (the current conditions, alerts).
    var hide: (() -> Void)?
}

/// "Customize…", "Edit the … tab" and "Hide …", at the end of a card's menu.
/// `customize` opens the card's own editor; without one, the Customize page.
struct CardMenuButtons: View {
    var customize: (() -> Void)?
    @Environment(\.cardMenu) private var menu
    @Environment(\.openCustomize) private var openCustomize

    var body: some View {
        Button("Customize…", systemImage: "slider.horizontal.3") { (customize ?? openCustomize.open)() }
        if let menu {
            Button(menu.editTitle, systemImage: "square.grid.2x2", action: menu.edit)
            if let hide = menu.hide {
                Button(menu.hideTitle, systemImage: "eye.slash", action: hide)
            }
        }
    }
}

extension View {
    /// Touch and hold gives the card's `CardMenu`: for a card with no menu of its own.
    func cardContextMenu(customize: (() -> Void)? = nil) -> some View {
        modifier(CardContextMenu(customize: customize))
    }
}

private struct CardContextMenu: ViewModifier {
    var customize: (() -> Void)?

    func body(content: Content) -> some View {
        content.contextMenu { CardMenuButtons(customize: customize) }
    }
}

/// An editor page in a sheet: a navigation stack with Done.
struct EditorSheet<Content: View>: View {
    @ViewBuilder var content: Content
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            content.toolbar {
                ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } }
            }
        }
    }
}
