import MapKit
import SwiftUI
import WhetherKit

/// The latest radar frame around the place (NEXRAD in the US, OpenWeather's layer
/// elsewhere with a key); tap to open it.
struct RadarSection: View {
    @Environment(\.colorTheme) private var colors
    let place: AppPlace
    let settings: Settings
    @State private var loop = RadarLoop()
    @State private var fullScreen = false
    @State private var editingOpacity = false
    @State private var editingOptions = false
    @Environment(\.radarOptions) private var radarOptions

    private var center: CLLocationCoordinate2D {
        CLLocationCoordinate2D(latitude: place.latitude, longitude: place.longitude)
    }

    private var source: RadarSource { settings.radarSource(for: place) }

    var body: some View {
        let options = radarOptions.wrappedValue.forLaunch
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                SectionTitle(text: "Radar")
                Spacer()
                if let label = loop.labels.last {
                    Text(label).font(.caption).foregroundStyle(.secondary)
                }
            }
            Button {
                fullScreen = true
            } label: {
                RadarMap(
                    center: center, frames: loop.frames, index: loop.frames.count - 1,
                    interactive: false, span: options.startZoom.cardSpan,
                    maximumZ: source == .openWeather ? Int(radarOpenweatherMaxZoom()) : 20,
                    style: options.style, opacity: options.opacity
                )
                .frame(height: 220)
                .clipShape(.rect(cornerRadius: 12))
                .overlay(alignment: .bottomTrailing) {
                    // OpenWeather's layer is one frame: there is no loop to play.
                    Image(systemName: source == .openWeather ? "arrow.up.left.and.arrow.down.right.circle.fill" : "play.circle.fill")
                        .font(.title)
                        .symbolRenderingMode(.palette)
                        .foregroundStyle(.white, colors.rain)
                        .padding(10)
                }
            }
            .buttonStyle(.plain)
            .accessibilityLabel(source == .openWeather ? "Open the radar" : "Play the radar loop")
            if source == .openWeather {
                Text(radarOpenweatherCredit()).font(.caption2).foregroundStyle(.secondary)
            }
            if let failure = loop.failure {
                Text(failure).font(.caption).foregroundStyle(.orange)
            }
        }
        // Touch and hold: the map, the speed, how far in, the opacity.
        .contextMenu {
            RadarOptionsMenu(options: radarOptions.forLaunch) { editingOpacity = true }
            // Customize… opens the options page; Edit the tab, Hide.
            CardMenuButtons { editingOptions = true }
        }
        .sheet(isPresented: $editingOpacity) { RadarOpacitySheet(options: radarOptions.forLaunch) }
        .sheet(isPresented: $editingOptions) {
            RadarOptionsEditor(options: radarOptions.forLaunch, sheet: true)
                .presentationDetents([.medium, .large])
        }
        .task(id: RadarTaskKey(place: place, settings: settings)) {
            await loop.load(place: place, settings: settings)
            #if DEBUG
                // Screenshots: `-show radar` opens the loop.
                let args = ProcessInfo.processInfo.arguments
                if let i = args.firstIndex(of: "-show"), args.indices.contains(i + 1),
                    args[i + 1] == "radar"
                {
                    fullScreen = true
                }
            #endif
        }
        .fullScreenCover(isPresented: $fullScreen) {
            RadarScreen(place: place, loop: loop, source: source)
        }
    }
}

/// The radar loop full screen: pan and zoom, play or pause, the frame's time.
struct RadarScreen: View {
    @Environment(\.colorTheme) private var colors
    let place: AppPlace
    @Bindable var loop: RadarLoop
    /// A close button (full screen), not in the Radar tab.
    var closable = true
    /// Where the radar comes from; with none, the globe and a note.
    var source: RadarSource = .nexrad
    @Environment(\.dismiss) private var dismiss
    @Environment(\.showsPatterns) private var patterns
    @Environment(\.colorVision) private var vision
    @State private var playing = true
    @State private var step = 0
    /// Apple's satellite Earth, zoomed out to the globe.
    @State private var globe = false
    @State private var started = false
    @State private var editingOpacity = false
    @State private var editingOptions = false
    @Environment(\.radarOptions) private var radarOptions

