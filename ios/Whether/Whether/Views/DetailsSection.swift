import SwiftUI
import WeatherKit
import WhetherKit

/// Alerts at the top: the title and period; tap for the full text.
struct AlertsSection: View {
    @Environment(\.colorTheme) private var colors
    @Environment(\.showsPatterns) private var patterns
    let alerts: [ViewModel.Alert]
    /// The read ones, by `WeatherStore.alertKey`: dimmed, and left out of the badge.
    let read: Set<String>
    let markRead: ([ViewModel.Alert]) -> Void
    @State private var shown: ViewModel.Alert?

    private func isRead(_ alert: ViewModel.Alert) -> Bool { read.contains(WeatherStore.alertKey(alert)) }

    var body: some View {
        VStack(alignment: .trailing, spacing: 8) {
            ForEach(alerts, id: \.self) { alert in
                let seen = isRead(alert)
                Button {
                    shown = alert
                    markRead([alert])
                } label: {
                    HStack(alignment: .top, spacing: 10) {
                        Image(systemName: seen ? "exclamationmark.triangle" : "exclamationmark.triangle.fill")
                            .foregroundStyle(colors.level(alert.level))
                        VStack(alignment: .leading, spacing: 2) {
                            Text(alert.title).font(.callout.weight(seen ? .regular : .semibold))
                            Text(alert.period).font(.caption).foregroundStyle(.secondary)
                        }
                        Spacer()
                        // The severity in words beside its color (patterns and symbols).
                        if patterns, let word = severityWord(level: alert.level, language: CoreLanguage.current) {
                            Text(word)
                                .font(.caption2.weight(.semibold))
                                .padding(.horizontal, 7)
                                .padding(.vertical, 2)
                                .overlay(Capsule().strokeBorder(colors.level(alert.level), lineWidth: 1.5))
                        }
                        Image(systemName: "chevron.right").font(.caption).foregroundStyle(.tertiary)
                    }
                    .padding(12)
                    .background(
                        colors.level(alert.level).opacity(seen ? 0.06 : 0.12), in: .rect(cornerRadius: 10))
                    .opacity(seen ? 0.75 : 1)
                }
                .buttonStyle(.plain)
                .contextMenu {
                    if !seen {
                        Button("Mark as read", systemImage: "checkmark.circle") { markRead([alert]) }
                        Divider()
                    }
                    // Alerts always show, so there is no Hide.
                    CardMenuButtons()
                }
                .accessibilityValue(seen ? "Read" : "New")
            }
            if alerts.filter({ !isRead($0) }).count > 1 {
                Button("Mark all as read") { markRead(alerts) }
                    .font(.caption)
            }
        }
        .sheet(item: $shown) { alert in
            NavigationStack {
                ScrollView {
                    VStack(alignment: .leading, spacing: 12) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(alert.period)
                            if let source = alert.source { Text(source) }
                        }
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                        if !alert.body.isEmpty { Text(alert.body) }
                        if let link = alert.url.flatMap(URL.init(string:)) {
                            Link("Details", destination: link)
                        }
                        if alert.isApple {
                            AppleWeatherCredit(fallback: "Alerts: Apple Weather").font(.caption)
                        }
                    }
                    .padding()
                }
                .navigationTitle(alert.title)
                .navigationBarTitleDisplayMode(.inline)
            }
            .presentationDetents([.medium, .large])
        }
        .onAppear {
            #if DEBUG
                // Screenshots: `-showAlert` opens the first alert.
                if ProcessInfo.processInfo.arguments.contains("-showAlert") { shown = alerts.first }
            #endif
        }
    }
}

extension Preferences.DetailChoice: @MainActor GridChoice {}

/// The detail cards: wind, gusts, humidity, pressure, UV … in the order chosen.
/// Touch and hold a card to edit them like the home screen.
struct DetailsSection: View {
    @Environment(\.colorTheme) private var colors
    let details: [ViewModel.Detail]
    @Binding var choices: [Preferences.DetailChoice]
    @State private var editing = false

