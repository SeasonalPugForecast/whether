import SwiftUI
import WhetherKit

/// A measure the hourly graph can draw.
enum GraphMeasure: String, CaseIterable, Identifiable {
    case temperature, actual, rain, wind, gusts, humidity, uv, pressure
    var id: Self { self }

    /// Its name where feels-like temperatures lead: the temperature line is
    /// then the feels-like one, and `actual` the air's.
    func title(feelsLeads: Bool) -> String {
        self == .temperature && feelsLeads ? "Feels like" : title
    }

    /// The measures offered: the air temperature only when it differs from the
    /// leading one.
    static func offered(feelsLeads: Bool) -> [GraphMeasure] {
        allCases.filter { feelsLeads || $0 != .actual }
    }

    /// The measures in `raw` (saved raw values), temperature when none.
    static func chosen(_ raw: [String]) -> [GraphMeasure] {
        MeasureList.shown(raw, known: allCases.map(\.rawValue)).compactMap(GraphMeasure.init(rawValue:))
    }

    var title: String {
        switch self {
        case .temperature: "Temperature"
        case .actual: "Actual"
        case .rain: "Rain"
        case .wind: "Wind"
        case .gusts: "Gusts"
        case .humidity: "Humidity"
        case .uv: "UV"
        case .pressure: "Pressure"
        }
    }

    /// Its line's color when it is not the first measure (that one is white
    /// over the weather's colors).
    func color(_ theme: ColorTheme) -> Color {
        let role: ColorRole =
            switch self {
            case .temperature: .graphTemperature
            case .actual: .graphActual
            case .rain: .rain
            case .wind, .gusts: .graphWind
            case .humidity: .graphHumidity
            case .uv: .graphUV
            case .pressure: .graphPressure
            }
        return theme.color(role)
    }

    /// Its value at an hour for a label on the line, as the core wrote it:
    /// "40°", "8 mph", "30%" (each reads as its own measure).
    func label(_ h: ViewModel.Hour) -> String { value(h).1 }

    /// Its value at an hour (display units) and its text, as the core wrote it.
    func value(_ h: ViewModel.Hour) -> (Double?, String) {
        switch self {
        case .temperature: (h.temperature.value.map(Double.init), h.temperature.text)
        case .actual: (h.actual.value.map(Double.init), h.actual.text)
        case .rain:
            (h.precipProbability.map(Double.init), h.precipProbability.map { chanceText(percent: $0) } ?? "–")
        case .wind: (h.wind.speed.value.map(Double.init), h.wind.speed.text)
        case .gusts: (h.wind.gust.value.map(Double.init), h.wind.gust.text)
        case .humidity: (h.humidity.map(Double.init), h.humidityText ?? "–")
        case .uv: (h.uv.map(Double.init), h.uvText ?? "–")
        case .pressure: (h.pressure.value.map(Double.init), h.pressure.text)
        }
    }
}

/// One measure across the hours, on its own scale.
struct GraphSeries: Equatable {
    let measure: GraphMeasure
    let values: [Double?]
    let range: ClosedRange<Double>
    /// The first measure: filled (rain: bars).
    let primary: Bool
    /// Its band of the graph's height, 0 (top) to 1 (bottom): each scale gets
    /// its own, so the lines and their labels do not cross.
    var band: ClosedRange<Double> = 0...1
}

/// One measure at one hour, with its neighbours: all a cell draws, so cells
/// compare cheaply and redraw only when their own hours change.
struct CellPoint: Equatable {
    let measure: GraphMeasure
    let previous: Double?
    let value: Double?
    let next: Double?
    let range: ClosedRange<Double>
    let band: ClosedRange<Double>
    let primary: Bool

    init(_ s: GraphSeries, at i: Int) {
        measure = s.measure
        value = s.values.indices.contains(i) ? s.values[i] : nil
        previous = i > 0 && s.values.indices.contains(i - 1) ? s.values[i - 1] : nil
        next = s.values.indices.contains(i + 1) ? s.values[i + 1] : nil
        range = s.range
        band = s.band
        primary = s.primary
    }

