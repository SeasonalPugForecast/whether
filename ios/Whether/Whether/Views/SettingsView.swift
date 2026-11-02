import SwiftUI
import UniformTypeIdentifiers
import WhetherKit

/// Units, clock, forecast length, alerts, and a link to the weather sources.
struct SettingsView: View {
    @Bindable var store: WeatherStore
    @Environment(\.dismiss) private var dismiss
    @Environment(\.openURL) private var openURL
    @Environment(\.scenePhase) private var phase
    @State private var importing = false
    @State private var importResult: String?
    /// "0.1.0 (1)": the app's version and build, from Version.xcconfig.
    private static let appVersion: String = {
        let info = Bundle.main.infoDictionary
        let version = info?["CFBundleShortVersionString"] as? String ?? "?"
        let build = info?["CFBundleVersion"] as? String ?? "?"
        return "\(version) (\(build))"
    }()
    /// Screenshots: `-show sections` opens the section editor.
    @State private var showSections = false
    /// Screenshots: `-show units` opens the custom units.
    @State private var showUnits = false
    /// Screenshots: `-show tabs` opens the tabs.
    @State private var showTabs = false
    /// Screenshots: `-show sources` opens the sources, `-show source-openweather` one of them.
    @State private var showSources = false
    /// Screenshots: `-show compare` opens Compare sources.
    @State private var showCompare = false
    /// Screenshots: `-show compare-models` opens Compare models.
    @State private var showCompareModels = false
    @State private var showSource: String?
    /// Screenshots: `-ack` opens the acknowledgements.
    @State private var showAcknowledgements = false
    /// Screenshots: `-show customize` opens the Customize page.
    @State private var showCustomize = false
    /// Screenshots: `-show colors` opens Colors.
    @State private var showColors = false