    private func detail(_ kind: String) -> ViewModel.Detail? { details.first { $0.kind == kind } }

    /// Whether gusts have their own card shown.
    private var gustsShown: Bool {
        choices.contains { $0.kind == "gusts" && $0.shown } && detail("gusts") != nil
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                SectionTitle(text: "Details")
                Spacer()
                DoneEditingButton(editing: $editing)
            }
            .contentShape(.rect)
            // The cards below have their own touch and hold (editing them like the home screen).
            .contextMenu { CardMenuButtons { withAnimation(.snappy) { editing = true } } }
            JiggleGrid(
                choices: $choices, editing: $editing, available: { detail($0.kind) != nil },
                title: { detail($0.kind)?.title ?? $0.kind }
            ) { choice in
                if let d = detail(choice.kind) { card(d) }
            }
        }
        // Cards the core gains later are offered in the Add row.
        .onChange(of: details.map(\.kind), initial: true) { _, kinds in
            let all = Preferences.addingMissing(choices, kinds: kinds)
            if all.count != choices.count { choices = all }
        }
        .onAppear {
            #if DEBUG
                // Screenshots: `-editdetails` starts editing.
                if ProcessInfo.processInfo.arguments.contains("-editdetails") { editing = true }
            #endif
        }
    }

    /// A card's symbol, by its kind.
    private func symbol(_ kind: String) -> String { Preferences.DetailChoice(kind: kind).symbol }

    private func card(_ detail: ViewModel.Detail) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            Label(detail.title, systemImage: symbol(detail.kind))
                .font(.caption.weight(.medium))
                .foregroundStyle(.secondary)
            HStack(spacing: 6) {
                Text(detail.value).font(.title3.weight(.medium)).monospacedDigit()
                if let degrees = detail.directionDeg {
                    // The arrow points where the wind blows to.
                    Image(systemName: "location.north.fill")
                        .font(.caption)
                        .rotationEffect(.degrees(Double(degrees) + 180))
                        .foregroundStyle(.secondary)
                }
            }
            // The wind card's "Gusts …" caption, unless gusts have their own card.
            if let caption = detail.caption, !(detail.kind == "wind" && gustsShown) {
                Text(caption).font(.caption).foregroundStyle(.secondary)
            }
            if let gauge = detail.gauge {
                Capsule()
                    .fill(.quaternary)
                    .frame(height: 4)
                    .overlay(alignment: .leading) {
                        GeometryReader { geo in
                            Capsule()
                                .fill(colors.level(detail.level))
                                .frame(width: geo.size.width * CGFloat(min(max(gauge, 0), 1)))
                        }
                    }
                    .padding(.top, 4)
            }
        }
        .frame(maxWidth: .infinity, minHeight: 64, alignment: .topLeading)
        .glassCard(padding: 12)
        .accessibilityElement(children: .combine)
    }
}

/// Sunrise, sunset, the day's light from first dawn to last dusk (a bar and
/// the times), solar noon, and the moon: its phase, how much is lit, and when
/// it rises and sets. Every time comes from the core. Which of these show, and
/// in what order, is `Preferences.sunItems`: touch and hold for the quick
/// switches and the editor.
struct SunMoonSection: View {
    @Environment(\.colorTheme) private var colors
    let sun: ViewModel.Sun
    let moon: ViewModel.Moon?
    @Environment(\.sunItems) private var sunItems
    @State private var editing = false
    @Environment(\.colorScheme) private var scheme

    /// The items to draw: the saved ones (screenshots may hide twilights for one
    /// launch with `-sunhide astronomical,nautical`, without saving).
    private var items: [SunItemChoice] {
        var items = sunItems.wrappedValue
        #if DEBUG
            let args = ProcessInfo.processInfo.arguments
            if let i = args.firstIndex(of: "-sunhide"), i + 1 < args.count {
                let kinds = args[i + 1].split(separator: ",").map(String.init)
                for item in SunItem.allCases where item.eventKind.map(kinds.contains) == true {
                    items.set(item, shown: false)
                }
            }
        #endif
        return items
    }