    /// The y of `v` in a graph from `top` to `bottom`.
    func y(_ v: Double, top: CGFloat, bottom: CGFloat) -> CGFloat {
        let f = (v - range.lowerBound) / max(range.upperBound - range.lowerBound, 0.001)
        let (a, b) = (top + (bottom - top) * band.lowerBound, top + (bottom - top) * band.upperBound)
        return a + (b - a) * (1 - CGFloat(min(max(f, 0), 1)))
    }
}

/// Every hour ahead as a graph that scrolls under a needle: the hour at the
/// needle is the one shown in full below; pinch to see fewer or more hours. One measure or several: the first is
/// filled and labelled, the others are colored lines on their own scales
/// (gusts dotted), and the legend gives each one's value at the needle. Each
/// hour draws its own piece, so only the hours on screen are drawn.
struct HourlyDetailPage: View {
    let view: ViewModel
    let cards: Bool
    /// The hour card's tiles, in order (long-press one to edit).
    @Binding var tiles: [Preferences.HourTileChoice]
    /// The graph's measures, the first filled (`GraphMeasure` raw values).
    @Binding var measures: [String]
    /// Its value labels and scales (Settings → Display → Hourly graph).
    @Binding var options: GraphOptions

    /// Every hour from the current one, with the label of its day.
    private var hours: [(day: String, hour: ViewModel.Hour)] {
        let now = view.hourly?.hours.first?.time ?? 0
        let days = view.daily?.days ?? []
        let byDay = days.flatMap { day in day.hours.filter { $0.time >= now }.map { (day.label, $0) } }
        return byDay.isEmpty ? (view.hourly?.hours ?? []).map { ($0.day, $0) } : byDay
    }

    // The hours and series are built here, once per forecast or choice of
    // measures; the page below re-renders as the needle moves without them.
    var body: some View {
        let hours = hours
        HourlyGraphPage(
            hours: hours, series: series(hours, GraphMeasure.chosen(measures)), cards: cards,
            sun: view.sun, moon: view.moon, tiles: $tiles, measures: $measures, options: $options
        )
        .onAppear {
            #if DEBUG
                // Screenshots: `-metric temperature,wind,gusts` picks the measures.
                let args = ProcessInfo.processInfo.arguments
                if let i = args.firstIndex(of: "-metric"), i + 1 < args.count {
                    measures = args[i + 1].split(separator: ",").map(String.init)
                }
            #endif
        }
    }

    /// Each measure's values and scale. Wind and gusts share a scale when both
    /// show (so the gusts sit above the wind), and so do the feels-like and air
    /// temperatures, unless each is set to its own; percentages run 0–100.
    private func series(
        _ hours: [(day: String, hour: ViewModel.Hour)], _ chosen: [GraphMeasure]
    ) -> [GraphSeries] {
        let values = Dictionary(
            uniqueKeysWithValues: chosen.map { m in (m, hours.map { m.value($0.hour).0 }) })
        func range(_ m: GraphMeasure) -> ClosedRange<Double> {
            switch m {
            case .rain, .humidity: return 0...100
            default:
                let scale = MeasureList.scale(of: m.rawValue, shared: options.sharedScales)
                let numbers = chosen
                    .filter { MeasureList.scale(of: $0.rawValue, shared: options.sharedScales) == scale }
                    .flatMap { values[$0] ?? [] }
                    .compactMap { $0 }
                let low = numbers.min() ?? 0
                let high = numbers.max() ?? 1
                let pad = max((high - low) * 0.15, 1)
                return (m == .uv ? 0 : low - pad)...(high + pad * 2)
            }
        }
        // One band per scale (wind and gusts share one), the first at the bottom.
        var scales: [ClosedRange<Double>] = []
        for m in chosen where !scales.contains(range(m)) {
            scales.append(range(m))
        }
        let n = Double(scales.count)
        return chosen.enumerated().map { i, m in
            let g = Double(scales.firstIndex(of: range(m)) ?? 0)
            let band = n > 1 ? ((n - 1 - g) / n)...((n - g) / n - 0.04) : 0...1
            return GraphSeries(
                measure: m, values: values[m] ?? [], range: range(m), primary: i == 0, band: band)
        }
    }

}

