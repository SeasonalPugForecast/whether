import SwiftUI
import WhetherKit

/// The next days: icon, chance of rain, the low–high bar on the week's range,
/// and a line in a forecaster's words. Tap a day for more. What a row shows and
/// what an opened day shows is `Preferences.dayRowItems` and `dayDetailItems`:
/// touch and hold a day for the quick switches and the editor.
struct DailySection: View {
    @Environment(\.colorTheme) private var colors
    let daily: ViewModel.Daily
    @State private var open: Set<String> = []
    @State private var editing = false
    @Environment(\.feelsLikeLeads) private var feelsLeads
    @Environment(\.dayRowItems) private var dayRowItems
    @Environment(\.dayDetailItems) private var dayDetailItems

    /// The row items to draw: the saved ones (screenshots may change them for one
    /// launch, without saving: `-dayshow humidity,wind` and `-dayhide outlook`,
    /// by raw value).
    private var rowChoices: [DayRowChoice] {
        var items = dayRowItems.wrappedValue
        #if DEBUG
            let args = ProcessInfo.processInfo.arguments
            for (flag, shown) in [("-dayshow", true), ("-dayhide", false)] {
                if let i = args.firstIndex(of: flag), i + 1 < args.count {
                    let names = args[i + 1].split(separator: ",").map(String.init)
                    for item in DayRowItem.allCases where names.contains(item.rawValue) {
                        items.set(item, shown: shown)
                    }
                }
            }
        #endif
        return items
    }

    /// The same for the opened day: `-daydetailhide hours,sun`.
    private var detailChoices: [DayDetailChoice] {
        var items = dayDetailItems.wrappedValue
        #if DEBUG
            let args = ProcessInfo.processInfo.arguments
            if let i = args.firstIndex(of: "-daydetailhide"), i + 1 < args.count {
                let names = args[i + 1].split(separator: ",").map(String.init)
                for item in DayDetailItem.allCases where names.contains(item.rawValue) {
                    items.set(item, shown: false)
                }
            }
        #endif
        return items
    }

