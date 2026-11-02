import SwiftUI
import WhetherKit

/// Settings → Display → Colors: the presets as a gallery of small skies,
/// each painted in its own colors with the weather now; a tap applies one at once,
/// and the forecast behind crossfades to it, each card shown with the reader's color
/// vision. Below: color vision, and patterns and symbols. With Increase
/// Contrast on, High contrast comes first and is offered at the top. Your own colors
///: "Customize…" (below the presets, or touch and hold a card) makes a custom
/// theme from a preset and opens its editor; custom themes are listed under the presets.
struct ColorsView: View {
    @Bindable var store: WeatherStore
    /// iOS's Increase Contrast: High contrast is offered first.
    @Environment(\.colorSchemeContrast) private var contrast
    /// The custom theme open in the editor.
    @State private var editing: EditedTheme?
    /// The custom theme being renamed, and the name typed.
    @State private var renaming: ColorTheme?
    @State private var newName = ""
    /// The screenshot arguments ran (DEBUG): once, not on every return to the page.
    @State private var launched = false

    private let columns = [GridItem(.flexible(), spacing: 12), GridItem(.flexible(), spacing: 12)]

    /// A custom theme's id, for the editor sheet.
    struct EditedTheme: Identifiable {
        let id: String
    }

    private var month: Int { Calendar.current.component(.month, from: .now) }

    var body: some View {
        let month = month
        let current = store.view?.current
        let vision = store.preferences.colorVision
        ScrollViewReader { proxy in
            Form {
                if suggestsHighContrast {
                    Section {
                        Button("Use High contrast") {
                            withAnimation(.easeInOut(duration: 0.4)) {
                                store.preferences.colorTheme = ColorPresets.highContrastID
                            }
                        }
                    } footer: {
                        Text("Increase Contrast is on. High contrast keeps every text and mark at its strongest.")
                    }
                }
                Section {
                    LazyVGrid(columns: columns, spacing: 12) {
                        // Each preset as this reader will see it: with the color vision's ramps.
                        ForEach(gallery(month: month).map { $0.applying(vision) }) { theme in
                            PresetCard(
                                theme: theme, sky: store.view?.theme ?? "clear_day",
                                icon: current?.icon ?? "clear_day", temperature: current?.temperature.text,
                                selected: theme.base == store.preferences.colorTheme,
                                customize: { customize(theme.base) }
                            ) {
                                withAnimation(.easeInOut(duration: 0.4)) {
                                    store.preferences.colorTheme = theme.base
                                }
                            }
                        }
                    }
                    .listRowInsets(EdgeInsets(top: 6, leading: 4, bottom: 6, trailing: 4))
                    .listRowBackground(Color.clear)
                    customizeButton
                } header: {
                    Text("Presets")
                } footer: {
                    Text(
                        "Each preset paints the skies, the cards and the colors that carry meaning: severity (UV, air quality, alerts), rain and snow, radar. Whether is the original. Seasons changes with the season, like the pug on the home screen. Customize makes your own colors from a preset (or touch and hold one)."
                    )
                }
                if !store.preferences.customThemes.isEmpty {
                    Section {
                        ForEach(store.preferences.customThemes) { theme in
                            customRow(theme, month: month)
                        }
                    } header: {
                        Text("Your colors")
                    } footer: {
                        Text(
                            "Tap to use. Swipe to rename or delete; touch and hold to edit, rename, duplicate, share or delete. Colors you didn't change follow the preset they came from."
                        )
                    }
                    .id("custom")
                }
                Section {
                    Picker("Color vision", selection: $store.preferences.colorVision) {
                        ForEach(ColorVision.allCases, id: \.self) { Text($0.title).tag($0) }
                    }
                    Picker("Patterns and symbols", selection: $store.preferences.patterns) {
                        ForEach(Preferences.PatternsChoice.allCases) { Text($0.title).tag($0) }
                    }
                } header: {
                    Text("Color vision")
                } footer: {
                    Text(
                        "Color vision replaces the colors that carry meaning with ones that stay apart for that kind of color vision, with any preset; the skies stay the preset's. Patterns and symbols add a second sign beside those colors: a hatch on snow, a word on severity. Automatic follows Differentiate Without Color (iOS Settings › Accessibility › Display & Text Size); No color and Graphite always show them."
                    )
                }
            }
            .task { await screenshotArguments(proxy) }
        }
        .navigationTitle("Colors")
        .navigationBarTitleDisplayMode(.inline)
        .sheet(item: $editing) { ThemeEditor(store: store, id: $0.id) }
        .alert(
            "Rename colors",
            isPresented: Binding(get: { renaming != nil }, set: { if !$0 { renaming = nil } })
        ) {
            TextField("Name", text: $newName)
            Button("Rename") {
                if let renaming, let name = CustomThemes.cleanName(newName) {
                    update(renaming.id) { $0.name = name }
                }
            }
            .disabled(CustomThemes.cleanName(newName) == nil)
            Button("Cancel", role: .cancel) {}
        }
    }