    var body: some View {
        let items = items
        let blocks = SunMoonLayout.blocks(items)
        let firstTimes = blocks.firstIndex(where: \.isTimes)
        VStack(alignment: .leading, spacing: 12) {
            SectionTitle(text: "Sun & moon")
            if blocks.isEmpty {
                Button("Choose what shows", systemImage: "slider.horizontal.3") { editing = true }
                    .font(.callout)
            }
            ForEach(Array(blocks.enumerated()), id: \.offset) { index, block in
                if Self.isMoon(block), index > 0, !Self.isMoon(blocks[index - 1]) {
                    Divider().opacity(0.4)
                }
                view(for: block, items: items, first: index == firstTimes)
            }
        }
        .font(.callout)
        .frame(maxWidth: .infinity, alignment: .leading)
        .contentShape(.rect)
        .contextMenu {
            ForEach([SunItem.astronomicalTwilight, .nauticalTwilight, .civilTwilight, .goldenHour]) { item in
                Toggle(
                    item.title, systemImage: item.symbol,
                    isOn: Binding(
                        get: { sunItems.wrappedValue.isShown(item) },
                        set: { sunItems.wrappedValue.set(item, shown: $0) }))
            }
            // Customize…, Edit the tab, Hide the card.
            CardMenuButtons { editing = true }
        }
        .sheet(isPresented: $editing) { SunMoonEditor(items: sunItems, sheet: true) }
        .onAppear {
            #if DEBUG
                // Screenshots: `-editsun` opens the editor.
                if ProcessInfo.processInfo.arguments.contains("-editsun") { editing = true }
            #endif
        }
    }

    private static func isMoon(_ block: SunBlock) -> Bool {
        switch block {
        case .moon, .moonTimes: true
        default: false
        }
    }

    /// Today's sunrise, sunset and noon as marks, in the core's words, for what is shown.
    private func sunMarks(_ items: [SunItemChoice]) -> (sun: [SunMark], noon: SunMark?) {
        let rises = [(sun.rise, "sunrise"), (sun.set, "sunset")].compactMap { event, kind in
            event.map { SunMark(kind: kind, label: $0.label, time: $0.time, at: $0.at) }
        }
        let noon = sun.noon.map { SunMark(kind: "noon", label: $0.label, time: $0.time, at: $0.at) }
        return (items.isShown(.sunTimes) ? rises : [], items.isShown(.solarNoon) ? noon : nil)
    }

    /// The moon's rise and set as marks, in the core's words.
    private var moonMarks: [SunMark] {
        guard let moon else { return [] }
        return [(moon.rise, "moonrise"), (moon.set, "moonset")].compactMap { event, kind in
            event.map { SunMark(kind: kind, label: $0.label, time: $0.time, at: $0.at) }
        }
    }

    /// The span the light bar covers, from its first stretch to its last.
    private func span(_ stretches: [LightStretch]) -> ClosedRange<Int64>? {
        guard let first = stretches.first?.start, let last = stretches.last?.end, first < last else {
            return nil
        }
        return first...last
    }

    /// The marks on the bar: the shown twilights, sunrise and sunset, solar noon and
    /// the moon's rise and set, inside the bar.
    private func barMarks(_ items: [SunItemChoice], stretches: [LightStretch]) -> [SunMark] {
        guard let span = span(stretches) else { return [] }
        let (sunMarks, noon) = sunMarks(items)
        let moon = items.isShown(.moonTimes) ? moonMarks : []
        return SunMoonLayout.barMarks(
            edges: sun.twilight.map(SunEdge.init), hiding: SunMoonLayout.hiddenTwilights(items),
            extra: sunMarks + [noon].compactMap { $0 } + moon, span: span)
    }