    var body: some View {
        NavigationStack {
            ScrollViewReader { proxy in
                Form {
                    Section {
                        Picker("Units", selection: $store.preferences.units) {
                            Text("Automatic").tag(Preferences.UnitChoice.automatic)
                            Text("°F, mph, in").tag(Preferences.UnitChoice.imperial)
                            Text("°C, km/h, mm").tag(Preferences.UnitChoice.metric)
                        }
                        NavigationLink {
                            UnitsEditor(store: store)
                        } label: {
                            LabeledContent("Custom…", value: store.preferences.unitOverrides.summary())
                        }
                        Picker("Clock", selection: $store.preferences.clock) {
                            Text("Automatic").tag(Preferences.ClockChoice.automatic)
                            Text("12-hour").tag(Preferences.ClockChoice.twelve)
                            Text("24-hour").tag(Preferences.ClockChoice.twentyFour)
                        }
                        Picker("Dates", selection: $store.preferences.dates) {
                            ForEach(Preferences.DateChoice.allCases) { Text($0.example).tag($0) }
                        }
                        Picker("Layout", selection: $store.preferences.layout) {
                            Text("Centered").tag(Preferences.LayoutChoice.centered)
                            Text("Classic").tag(Preferences.LayoutChoice.darkSky)
                        }
                        Picker("Background", selection: $store.preferences.background) {
                            Text("The weather's sky").tag(Preferences.BackgroundChoice.sky)
                            Text("Plain").tag(Preferences.BackgroundChoice.plain)
                        }
                        Toggle("Sun and moon in the sky", isOn: $store.preferences.skyBodies)
                        Picker("Appearance", selection: $store.preferences.appearance) {
                            Text("By the sun").tag(Preferences.AppearanceChoice.sun)
                            Text("Like the phone").tag(Preferences.AppearanceChoice.system)
                            Text("Light").tag(Preferences.AppearanceChoice.light)
                            Text("Dark").tag(Preferences.AppearanceChoice.dark)
                        }
                        NavigationLink {
                            ColorsView(store: store)
                        } label: {
                            LabeledContent("Colors", value: store.colorTheme.name)
                        }
                        Picker("App icon", selection: $store.preferences.appIcon) {
                            ForEach(Preferences.AppIconChoice.allCases) { Text($0.title).tag($0) }
                        }
                        // Every editor in one place: the tabs, the cards, their contents.
                        NavigationLink {
                            CustomizeView(store: store)
                        } label: {
                            Label("Customize", systemImage: "slider.horizontal.3")
                        }
                    } header: {
                        Text("Display")
                    } footer: {
                        Text(
                            "The sky follows the weather and time of day (Appearance applies to the plain background). Colors: presets for the skies and the colors that carry meaning, and color vision. The sun and moon cross the top of the Now tab as they cross the sky at the place shown. By the sun: dark from sunset to sunrise at the place shown. By the season, the pug on the home screen changes with the seasons; iOS confirms each change."
                        )
                    }
                    Section {
                        Picker("Days", selection: $store.preferences.days) {
                            ForEach(WeatherStore.dayChoices, id: \.self) { Text("\($0)").tag($0) }
                        }
                        Toggle("Day icons from sunrise to sunset", isOn: $store.preferences.daylightIcons)
                        Toggle("Feels-like temperatures", isOn: $store.preferences.feelsLike)
                        Picker("Refresh", selection: $store.preferences.refreshMinutes) {
                            ForEach(Preferences.refreshChoices, id: \.self) { minutes in
                                Text(minutes == 60 ? "Every hour" : "Every \(minutes) minutes").tag(minutes)
                            }
                        }
                    } header: {
                        Text("Forecast").id("forecast")
                    } footer: {
                        Text(
                            "Rain overnight does not make a sunny day show rain clouds. Feels-like temperatures lead the big number, the hours and the highs and lows, with the actual temperature beside it. History and normals are under Sources. The forecast refreshes when you open the app after two thirds of the Refresh interval, and iOS runs background refreshes about that often, less when the battery is low."
                        )
                    }
                    Section {
                        Toggle("Notify about weather alerts", isOn: $store.preferences.alertNotifications)
                        Toggle("Shared plan changes", isOn: $store.preferences.planChangeNotices)
                        if store.notificationStatus == .denied {
                            Text("Notifications are off for Pug Forecast")
                                .foregroundStyle(.secondary)
                            Button("Open Settings") {
                                if let url = URL(string: UIApplication.openSettingsURLString) {
                                    openURL(url)
                                }
                            }
                        }
                    } header: {
                        Text("Alerts").id("alerts")
                    } footer: {
                        Text(
                            "Alerts come from the National Weather Service for US places. Elsewhere they come from One Call, and from Apple Weather where it has them (a switch under Sources; Apple's alerts show when they were checked, not a period). Shared plan changes: when someone changes the dates, place or name of a plan shared with you, a banner shows in the app, and a notification when it is closed."
                        )
                    }
                    Section {
                        NavigationLink {
                            SourcesView(store: store)
                        } label: {
                            Text("Sources")
                        }
                    } header: {
                        Text("Sources").id("sources")
                    } footer: {
                        Text(
                            "Tried in this order: \(Sources.summary(store.preferences.sources, store.preferences.sourceFacts())). Open-Meteo is always the fallback, and fills what a source lacks."
                        )
                    }
                    Section {
                        Button("Import config.toml…") { importing = true }
                    } header: {
                        Text("Advanced").id("advanced")
                    } footer: {
                        Text(
                            "If you also use whether on a computer, bring its config.toml: its places and settings. Put the file in Files first (On My iPhone › Pug Forecast)."
                        )
                    }
                    Section("About") {
                        LabeledContent("Version", value: Self.appVersion)
                        LabeledContent("Core", value: coreVersion())
                        Link("Privacy", destination: Project.privacyURL)
                        NavigationLink("Acknowledgements") { AcknowledgementsView() }
                        Text("Weather data by Open-Meteo.com and, with your own keys, OpenWeather and Pirate Weather. History from NOAA's Regional Climate Centers (ACIS) and Open-Meteo (ERA5: contains modified Copernicus Climate Change Service information). Radar: NOAA NEXRAD via Iowa Environmental Mesonet.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
                .fileImporter(
                    isPresented: $importing,
                    allowedContentTypes: [UTType(filenameExtension: "toml") ?? .data, .plainText]
                ) { result in
                    Task { importResult = await load(result) }
                }
                .alert(
                    "Import", isPresented: Binding(get: { importResult != nil }, set: { if !$0 { importResult = nil } })
                ) {
                    Button("OK") {}
                } message: {
                    Text(importResult ?? "")
                }
                .navigationDestination(isPresented: $showSections) { SectionsEditor(store: store) }
                .navigationDestination(isPresented: $showUnits) { UnitsEditor(store: store) }
                .navigationDestination(isPresented: $showTabs) { TabsEditor(tabs: $store.preferences.tabs) }
                .navigationDestination(isPresented: $showCustomize) { CustomizeView(store: store) }
                .navigationDestination(isPresented: $showColors) { ColorsView(store: store) }
                .navigationDestination(isPresented: $showSources) { SourcesView(store: store) }
                .navigationDestination(isPresented: $showCompare) { CompareSourcesView(store: store) }
                .navigationDestination(isPresented: $showCompareModels) {
                    CompareSourcesView(store: store, kind: .models)
                }
                .navigationDestination(item: $showSource) { SourceDetail(store: store, provider: $0) }
                .navigationDestination(isPresented: $showAcknowledgements) { AcknowledgementsView() }
                .onAppear {
                    #if DEBUG
                        let args = ProcessInfo.processInfo.arguments
                        if let i = args.firstIndex(of: "-show"), i + 1 < args.count, args[i + 1] == "sections" {
                            showSections = true
                        }
                        if let i = args.firstIndex(of: "-show"), i + 1 < args.count, args[i + 1] == "units" {
                            showUnits = true
                        }
                        if let i = args.firstIndex(of: "-show"), i + 1 < args.count, args[i + 1] == "tabs" {
                            showTabs = true
                        }
                        if let i = args.firstIndex(of: "-show"), i + 1 < args.count, args[i + 1] == "customize" {
                            showCustomize = true
                        }
                        if let i = args.firstIndex(of: "-show"), i + 1 < args.count, args[i + 1] == "colors" {
                            showColors = true
                        }
                        if let i = args.firstIndex(of: "-show"), i + 1 < args.count, args[i + 1] == "sources" {
                            showSources = true
                        }
                        if let i = args.firstIndex(of: "-show"), i + 1 < args.count, args[i + 1] == "compare" {
                            showCompare = true
                        }
                        if let i = args.firstIndex(of: "-show"), i + 1 < args.count, args[i + 1] == "compare-models" {
                            showCompareModels = true
                        }
                        if let i = args.firstIndex(of: "-show"), i + 1 < args.count,
                            args[i + 1] == "source-openweather"
                        {
                            showSource = Sources.openWeather
                        }
                        if let i = args.firstIndex(of: "-show"), i + 1 < args.count,
                            args[i + 1] == "source-pirate-weather"
                        {
                            showSource = Sources.pirateWeather
                        }
                        if args.contains("-ack") { showAcknowledgements = true }
                    #endif
                }
                .navigationTitle("Settings")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Done") {
                            Task { await store.requestNotificationsIfUseful() }
                            dismiss()
                        }
                    }
                }
                // Back from the Settings app, the permission may have changed.
                .task(id: phase) { await store.updateNotificationStatus() }
                .task {
                    #if DEBUG
                        // Screenshots: `-show settings -scrollTo sources` (or `forecast`, `alerts`, `advanced`).
                        let args = ProcessInfo.processInfo.arguments
                        guard let i = args.firstIndex(of: "-scrollTo"), i + 1 < args.count,
                            ["forecast", "sources", "alerts", "advanced"].contains(args[i + 1])
                        else { return }
                        try? await Task.sleep(for: .milliseconds(400))
                        proxy.scrollTo(args[i + 1], anchor: .top)
                    #endif
                }
            }
        }
    }