/// The hourly page as the needle moves: the measures' menu and legend, the
/// scrolling graph and the hour in full. Only `selected` changes as it scrolls.
struct HourlyGraphPage: View {
    let hours: [(day: String, hour: ViewModel.Hour)]
    let series: [GraphSeries]
    let cards: Bool
    /// Today's sun (with its twilight) and moon, under the hours.
    let sun: ViewModel.Sun?
    let moon: ViewModel.Moon?
    @Binding var tiles: [Preferences.HourTileChoice]
    @Binding var measures: [String]
    @Binding var options: GraphOptions
    @State private var selected = 0
    /// The measures' editor is open.
    @State private var editing = false
    /// The legend chip a drag is over.
    @State private var dropTarget: GraphMeasure?
    /// The cards' order and which show (`Preferences.hourlyCards`), and their editor.
    @Environment(\.hourlyCards) private var hourlyCards
    /// Rain and snow hours get their symbol under the line.
    @Environment(\.showsPatterns) private var patterns
    @Environment(\.colorTheme) private var colors
    @State private var editingCards = false
    /// One hour's width after a pinch (`nil`: the default, `baseStep`).
    @State private var zoomedStep: CGFloat?
    @Environment(\.feelsLikeLeads) private var feelsLeads
    /// The graph's value labels follow Dynamic Type.
    @ScaledMetric(relativeTo: .caption2) private var labelSize: CGFloat = 10

    /// Width of one hour before a pinch: wider with larger text, so the labels fit.
    @ScaledMetric(relativeTo: .caption2) private var baseStep: CGFloat = 54

    /// One hour's width now; a pinch changes it from about 3 hours across the
    /// card to about a day (narrower and the needle could not reach the middle).
    private var step: Binding<CGFloat> {
        Binding(get: { zoomedStep ?? baseStep }, set: { zoomedStep = $0 })
    }

    private var chosen: [GraphMeasure] { GraphMeasure.chosen(measures) }

