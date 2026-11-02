import SwiftUI
import WhetherKit

/// Your own colors: every role of a custom theme, grouped as Sky, Weather,
/// Temperature, Severity, Precipitation, Radar, Sun & moon and Accent, each with a
/// color picker (two where light and dark backgrounds differ) and a reset button once
/// it differs from the base preset. Above the list: a sample card, "Preview as" a
/// reader with another color vision, and the contrast lines (they warn, never block).
/// Only the roles that differ from the base are saved (`ColorTheme.setting`). Share
/// (top left) sends them as a `.whethertheme` file.
struct ThemeEditor: View {
    @Bindable var store: WeatherStore
    /// The custom theme's id.
    let id: String
    @Environment(\.dismiss) private var dismiss
    @State private var preview = PreviewVision.typical
    @State private var confirmRestore = false

    /// "Preview as": the editor's swatches and sample through a simulation.
    enum PreviewVision: String, CaseIterable, Identifiable {
        case typical, protan, deutan, tritan, noColor
        var id: Self { self }

        var deficiency: Deficiency? {
            switch self {
            case .typical: nil
            case .protan: .protan
            case .deutan: .deutan
            case .tritan: .tritan
            case .noColor: .achromat
            }
        }

        var title: String {
            switch self {
            case .typical: "Typical"
            case .protan: "Protan"
            case .deutan: "Deutan"
            case .tritan: "Tritan"
            case .noColor: "No color"
            }
        }

        /// Who sees it so, for the contrast line.
        var readers: String {
            switch self {
            case .typical: "typical vision"
            case .protan: "protan readers"
            case .deutan: "deutan readers"
            case .tritan: "tritan readers"
            case .noColor: "readers without color"
            }
        }
    }

    private var month: Int { Calendar.current.component(.month, from: .now) }

    private var saved: ColorTheme? { store.preferences.customThemes.first { $0.id == id } }

    var body: some View {
        NavigationStack {
            Group {
                if let saved {
                    editor(saved, base: ColorPresets.preset(saved.base, month: month) ?? ColorPresets.whether)
                } else {
                    ContentUnavailableView("These colors were deleted", systemImage: "paintpalette")
                }
            }
            .navigationTitle(saved?.name ?? "Colors")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                // Send these colors to someone as a `.whethertheme` file.
                if let saved {
                    ToolbarItem(placement: .topBarLeading) { ThemeShareButton(theme: saved) }
                }
                ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } }
            }
        }
        .task {
            #if DEBUG
                // Screenshots: `-previewAs deutan` (or protan, tritan, noColor).
                let args = ProcessInfo.processInfo.arguments
                if let i = args.firstIndex(of: "-previewAs"), i + 1 < args.count,
                    let vision = PreviewVision(rawValue: args[i + 1])
                {
                    preview = vision
                }
            #endif
        }
    }

    private func editor(_ saved: ColorTheme, base: ColorTheme) -> some View {
        let theme = saved.completed(from: base)
        let shown = preview.deficiency.map { theme.simulated(as: $0) } ?? theme
        let current = store.view?.current
        return List {
            Section {
                ThemeSample(
                    theme: shown, sky: store.view?.theme ?? "clear_day", icon: current?.icon ?? "clear_day",
                    temperature: current?.temperature.text)
                    .listRowInsets(EdgeInsets(top: 6, leading: 0, bottom: 6, trailing: 0))
                    .listRowBackground(Color.clear)
                VStack(alignment: .leading, spacing: 6) {
                    Text("Preview as").font(.subheadline).foregroundStyle(.secondary)
                    Picker("Preview as", selection: $preview) {
                        ForEach(PreviewVision.allCases) { Text($0.title).tag($0) }
                    }
                    .pickerStyle(.segmented)
                    .labelsHidden()
                }
                checks(theme)
            } footer: {
                if store.preferences.colorVision != .typical {
                    Text(
                        "Color vision is set to \(store.preferences.colorVision.title): it paints the colors that carry meaning (timeline, temperature, severity, precipitation, radar, twilight) with its own, so changes to those show with Color vision set to Typical."
                    )
                }
            }
            ForEach(ColorRoleGroup.allCases, id: \.self) { group in
                Section(group.title) {
                    ForEach(ColorRole.roles(in: group), id: \.self) { role in
                        RoleRow(
                            role: role, color: theme[role], differs: saved.differs(role, from: base),
                            preview: preview.deficiency,
                            set: { color in update { $0 = $0.setting(role, to: color, base: base) } },
                            reset: { update { $0.colors[role] = nil } })
                    }
                }
            }
            Section {
                Button("Restore the defaults", role: .destructive) { confirmRestore = true }
                    .disabled(saved.colors.isEmpty)
            } footer: {
                Text("Every color goes back to \(base.name)'s.")
            }
        }
        .confirmationDialog(
            "Restore \(base.name)'s colors?", isPresented: $confirmRestore, titleVisibility: .visible
        ) {
            Button("Restore the defaults", role: .destructive) { update { $0.colors = [:] } }
        } message: {
            Text("Every color you changed in \(saved.name) goes back to \(base.name)'s.")
        }
    }

    /// The contrast lines: white text on the skies, marks on their cards, and the
    /// closest neighbouring steps as the preview's readers see them.
    @ViewBuilder private func checks(_ theme: ColorTheme) -> some View {
        let sky = ThemeChecks.skyText(theme)
        let mark = ThemeChecks.mark(theme)
        let apart = ThemeChecks.apart(theme, as: preview.deficiency)
        CheckLine(
            passes: sky.passes,
            text: "White text on the \(sky.role.skyName ?? "") sky: \(ratio(sky.ratio))"
                + (sky.passes ? "" : ", below \(ratio(ThemeChecks.textMinimum))"))
        CheckLine(
            passes: mark.passes,
            text: "\(mark.role.title) (\(mark.role.group.title.lowercased())) on \(cardName(mark.card)): \(ratio(mark.ratio))"
                + (mark.passes ? "" : ", below \(ratio(ThemeChecks.markMinimum))"))
        CheckLine(
            passes: apart.passes,
            text: "\(apart.first.group.title): \(apart.first.title) vs \(apart.second.title) for \(preview.readers): "
                + (apart.passes ? "apart" : "too close"))
    }

    /// A contrast ratio as "4.5:1" (not a weather value: the core never sees it).
    private func ratio(_ value: Double) -> String {
        "\(value.formatted(.number.precision(.fractionLength(1)))):1"
    }

    private func cardName(_ card: ThemeChecks.Card) -> String {
        switch card {
        case .light: "the light background"
        case .dark: "the dark background"
        case .sky(let sky): "the \(sky.replacingOccurrences(of: "_", with: " ")) sky's cards"
        }
    }

    /// Changes this custom theme in place.
    private func update(_ change: (inout ColorTheme) -> Void) {
        guard let i = store.preferences.customThemes.firstIndex(where: { $0.id == id }) else { return }
        change(&store.preferences.customThemes[i])
    }
}