    /// Below the presets: customize the chosen preset, or edit the chosen custom theme.
    @ViewBuilder private var customizeButton: some View {
        let chosen = store.preferences.colorTheme
        if let custom = store.preferences.customThemes.first(where: { $0.id == chosen }) {
            Button("Edit \(custom.name)…", systemImage: "paintbrush") { editing = EditedTheme(id: custom.id) }
        } else {
            let preset = ColorPresets.preset(chosen, month: month) ?? ColorPresets.whether
            Button("Customize \(preset.name)…", systemImage: "paintbrush") { customize(preset.base) }
        }
    }

    /// A custom theme's row: tap to use it; the brush opens its editor.
    private func customRow(_ theme: ColorTheme, month: Int) -> some View {
        let selected = store.preferences.colorTheme == theme.id
        let full = ColorPresets.completed(theme, month: month).applying(store.preferences.colorVision)
        return HStack(spacing: 12) {
            Button {
                withAnimation(.easeInOut(duration: 0.4)) { store.preferences.colorTheme = theme.id }
            } label: {
                HStack(spacing: 12) {
                    ThemeSwatch(theme: full, sky: store.view?.theme ?? "clear_day")
                    Text(theme.name).foregroundStyle(.primary)
                    Spacer(minLength: 4)
                    if selected {
                        Image(systemName: "checkmark").foregroundStyle(.tint).accessibilityHidden(true)
                    }
                }
                .contentShape(.rect)
            }
            .buttonStyle(.plain)
            .accessibilityAddTraits(selected ? .isSelected : [])
            Button("Edit", systemImage: "paintbrush") { editing = EditedTheme(id: theme.id) }
                .labelStyle(.iconOnly)
                .buttonStyle(.borderless)
        }
        .swipeActions(edge: .trailing) {
            Button("Delete", role: .destructive) { delete(theme) }
            Button("Rename") { startRenaming(theme) }
        }
        .contextMenu {
            Button("Edit…", systemImage: "paintbrush") { editing = EditedTheme(id: theme.id) }
            Button("Rename", systemImage: "pencil") { startRenaming(theme) }
            Button("Duplicate", systemImage: "plus.square.on.square") { duplicate(theme) }
            ThemeShareButton(theme: theme, title: "Share…")
            Button("Delete", systemImage: "trash", role: .destructive) { delete(theme) }
        }
    }

    /// Makes "My <Preset>" from the preset `id`, uses it and opens its editor.
    @discardableResult
    private func customize(_ id: String, open: Bool = true) -> ColorTheme? {
        guard let preset = ColorPresets.preset(id, month: month) else { return nil }
        let theme = CustomThemes.duplicate(preset, existing: store.preferences.customThemes)
        var prefs = store.preferences
        prefs.customThemes.append(theme)
        prefs.colorTheme = theme.id
        withAnimation(.easeInOut(duration: 0.4)) { store.preferences = prefs }
        if open { editing = EditedTheme(id: theme.id) }
        return theme
    }

