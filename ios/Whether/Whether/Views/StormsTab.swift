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
    /// The hurricane to frame on the map (the Storm ahead card's), by NHC id.
    var focus: String?
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
                    close: close, focus: focus
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
    /// The hurricane to frame on the map, by NHC id.
    let focus: String?
    let loadPastRuns: (StormsView.Event) async -> Void
    @Environment(\.colorTheme) private var colors
    /// The models the chips hide, by model id: for this screen only, never saved.
    @State private var hidden: Set<String> = []
    /// The hurricanes the chips hide, by NHC id, and the guidance models, by aid code.
    @State private var hiddenHurricanes: Set<String> = []
    @State private var hiddenAids: Set<String> = []
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
                    view: view, step: step, hidden: hidden, hiddenHurricanes: hiddenHurricanes,
                    hiddenAids: hiddenAids, focus: focus,
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
        // The Storm ahead card opens a hurricane: it shows, whatever its chip said.
        .onChange(of: focus, initial: true) {
            if let focus { hiddenHurricanes.remove(focus) }
        }
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

    /// The model chips (tap one to show or hide it), under them the hurricanes' chips and
    /// their guidance models' chips, and, full screen, the close button.
    private var topRow: some View {
        HStack(alignment: .top, spacing: 8) {
            VStack(alignment: .leading, spacing: 6) {
                if !view.models.isEmpty {
                    // Every chip with its run when they fit, else without the runs, else scrolling.
                    ViewThatFits(in: .horizontal) {
                        chips(runs: true)
                        chips(runs: false)
                        ScrollView(.horizontal, showsIndicators: false) { chips(runs: false) }
                            .scrollClipDisabled()
                    }
                }
                if !view.hurricanes.isEmpty {
                    hurricaneChips
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            CloseButton(close: close)
        }
        .padding(.top, 6)
    }

    /// A chip per hurricane (tap to show or hide it with its cone and tracks), then one
    /// per guidance model (tap to show or hide that model's tracks), wrapping onto more
    /// rows when they don't fit.
    private var hurricaneChips: some View {
        ChipFlow(spacing: 6) {
            ForEach(view.hurricanes) { hurricane in
                HurricaneToggle(hurricane: hurricane, shown: !hiddenHurricanes.contains(hurricane.id)) {
                    withAnimation(.snappy) { hiddenHurricanes = StormMapRules.toggled(hiddenHurricanes, hurricane.id) }
                }
            }
            ForEach(StormMapRules.aidChips(view.hurricanes)) { aid in
                AidToggle(aid: aid, shown: !hiddenAids.contains(aid.aid)) {
                    withAnimation(.snappy) { hiddenAids = StormMapRules.toggled(hiddenAids, aid.aid) }
                }
            }
        }
        .padding(.vertical, 2)
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
            // No steps (hurricanes alone, no model lows): only the credits.
            if !view.steps.isEmpty {
                stepRow
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

    /// Play, the step's time and the slider.
    @ViewBuilder private var stepRow: some View {
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

/// A model's chip on the map: its band as the map draws it, its short name and run;
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

/// Chips in rows, left to right, starting a new row when the next doesn't fit.
private struct ChipFlow: Layout {
    var spacing: CGFloat = 6

    /// Each chip's origin in a row layout `width` wide, and the size it all takes.
    private func arrange(_ subviews: Subviews, width: CGFloat) -> (origins: [CGPoint], size: CGSize) {
        var origins: [CGPoint] = []
        var x: CGFloat = 0, y: CGFloat = 0, row: CGFloat = 0, widest: CGFloat = 0
        for subview in subviews {
            let size = subview.sizeThatFits(.unspecified)
            if x > 0, x + size.width > width {
                x = 0
                y += row + spacing
                row = 0
            }
            origins.append(CGPoint(x: x, y: y))
            widest = max(widest, x + size.width)
            x += size.width + spacing
            row = max(row, size.height)
        }
        return (origins, CGSize(width: widest, height: y + row))
    }

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        arrange(subviews, width: proposal.width ?? .infinity).size
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        let origins = arrange(subviews, width: bounds.width).origins
        for (subview, origin) in zip(subviews, origins) {
            subview.place(
                at: CGPoint(x: bounds.minX + origin.x, y: bounds.minY + origin.y), proposal: .unspecified)
        }
    }
}

/// A hurricane's chip on the map: its symbol in its level's color and its name; tap to
/// show or hide it with its cone and tracks.
private struct HurricaneToggle: View {
    let hurricane: StormsView.Hurricane
    let shown: Bool
    let toggle: () -> Void
    @Environment(\.colorTheme) private var colors

    var body: some View {
        let color = colors.level(hurricane.level)
        Button(action: toggle) {
            HStack(spacing: 5) {
                Image(systemName: "hurricane")
                    .font(.caption.weight(.bold))
                    .foregroundStyle(color)
                Text(hurricane.name).font(.caption.weight(.semibold))
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
        .accessibilityLabel(hurricane.title)
        .accessibilityValue(shown ? "On the map" : "Hidden")
        .accessibilityAddTraits(shown ? [.isButton, .isSelected] : .isButton)
    }
}

/// A hurricane guidance model's chip: its band as the map draws it and its short name;
/// tap to show or hide its tracks.
private struct AidToggle: View {
    let aid: StormsView.Aid
    let shown: Bool
    let toggle: () -> Void
    @Environment(\.colorTheme) private var colors

    var body: some View {
        let color = colors.model(aid.role)
        Button(action: toggle) {
            HStack(spacing: 5) {
                DashSample(role: aid.role, color: color)
                    .frame(width: 22, height: 7)
                Text(aid.short).font(.caption.weight(.semibold))
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
        .accessibilityLabel("\(aid.short) guidance")
        .accessibilityValue(shown ? "On the map" : "Hidden")
        .accessibilityAddTraits(shown ? [.isButton, .isSelected] : .isButton)
    }
}

/// A short piece of a model's track as the map draws it, as a legend: a band in its
/// color, translucent between edges in its dash.
struct DashSample: View {
    let role: String
    let color: Color

    var body: some View {
        Canvas { context, size in
            let edge = StormMapRules.edgeWidth
            let band = CGRect(x: edge / 2, y: edge / 2, width: size.width - edge, height: size.height - edge)
            let shape = Path(roundedRect: band, cornerRadius: band.height / 2)
            context.fill(shape, with: .color(color.opacity(StormMapRules.fillAlpha)))
            let dash = StormMapRules.dash(role).map { CGFloat($0) }
            context.stroke(
                shape, with: .color(color),
                style: StrokeStyle(lineWidth: edge, lineCap: .round, lineJoin: .round, dash: dash))
        }
        .accessibilityHidden(true)
    }
}