    @ViewBuilder
    private func view(for block: SunBlock, items: [SunItemChoice], first: Bool) -> some View {
        let hidden = SunMoonLayout.hiddenTwilights(items)
        switch block {
        case .header(let sunTimes, let daylight):
            HStack(spacing: 18) {
                if sunTimes {
                    Label(sun.sunrise, systemImage: "sunrise.fill")
                    Label(sun.sunset, systemImage: "sunset.fill")
                }
                if daylight { Text(sun.daylight).foregroundStyle(sunTimes ? .secondary : .primary) }
            }
            .foregroundStyle(colors.color(.twilightSun))
        case .bar:
            let stretches = SunMoonLayout.lightBar(edges: sun.twilight.map(SunEdge.init), hiding: hidden)
            if !stretches.isEmpty {
                let marks = barMarks(items, stretches: stretches)
                DayLightBar(stretches: stretches, marks: marks)
                    .accessibilityElement(children: .ignore)
                    .accessibilityLabel(marks.map { "\($0.label) \($0.time)" }.joined(separator: ", "))
            }
        case .times(let kinds, let noon):
            let (sunMarks, noonMark) = sunMarks(items)
            // The first list takes sunrise and sunset; the moon's rows are there when the
            // bar shows them (every symbol on the bar has its row).
            let stretches = SunMoonLayout.lightBar(edges: sun.twilight.map(SunEdge.init), hiding: hidden)
            let moon: [SunMark] =
                if first, items.isShown(.moonTimes), items.isShown(.lightBar), let span = span(stretches) {
                    moonMarks.filter { span.contains($0.at) }
                } else { [] }
            let rows = SunMoonLayout.rows(
                edges: sun.twilight.map(SunEdge.init), kinds: kinds, noon: noonMark, showNoon: noon,
                extra: first ? sunMarks + moon : [])
            if !rows.isEmpty {
                Grid(alignment: .leading, horizontalSpacing: 10, verticalSpacing: 6) {
                    ForEach(Array(rows.enumerated()), id: \.offset) { _, row in
                        self.row(row)
                    }
                }
                .font(.subheadline)
            }
        case .moon(let phase, let illumination):
            if let moon {
                HStack(alignment: .firstTextBaseline) {
                    if phase {
                        Label(moon.phaseLabel, systemImage: WeatherSymbols.symbol(moon.icon))
                        Spacer()
                    }
                    if illumination, let lit = moon.illumination {
                        if phase {
                            Text(lit).foregroundStyle(.secondary)
                        } else {
                            Label(lit, systemImage: "circle.lefthalf.filled")
                        }
                    }
                }
            }
        case .moonTimes:
            if let moon, moon.moonrise != nil || moon.moonset != nil {
                HStack(spacing: 18) {
                    if let rise = moon.moonrise {
                        Label(rise, systemImage: SunMoonLayout.symbol(kind: "moonrise"))
                    }
                    if let set = moon.moonset {
                        Label(set, systemImage: SunMoonLayout.symbol(kind: "moonset"))
                    }
                }
                .foregroundStyle(.secondary)
                .accessibilityElement(children: .combine)
                .accessibilityLabel(
                    [moon.rise, moon.set].compactMap { $0 }
                        .map { "\($0.label) \($0.time)" }.joined(separator: ", "))
            }
        }
    }

    /// A row of the times list: the moment's symbol (the one on the bar) in its color,
    /// its words and its time.
    private func row(_ row: SunMark) -> some View {
        GridRow {
            Image(systemName: SunMoonLayout.symbol(kind: row.kind))
                .font(.system(size: 12))
                .foregroundStyle(Self.symbolColor(row.kind, dark: scheme == .dark, in: colors))
                .frame(width: 16)
                .accessibilityHidden(true)
            Text(row.label).foregroundStyle(.secondary).frame(maxWidth: .infinity, alignment: .leading)
            Text(row.time).monospacedDigit().gridColumnAlignment(.trailing)
        }
        .accessibilityElement(children: .combine)
    }

