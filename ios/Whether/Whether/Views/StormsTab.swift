import SwiftUI
import WhetherKit

/// The coming storms at the place shown, several weather models side by side: their
/// low-pressure tracks on a map with a time scrubber, and in a drawer below what each
/// model brings, how well they agree, the ensembles' spread and how the runs moved.
/// Loads only while it shows.
struct StormsTab: View {
    @Bindable var store: WeatherStore
    /// Closes the screen when it is shown full screen (from the Storm ahead card); `nil`
    /// in the Storms tab.
    var close: (() -> Void)?
    @State private var storms = StormsStore()
    @Environment(\.colorTheme) private var colors

    private var place: AppPlace? {
        store.places.indices.contains(store.active) ? store.places[store.active] : store.places.first
    }

    /// The core's settings: units, clock and dates (Open-Meteo needs no key).
    private var settings: Settings { store.preferences.settings(withKey: false) }

    var body: some View {
        Group {
            if let view = storms.view {
                StormsScreen(
                    view: view, stale: storms.stale, failure: storms.failure, loading: storms.loading,
                    close: close
                ) { await storms.loadPastRuns(for: $0) }
            } else if let failure = storms.failure {
                ContentUnavailableView {
                    Label(failure, systemImage: "hurricane")
                } description: {
                    Text("The storm outlook couldn't be fetched.")
                } actions: {
                    Button("Try again") { Task { await load() } }
                        .buttonStyle(.bordered)
                    if let detail = storms.failureDetail {
                        DisclosureGroup("Details") { Text(detail).font(.caption) }
                            .padding(.horizontal, 32)
                    }
                }
                .overlay(alignment: .topTrailing) { CloseButton(close: close).padding() }
            } else {
                VStack(spacing: 12) {
                    ProgressView()
                    Text("Fetching the storm outlook…")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .accessibilityElement(children: .combine)
                .overlay(alignment: .topTrailing) { CloseButton(close: close).padding() }
            }
        }
        .environment(
            \.skyCard,
            store.preferences.background == .sky ? store.view.map { colors.card($0.theme) } : nil)
        .task(id: place) { await load() }
        .onChange(of: settings) { storms.rebuild(settings: settings) }
        .task {
            // The steps' "Now" and the events' days move on with the clock, and a model
            // may have a newer run: `load` reads the run times only when they are due
            // (`StormsRules.isDue`, every 15 minutes), so most ticks send nothing.
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(300))
                guard !Task.isCancelled else { break }
                if storms.view != nil { await load() }
                storms.rebuild()
            }
        }
    }

    private func load() async {
        guard let place else { return }
        await storms.load(place: place, settings: settings)
    }
}

/// The close button of the full-screen Storms screen (none in the tab).
private struct CloseButton: View {
    let close: (() -> Void)?

    var body: some View {
        if let close {
            Button(action: close) {
                Image(systemName: "xmark")
                    .font(.headline)
                    .padding(12)
                    .background(.regularMaterial, in: .circle)
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Close")
        }
    }
}

/// The map filling the screen, the model chips over its top, the scrubber and the
/// drawer over its bottom. The drawer is a panel in the screen rather than a sheet: a
/// sheet is presented over the whole window, where it would cover the tab bar and stay
/// up on the other tabs.
private struct StormsScreen: View {
    let view: StormsView
    let stale: Bool
    let failure: String?
    let loading: Bool
    let close: (() -> Void)?
    let loadPastRuns: (StormsView.Event) async -> Void
    @Environment(\.colorTheme) private var colors
    /// The models the chips hide, by model id: for this screen only, never saved.
    @State private var hidden: Set<String> = []
    @State private var step = 0
    @State private var playing = false
    @State private var expanded = false
    /// The drawer's title row and the bottom of the agreement card (or the summary)
    /// below it: together, the drawer's small height.
    @State private var titleHeight: CGFloat = 44
    @State private var peekBottom: CGFloat = 120
    @State private var scrubberHeight: CGFloat = 110
    @State private var chipsHeight: CGFloat = 44
    @State private var started = false

