import SwiftUI
import WhetherKit

extension EnvironmentValues {
    /// The radar's options (`Preferences.radar`), for the radar to read and its
    /// menu to change. Set once, by the forecast screen.
    @Entry var radarOptions: Binding<RadarOptions> = .constant(RadarOptions())
}

extension RadarOptions {
    /// These options, with a screenshot launch's arguments over them (debug
    /// builds only; nothing is saved).
    var forLaunch: RadarOptions {
        #if DEBUG
            return overridden(by: ProcessInfo.processInfo.arguments)
        #else
            return self
        #endif
    }
}

extension Binding where Value == RadarOptions {
    /// This binding as the radar shows it: with a screenshot launch's arguments
    /// over the saved options (debug builds only); writes go through unchanged.
    var forLaunch: Binding<RadarOptions> {
        Binding(get: { wrappedValue.forLaunch }, set: { wrappedValue = $0 })
    }
}

/// The radar's quick choices: the map under it, the loop's speed, how far in it
/// opens, and a way to the opacity. The content of the radar screen's menu and of
/// the card's touch and hold menu.
struct RadarOptionsMenu: View {
    @Binding var options: RadarOptions
    /// Opens the opacity sheet.
    var opacity: () -> Void

    var body: some View {
        Picker(selection: $options.style) {
            ForEach(RadarOptions.MapStyle.allCases) { Label($0.title, systemImage: $0.symbol).tag($0) }
        } label: {
            Label("Map style", systemImage: "map")
        }
        .pickerStyle(.menu)
        Picker(selection: $options.speed) {
            ForEach(RadarOptions.LoopSpeed.allCases) { Text($0.title).tag($0) }
        } label: {
            Label("Loop speed", systemImage: "speedometer")
        }
        .pickerStyle(.menu)
        Picker(selection: $options.startZoom) {
            ForEach(RadarOptions.StartZoom.allCases) { Text($0.title).tag($0) }
        } label: {
            Label("Start zoomed to", systemImage: "scope")
        }
        .pickerStyle(.menu)
        Button("Opacity…", systemImage: "circle.lefthalf.filled", action: opacity)
    }
}

/// How strongly the rain is drawn: a slider in a small sheet that leaves the map
/// behind it live, so the change shows as it is made.
struct RadarOpacitySheet: View {
    @Environment(\.colorTheme) private var colors
    @Binding var options: RadarOptions
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text("Radar opacity").font(.headline)
                Spacer()
                Button("Done") { dismiss() }.fontWeight(.semibold)
            }
            HStack(spacing: 12) {
                Image(systemName: "circle").foregroundStyle(.secondary)
                Slider(value: $options.opacity, in: RadarOptions.opacityRange)
                    .tint(colors.rain)
                    .accessibilityLabel("Radar opacity")
                    .accessibilityValue(percent)
                Image(systemName: "circle.fill").foregroundStyle(.secondary)
            }
            HStack {
                Text(percent).font(.callout.monospacedDigit())
                Spacer()
                Button("Restore the default") { options.opacity = RadarOptions.defaultOpacity }
                    .font(.callout)
                    .disabled(options.opacity == RadarOptions.defaultOpacity)
            }
        }
        .padding()
        .frame(maxHeight: .infinity, alignment: .top)
        .presentationDetents([.height(190)])
        .presentationBackgroundInteraction(.enabled)
        .presentationBackground(.regularMaterial)
    }

    private var percent: String { opacityText(options.opacity) }
}

/// The radar's choices as a page: Settings → Display → Radar.
struct RadarOptionsEditor: View {
    @Environment(\.colorTheme) private var colors
    @Binding var options: RadarOptions
    var sheet = false
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        if sheet {
            NavigationStack {
                form.toolbar {
                    ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } }
                }
            }
        } else {
            form
        }
    }

    private var form: some View {
        Form {
            Section {
                Picker("Map style", selection: $options.style) {
                    ForEach(RadarOptions.MapStyle.allCases) { Label($0.title, systemImage: $0.symbol).tag($0) }
                }
                .pickerStyle(.navigationLink)
            } footer: {
                Text("The map under the rain. Muted is the quiet standard map the radar always had.")
            }
            Section {
                Picker("Loop speed", selection: $options.speed) {
                    ForEach(RadarOptions.LoopSpeed.allCases) { Text($0.title).tag($0) }
                }
                .pickerStyle(.segmented)
            } header: {
                Text("Loop speed")
            } footer: {
                Text("How quickly the frames of the past hour step by.")
            }
            Section {
                HStack(spacing: 12) {
                    Slider(value: $options.opacity, in: RadarOptions.opacityRange)
                        .tint(colors.rain)
                        .accessibilityLabel("Radar opacity")
                    Text(opacityText(options.opacity))
                        .font(.callout.monospacedDigit())
                        .frame(width: 48, alignment: .trailing)
                }
            } header: {
                Text("Opacity")
            }
            Section {
                Picker("Start zoomed to", selection: $options.startZoom) {
                    ForEach(RadarOptions.StartZoom.allCases) { Text($0.title).tag($0) }
                }
                .pickerStyle(.segmented)
            } header: {
                Text("Start zoomed to")
            } footer: {
                Text("How far in the radar opens: about 50 km across for the city, 150 for the region and 450 for the state.")
            }
            Section {
                Button("Restore the defaults") { options = RadarOptions() }
                    .disabled(options == RadarOptions())
            }
        }
        .navigationTitle("Radar")
        .navigationBarTitleDisplayMode(.inline)
    }
}

/// The radar's opacity as a percentage, written by the core (Swift
/// does not format values).
func opacityText(_ opacity: Double) -> String {
    chanceText(percent: UInt8(clamping: Int((opacity * 100).rounded())))
}
