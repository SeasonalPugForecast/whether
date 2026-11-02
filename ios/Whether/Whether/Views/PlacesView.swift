import SwiftUI
import UniformTypeIdentifiers
import WhetherKit

/// The saved places: switch, reorder, delete, search for more, or add where you are.
struct PlacesView: View {
    @Bindable var store: WeatherStore
    @Environment(\.dismiss) private var dismiss
    @State private var query = Self.debugQuery
    /// Places suggested as you type, nearest the place shown first.
    @State private var suggestions: PlaceSuggestions
    /// Why "Current location" or a pick did not work.
    @State private var status: String?
    @State private var locating = false
    /// The place being renamed, and the name typed so far.
    @State private var renaming: AppPlace?
    @State private var newName = ""

    init(store: WeatherStore) {
        self.store = store
        let shown = store.places.indices.contains(store.active) ? store.places[store.active] : nil
        _suggestions = State(initialValue: PlaceSuggestions(store: store, near: shown))
    }

    var body: some View {
        NavigationStack {
            List {
                if query.trimmingCharacters(in: .whitespaces).isEmpty {
                    saved
                } else {
                    found
                }
            }
            .navigationTitle("Places")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) { EditButton() }
                ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } }
            }
            .searchable(text: $query, prompt: "City or ZIP code")
            .alert(
                "Rename place",
                isPresented: Binding(get: { renaming != nil }, set: { if !$0 { renaming = nil } })
            ) {
                TextField("Name", text: $newName)
                Button("Rename") {
                    if let renaming { store.rename(id: renaming.id, to: newName) }
                }
                .disabled(newName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                Button("Cancel", role: .cancel) {}
            }
            .onChange(of: query, initial: true) { suggestions.update(query) }
            .task {
                #if DEBUG
                    // Screenshots: `-search ph -pickFirst` adds the first suggestion.
                    guard ProcessInfo.processInfo.arguments.contains("-pickFirst") else { return }
                    try? await Task.sleep(for: .seconds(4))
                    if let first = suggestions.items.first, let place = try? await suggestions.place(for: first) {
                        await store.add(place)
                        dismiss()
                    }
                #endif
            }
        }
    }

    private var saved: some View {
        Group {
            Section {
                ForEach(store.places) { place in
                    row(place)
                }
                .onDelete { store.remove(at: $0) }
                // One way to reorder (Edit, or the system's touch-and-drag where
                // it offers it): a second drag-and-drop competed with it and with
                // the touch-and-hold menu.
                .onMove { store.move(from: $0, to: $1) }
            }
            Section {
                if store.places.contains(where: \.follows) {
                    Button {
                        Task { await addCurrent(follow: false) }
                    } label: {
                        Label(locating ? "Finding you…" : "Add current location", systemImage: "mappin")
                    }
                    .disabled(locating)
                } else {
                    Menu {
                        Button("Follow me", systemImage: "location.fill") {
                            Task { await addCurrent(follow: true) }
                        }
                        Button("Add this place", systemImage: "mappin") {
                            Task { await addCurrent(follow: false) }
                        }
                    } label: {
                        Label(locating ? "Finding you…" : "Current location", systemImage: "location.fill")
                    }
                    .disabled(locating)
                }
            } footer: {
                VStack(alignment: .leading, spacing: 4) {
                    if let status { Text(status) }
                    if store.locationOff { Text("Location access is off") }
                    if store.places.contains(where: \.follows) {
                        Text("The place with the location symbol follows you, and updates when the app opens.")
                    }
                    if store.places.count > 1 {
                        Text(
                            "Swipe right to pin a place, or touch and hold for more. Move a place with Edit, or Move to top and bottom in its menu: dropped above the last pinned place it is pinned, and a pinned place dropped below it is unpinned. The star is the favourite."
                        )
                    }
                }
            }
        }
    }

    /// One saved place: tap to show it; swipe or touch and hold for the rest.
    private func row(_ place: AppPlace) -> some View {
        let favourite = store.preferences.favouritePlaceID == place.id
        return Button {
            store.show(id: place.id)
            dismiss()
        } label: {
            HStack {
                // One column for the marks, so the names line up.
                if store.places.contains(where: { $0.follows || $0.pinned }) {
                    Group {
                        if place.follows {
                            Image(systemName: "location.fill")
                                .foregroundStyle(.tint)
                                .accessibilityLabel("Follows you")
                        } else if place.pinned {
                            Image(systemName: "pin.fill")
                                .foregroundStyle(.yellow)
                                .accessibilityLabel("Pinned")
                        } else {
                            Color.clear
                        }
                    }
                    .frame(width: 22)
                }
                VStack(alignment: .leading, spacing: 2) {
                    // Concrete colors: in a list button `.primary` resolves to the tint.
                    Text(place.name).foregroundStyle(Color.primary)
                    if let detail = place.detail, !detail.isEmpty {
                        Text(detail).font(.caption).foregroundStyle(Color.secondary)
                    }
                }
                Spacer()
                if favourite {
                    Image(systemName: "star.fill")
                        .foregroundStyle(.yellow)
                        .accessibilityLabel("Favourite")
                }
                if store.places.indices.contains(store.active), store.places[store.active].id == place.id {
                    Image(systemName: "checkmark").foregroundStyle(.tint)
                }
            }
        }
        .swipeActions(edge: .leading) {
            // The place that follows you is always first.
            if !place.follows {
                Button(place.pinned ? "Unpin" : "Pin", systemImage: place.pinned ? "pin.slash.fill" : "pin.fill") {
                    withAnimation { store.togglePin(id: place.id) }
                }
                .tint(.yellow)
            }
        }
        .swipeActions(edge: .trailing) {
            Button("Delete", role: .destructive) { delete(place) }
            // A place that follows you is named where you are.
            if !place.follows {
                Button("Rename") { startRenaming(place) }
            }
        }
        .contextMenu {
            if !place.follows {
                Button(place.pinned ? "Unpin" : "Pin", systemImage: place.pinned ? "pin.slash" : "pin") {
                    withAnimation { store.togglePin(id: place.id) }
                }
                Button("Rename", systemImage: "pencil") { startRenaming(place) }
                Button("Move to top", systemImage: "arrow.up.to.line") {
                    withAnimation { store.move(id: place.id, toTop: true) }
                }
                Button("Move to bottom", systemImage: "arrow.down.to.line") {
                    withAnimation { store.move(id: place.id, toTop: false) }
                }
            }
            Button(
                favourite ? "Remove favourite" : "Make favourite",
                systemImage: favourite ? "star.slash" : "star"
            ) {
                store.toggleFavourite(id: place.id)
            }
            Button("Delete", systemImage: "trash", role: .destructive) { delete(place) }
        }
        .moveDisabled(place.follows)
    }

    private func delete(_ place: AppPlace) {
        if let index = store.places.firstIndex(where: { $0.id == place.id }) {
            withAnimation { store.remove(at: IndexSet(integer: index)) }
        }
    }

    private func startRenaming(_ place: AppPlace) {
        newName = place.name
        renaming = place
    }

    private var found: some View {
        Section {
            ForEach(suggestions.items) { suggestion in
                Button {
                    Task {
                        do {
                            await store.add(try await suggestions.place(for: suggestion))
                            dismiss()
                        } catch {
                            status = userMessage(error).title
                        }
                    }
                } label: {
                    SuggestionRow(suggestion: suggestion)
                }
            }
        } footer: {
            VStack(alignment: .leading) {
                if let status = status ?? suggestions.status { Text(status) }
                if let detail = suggestions.statusDetail {
                    DisclosureGroup("Details") { Text(detail) }
                }
            }
        }
    }

    /// Screenshots: `-search Brooklyn` starts with a query.
    private static var debugQuery: String {
        #if DEBUG
            let args = ProcessInfo.processInfo.arguments
            if let i = args.firstIndex(of: "-search"), i + 1 < args.count { return args[i + 1] }
        #endif
        return ""
    }

    /// Adds where you are, as a place that follows you or a fixed one.
    private func addCurrent(follow: Bool) async {
        locating = true
        defer { locating = false }
        do {
            let here = try await CurrentLocation.place()
            if follow { await store.follow(here) } else { await store.add(here) }
            dismiss()
        } catch {
            status = error.localizedDescription
        }
    }
}

/// A suggested place: its name, and where it is.
struct SuggestionRow: View {
    let suggestion: PlaceSuggestion

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(suggestion.title).foregroundStyle(.primary)
            if let subtitle = suggestion.subtitle {
                Text(subtitle).font(.caption).foregroundStyle(.secondary)
            }
        }
    }
}