    /// The light at a twilight's edge: indigo deep night to gold. Sunrise, sunset and noon
    /// have their own colors (for symbols: the bar's gold is too pale to read as a shape).
    static func color(_ kind: String, in theme: ColorTheme) -> Color {
        switch kind {
        case "astronomical": theme.color(.twilightAstronomical)
        case "nautical": theme.color(.twilightNautical)
        case "civil": theme.color(.twilightCivil)
        case "day": theme.color(.twilightDay)
        case "sunrise", "sunset": theme.color(.twilightSun)
        case "noon": theme.color(.twilightNoon)
        case "moonrise", "moonset": Color.secondary
        default: theme.color(.twilightGolden)
        }
    }

    /// The color of a kind's symbol: its color, lifted toward white on the dark cards and
    /// sky (the deep blues would vanish there) and toward black on light ones (the
    /// golds would), so the shape reads. The bar's stretches keep `color`.
    static func symbolColor(_ kind: String, dark: Bool, in theme: ColorTheme) -> Color {
        if kind.hasPrefix("moon") { return .secondary }
        let c = UIColor(color(kind, in: theme)).rgba
        let (target, amount): (Double, Double) =
            dark
            ? (1, ["astronomical", "nautical"].contains(kind) ? 0.5 : 0.2)
            : (0, ["golden", "noon", "sunrise", "sunset", "civil"].contains(kind) ? 0.2 : 0)
        let toward = RGBA(red: target, green: target, blue: target)
        return Color(ColorMath.mix(RGBA(red: c.red, green: c.green, blue: c.blue), toward, amount))
    }
}

extension SunEdge {
    init(_ event: ViewModel.SunEvent) {
        self.init(
            at: event.at, morning: event.morning, kind: event.kind, label: event.label, time: event.time)
    }
}

/// The day's light as a bar from the first shown dawn to the last shown dusk:
/// each stretch in its twilight's color (`SunMoonLayout.lightBar`), the day
/// itself bright, and a mark at now. Each moment (`marks`) has its symbol in a
/// strip above the bar or below it, at its time, so the colors are not the only
/// cue (`SunMoonLayout.placeMarks`).
private struct DayLightBar: View {
    let stretches: [LightStretch]
    let marks: [SunMark]
    /// The bar's width in points, once laid out (a guess until then).
    @State private var width: CGFloat = 300
    @Environment(\.colorScheme) private var scheme
    @Environment(\.colorTheme) private var colors
    private static let symbol: CGFloat = 12

    var body: some View {
        let span = (stretches.first?.start ?? 0)...(stretches.last?.end ?? 0)
        let placed = SunMoonLayout.placeMarks(marks, span: span, width: Double(width))
        VStack(spacing: 3) {
            if placed.contains(where: \.above) { strip(placed.filter(\.above)) }
            bar
            if placed.contains(where: { !$0.above }) { strip(placed.filter { !$0.above }) }
        }
        .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width = $0 }
    }

    private var bar: some View {
        GeometryReader { geo in
            let start = Double(stretches.first?.start ?? 0)
            let span = max(Double(stretches.last?.end ?? 0) - start, 1)
            let x = { (t: Int64) in CGFloat((Double(t) - start) / span) * geo.size.width }
            ZStack(alignment: .leading) {
                ForEach(Array(stretches.enumerated()), id: \.offset) { _, s in
                    Rectangle()
                        .fill(SunMoonSection.color(s.kind, in: colors))
                        .frame(width: max(x(s.end) - x(s.start), 0))
                        .offset(x: x(s.start))
                }
                let now = Int64(Date.now.timeIntervalSince1970)
                if let first = stretches.first?.start, let last = stretches.last?.end,
                    (first...last).contains(now)
                {
                    Capsule().fill(.white).frame(width: 3, height: 14).offset(x: x(now) - 1.5)
                }
            }
            // Offsets do not widen a stack: the bar spans the whole width.
            .frame(width: geo.size.width, height: geo.size.height, alignment: .leading)
            .clipShape(.capsule)
        }
        .frame(height: 10)
    }

    /// A row of symbols, each centred at its x.
    private func strip(_ placed: [PlacedMark]) -> some View {
        ZStack(alignment: .leading) {
            ForEach(Array(placed.enumerated()), id: \.offset) { _, p in
                Image(systemName: SunMoonLayout.symbol(kind: p.mark.kind))
                    .font(.system(size: Self.symbol))
                    .foregroundStyle(SunMoonSection.symbolColor(p.mark.kind, dark: scheme == .dark, in: colors))
                    .frame(width: 16, height: 14)
                    .offset(x: p.x - 8)
            }
        }
        .frame(maxWidth: .infinity, minHeight: 14, maxHeight: 14, alignment: .leading)
    }
}