    var body: some View {
        GeometryReader { geo in
            let peek = titleHeight + peekBottom + 16
            // Up to the chips, which stay to show and hide the plumes.
            let large = max(peek, geo.size.height - chipsHeight - 16)
            ZStack(alignment: .bottom) {
                StormsMap(
                    view: view, step: step, hidden: hidden,
                    insets: UIEdgeInsets(
                        top: geo.safeAreaInsets.top + chipsHeight + 16, left: 24,
                        bottom: geo.safeAreaInsets.bottom + peek + scrubberHeight + 24, right: 24)
                )
                .ignoresSafeArea()
                VStack(spacing: 8) {
                    topRow
                        .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { chipsHeight = $0 }
                    Spacer(minLength: 0)
                    if !expanded {
                        scrubber
                            .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { scrubberHeight = $0 }
                            .transition(.opacity)
                    }
                    drawer(height: expanded ? large : peek)
                }
                .padding(.horizontal, 8)
            }
        }
        .task(id: view.steps.count) { await start() }
        .task(id: playing) {
            guard playing, view.steps.count > 1 else { return }
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(700))
                guard !Task.isCancelled else { return }
                step = (step + 1) % view.steps.count
            }
        }
    }

    /// The scrubber's first step, and a launch's choices for screenshots.
    private func start() async {
        guard !started else { return }
        started = true
        var wanted: Int?
        #if DEBUG
            // `-stormstep 8` starts the scrubber at that step; `-stormsheet large` opens the
            // drawer, as `-scrollTo storms-plumes` (or `storms-runs`, `storms-credits`) does.
            let args = ProcessInfo.processInfo.arguments
            if let i = args.firstIndex(of: "-stormstep"), i + 1 < args.count { wanted = Int(args[i + 1]) }
            if let i = args.firstIndex(of: "-stormsheet"), i + 1 < args.count, args[i + 1] == "large" {
                expanded = true
            }
            if args.contains("-scrollTo") { expanded = true }
        #endif
        step = StormMapRules.startStep(wanted, count: view.steps.count)
    }

    /// The model chips (tap one to show or hide it) and, full screen, the close button.
    private var topRow: some View {
        HStack(alignment: .top, spacing: 8) {
            // Every chip with its run when they fit, else without the runs, else scrolling.
            ViewThatFits(in: .horizontal) {
                chips(runs: true)
                chips(runs: false)
                ScrollView(.horizontal, showsIndicators: false) { chips(runs: false) }
                    .scrollClipDisabled()
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            CloseButton(close: close)
        }
        .padding(.top, 6)
    }

    private func chips(runs: Bool) -> some View {
        HStack(spacing: 6) {
            ForEach(view.models) { chip in
                ModelToggle(chip: chip, shown: !hidden.contains(chip.model), showsRun: runs) {
                    withAnimation(.snappy) { hidden = StormMapRules.toggled(hidden, chip.model) }
                }
            }
        }
        .padding(.vertical, 2)
        .fixedSize()
    }

    /// Play, the step's time and the slider over the steps; the data credits under them
    /// (Apple's map shows its own).
    private var scrubber: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 12) {
                if view.steps.count > 1 {
                    Button {
                        playing.toggle()
                    } label: {
                        Image(systemName: playing ? "pause.fill" : "play.fill")
                            .font(.title3)
                            .frame(width: 26)
                    }
                    .accessibilityLabel(playing ? "Pause" : "Play")
                }
                Text(view.steps.indices.contains(step) ? view.steps[step].label : "")
                    .font(.subheadline.weight(.semibold).monospacedDigit())
                Spacer(minLength: 0)
            }
            if view.steps.count > 1 {
                Slider(
                    value: Binding(
                        get: { Double(step) },
                        set: {
                            playing = false
                            step = Int($0.rounded())
                        }),
                    in: 0...Double(view.steps.count - 1), step: 1
                )
                .accessibilityLabel("Time on the map")
                .accessibilityValue(view.steps.indices.contains(step) ? view.steps[step].label : "")
            }
            ForEach(view.attribution, id: \.self) { line in
                Text(line)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .background(.regularMaterial, in: .rect(cornerRadius: 20))
        .sensoryFeedback(.selection, trigger: step) { _, _ in !playing }
    }

    /// "At New York": the agreement card when small; drag it up (or tap the title) for
    /// each model's numbers, the plumes, the runs and the credits.
    private func drawer(height: CGFloat) -> some View {
        VStack(spacing: 0) {
            Button {
                withAnimation(.snappy) { expanded.toggle() }
            } label: {
                VStack(spacing: 6) {
                    Capsule()
                        .fill(.secondary)
                        .frame(width: 36, height: 5)
                        .padding(.top, 6)
                    HStack(spacing: 8) {
                        Text("At \(view.place)")
                            .font(.title3.weight(.bold))
                            .accessibilityAddTraits(.isHeader)
                        if loading { ProgressView().controlSize(.small) }
                        Spacer(minLength: 0)
                        Image(systemName: expanded ? "chevron.down" : "chevron.up")
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(.secondary)
                    }
                    .padding(.horizontal, 16)
                }
                .contentShape(.rect)
            }
            .buttonStyle(.plain)
            .accessibilityHint(expanded ? "Shows the map" : "Shows each model's numbers")
            .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { titleHeight = $0 }
            .simultaneousGesture(
                DragGesture(minimumDistance: 10).onEnded { drag in
                    withAnimation(.snappy) {
                        if drag.translation.height < -30 { expanded = true }
                        if drag.translation.height > 30 { expanded = false }
                    }
                })
            ScrollViewReader { proxy in
                ScrollView {
                    StormsContent(
                        view: view, stale: stale, failure: failure, hidden: hidden,
                        peek: { peekBottom = $0 }, loadPastRuns: loadPastRuns
                    )
                    .padding(.horizontal, 12)
                    .padding(.top, 10)
                    .padding(.bottom, 24)
                    .id("storms-top")
                }
                .scrollDisabled(!expanded)
                .onChange(of: expanded) {
                    if !expanded { proxy.scrollTo("storms-top", anchor: .top) }
                }
                .task(id: expanded) {
                    #if DEBUG
                        // Screenshots: `-scrollTo storms-plumes` scrolls once the drawer is open.
                        let args = ProcessInfo.processInfo.arguments
                        guard expanded, let i = args.firstIndex(of: "-scrollTo"), i + 1 < args.count else {
                            return
                        }
                        try? await Task.sleep(for: .milliseconds(700))
                        proxy.scrollTo(args[i + 1], anchor: .top)
                    #endif
                }
            }
        }
        .frame(height: height, alignment: .top)
        .frame(maxWidth: .infinity)
        .clipShape(.rect(cornerRadius: 24))
        .background(.regularMaterial, in: .rect(cornerRadius: 24))
        .shadow(color: .black.opacity(0.18), radius: 10, y: 2)
    }
}

