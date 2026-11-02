import SwiftUI
import WhetherKit

/// The weather sources: the order they are tried in, a page per source (its key
/// and settings), the fallbacks that always come last, and the supplements.
struct SourcesView: View {
    @Bindable var store: WeatherStore
    @State private var editMode = EditMode.inactive

    /// What the status lines say: the keys, One Call's count and Pirate Weather's month.
    private var sourceFacts: Sources.Facts {
        var facts = store.preferences.sourceFacts(callsToday: store.oneCallCallsToday())
        facts.monthUsage[Sources.pirateWeather] = store.pirateWeatherUsage()
        return facts
    }

    var body: some View {
        let facts = sourceFacts
        List {
            Section {
                NavigationLink {
                    CompareSourcesView(store: store)
                } label: {
                    Label("Compare sources", systemImage: "rectangle.split.3x1")
                }
            } footer: {
                Text("Up to three sources side by side for the place shown, to see what each brings.")
            }
            Section {
                ForEach(Sources.orderable(store.preferences.sources)) { row in
                    NavigationLink {
                        SourceDetail(store: store, provider: row.provider)
                    } label: {
                        SourceRowLabel(provider: row.provider, status: Sources.status(row, facts), serving: Sources.serves(row, facts))
                    }
                }
                .onMove { Sources.move(&store.preferences.sources, fromOffsets: $0, toOffset: $1) }
            } header: {
                Text("Tried in this order")
            } footer: {
                Text(
                    "The first source with data for a place serves it. Tap one for its key and settings; Reorder moves them."
                )
            }
            Section {
                if let open = store.preferences.sources.first(where: { $0.provider == Sources.openMeteo }) {
                    SourceRowLabel(
                        provider: open.provider, status: Sources.status(open, facts), serving: true,
                        pinned: true)
                }
                // Off when the build has no API contact (a placeholder project URL).
                Toggle(
                    "Backup forecast from MET Norway",
                    isOn: apiContact() == nil ? .constant(false) : $store.preferences.metNorway
                )
                .disabled(apiContact() == nil)
            } header: {
                Text("Always last")
            } footer: {
                Text(
                    "Open-Meteo is the fallback: it serves when nothing above does, and fills what the others lack (UV, moon, air quality, the next hour). MET Norway (Norwegian Meteorological Institute) forecasts anywhere, without a key, but only when Open-Meteo cannot, and only while you use the app (never for widgets or in the background). It is asked again only after the time it names."
                        + (apiContact() == nil ? " Needs the app's contact address." : "")
                )
            }
            Section {
                Toggle("Fill gaps from Open-Meteo", isOn: $store.preferences.fillGaps)
                Toggle("Next hour from Apple Weather", isOn: $store.preferences.appleMinutes)
                Toggle("Alerts from Apple Weather", isOn: $store.preferences.appleAlerts)
                Toggle("Rain Live Activity", isOn: $store.preferences.rainActivity)
                Toggle("History and normals", isOn: $store.preferences.history)
            } header: {
                Text("Besides the order").id("supplements")
            } footer: {
                Text(
                    "Apple Weather gives the next hour minute by minute where it has it (US, Canada, UK, Ireland), and alerts outside the US; when these are on, the place's coordinates go to Apple (alerts at most every 30 minutes). Official US alerts always come from the National Weather Service. Fill gaps lets Open-Meteo add what a source lacks. History shows today's normals, records and chance of rain, yesterday against normal, and the normals on each day's bar: from a nearby NOAA station in the US, from the ERA5 reanalysis elsewhere, fetched once a day."
                )
            }
        }
        .environment(\.editMode, $editMode)
        .navigationTitle("Sources")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button(editMode == .active ? "Done" : "Reorder") {
                    withAnimation { editMode = editMode == .active ? .inactive : .active }
                }
            }
        }
    }
}

/// A source's name, what it covers and adds, and its status.
struct SourceRowLabel: View {
    let provider: String
    let status: String
    let serving: Bool
    var pinned = false