    var body: some View {
        let options = radarOptions.wrappedValue.forLaunch
        ZStack {
            RadarMap(
                center: CLLocationCoordinate2D(latitude: place.latitude, longitude: place.longitude),
                frames: loop.frames, index: loop.index,
                // About 150 km across (county scale) unless the options say otherwise.
                span: options.startZoom.span, globe: globe,
                maximumZ: source == .openWeather ? Int(radarOpenweatherMaxZoom()) : 20,
                style: options.style, opacity: options.opacity
            )
            .ignoresSafeArea()
            VStack {
                HStack {
                    Spacer()
                    VStack(spacing: 10) {
                        if closable {
                            Button {
                                dismiss()
                            } label: {
                                Image(systemName: "xmark")
                                    .font(.headline)
                                    .padding(12)
                                    .background(.regularMaterial, in: .circle)
                            }
                            .buttonStyle(.plain)
                            .accessibilityLabel("Close")
                        }
                        Button {
                            withAnimation { globe.toggle() }
                        } label: {
                            Image(systemName: globe ? "map" : "globe.americas.fill")
                                .font(.headline)
                                .padding(12)
                                .background(.regularMaterial, in: .circle)
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel(globe ? "Back to the map" : "Show the globe")
                        // With no radar there is nothing to set but the globe.
                        if source != .none { optionsMenu }
                    }
                }
                .padding(.top, closable ? 0 : 70)
                Spacer()
                switch source {
                case .nexrad: controls
                case .openWeather: openWeatherCredit
                case .none:
                    Text(
                        "Radar covers the lower 48 US states. With an OpenWeather key it shows precipitation worldwide."
                    )
                    .font(.footnote)
                    .multilineTextAlignment(.center)
                    .padding(14)
                    .background(.regularMaterial, in: .rect(cornerRadius: 20))
                    .accessibilityIdentifier("radar-us-only")
                }
            }
            .padding()
        }
        .sheet(isPresented: $editingOpacity) { RadarOpacitySheet(options: radarOptions.forLaunch) }
        .sheet(isPresented: $editingOptions) {
            RadarOptionsEditor(options: radarOptions.forLaunch, sheet: true)
                .presentationDetents([.medium, .large])
        }
        .task {
            // Outside the US there is only the Earth to see; in it, the
            // radar opens close in (the globe button flies out).
            if source == .none && !started {
                started = true
                try? await Task.sleep(for: .milliseconds(400))
                globe = true
            }
            #if DEBUG
                // Screenshots: `-globe` starts on the globe.
                let args = ProcessInfo.processInfo.arguments
                if args.contains("-globe") {
                    try? await Task.sleep(for: .seconds(1))
                    globe = true
                }
                // `-radarsheet opacity` or `options` opens that sheet.
                if let i = args.firstIndex(of: "-radarsheet"), args.indices.contains(i + 1) {
                    try? await Task.sleep(for: .seconds(1))
                    editingOpacity = args[i + 1] == "opacity"
                    editingOptions = args[i + 1] == "options"
                }
            #endif
        }
        .task(id: LoopKey(playing: playing, milliseconds: options.speed.frameMilliseconds)) {
            // One frame (OpenWeather's layer) has nothing to step through.
            guard playing, loop.frames.count > 1 else { return }
            step = loop.index
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(options.speed.frameMilliseconds))
                loop.tick(&step)
            }
        }
    }

    /// The radar's quick choices, and a page with all of them.
    private var optionsMenu: some View {
        Menu {
            RadarOptionsMenu(options: radarOptions.forLaunch) { editingOpacity = true }
            Divider()
            Button("All radar options…", systemImage: "slider.horizontal.3") {
                editingOptions = true
            }
        } label: {
            Image(systemName: "slider.horizontal.3")
                .font(.headline)
                .padding(12)
                .background(.regularMaterial, in: .circle)
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Radar options")
    }

    /// OpenWeather's layer: the current frame, its credit, no loop controls.
    private var openWeatherCredit: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text("Precipitation now").font(.headline)
            Text(radarOpenweatherCredit())
                .font(.caption2)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding()
        .background(.regularMaterial, in: .rect(cornerRadius: 20))
    }

    /// The legend as five labeled steps rather than a gradient: whenever the tiles are
    /// repainted in the theme's steps (`RadarPalette.repaintRamp`, as `RadarMap` does).
    private var stepped: Bool { RadarPalette.repaintRamp(colors, vision: vision, patterns: patterns) != nil }

    private var controls: some View {
        VStack(spacing: 10) {
            HStack(spacing: 14) {
                Button {
                    playing.toggle()
                } label: {
                    Image(systemName: playing ? "pause.fill" : "play.fill")
                        .font(.title2)
                        .frame(width: 28)
                }
                .accessibilityLabel(playing ? "Pause" : "Play")
                Text(loop.labels.indices.contains(loop.index) ? loop.labels[loop.index] : "")
                    .font(.headline.monospacedDigit())
                Spacer()
                if !stepped { RadarLegend() }
            }
            // Repainted (another theme, color vision or patterns): five labeled steps.
            if stepped { RadarSteps() }
            if loop.frames.count > 1 {
                // Dragging pauses the loop and steps through the frames.
                Slider(
                    value: Binding(
                        get: { Double(loop.index) },
                        set: {
                            playing = false
                            loop.index = Int($0.rounded())
                        }),
                    in: 0...Double(loop.frames.count - 1), step: 1
                )
                .tint(colors.rain)
                .accessibilityLabel("Radar time")
                .accessibilityValue(
                    loop.labels.indices.contains(loop.index) ? loop.labels[loop.index] : "")
            }
            Text(radarAttribution(onAppleMap: true))
                .font(.caption2)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding()
        .background(.regularMaterial, in: .rect(cornerRadius: 20))
        .sensoryFeedback(.selection, trigger: loop.index) { _, _ in !playing }
    }
}