    var body: some View {
        ScrollViewReader { proxy in
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                let graphCard = VStack(alignment: .leading, spacing: 10) {
                    HStack {
                        SectionTitle(text: "Hour by hour")
                        Spacer()
                        MeasureMenu(measures: $measures, feelsLeads: feelsLeads) { editing = true }
                    }
                    // The menu is on the title row only: holding the graph
                    // itself scrolls it (or pinches), never lifts the card.
                    .contentShape(.rect)
                    .cardContextMenu { editing = true }
                    if hours.isEmpty {
                        Text("No hourly forecast").foregroundStyle(.secondary)
                    } else {
                        NeedleDay(hour: hours[shown].hour)
                        legend(series, at: shown, hours: hours)
                        graph(hours, series: series)
                    }
                }
                .glassCard(cards)
                ForEach(cardChoices.filter(\.shown)) { choice in
                    Group {
                        switch choice.item {
                        case .graph:
                            graphCard
                        case .hour:
                            if hours.indices.contains(shown) {
                                HourDetailCard(day: hours[shown].day, hour: hours[shown].hour, tiles: $tiles)
                                    .glassCard(cards)
                            }
                        case .sun:
                            if let sun {
                                SunMoonSection(sun: sun, moon: moon).glassCard(cards).id("sun")
                            }
                        }
                    }
                    .environment(\.cardMenu, menu(for: choice.item))
                }
                if cardChoices.allSatisfy({ !$0.shown }) {
                    Button("Choose the cards", systemImage: "square.grid.2x2") { editingCards = true }
                        .font(.callout)
                        .frame(maxWidth: .infinity)
                        .padding(.top, 24)
                }
            }
            .padding(.horizontal, cards ? 16 : 20)
            .padding(.top, 12)
            .padding(.bottom, 40)
        }
        .task {
            #if DEBUG
                if let i = ProcessInfo.processInfo.arguments.firstIndex(of: "-pickHour"),
                    i + 1 < ProcessInfo.processInfo.arguments.count,
                    let n = Int(ProcessInfo.processInfo.arguments[i + 1])
                {
                    selected = n
                }
            #endif
            #if DEBUG
                // Screenshots: `-tab hourly -scrollTo sun` shows the sun and moon;
                // `-editmeasures` opens the measures' editor.
                let args = ProcessInfo.processInfo.arguments
                if args.contains("-editmeasures") {
                    try? await Task.sleep(for: .milliseconds(800))
                    editing = true
                }
                if args.contains("-edithourly") { editingCards = true }
                if let i = args.firstIndex(of: "-scrollTo"), i + 1 < args.count, args[i + 1] == "sun" {
                    try? await Task.sleep(for: .milliseconds(800))
                    proxy.scrollTo("sun", anchor: .top)
                }
            #endif
        }
        }
        .sensoryFeedback(.selection, trigger: selected)
        .sheet(isPresented: $editing) {
            MeasuresEditorSheet(kind: .graph, measures: $measures, options: $options, feelsLeads: feelsLeads)
        }
        .sheet(isPresented: $editingCards) {
            CardsEditor(items: hourlyCards, title: "Hourly tab", sheet: true)
        }
    }

    /// The cards to draw: the saved ones (screenshots may choose them for one
    /// launch, without saving: `-hourlycards sun,hour` shows those two, in that
    /// order; `-edithourly` opens the editor).
    private var cardChoices: [HourlyCardChoice] {
        #if DEBUG
            if let names = ProcessInfo.processInfo.launchList("-hourlycards") {
                return HourlyCardChoice.arranged(by: names)
            }
        #endif
        return hourlyCards.wrappedValue
    }

    /// A card's touch-and-hold entries: edit the tab, hide the card.
    private func menu(for card: HourlyCard) -> CardMenu {
        CardMenu(
            editTitle: "Edit the Hourly tab", edit: { editingCards = true },
            hideTitle: "Hide \(card.title)",
            hide: { withAnimation(.snappy) { hourlyCards.wrappedValue.set(card, shown: false) } })
    }

    /// What each line is, and its value at the needle. Tap one to hide its line
    /// (the menu brings it back; the last one stays), drag one onto another to
    /// swap their places, touch and hold for a menu: fill it, move it, hide it.
    private func legend(
        _ series: [GraphSeries], at index: Int, hours: [(day: String, hour: ViewModel.Hour)]
    ) -> some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(series, id: \.measure) { s in
                    Button {
                        guard chosen.count > 1 else { return }
                        withAnimation(.snappy) {
                            measures = MeasureList.hide(s.measure.rawValue, in: chosen.map(\.rawValue))
                        }
                    } label: {
                        HStack(spacing: 5) {
                            Capsule()
                                .fill(s.primary ? .white : s.measure.color(colors))
                                .frame(width: 14, height: 3)
                                .mask {
                                    if s.measure == .gusts {
                                        HStack(spacing: 2) {
                                            ForEach(0..<4, id: \.self) { _ in Circle().frame(width: 3) }
                                        }
                                    } else {
                                        Rectangle()
                                    }
                                }
                            Text(s.measure.title(feelsLeads: feelsLeads)).foregroundStyle(.secondary)
                            Text(hours.indices.contains(index) ? s.measure.value(hours[index].hour).1 : "–")
                                .monospacedDigit()
                        }
                        .font(.caption2)
                        .padding(.horizontal, 8)
                        .padding(.vertical, 4)
                        .background(.white.opacity(s.primary ? 0.16 : 0.07), in: .capsule)
                        .overlay {
                            if dropTarget == s.measure {
                                Capsule().strokeBorder(.white, lineWidth: 1.5)
                            }
                        }
                    }
                    .buttonStyle(.plain)
                    .accessibilityHint(chosen.count > 1 ? "Hides its line" : "")
                    .contentShape(.contextMenuPreview, .capsule)
                    .draggable(MeasureDrag(id: s.measure.rawValue))
                    .dropDestination(for: MeasureDrag.self) { items, _ in
                        guard let dragged = items.first else { return false }
                        withAnimation(.snappy) {
                            measures = MeasureList.place(
                                dragged.id, at: s.measure.rawValue, in: chosen.map(\.rawValue))
                        }
                        return true
                    } isTargeted: { over in
                        if over { dropTarget = s.measure } else if dropTarget == s.measure { dropTarget = nil }
                    }
                    .contextMenu {
                        MeasureActions(measure: s.measure, kind: .graph, measures: $measures) { editing = true }
                    }
                }
            }
            .padding(.vertical, 2)
        }
    }

    /// The graph, scrolled under the needle (on now at first, then in the
    /// middle): the hour there is the one shown. Tap an hour to bring the needle
    /// to it; pinch to zoom. Lazily drawn, so only the hours in view are.
    private func graph(
        _ hours: [(day: String, hour: ViewModel.Hour)], series: [GraphSeries]
    ) -> some View {
        let width = step.wrappedValue
        // Fewer times and values as the hours narrow, on round hours.
        let axis = HourWindow.labelStride(step: width, spacing: 40)
        // A value label every few hours: automatic, or the choice where it fits.
        let values = HourWindow.valueStride(every: options.valueEvery, step: width, labelWidth: 36 * labelSize / 10)
        return HourScroller(count: hours.count, selected: $selected, step: step, zoom: 7...108) {
            // The needle, behind the graph.
            Capsule()
                .fill(.white.opacity(0.55))
                .frame(width: 2, height: 220 - 26)
                .frame(maxHeight: .infinity, alignment: .top)
                .padding(.top, 4)
        } strip: {
            LazyHStack(spacing: 0) {
                ForEach(hours.indices, id: \.self) { i in
                    HourCell(
                        hour: Self.sample(hours[i].hour, at: i),
                        points: series.map { CellPoint($0, at: i) },
                        labelled: i % values == 0,
                        labelSize: labelSize,
                        atNeedle: i == shown,
                        showsAxis: i % axis == 0 || i == shown,
                        dayStart: i > 0 && hours[i].day != hours[i - 1].day ? hours[i].hour.dayAndDate : nil,
                        colors: colors,
                        patterns: patterns
                    )
                    // Unchanged cells are not drawn again as the needle moves.
                    .equatable()
                    .frame(width: width)
                    // A day's pill runs over the hours after it.
                    .zIndex(i > 0 && hours[i].day != hours[i - 1].day ? 1 : 0)
                }
            }
        }
        .frame(height: 220)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Hours")
        .accessibilityValue(hours.indices.contains(shown) ? hours[shown].hour.label : "")
        .accessibilityAdjustableAction { direction in
            switch direction {
            case .increment: selected = HourWindow.clamp(shown + 1, count: hours.count)
            case .decrement: selected = HourWindow.clamp(shown - 1, count: hours.count)
            default: break
            }
        }
    }

    /// The chosen hour, within the hours there are.
    private var shown: Int { HourWindow.clamp(selected, count: hours.count) }

    /// The hour as the graph draws it (screenshots: `-precipSample` makes hours 1 to 6
    /// rain, then snow, for the launch only, to show the wet hours' symbols).
    private static func sample(_ hour: ViewModel.Hour, at i: Int) -> ViewModel.Hour {
        #if DEBUG
            if (1...6).contains(i), ProcessInfo.processInfo.arguments.contains("-precipSample") {
                var wet = hour
                wet.icon = i <= 3 ? "rain" : "snow"
                return wet
            }
        #endif
        return hour
    }
}