    var body: some View {
        let layout = DayRowLayout.make(rowChoices)
        let blocks = DayDetailLayout.blocks(detailChoices)
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                SectionTitle(text: "Next \(daily.days.count) days")
                Spacer()
                if feelsLeads {
                    Text("Feels-like highs and lows").font(.caption2).foregroundStyle(.secondary)
                }
            }
                .onAppear {
                    #if DEBUG
                        // Screenshots: `-expandDay 2` opens the third day.
                        let args = ProcessInfo.processInfo.arguments
                        if let i = args.firstIndex(of: "-expandDay"), i + 1 < args.count,
                            let day = Int(args[i + 1]), daily.days.indices.contains(day)
                        {
                            open.insert(daily.days[day].dateShort)
                        }
                        // Screenshots: `-editdays` opens the editor.
                        if args.contains("-editdays") { editing = true }
                    #endif
                }
            VStack(spacing: 0) {
                ForEach(daily.days, id: \.dateShort) { day in
                    row(day, layout: layout)
                    if open.contains(day.dateShort) {
                        details(day, layout: layout, blocks: blocks)
                    }
                    Divider()
                }
            }
        }
        .contextMenu {
            ForEach([DayRowItem.precipChance, .outlook, .airTemps, .humidity, .wind, .uv]) { item in
                Toggle(
                    item.title, systemImage: item.symbol,
                    isOn: Binding(
                        get: { rowChoices.isShown(item) },
                        set: { dayRowItems.wrappedValue.set(item, shown: $0) }))
            }
            // Customize…, Edit the tab, Hide the card.
            CardMenuButtons { editing = true }
        }
        .sheet(isPresented: $editing) {
            DayRowsEditor(rows: dayRowItems, details: dayDetailItems, sheet: true)
        }
    }

    private func row(_ day: ViewModel.Day, layout: DayRowLayout) -> some View {
        Button {
            withAnimation(.snappy) {
                if open.contains(day.dateShort) {
                    open.remove(day.dateShort)
                } else {
                    open.insert(day.dateShort)
                }
            }
        } label: {
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 10) {
                    VStack(alignment: .leading, spacing: 0) {
                        Text(day.label)
                        Text(day.dateShort).font(.caption2).foregroundStyle(.secondary)
                    }
                    .lineLimit(1)
                    .minimumScaleFactor(0.8)
                    .frame(width: 78, alignment: .leading)
                    ConditionIcon(icon: day.icon, size: 20).frame(width: 28)
                    if layout.chance {
                        Text(day.precipProbability.map { $0 >= 10 ? chanceText(percent: $0) : "" } ?? "")
                            .font(.caption)
                            .foregroundStyle(colors.rain)
                            .frame(width: 34, alignment: .leading)
                    }
                    if layout.bar {
                        Text(day.low.text)
                            .foregroundStyle(.secondary)
                            .frame(width: 40, alignment: .trailing)
                        RangeBar(
                            low: day.range.first ?? 0, high: day.range.last ?? 1,
                            // Normals are air temperatures: no marks on a feels-like bar
                            // (the opened day lists both).
                            normalLow: feelsLeads ? nil : fraction(day.normalLow),
                            normalHigh: feelsLeads ? nil : fraction(day.normalHigh)
                        )
                        .frame(height: 6)
                        Text(day.high.text).frame(width: 40, alignment: .leading)
                    } else {
                        // Without the bar the temperatures stand on their own.
                        Spacer(minLength: 0)
                        Text(day.low.text).foregroundStyle(.secondary)
                        Text("–").foregroundStyle(.tertiary)
                        Text(day.high.text)
                    }
                }
                .monospacedDigit()
                ForEach(Array(layout.lines.enumerated()), id: \.offset) { _, line in
                    switch line {
                    case .outlook:
                        // The day as a forecaster would put it.
                        if let outlook = day.outlook {
                            Text(outlook)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .lineLimit(open.contains(day.dateShort) ? nil : 2)
                                .multilineTextAlignment(.leading)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    case .facts(let facts):
                        if let text = factsLine(day, facts) {
                            Text(text)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .multilineTextAlignment(.leading)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    }
                }
            }
            .padding(.vertical, 10)
            .contentShape(.rect)
            .accessibilityElement(children: .combine)
            .accessibilityValue(day.outlook ?? day.precipText ?? day.conditionLabel)
            .accessibilityHint("Shows the day's details")
        }
        .buttonStyle(.plain)
    }

    /// The day's small facts as one line, each as the core wrote it. The UV
    /// index is the strongest hour's.
    private func factsLine(_ day: ViewModel.Day, _ facts: [DayFact]) -> String? {
        DayFacts.line(
            facts,
            air: feelsLeads ? "\(day.actualHigh.text) / \(day.actualLow.text)" : nil,
            humidity: humidityRange(day.hours),
            wind: day.windMax.value == nil ? nil : day.windMax.text,
            uv: day.hours.filter { $0.uv != nil && $0.uvText != nil }
                .max { $0.uv! < $1.uv! }?.uvText)
    }

    /// A normal's place on the period's range (0…1), for the bar's ticks; none
    /// outside it (at the bar's end it would read as the week's extreme).
    private func fraction(_ q: ViewModel.Quantity?) -> Float? {
        guard let v = q?.value, daily.periodMax > daily.periodMin else { return nil }
        let f = (v - daily.periodMin) / (daily.periodMax - daily.periodMin)
        return (0...1).contains(f) ? f : nil
    }

    private func details(_ day: ViewModel.Day, layout: DayRowLayout, blocks: [DayDetailBlock]) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            // The day in words, unless its row already says it.
            if !(layout.lines.contains(.outlook) && day.outlook != nil) {
                Text(day.outlook ?? day.precipText ?? "\(day.conditionLabel).").font(.callout)
            }
            ForEach(Array(blocks.enumerated()), id: \.offset) { _, block in
                switch block {
                case .timeline(let stretches, let hours):
                    if !day.hours.isEmpty {
                        DayTimeline(day: day, showStretches: stretches, showHours: hours)
                    }
                case .lines(let lines):
                    VStack(alignment: .leading, spacing: 4) {
                        ForEach(lines, id: \.self) { line in captionLine(line, day) }
                    }
                    .font(.caption)
                    .foregroundStyle(.secondary)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.bottom, 14)
        .transition(.opacity.combined(with: .move(edge: .top)))
    }

    @ViewBuilder
    private func captionLine(_ line: DayDetailLine, _ day: ViewModel.Day) -> some View {
        switch line {
        case .totals:
            Text(
                [
                    "Precipitation \(day.precipSum.text)",
                    day.windMax.value.map { _ in "Wind \(day.windMax.text)" },
                    day.gustMax?.value.map { _ in "Gusts \(day.gustMax!.text)" },
                    humidityRange(day.hours).map { "Humidity \($0)" },
                ]
                    .compactMap { $0 }.joined(separator: " · "))
        case .sun:
            if let sunrise = day.sunrise, let sunset = day.sunset {
                Text("Sunrise \(sunrise) · Sunset \(sunset)")
            }
        case .airTemps:
            if feelsLeads {
                Text("Actual high \(day.actualHigh.text) · low \(day.actualLow.text)")
            }
        case .normals:
            if let high = day.normalHigh, let low = day.normalLow, high.value != nil {
                Text("Normal \(high.text) / \(low.text)")
            }
        }
    }
}