/// What restarts the loop's timer: pausing, playing, or a new speed.
private struct LoopKey: Equatable {
    var playing: Bool
    var milliseconds: Int
}

/// What the radar's colors mean: NOAA's own scale from light (green) to heavy (red
/// and magenta). Shown only over NOAA's unrepainted tiles, in Whether's radar colors
/// (`RadarPalette.repaintRamp`), so the words below say what the map shows.
struct RadarLegend: View {
    @Environment(\.colorTheme) private var colors
    var body: some View {
        HStack(spacing: 6) {
            Text("Light").font(.caption2).foregroundStyle(.secondary).fixedSize()
            Capsule()
                .fill(
                    LinearGradient(
                        colors: ColorRole.radar.map(colors.color), startPoint: .leading,
                        endPoint: .trailing)
                )
                .frame(width: 56, height: 6)
            Text("Heavy").font(.caption2).foregroundStyle(.secondary).fixedSize()
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Green is light, red and purple heavy")
    }
}

/// The radar's five steps, each a swatch with its word (the core's), light to heavy:
/// the legend whenever the tiles are repainted in the theme's steps (another theme,
/// color vision, or patterns and symbols), at the words' own bands (`RadarPalette`:
/// below 20 dBZ, to 35, 45, 55, and above).
struct RadarSteps: View {
    @Environment(\.colorTheme) private var colors

    var body: some View {
        let words = radarStepWords(language: CoreLanguage.current)
        HStack(spacing: 3) {
            ForEach(ColorRole.radar.indices, id: \.self) { i in
                VStack(spacing: 3) {
                    RoundedRectangle(cornerRadius: 2)
                        .fill(Color(colors[ColorRole.radar[i]].light))  // the tiles' own color
                        .frame(height: 8)
                    Text(words.indices.contains(i) ? words[i] : "")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .minimumScaleFactor(0.7)
                }
                .frame(maxWidth: .infinity)
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Radar, light to heavy")
        .accessibilityValue(words.joined(separator: ", "))
    }
}