    var body: some View {
        let info = Sources.info(provider)
        VStack(alignment: .leading, spacing: 3) {
            HStack(spacing: 6) {
                Text(info?.name ?? provider).font(.headline)
                if pinned { Image(systemName: "pin.fill").font(.caption).foregroundStyle(.secondary) }
            }
            if let info {
                Text("\(info.covers) · \(info.adds)")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Text(status)
                .font(.footnote)
                .foregroundStyle(serving ? Color.accentColor : Color.secondary)
        }
        .accessibilityElement(children: .combine)
    }
}

/// One source's page: whether it is used, its key, and its own settings.
struct SourceDetail: View {
    @Bindable var store: WeatherStore
    let provider: String
    /// The forecast model presets, best match first.
    private static let presets = modelPresets()
    /// Whether a key is saved (the page's own copy, updated as it is typed).
    @State private var hasKey: Bool

    init(store: WeatherStore, provider: String) {
        self.store = store
        self.provider = provider
        _hasKey = State(initialValue: Keychain.key(for: provider) != nil)
    }

    private var info: Sources.Info? { Sources.info(provider) }

    var body: some View {
        Form {
            Section {
                if let info {
                    LabeledContent("Covers", value: info.covers)
                    LabeledContent("Adds", value: info.adds)
                }
                if Sources.movable.contains(provider) {
                    Toggle(
                        "Use \(info?.name ?? provider)",
                        isOn: Binding(
                            get: { store.preferences.enabled(provider) },
                            set: { store.preferences.set(provider, enabled: $0) }))
                }
            }
            switch provider {
            case Sources.openWeather: openWeather
            case Sources.pirateWeather: pirateWeather
            case Sources.models: models
            case Sources.nws:
                Section {
                } footer: {
                    Text(
                        "No key needed. Official US alerts come from the National Weather Service whether or not this is on; switched on, it also forecasts US places (its hourly grid and the nearest station's conditions), with Open-Meteo filling UV, air quality and the days past the seventh."
                    )
                }
            default: EmptyView()
            }
        }
        .navigationTitle(info?.name ?? provider)
        .navigationBarTitleDisplayMode(.inline)
    }

    // MARK: OpenWeather

    @ViewBuilder private var openWeather: some View {
        Section {
            ProviderKeyEditor(
                provider: provider, signUp: info?.signUp,
                check: { await store.checkKey($0) },
                onSaved: { keySaved($0) })
        } header: {
            Text("API key").id("openweather")
        } footer: {
            Text(
                "Your own OpenWeather key is optional; without it, Open-Meteo serves. The key stays in this phone's Keychain."
            )
        }
        Section {
            Toggle("One Call 4.0", isOn: $store.preferences.oneCall)
                .disabled(!hasKey)
            if store.preferences.oneCall {
                Stepper(
                    "Calls a day: \(store.preferences.dailyCalls)",
                    value: $store.preferences.dailyCalls, in: 10...1000, step: 10)
                Text(
                    oneCallUsageText(
                        used: store.oneCallCallsToday(),
                        limit: UInt32(clamping: store.preferences.dailyCalls))
                )
                .font(.footnote)
                .foregroundStyle(.secondary)
            }
        } header: {
            Text("One Call")
        } footer: {
            Text(
                "One Call 4.0 adds official alerts and is metered: OpenWeather's plan includes 1,000 free calls a day, and the stepper sets how many this phone may use."
            )
        }
    }

    // MARK: Pirate Weather