/// The day and date of the hour under the needle, large enough to read while
/// sliding (the axis below shows only times): "Tomorrow, Nov 2 · 7 AM".
private struct NeedleDay: View {
    let hour: ViewModel.Hour

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 6) {
            Text(hour.day).font(.headline)
            Text(hour.dateShort).font(.headline).foregroundStyle(.secondary)
            Text(hour.label).font(.subheadline).foregroundStyle(.secondary)
        }
        .monospacedDigit()
        .lineLimit(1)
        .minimumScaleFactor(0.8)
        .contentTransition(.numericText())
        .animation(.snappy, value: hour.time)
        .accessibilityElement(children: .combine)
    }
}

/// One hour's piece of the graph: each measure from halfway to the previous
/// hour to halfway to the next (rain first: bars), over the hour's weather
/// color; the measures' values every few hours, a day's name where one
/// begins, and the hour's icon and time below.
private struct HourCell: View, Equatable {
    let hour: ViewModel.Hour
    let points: [CellPoint]
    let labelled: Bool
    /// The font size of the value labels (scaled with Dynamic Type by the page).
    let labelSize: CGFloat
    /// Under the needle: its time stands out on the axis.
    let atNeedle: Bool
    /// Its icon and time show (every few hours when the hours are narrow).
    let showsAxis: Bool
    let dayStart: String?
    /// The color theme (a property, not the environment, so cells stay `Equatable`).
    let colors: ColorTheme
    /// Wet hours carry their symbol on the weather strip, not only its color.
    let patterns: Bool