    /// A copy of a custom theme, listed after it.
    private func duplicate(_ theme: ColorTheme) {
        let themes = store.preferences.customThemes
        let copy = CustomThemes.duplicate(theme, existing: themes)
        let at = (themes.firstIndex { $0.id == theme.id } ?? themes.count - 1) + 1
        withAnimation { store.preferences.customThemes.insert(copy, at: at) }
    }

    /// Deletes a custom theme; while it is in use, its preset takes over.
    private func delete(_ theme: ColorTheme) {
        var prefs = store.preferences
        prefs.customThemes.removeAll { $0.id == theme.id }
        if prefs.colorTheme == theme.id { prefs.colorTheme = theme.base }
        withAnimation { store.preferences = prefs }
    }

    private func startRenaming(_ theme: ColorTheme) {
        newName = theme.name
        renaming = theme
    }

    /// Changes the custom theme `id` in place.
    private func update(_ id: String, _ change: (inout ColorTheme) -> Void) {
        guard let i = store.preferences.customThemes.firstIndex(where: { $0.id == id }) else { return }
        change(&store.preferences.customThemes[i])
    }

    /// Screenshots (DEBUG, for the launch only; `Preferences.save` skips them):
    /// `-customTheme aurora` makes "My Aurora", `-editTheme aurora` also opens its editor
    /// with the clear day's top changed to a pale blue (so a reset button and a contrast
    /// warning show), and `-scrollTo custom` shows the custom themes.
    private func screenshotArguments(_ proxy: ScrollViewProxy) async {
        #if DEBUG
            guard !launched else { return }
            launched = true
            let args = ProcessInfo.processInfo.arguments
            if let i = args.firstIndex(where: { $0 == "-customTheme" || $0 == "-editTheme" }), i + 1 < args.count {
                try? await Task.sleep(for: .milliseconds(300))
                let edit = args[i] == "-editTheme"
                if let theme = customize(args[i + 1], open: edit), edit {
                    update(theme.id) { $0.colors[.skyClearDayTop] = ThemeColor(light: RGBA(rgb: 0x8FC8FF)) }
                }
            }
            if let i = args.firstIndex(of: "-scrollTo"), i + 1 < args.count, args[i + 1] == "custom" {
                try? await Task.sleep(for: .milliseconds(600))
                proxy.scrollTo("custom", anchor: .top)
            }
        #endif
    }

    /// The presets in order; High contrast first when Increase Contrast is on.
    private func gallery(month: Int) -> [ColorTheme] {
        let all = ColorPresets.gallery(month: month)
        guard contrast == .increased else { return all }
        return all.filter { $0.base == ColorPresets.highContrastID }
            + all.filter { $0.base != ColorPresets.highContrastID }
    }

    /// Increase Contrast is on and another preset is chosen.
    private var suggestsHighContrast: Bool {
        contrast == .increased && store.preferences.colorTheme != ColorPresets.highContrastID
    }
}

/// One preset in the gallery: its sample (`ThemeSample`) as a button that applies it,
/// ringed when chosen; touch and hold to use it or customize it.
private struct PresetCard: View {
    let theme: ColorTheme
    let sky: String
    let icon: String
    let temperature: String?
    let selected: Bool
    let customize: () -> Void
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            ThemeSample(theme: theme, sky: sky, icon: icon, temperature: temperature, selected: selected)
                .overlay {
                    RoundedRectangle(cornerRadius: 17)
                        .strokeBorder(selected ? Color.accentColor : .clear, lineWidth: 3)
                        .padding(-3)
                }
        }
        .buttonStyle(.plain)
        .contextMenu {
            Button("Use", systemImage: "checkmark.circle", action: action)
            Button("Customize…", systemImage: "paintbrush", action: customize)
        }
        .accessibilityLabel(theme.name)
        .accessibilityAddTraits(selected ? .isSelected : [])
        .accessibilityAction(named: "Customize", customize)
    }
}