/// A day's hours: its stretches of weather across the width, and every third
/// hour's time, icon and temperature underneath. Either part can be left out
/// (`Preferences.dayDetailItems`).
struct DayTimeline: View {
    let day: ViewModel.Day
    var showStretches = true
    var showHours = true
    /// The tapped stretch, an index into `day.timeline`.
    @State private var selected: Int?
    /// The hour under the finger while sliding along the day, an index into
    /// `day.hours`.
    @State private var hour: Int?

    /// Picks the hour at `x` and its stretch.
    private func scrub(_ x: CGFloat, perHour: CGFloat) {
        guard !day.hours.isEmpty else { return }
        let i = min(max(Int(x / max(perHour, 1)), 0), day.hours.count - 1)
        guard i != hour else { return }
        hour = i
        selected = day.timeline.firstIndex { i >= $0.start && i < $0.start + $0.len }
    }

    private var marks: [ViewModel.Hour] {
        day.hours.enumerated().filter { $0.offset % 3 == 0 }.map(\.element)
    }

    /// The hour shown when the day opens: now on today, else midday.
    private var firstHour: Int? {
        guard !day.hours.isEmpty else { return nil }
        let now = Int64(Date.now.timeIntervalSince1970)
        if let i = day.hours.lastIndex(where: { $0.time <= now }), now - day.hours[i].time < 3600 { return i }
        return min(day.hours.count / 2, day.hours.count - 1)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            // Above the bar, as in the next 24 hours, so the hand sliding along it
            // doesn't hide them; an hour is always chosen and the stretch's facts keep
            // two lines, so the bar stays put under the finger.
            if showHours, let hour, day.hours.indices.contains(hour) {
                HourDetail(hour: day.hours[hour], label: nil, showsDay: true)
            }
            if showStretches, let selected, day.timeline.indices.contains(selected) {
                StretchDetail(day: day, span: day.timeline[selected])
            }
            GeometryReader { geo in
                let perHour = geo.size.width / CGFloat(max(day.hours.count, 1))
                VStack(alignment: .leading, spacing: 6) {
                    if showStretches {
                        // The bar only shows the stretch: the slide below picks it,
                        // so a single tap selects (the bar's own tap would toggle
                        // the stretch the slide just picked off again).
                        TimelineBar(
                            spans: day.timeline, height: 24, totalWidth: geo.size.width,
                            selected: Binding(get: { selected }, set: { _ in }))
                            .overlay(alignment: .leading) {
                                // The hour being slid over.
                                if let hour {
                                    Capsule()
                                        .fill(.white)
                                        .frame(width: 3, height: 34)
                                        .offset(x: perHour * (CGFloat(hour) + 0.5) - 1.5)
                                        .shadow(radius: 2)
                                }
                            }
                    }
                    if showHours {
                    HStack(alignment: .top, spacing: 0) {
                        ForEach(marks, id: \.time) { hour in
                            VStack(alignment: .leading, spacing: 4) {
                                Text(hour.label).font(.caption2).foregroundStyle(.secondary)
                                    .opacity(self.hour == nil ? 1 : 0.3)
                                ConditionIcon(icon: hour.icon, size: 15).frame(height: 18)
                                Text(hour.temperature.text).font(.caption).monospacedDigit()
                            }
                            .lineLimit(1)
                            .minimumScaleFactor(0.7)
                            .frame(width: perHour * 3, alignment: .leading)
                        }
                    }
                    .overlay(alignment: .topLeading) {
                        // The slid-to hour's time on the axis, under the marker.
                        if let hour, day.hours.indices.contains(hour) {
                            Text(day.hours[hour].label)
                                .font(.caption2.weight(.bold))
                                .foregroundStyle(.black)
                                .padding(.horizontal, 5)
                                .background(.white, in: .capsule)
                                .fixedSize()
                                .position(
                                    x: min(max(perHour * (CGFloat(hour) + 0.5), 24), geo.size.width - 24),
                                    y: 7)
                        }
                    }
                    }
                }
                .contentShape(.rect)
                // Slide along the day for each hour; the page still scrolls.
                .simultaneousGesture(
                    DragGesture(minimumDistance: 0)
                        .onChanged { scrub($0.location.x, perHour: perHour) }
                )
            }
            // The bar with the hours under it; either alone is shorter.
            .frame(height: showStretches ? (showHours ? 88 : 34) : 58)
            if day.hours.count > 1 {
                Text("Slide along the day for each hour.").font(.caption2).foregroundStyle(.tertiary)
            }
        }
        .sensoryFeedback(.selection, trigger: hour)
        .onAppear {
            if hour == nil, let first = firstHour {
                hour = first
                selected = day.timeline.firstIndex { first >= $0.start && first < $0.start + $0.len }
            }
            #if DEBUG
                // Screenshots: `-scrubHour 15` slides to that hour.
                let a = ProcessInfo.processInfo.arguments
                if let i = a.firstIndex(of: "-scrubHour"), i + 1 < a.count, let n = Int(a[i + 1]),
                    day.hours.indices.contains(n)
                {
                    hour = n
                    selected = day.timeline.firstIndex { n >= $0.start && n < $0.start + $0.len }
                }
                // Screenshots: `-stretch 1` selects the second stretch.
                let args = ProcessInfo.processInfo.arguments
                if let i = args.firstIndex(of: "-stretch"), i + 1 < args.count, let n = Int(args[i + 1]),
                    day.timeline.indices.contains(n)
                {
                    selected = n
                }
            #endif
        }
    }
}