    /// Reads the picked file and imports it; what happened, for the alert.
    private func load(_ result: Result<URL, Error>) async -> String {
        do {
            let url = try result.get()
            let scoped = url.startAccessingSecurityScopedResource()
            let text = try String(contentsOf: url, encoding: .utf8)
            if scoped { url.stopAccessingSecurityScopedResource() }
            return try await store.importDesktopConfig(text).joined(separator: "\n")
        } catch WhetherError.Config(let message) {
            return message
        } catch {
            let message = userMessage(error)
            return [message.title, message.detail].compactMap { $0 }.joined(separator: "\n\n")
        }
    }
}

/// The Now tab's sections: drag to reorder, switch off to hide.
struct SectionsEditor: View {
    @Bindable var store: WeatherStore

    var body: some View {
        List {
            Section {
                ForEach($store.preferences.sections) { $choice in
                    Toggle(choice.section.title, isOn: $choice.shown)
                        // The current conditions and alerts always show; they can move.
                        .disabled(!Preferences.hideable(choice.section))
                }
                .onMove { store.preferences.sections.move(fromOffsets: $0, toOffset: $1) }
            } footer: {
                Text("Drag the handles to reorder the Now tab; switch a section off to hide it.")
            }
            Section {
                Button("Restore the default order") {
                    store.preferences.sections = Preferences.defaultSections
                }
            }
        }
        .environment(\.editMode, .constant(.active))
        .navigationTitle("Sections")
        .navigationBarTitleDisplayMode(.inline)
    }
}

/// One unit per quantity, over the Units choice: Automatic keeps that choice's own.
struct UnitsEditor: View {
    @Bindable var store: WeatherStore
    /// Every unit the core can show, with its symbol.
    private let labels = unitLabels()

    var body: some View {
        Form {
            Section {
                picker("Temperature", $store.preferences.unitOverrides.temperature, labels.temperature.map { ($0.unit, $0.label) })
                picker("Wind", $store.preferences.unitOverrides.wind, labels.wind.map { ($0.unit, $0.label) })
                picker("Pressure", $store.preferences.unitOverrides.pressure, labels.pressure.map { ($0.unit, $0.label) })
                picker("Rain and snow", $store.preferences.unitOverrides.precipitation, labels.precipitation.map { ($0.unit, $0.label) })
                picker("Distance", $store.preferences.unitOverrides.distance, labels.distance.map { ($0.unit, $0.label) })
            } footer: {
                Text("Automatic uses the Units choice in Settings: the phone's region, or the one you picked there.")
            }
            Section {
                Button("Back to automatic") { store.preferences.unitOverrides = Preferences.UnitPicks() }
                    .disabled(store.preferences.unitOverrides.isAutomatic)
            }
        }
        .navigationTitle("Custom units")
        .navigationBarTitleDisplayMode(.inline)
    }

    /// A picker of the core's units, Automatic first; the pick is kept as the unit's case name.
    private func picker<T>(_ title: String, _ pick: Binding<String?>, _ options: [(T, String)]) -> some View {
        Picker(title, selection: pick) {
            Text("Automatic").tag(String?.none)
            ForEach(options.map { (String(describing: $0.0), $0.1) }, id: \.0) { id, label in
                Text(label).tag(String?.some(id))
            }
        }
    }
}