/// A theme's sample: a small sky in its colors with the weather now's icon and
/// temperature (the core's text), its severity scale as a pill, and a sample of rain
/// turning to snow as a sparkline. The gallery's cards and the editor's sample.
struct ThemeSample: View {
    let theme: ColorTheme
    let sky: String
    let icon: String
    let temperature: String?
    var selected = false
    @Environment(\.showsPatterns) private var patterns

    /// A made-up spell of precipitation for the sparkline: how high each bar is (0…1)
    /// and its kind, rain turning to mixed, then snow.
    private static let sample: [(height: Double, role: ColorRole)] = [
        (0.30, .precipRain), (0.55, .precipRain), (0.85, .precipRain), (1.00, .precipRain),
        (0.75, .precipMixed), (0.60, .precipMixed), (0.50, .precipSnow), (0.65, .precipSnow),
        (0.45, .precipSnow), (0.25, .precipSnow),
    ]

    var body: some View {
        let (top, bottom) = theme.sky(sky)
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text(theme.name)
                    .font(.subheadline.weight(.semibold))
                    .lineLimit(1)
                    .minimumScaleFactor(0.8)
                Spacer(minLength: 4)
                if selected {
                    Image(systemName: "checkmark.circle.fill").accessibilityHidden(true)
                }
            }
            HStack(spacing: 8) {
                ConditionIcon(icon: icon, size: 24)
                if let temperature {
                    Text(temperature).font(.title2.weight(.light)).monospacedDigit()
                }
            }
            HStack(alignment: .bottom, spacing: 8) {
                // The severity scale, good to extreme.
                HStack(spacing: 0) {
                    ForEach(ColorRole.levels, id: \.self) { Rectangle().fill(theme.color($0)) }
                }
                .frame(width: 54, height: 10)
                .clipShape(.capsule)
                Spacer(minLength: 0)
                HStack(alignment: .bottom, spacing: 2) {
                    ForEach(Self.sample.indices, id: \.self) { i in
                        let role = Self.sample[i].role
                        Capsule()
                            .fill(theme.color(role))
                            .overlay {
                                // Mixed and snow hatched, as the precipitation bars are.
                                if (patterns || theme.patterns) && role != .precipRain {
                                    Hatch(spacing: 3)
                                        .stroke(theme.hatch(on: role, dark: true), lineWidth: 1)
                                        .clipShape(.capsule)
                                }
                            }
                            .frame(width: 3, height: 4 + 18 * Self.sample[i].height)
                    }
                }
            }
            .padding(8)
            .background(theme.card(sky).opacity(0.5), in: .rect(cornerRadius: 8))
        }
        .foregroundStyle(.white)
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            LinearGradient(colors: [top, bottom], startPoint: .top, endPoint: .bottom),
            in: .rect(cornerRadius: 14)
        )
        // The theme's own colors, as on its sky (always dark, like the forecast's).
        .environment(\.colorTheme, theme)
        .environment(\.colorScheme, .dark)
    }
}

/// A custom theme's small swatch in the list: its sky with the severity scale below.
private struct ThemeSwatch: View {
    let theme: ColorTheme
    let sky: String

    var body: some View {
        let (top, bottom) = theme.sky(sky)
        VStack(spacing: 0) {
            LinearGradient(colors: [top, bottom], startPoint: .top, endPoint: .bottom)
            HStack(spacing: 0) {
                ForEach(ColorRole.levels, id: \.self) { Rectangle().fill(theme.color($0)) }
            }
            .frame(height: 5)
        }
        .frame(width: 40, height: 28)
        .clipShape(.rect(cornerRadius: 6))
        .environment(\.colorScheme, .dark)
        .accessibilityHidden(true)
    }
}