/// One stretch of a day: its weather and hours, then its temperatures, the
/// highest chance of rain and the strongest wind (as the core wrote them).
struct StretchDetail: View {
    @Environment(\.colorTheme) private var colors
    let day: ViewModel.Day
    let span: ViewModel.TimelineSpan

    private var hours: ArraySlice<ViewModel.Hour> {
        let end = min(span.start + span.len, day.hours.count)
        return span.start < end ? day.hours[span.start..<end] : []
    }

    private var facts: [String] {
        let hours = hours
        var out: [String] = []
        let byTemp = hours.filter { $0.temperature.value != nil }
        if let low = byTemp.min(by: { $0.temperature.value! < $1.temperature.value! }),
            let high = byTemp.max(by: { $0.temperature.value! < $1.temperature.value! })
        {
            out.append(
                low.temperature.text == high.temperature.text
                    ? low.temperature.text : "\(low.temperature.text)–\(high.temperature.text)")
        }
        if let wettest = hours.compactMap(\.precipProbability).max(), wettest > 0 {
            out.append("rain up to \(chanceText(percent: wettest))")
        }
        if let windiest = hours.filter({ $0.wind.speed.value != nil })
            .max(by: { $0.wind.speed.value! < $1.wind.speed.value! })
        {
            out.append("wind up to \(windiest.wind.speed.text)")
        }
        if let gustiest = hours.filter({ $0.wind.gust.value != nil })
            .max(by: { $0.wind.gust.value! < $1.wind.gust.value! })
        {
            out.append("gusts up to \(gustiest.wind.gust.text)")
        }
        if let humidity = humidityRange(hours) {
            out.append("humidity \(humidity)")
        }
        return out
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 8) {
                Circle().fill(colors.timeline(span.group)).frame(width: 9, height: 9)
                Text(span.label).font(.callout.weight(.semibold))
                if let first = hours.first, let last = hours.last {
                    Text(first.time == last.time ? first.label : "\(first.label)–\(last.label)")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
            }
            .lineLimit(1)
            .minimumScaleFactor(0.8)
            // Two lines kept whatever the stretch, so what is below doesn't move.
            Text(facts.joined(separator: " · ")).font(.caption).foregroundStyle(.secondary)
                .lineLimit(2, reservesSpace: true)
        }
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(.white.opacity(0.08), in: .rect(cornerRadius: 12))
        .accessibilityElement(children: .combine)
    }
}