/// One contrast line: a checkmark when it reaches its threshold, a warning below it.
private struct CheckLine: View {
    let passes: Bool
    let text: String

    var body: some View {
        Label {
            Text(text).font(.footnote)
        } icon: {
            Image(systemName: passes ? "checkmark.circle.fill" : "exclamationmark.triangle.fill")
                .foregroundStyle(passes ? .green : .orange)
                .accessibilityLabel(passes ? "Passes" : "Warning")
        }
    }
}

/// One role: its name, a reset button once it differs from the base, the simulated
/// swatches while previewing, and its color picker (light, and dark where it has one).
private struct RoleRow: View {
    let role: ColorRole
    /// Its color in the completed theme.
    let color: ThemeColor
    let differs: Bool
    let preview: Deficiency?
    let set: (ThemeColor) -> Void
    let reset: () -> Void

    var body: some View {
        let hasDark = color.dark.map { $0 != color.light } ?? false
        HStack(spacing: 10) {
            VStack(alignment: .leading, spacing: 2) {
                Text(role.title)
                if hasDark {
                    Text("Light and dark backgrounds").font(.caption).foregroundStyle(.secondary)
                }
            }
            Spacer(minLength: 4)
            if differs {
                Button("Reset \(role.title)", systemImage: "arrow.uturn.backward.circle", action: reset)
                    .labelStyle(.iconOnly)
                    .buttonStyle(.borderless)
            }
            if let preview {
                // How the preview's readers see this color.
                HStack(spacing: 4) {
                    swatch(ColorMath.simulate(color.light, preview))
                    if hasDark, let dark = color.dark { swatch(ColorMath.simulate(dark, preview)) }
                }
                .accessibilityHidden(true)
            }
            ColorPicker(role.title, selection: binding(dark: false), supportsOpacity: color.light.alpha < 1)
                .labelsHidden()
            if hasDark {
                ColorPicker(
                    "\(role.title), dark backgrounds", selection: binding(dark: true),
                    supportsOpacity: (color.dark?.alpha ?? 1) < 1
                )
                .labelsHidden()
            }
        }
    }

    private func swatch(_ c: RGBA) -> some View {
        RoundedRectangle(cornerRadius: 5)
            .fill(Color(c))
            .overlay { RoundedRectangle(cornerRadius: 5).strokeBorder(.secondary.opacity(0.5), lineWidth: 0.5) }
            .frame(width: 22, height: 22)
    }

    /// The picker's color: the light or dark variant. A pick is rounded to the 256
    /// steps a theme saves (so picking the base's color again matches it) and clamped
    /// to sRGB; it replaces a system color with its own.
    private func binding(dark: Bool) -> Binding<Color> {
        Binding(
            get: { Color(color.resolved(dark: dark)) },
            set: { picked in
                guard let rgba = RGBA(hex: UIColor(picked).rgba.clamped.hex),
                    rgba.hex != color.resolved(dark: dark).hex
                else { return }
                var out = ThemeColor(light: color.light, dark: color.dark)
                if dark { out.dark = rgba } else { out.light = rgba }
                set(out)
            })
    }
}