    var body: some View {
        VStack(spacing: 6) {
            Canvas { ctx, size in draw(&ctx, size) }
                .frame(height: 150)
                .overlay(alignment: .topLeading) {
                    // The day and date where it begins, on a pill over the lines;
                    // it runs on over the next hours (this cell is drawn above them).
                    if let dayStart {
                        Text(dayStart)
                            .font(.caption2.weight(.semibold))
                            .foregroundStyle(.white)
                            .padding(.horizontal, 6)
                            .padding(.vertical, 2)
                            .background(.black.opacity(0.35), in: .capsule)
                            .overlay(Capsule().strokeBorder(.white.opacity(0.3), lineWidth: 0.5))
                            .fixedSize()
                            .offset(x: 3, y: 3)
                    }
                }
            // The axis: every hour, or every few when narrow; the needle's stands out.
            ConditionIcon(icon: hour.icon, size: 18).frame(height: 20)
                .opacity(showsAxis ? 1 : 0)
            Text(hour.label)
                .font(.caption2.weight(atNeedle ? .bold : .regular))
                .foregroundStyle(atNeedle ? AnyShapeStyle(.primary) : AnyShapeStyle(.secondary))
                .lineLimit(1)
                .fixedSize()
                .padding(.horizontal, 3)
                .background(.white.opacity(atNeedle ? 0.22 : 0), in: .capsule)
                .opacity(showsAxis ? 1 : 0)
        }
    }

    /// The graph's color for an hour's weather: the timeline's blues and purple
    /// for rain, snow and storms, and distinct colors for the dry hours (gold
    /// sun, moonlight at night, sand for some cloud, slate for overcast).
    static func color(_ group: WeatherSymbols.Group, isDay: Bool, in theme: ColorTheme) -> Color {
        switch group {
        case .clear: theme.color(isDay ? .graphClearDay : .graphClearNight)
        case .partlyCloudy: theme.color(isDay ? .graphPartlyCloudyDay : .graphPartlyCloudyNight)
        case .cloudy: theme.color(.graphCloudy)
        case .fog: theme.color(.graphFog)
        default: theme.condition(group)
        }
    }

    /// The symbol marking a wet hour's group on the graph, or `nil` for a dry one.
    static func wetSymbol(_ group: WeatherSymbols.Group) -> String? {
        switch group {
        case .drizzle, .rain, .heavyRain: "drop.fill"
        case .snow: "snowflake"
        case .storm: "cloud.bolt.fill"
        default: nil
        }
    }

    /// The smooth piece through this hour: from halfway to the previous hour,
    /// through it, to halfway to the next, with the slopes the neighbours use
    /// too, so the pieces join.
    private func piece(_ p: CGFloat, _ v: CGFloat, _ n: CGFloat, width w: CGFloat) -> Path {
        let mid = w / 2
        let point = CGPoint(x: mid, y: v)
        let left = CGPoint(x: 0, y: (p + v) / 2)
        let right = CGPoint(x: w, y: (v + n) / 2)
        let slopeLeft = (v - p) / w
        let slopeMid = (n - p) / (2 * w)
        let slopeRight = (n - v) / w
        let h = w / 2
        var line = Path()
        line.move(to: left)
        line.addCurve(
            to: point,
            control1: CGPoint(x: h / 3, y: left.y + slopeLeft * h / 3),
            control2: CGPoint(x: mid - h / 3, y: v - slopeMid * h / 3))
        line.addCurve(
            to: right,
            control1: CGPoint(x: mid + h / 3, y: v + slopeMid * h / 3),
            control2: CGPoint(x: w - h / 3, y: right.y - slopeRight * h / 3))
        return line
    }