/// A day's low–high span on the period's range (both 0…1), with the normal
/// low and high as ticks when history is on.
struct RangeBar: View {
    @Environment(\.colorTheme) private var colors
    let low: Float
    let high: Float
    var normalLow: Float? = nil
    var normalHigh: Float? = nil

    var body: some View {
        GeometryReader { geo in
            let w = geo.size.width
            let start = CGFloat(min(max(low, 0), 1)) * w
            let span = max(CGFloat(min(max(high, 0), 1)) * w - start, 6)
            ZStack(alignment: .leading) {
                Capsule().fill(.quaternary)
                Capsule()
                    .fill(
                        LinearGradient(
                            colors: [colors.color(.tempRangeLow), colors.color(.tempRangeHigh)],
                            startPoint: .leading, endPoint: .trailing)
                    )
                    .frame(width: span)
                    .offset(x: start)
                if let normalLow { tick(normalLow, width: w, height: geo.size.height) }
                if let normalHigh { tick(normalHigh, width: w, height: geo.size.height) }
            }
        }
        .accessibilityHidden(true)
    }

    /// A normal's tick at `f` (0…1) of the bar.
    private func tick(_ f: Float, width w: CGFloat, height: CGFloat) -> some View {
        Capsule()
            .fill(.white.opacity(0.9))
            .frame(width: 2, height: height + 6)
            .offset(x: CGFloat(f) * w - 1)
    }
}

/// The lowest and highest humidity among `hours`, as the core wrote them:
/// "45%–80%", or one value when they match; `nil` without any.
func humidityRange(_ hours: some Collection<ViewModel.Hour>) -> String? {
    let known = hours.filter { $0.humidity != nil && $0.humidityText != nil }
    guard let low = known.min(by: { $0.humidity! < $1.humidity! })?.humidityText,
        let high = known.max(by: { $0.humidity! < $1.humidity! })?.humidityText
    else { return nil }
    return low == high ? low : "\(low)–\(high)"
}
