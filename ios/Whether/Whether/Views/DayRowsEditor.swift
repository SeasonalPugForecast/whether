import SwiftUI

extension EnvironmentValues {
    /// What each day row shows (`Preferences.dayRowItems`), for the rows to read
    /// and their menu to change. Set once, by the forecast screen.
    @Entry var dayRowItems: Binding<[DayRowChoice]> = .constant(DayRowChoice.defaults)
    /// What an opened day shows (`Preferences.dayDetailItems`).
    @Entry var dayDetailItems: Binding<[DayDetailChoice]> = .constant(DayDetailChoice.defaults)
}

/// What the day rows show, and what an opened day shows: switch one off to hide
/// it, drag the handles to reorder. A sheet from a day's menu (`sheet`), or a
/// page of Settings.
struct DayRowsEditor: View {
    @Binding var rows: [DayRowChoice]
    @Binding var details: [DayDetailChoice]
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
                ForEach($rows) { $choice in
                    Toggle(isOn: $choice.shown.animation(.snappy)) {
                        Label(choice.item.title, systemImage: choice.item.symbol)
                    }
                }
                .onMove { rows.move(fromOffsets: $0, toOffset: $1) }
            } header: {
                Text("On each day")
            } footer: {
                Text(
                    "The chance of rain and the bar stay on the day's own line; the rest follow underneath in this order. The air temperature shows when feels-like leads."
                )
            }
            Section {
                ForEach($details) { $choice in
                    Toggle(isOn: $choice.shown.animation(.snappy)) {
                        Label(choice.item.title, systemImage: choice.item.symbol)
                    }
                }
                .onMove { details.move(fromOffsets: $0, toOffset: $1) }
            } header: {
                Text("When a day is open")
            } footer: {
                Text(
                    "The stretches and the hours share one strip, drawn where the first of them is. A day without the forecaster's line on its row gets it here."
                )
            }
            Section {
                Button("Restore the defaults") {
                    rows = DayRowChoice.defaults
                    details = DayDetailChoice.defaults
                }
            }
        }
        .environment(\.editMode, .constant(.active))
        .navigationTitle("Day rows")
        .navigationBarTitleDisplayMode(.inline)
    }
}