    private func draw(_ ctx: inout GraphicsContext, _ size: CGSize) {
        let top: CGFloat = 26
        // The hour's weather as a strip along the bottom (Dark Sky's colors).
        let strip: CGFloat = 6
        let weather = Self.color(WeatherSymbols.group(hour.icon), isDay: hour.isDay, in: colors)
        ctx.fill(
            Path(CGRect(x: 0.5, y: size.height - strip, width: size.width - 1, height: strip)),
            with: .color(weather))
        let bottom = size.height - strip - 2
        let w = size.width
        let mid = w / 2
        if dayStart != nil {
            var rule = Path()
            rule.move(to: CGPoint(x: 0, y: 4))
            rule.addLine(to: CGPoint(x: 0, y: bottom))
            ctx.stroke(rule, with: .color(.white.opacity(0.3)), style: StrokeStyle(lineWidth: 1, dash: [3, 3]))
        }
        // Every few hours (`labelled`), each line's value where it crosses the hour.
        var labels: [(point: CGPoint, text: String, color: Color)] = []
        for s in points {
            guard let v = s.value else { continue }
            let y = { (v: Double) in s.y(v, top: top, bottom: bottom) }
            let here = y(v)
            let previous = s.previous.map(y) ?? here
            let next = s.next.map(y) ?? here
            if labelled {
                labels.append(
                    (CGPoint(x: mid, y: here), s.measure.label(hour), s.primary ? .white : s.measure.color(colors)))
            }
            if s.measure == .rain && s.primary {
                let barWidth = min(18, w * 0.7)
                let bar = CGRect(x: mid - barWidth / 2, y: here, width: barWidth, height: bottom - here)
                ctx.fill(Path(roundedRect: bar, cornerRadius: 4), with: .color(colors.rain))
                continue
            }
            let line = piece(previous, here, next, width: w)
            if s.primary {
                var area = line
                area.addLine(to: CGPoint(x: w, y: bottom))
                area.addLine(to: CGPoint(x: 0, y: bottom))
                area.closeSubpath()
                // Tinted with the hour's weather: blue for rain, grey for cloud.
                ctx.fill(
                    area,
                    with: .linearGradient(
                        Gradient(colors: [weather.opacity(0.75), weather.opacity(0.08)]),
                        startPoint: CGPoint(x: 0, y: top), endPoint: CGPoint(x: 0, y: bottom)))
            }
            let color: Color = s.primary ? .white : s.measure.color(colors)
            let style =
                s.measure == .gusts
                ? StrokeStyle(lineWidth: 2.2, lineCap: .round, dash: [1, 4])
                : StrokeStyle(lineWidth: s.primary ? 2.5 : 2, lineCap: .round)
            ctx.stroke(line, with: .color(color), style: style)
        }
        // Above its point, else below when another label is there, else just
        // the dot.
        var taken: [ClosedRange<CGFloat>] = []
        for l in labels.sorted(by: { $0.point.y < $1.point.y }) {
            ctx.fill(
                Path(ellipseIn: CGRect(x: l.point.x - 3, y: l.point.y - 3, width: 6, height: 6)),
                with: .color(l.color))
            // Spots scale with the label (they were 14 tall for 10 pt).
            let k = labelSize / 10
            let above = (l.point.y - 21 * k)...(l.point.y - 7 * k)
            let below = (l.point.y + 6 * k)...(l.point.y + 20 * k)
            let free = { (r: ClosedRange<CGFloat>) in
                r.lowerBound >= 0 && !taken.contains { $0.overlaps(r) }
            }
            guard let spot = free(above) ? above : free(below) ? below : nil else { continue }
            taken.append(spot)
            ctx.draw(
                Text(l.text).font(.system(size: labelSize, weight: .semibold)).foregroundStyle(l.color),
                at: CGPoint(x: l.point.x, y: spot.lowerBound), anchor: .top)
        }
        // Patterns and symbols, over the bars and lines: rain, snow and storm hours
        // show their symbol just above the strip, so its color isn't the only sign.
        if patterns, let symbol = Self.wetSymbol(WeatherSymbols.group(hour.icon)) {
            ctx.draw(
                Text(Image(systemName: symbol)).font(.system(size: 9, weight: .semibold))
                    .foregroundStyle(.white.opacity(0.85)),
                at: CGPoint(x: mid, y: bottom - 1), anchor: .bottom)
        }
    }
}