/// When the data is from, any problem, and the required credit.
struct FooterSection: View {
    let view: ViewModel
    /// Apple's minutes are in the forecast: its mark and data sources are shown.
    var appleWeather = false
    /// What Apple's credit says while its mark loads: the next hour, the alerts or both.
    var appleCredit: LocalizedStringKey = "Next hour: Apple Weather"
    /// The raw text behind `view.status.error`, behind a "Details" disclosure.
    var errorDetail: String?
    /// Why OpenWeather did not serve while another provider did, and the raw
    /// attempts behind it.
    var keyProblem: (title: String, detail: String?)?
    /// The place follows you, but location access is off: it keeps its last coordinates.
    var locationOff = false

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            if locationOff {
                Label("Location access is off", systemImage: "location.slash")
                    .foregroundStyle(.orange)
            }
            if let error = view.status.error {
                Label(error, systemImage: "exclamationmark.triangle")
                    .foregroundStyle(.orange)
                if let errorDetail {
                    DisclosureGroup("Details") {
                        Text(errorDetail).textSelection(.enabled).foregroundStyle(.secondary)
                    }
                }
            } else if view.status.stale {
                Label("This data is out of date", systemImage: "clock.badge.exclamationmark")
                    .foregroundStyle(.orange)
            }
            Text("\(view.status.updated) · \(view.status.provider)")
                .foregroundStyle(.secondary)
            if let keyProblem {
                Label(keyProblem.title, systemImage: "key")
                    .foregroundStyle(.orange)
                if let detail = keyProblem.detail {
                    DisclosureGroup("Details") {
                        Text(detail).textSelection(.enabled).foregroundStyle(.secondary)
                    }
                }
            }
            Text(view.attribution).foregroundStyle(.tertiary)
            if appleWeather {
                AppleWeatherCredit(fallback: appleCredit)
            }
        }
        .font(.caption)
        .padding(.top, 8)
    }
}

/// Apple's Weather mark and the link to its data sources, as WeatherKit's
/// terms require.
struct AppleWeatherCredit: View {
    @Environment(\.colorScheme) private var scheme
    @State private var attribution: WeatherAttribution?
    /// What shows until Apple's mark loads (or without it).
    var fallback: LocalizedStringKey = "Next hour: Apple Weather"

    var body: some View {
        HStack(spacing: 8) {
            if let attribution {
                AsyncImage(
                    url: scheme == .dark ? attribution.combinedMarkDarkURL : attribution.combinedMarkLightURL
                ) { image in
                    image.resizable().scaledToFit()
                } placeholder: {
                    Text(attribution.serviceName)
                }
                .frame(height: 12)
                Link("Other data sources", destination: attribution.legalPageURL)
            } else {
                Text(fallback)
            }
        }
        .task { attribution = await MinuteForecast.attribution() }
    }
}