    @ViewBuilder private var pirateWeather: some View {
        Section {
            ProviderKeyEditor(
                provider: provider, signUp: info?.signUp,
                check: { await store.checkPirateKey($0) },
                onSaved: { keySaved($0) })
        } header: {
            Text("API key")
        } footer: {
            Text(
                "Your own Pirate Weather key is optional; without it the other sources serve. The key stays in this phone's Keychain. Test key makes one small forecast request, which counts as one of the key's calls for the month."
            )
        }
        Section {
            Stepper(
                "Calls a month: \(store.preferences.pirateWeatherCalls)",
                value: $store.preferences.pirateWeatherCalls, in: 500...20000, step: 500)
            if hasKey {
                Text(store.pirateWeatherUsage())
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
        } header: {
            Text("Monthly calls")
        } footer: {
            Text(
                "Every refresh of a place is one call, and a free key allows 10,000 a month; the stepper sets how many this phone may use, leaving the rest for other devices and for key tests. When they are used up the next source serves, until the month starts again (UTC). Pirate Weather blends NOAA and other public weather models and adds the next hour of rain and alerts."
            )
        }
    }

    /// The key was saved (or cleared): without one OpenWeather's One Call switches
    /// off, and a first key turns the source on.
    private func keySaved(_ saved: Bool) {
        let first = saved && !hasKey
        hasKey = saved
        if !saved && provider == Sources.openWeather { store.preferences.oneCall = false }
        if first { store.preferences.set(provider, enabled: true) }
    }

    // MARK: Forecast models

    @ViewBuilder private var models: some View {
        Section {
            Picker(
                "Forecast models",
                selection: Binding(
                    get: { store.preferences.models },
                    set: {
                        // A preset turns the row on; "Best match" is no models.
                        store.preferences.setModels($0)
                    })
            ) {
                ForEach(Self.presets, id: \.name) { preset in
                    Text(preset.name).tag(preset.models)
                }
                if !Self.presets.contains(where: { $0.models == store.preferences.models }) {
                    // Imported from a config.toml and not a preset: kept as it is.
                    Text(store.preferences.models.joined(separator: " → "))
                        .tag(store.preferences.models)
                }
            }
            .pickerStyle(.inline)
            .labelsHidden()
        } footer: {
            Text(
                "Forecast models come first where they have data (the NOAA ones cover the lower 48), each value from the first model that has it; Open-Meteo's best match fills the rest."
            )
        }
        Section {
            NavigationLink {
                CompareSourcesView(store: store, kind: .models)
            } label: {
                Label("Compare models", systemImage: "rectangle.split.3x1")
            }
        } footer: {
            Text("Up to three presets side by side for the place shown, to see what each forecasts.")
        }
    }
}

/// A provider's key field: saved to the Keychain 0.5 s after typing (at once when
/// cleared, and when the page goes away), with a Test button and a sign-up link.
struct ProviderKeyEditor: View {
    let provider: String
    var signUp: URL?
    /// Asks the provider whether the key works (a free call); nil: no test.
    var check: ((String) async -> KeyCheck)?
    /// Called after a save, with whether a key is now stored.
    var onSaved: (Bool) -> Void

    @State private var key: String
    /// The key as the Keychain holds it.
    @State private var savedKey: String
    @State private var keyCheck: KeyCheck?
    @State private var checking = false

    init(
        provider: String, signUp: URL? = nil, check: ((String) async -> KeyCheck)? = nil,
        onSaved: @escaping (Bool) -> Void
    ) {
        self.provider = provider
        self.signUp = signUp
        self.check = check
        self.onSaved = onSaved
        let stored = Keychain.key(for: provider) ?? ""
        _key = State(initialValue: stored)
        _savedKey = State(initialValue: stored)
    }

    private var trimmed: String { key.trimmingCharacters(in: .whitespacesAndNewlines) }

    var body: some View {
        SecureField("API key", text: $key)
            .textInputAutocapitalization(.never)
            .autocorrectionDisabled()
            .onChange(of: key) {
                keyCheck = nil
                // Cleared: saved at once, so what depends on it switches off now.
                if trimmed.isEmpty { save() }
            }
            .task(id: key) {
                try? await Task.sleep(for: .milliseconds(500))
                if !Task.isCancelled { save() }
            }
            // Swiping the page away keeps the key too.
            .onDisappear { save() }
        if let check {
            VStack(alignment: .leading, spacing: 4) {
                Button(checking ? "Testing…" : "Test key") {
                    Task {
                        save()
                        checking = true
                        keyCheck = await check(savedKey)
                        checking = false
                    }
                }
                .disabled(trimmed.isEmpty || checking)
                if let keyCheck {
                    Label(
                        keyCheck.message,
                        systemImage: keyCheck.accepted ? "checkmark.circle" : "exclamationmark.triangle"
                    )
                    .font(.footnote)
                    .foregroundStyle(keyCheck.accepted ? Color.secondary : Color.orange)
                }
            }
            .onAppear {
                #if DEBUG
                    // Screenshots: `-keyStatus accepted` shows a passed key test
                    // without calling the provider.
                    let args = ProcessInfo.processInfo.arguments
                    if let i = args.firstIndex(of: "-keyStatus"), i + 1 < args.count, args[i + 1] == "accepted" {
                        keyCheck = KeyCheck(accepted: true, message: "Key accepted")
                    }
                #endif
            }
        }
        if let signUp {
            Link(destination: signUp) {
                Label("Get a key", systemImage: "arrow.up.right.square")
            }
        }
    }

    /// Writes the key to the Keychain when it changed.
    private func save() {
        guard trimmed != savedKey else { return }
        Keychain.setKey(trimmed, for: provider)
        savedKey = trimmed
        onSaved(!trimmed.isEmpty)
    }
}