extension Preferences.HourTile: CustomStringConvertible {
    var description: String { rawValue }
}

extension Preferences.HourTileChoice: @MainActor GridChoice {}

/// One hour in full: its time, icon, temperature and condition, then the
/// chosen measures as tiles. Touch and hold a tile to edit them like the home
/// screen (`JiggleGrid`).
struct HourDetailCard: View {
    let day: String
    let hour: ViewModel.Hour
    @Binding var tiles: [Preferences.HourTileChoice]
    @State private var editing = false
    @Environment(\.feelsLikeLeads) private var feelsLeads

    /// The feels-like tile shows the air temperature when feels-like leads.
    private func title(_ tile: Preferences.HourTile) -> String {
        tile == .feelsLike && feelsLeads ? "Actual" : tile.title
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack(spacing: 16) {
                ConditionIcon(icon: hour.icon, size: 46)
                VStack(alignment: .leading, spacing: 0) {
                    Text([day, hour.dateShort].filter { !$0.isEmpty }.joined(separator: ", ") + " · \(hour.label)")
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                    if feelsLeads { FeelsLikeTag() }
                    Text(hour.temperature.text).font(.system(size: 46, weight: .light)).monospacedDigit()
                    Text(hour.conditionLabel).font(.headline)
                }
                Spacer()
                DoneEditingButton(editing: $editing)
            }
            .contentShape(.rect)
            // The tiles below have their own touch and hold (editing them).
            .cardContextMenu { withAnimation(.snappy) { editing = true } }
            JiggleGrid(choices: $tiles, editing: $editing, columns: 3, spacing: 16, title: { title($0.tile) }) {
                choice in
                content(choice.tile)
                    .padding(editing ? 8 : 0)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(.white.opacity(editing ? 0.08 : 0), in: .rect(cornerRadius: 12))
            }
            if !editing && !tiles.contains(where: \.shown) {
                Button("Choose the hour's cards", systemImage: "square.grid.3x3") {
                    withAnimation(.snappy) { editing = true }
                }
                .font(.callout)
            }
        }
        .onAppear {
            #if DEBUG
                // Screenshots: `-edittiles` starts editing.
                if ProcessInfo.processInfo.arguments.contains("-edittiles") { editing = true }
            #endif
        }
    }

    /// A tile's title, value and caption, as the core wrote them.
    private func content(_ tile: Preferences.HourTile) -> some View {
        let (value, caption): (String, String?) =
            switch tile {
            case .feelsLike: feelsLeads ? (hour.actual.text, nil) : (hour.feelsLike.text, nil)
            case .rain:
                (hour.precipProbability.map { chanceText(percent: $0) } ?? "–", hour.precipitation.text)
            case .wind: ("\(hour.wind.speed.text) \(hour.wind.compass)", nil)
            case .gusts: (hour.wind.gust.text, nil)
            case .humidity: (hour.humidityText ?? "–", nil)
            case .dewPoint: (hour.dewPoint.text, nil)
            case .uv: (hour.uvText ?? "–", nil)
            case .pressure: (hour.pressure.text, nil)
            case .visibility: (hour.visibility.text, nil)
            case .cloudCover: (hour.cloudCover.text, nil)
            }
        return VStack(alignment: .leading, spacing: 3) {
            Label(title(tile), systemImage: tile.symbol)
                .font(.caption2.weight(.medium))
                .foregroundStyle(.secondary)
                .lineLimit(1)
            Text(value).font(.callout.weight(.medium)).monospacedDigit().lineLimit(1)
                .minimumScaleFactor(0.7)
            if let caption {
                Text(caption).font(.caption2).foregroundStyle(.secondary).lineLimit(1)
            }
        }
        .accessibilityElement(children: .combine)
    }
}