/// A model's chip on the map: its line as the map draws it, its short name and run;
/// tap to show or hide it.
private struct ModelToggle: View {
    let chip: StormsView.ModelChip
    let shown: Bool
    /// The run ("12z") beside the name, when the chips fit with it.
    var showsRun = true
    let toggle: () -> Void
    @Environment(\.colorTheme) private var colors

    var body: some View {
        let color = colors.model(chip.role)
        Button(action: toggle) {
            HStack(spacing: 5) {
                DashSample(role: chip.role, color: color)
                    .frame(width: 26, height: 8)
                Text(chip.short).font(.caption.weight(.semibold))
                if showsRun {
                    Text(chip.run).font(.caption).foregroundStyle(.secondary)
                }
                if !shown {
                    Image(systemName: "eye.slash").font(.caption2)
                }
            }
            .opacity(shown ? 1 : 0.5)
            .padding(.horizontal, 9)
            .padding(.vertical, 7)
            .background(.regularMaterial, in: .capsule)
            .overlay(Capsule().strokeBorder(color.opacity(shown ? 0.9 : 0.35), lineWidth: 1.5))
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(chip.label), \(chip.run) run")
        .accessibilityValue(shown ? "On the map" : "Hidden")
        .accessibilityAddTraits(shown ? [.isButton, .isSelected] : .isButton)
    }
}

/// A short piece of a model's track, in its color and dash, as a legend.
struct DashSample: View {
    let role: String
    let color: Color

    var body: some View {
        Canvas { context, size in
            var path = Path()
            path.move(to: CGPoint(x: 2, y: size.height / 2))
            path.addLine(to: CGPoint(x: size.width - 2, y: size.height / 2))
            // The map's dash at 60 %, so the short sample still shows the whole pattern.
            let dash = StormMapRules.dash(role).map { CGFloat($0) * 0.6 }
            context.stroke(
                path, with: .color(color), style: StrokeStyle(lineWidth: 2.5, lineCap: .round, dash: dash))
        }
        .accessibilityHidden(true)
    }
}
