import SwiftUI
import WhetherKit

/// Dark Sky's timeline: stretches of weather over 24 hours, labelled, with the
/// hours, icons and the chosen measures underneath (temperature, humidity,
/// wind, gusts … any mix, one row each). It scrolls like a list under a marker
/// that starts on now at the left and stays in the middle once there: the hour
/// under it is the one chosen; tap an hour to bring it there, pinch to show
/// fewer or more hours.
struct HourlySection: View {
    @Environment(\.colorTheme) private var colors
    let hourly: ViewModel.Hourly
    /// The measures shown per hour, in order (`GraphMeasure` raw values; the
    /// Hourly graph offers the same ones).
    @Binding var rows: [String]
    @Environment(\.feelsLikeLeads) private var feelsLeads
    /// The chosen hour, an index into `hours`.
    @State private var selected = 0
    /// One hour's width (a pinch changes it; the values need about 30 points).
    @State private var width: CGFloat = 46
    /// The rows' editor is open.
    @State private var editing = false

    private var hours: [ViewModel.Hour] { Array(hourly.hours.prefix(24)) }

    private var chosen: [GraphMeasure] { GraphMeasure.chosen(rows) }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                SectionTitle(text: "Next 24 hours")
                Spacer()
                MeasureMenu(measures: $rows, feelsLeads: feelsLeads) { editing = true }
            }
            HourDetail(
                hour: hours.indices.contains(selected) ? hours[selected] : nil, label: spanLabel(selected),
                showsDay: true)
            legend
            HourScroller(count: hours.count, selected: $selected, step: $width, zoom: 30...72) {
                // The marker: the chosen hour's column.
                RoundedRectangle(cornerRadius: 8)
                    .fill(.quaternary.opacity(0.6))
                    .frame(width: width)
                    .padding(.vertical, 4)
            } strip: {
                strip
            }
            .accessibilityElement(children: .ignore)
            .accessibilityLabel("Hours")
            .accessibilityValue(accessibilityValue)
            .accessibilityAdjustableAction { direction in
                switch direction {
                case .increment: selected = HourWindow.clamp(selected + 1, count: hours.count)
                case .decrement: selected = HourWindow.clamp(selected - 1, count: hours.count)
                default: break
                }
            }
            .frame(height: 96 + CGFloat(chosen.count) * 22)
            .sensoryFeedback(.selection, trigger: selected)
        }
        // Touch and hold the card (the legend's rows have their own menu).
        .contentShape(.rect)
        .contextMenu { CardMenuButtons { editing = true } }
        .sheet(isPresented: $editing) {
            MeasuresEditorSheet(kind: .rows, measures: $rows, options: nil, feelsLeads: feelsLeads)
        }
        .onAppear {
            #if DEBUG
                // Screenshots: `-pickHour 6` chooses that hour, `-rows
                // temperature,humidity,gusts` the measures, `-editrows` opens their editor.
                if ProcessInfo.processInfo.arguments.contains("-editrows") { editing = true }
                let args = ProcessInfo.processInfo.arguments
                if let i = args.firstIndex(of: "-pickHour"), i + 1 < args.count, let n = Int(args[i + 1]) {
                    selected = HourWindow.clamp(n, count: hours.count)
                }
                if let i = args.firstIndex(of: "-rows"), i + 1 < args.count {
                    rows = args[i + 1].split(separator: ",").map(String.init)
                }
            #endif
        }
    }

    /// The hours: times, the stretches of weather, icons and a row per measure.
    private var strip: some View {
        VStack(alignment: .leading, spacing: 6) {
            row { hour, index in
                // The chosen hour's time stands out on the axis; when the hours
                // are narrow, every few hours have theirs.
                Text(hour.label)
                    .font(.caption2.weight(index == selected ? .bold : .regular))
                    .foregroundStyle(index == selected ? AnyShapeStyle(.primary) : AnyShapeStyle(.secondary))
                    .lineLimit(1)
                    .fixedSize()
                    .opacity(index == selected || index % axisStride == 0 ? 1 : 0)
            }
            TimelineBar(spans: hourly.timeline, hourWidth: width, height: 30)
            row { hour, _ in ConditionIcon(icon: hour.icon, size: 18) }
            ForEach(Array(chosen.enumerated()), id: \.element) { i, measure in
                row { hour, _ in
                    Text(measure.value(hour).1)
                        .font(i == 0 ? .callout : .caption)
                        .foregroundStyle(i == 0 ? AnyShapeStyle(.primary) : AnyShapeStyle(measure.color(colors)))
                        .monospacedDigit()
                        .lineLimit(1)
                        .minimumScaleFactor(0.7)
                }
            }
        }
        .padding(.vertical, 4)
    }

    /// Every how many hours a time shows on the axis.
    private var axisStride: Int { HourWindow.labelStride(step: width, spacing: 44) }

    /// Which row is which, in the order of the lines. Touch and hold one for a
    /// menu: lead with it, move it up or down, hide it.
    private var legend: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                ForEach(Array(chosen.enumerated()), id: \.element) { i, measure in
                    HStack(spacing: 4) {
                        Circle().fill(i == 0 ? Color.white : measure.color(colors)).frame(width: 6, height: 6)
                        Text(measure.title(feelsLeads: feelsLeads))
                    }
                    .padding(.horizontal, 6)
                    .padding(.vertical, 4)
                    .contentShape(.contextMenuPreview, .capsule)
                    .contextMenu {
                        MeasureActions(measure: measure, kind: .rows, measures: $rows) { editing = true }
                    }
                    .accessibilityElement(children: .combine)
                    .accessibilityHint("Touch and hold for its menu")
                }
            }
        }
        .font(.caption2)
        .foregroundStyle(.secondary)
    }

    /// One column per hour.
    private func row(@ViewBuilder _ cell: @escaping (ViewModel.Hour, Int) -> some View) -> some View {
        HStack(spacing: 0) {
            ForEach(Array(hours.enumerated()), id: \.element.time) { index, hour in
                cell(hour, index).frame(width: width)
            }
        }
    }

    private var accessibilityValue: String {
        guard hours.indices.contains(selected) else { return "" }
        let hour = hours[selected]
        return ([hour.label, spanLabel(selected)] + chosen.map { "\($0.title(feelsLeads: feelsLeads)) \($0.value(hour).1)" })
            .compactMap { $0 }.joined(separator: ", ")
    }

    /// The condition of the stretch holding hour `index`.
    private func spanLabel(_ index: Int) -> String? {
        hourly.timeline.first { index >= $0.start && index < $0.start + $0.len }?.label
    }
}

/// Picks the measures shown per hour (any mix); the menu stays open to pick
/// several, and one always stays.
struct MeasureMenu: View {
    @Binding var measures: [String]
    let feelsLeads: Bool
    /// Opens the editor (the "Customize…" item).
    var customize: () -> Void

    private var chosen: [GraphMeasure] { GraphMeasure.chosen(measures) }

    var body: some View {
        Menu {
            ForEach(GraphMeasure.offered(feelsLeads: feelsLeads)) { m in
                Button {
                    toggle(m)
                } label: {
                    if chosen.contains(m) {
                        Label(m.title(feelsLeads: feelsLeads), systemImage: "checkmark")
                    } else {
                        Text(m.title(feelsLeads: feelsLeads))
                    }
                }
            }
            Divider()
            Button("Customize…", systemImage: "slider.horizontal.3", action: customize)
                // The pick buttons keep the menu open; this one leaves it.
                .menuActionDismissBehavior(.enabled)
        } label: {
            HStack(spacing: 4) {
                Text(
                    chosen.count > 2
                        ? "\(chosen[0].title(feelsLeads: feelsLeads)) + \(chosen.count - 1)"
                        : chosen.map { $0.title(feelsLeads: feelsLeads) }.joined(separator: " + ")
                )
                .lineLimit(1)
                Image(systemName: "chevron.up.chevron.down").font(.caption2)
            }
            .font(.caption)
        }
        .menuActionDismissBehavior(.disabled)
        .accessibilityLabel("Measures shown")
    }

    /// Adds `m`, or removes it while another stays.
    private func toggle(_ m: GraphMeasure) {
        var list = chosen
        if let i = list.firstIndex(of: m) {
            guard list.count > 1 else { return }
            list.remove(at: i)
        } else {
            list.append(m)
        }
        withAnimation(.snappy) { measures = list.map(\.rawValue) }
    }
}

/// The tapped hour in one line: time, condition, temperature, rain and wind.
struct HourDetail: View {
    @Environment(\.colorTheme) private var colors
    let hour: ViewModel.Hour?
    let label: String?
    /// Its day and date over the row ("Tomorrow, Nov 2"), for hours that
    /// run past midnight.
    var showsDay = false
    @Environment(\.feelsLikeLeads) private var feelsLeads

    var body: some View {
        if let hour {
            VStack(alignment: .leading, spacing: 2) {
            // Over the whole row, so it never narrows the condition beside the time.
            if showsDay {
                Text(hour.dayAndDate).font(.caption.weight(.semibold)).foregroundStyle(.secondary)
                    .contentTransition(.numericText())
            }
            HStack(spacing: 10) {
                Text(hour.label).font(.callout.weight(.semibold)).fixedSize()
                if let label {
                    Text(label).font(.callout).lineLimit(1).minimumScaleFactor(0.8).layoutPriority(1)
                }
                Spacer(minLength: 4)
                if feelsLeads {
                    // The feels-like temperature over the air's.
                    VStack(alignment: .trailing, spacing: 0) {
                        Text(hour.temperature.text).font(.callout)
                        Text("air \(hour.actual.text)").font(.caption2).foregroundStyle(.secondary)
                    }
                    .monospacedDigit()
                    .fixedSize()
                    .accessibilityElement(children: .combine)
                    .accessibilityLabel("Feels like \(hour.temperature.text), air \(hour.actual.text)")
                } else {
                    Text(hour.temperature.text).font(.callout).monospacedDigit()
                }
                // The chance of rain with the humidity under it.
                VStack(alignment: .leading, spacing: 0) {
                    if let chance = hour.precipProbability {
                        Label(chanceText(percent: chance), systemImage: "drop.fill")
                            .foregroundStyle(colors.rain)
                    }
                    if let humidity = hour.humidityText {
                        Label(humidity, systemImage: "humidity")
                            .foregroundStyle(.secondary)
                            .accessibilityLabel("Humidity \(humidity)")
                    }
                }
                .lineLimit(1)
                .fixedSize()
                // The wind with its gusts under it.
                Label {
                    VStack(alignment: .leading, spacing: 0) {
                        Text(hour.wind.speed.text)
                        if hour.wind.gust.value != nil {
                            Text("gusts \(hour.wind.gust.text)").font(.caption2)
                        }
                    }
                } icon: {
                    Image(systemName: "wind")
                }
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .fixedSize()
            }
            }
            .font(.caption)
            .labelStyle(.titleAndIcon)
            // Shrinks a little rather than wrap or cut the condition.
            .minimumScaleFactor(0.75)
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            // A capsule for one line; two lines get gentler corners.
            .background(.quaternary.opacity(0.5), in: .rect(cornerRadius: showsDay ? 20 : 999))
            .transition(.opacity.combined(with: .move(edge: .top)))
        }
    }
}

/// Stretches of weather side by side, each `hourWidth` per hour (or sharing
/// `totalWidth` when given), labelled when there is room.
struct TimelineBar: View {
    @Environment(\.colorTheme) private var colors
    let spans: [ViewModel.TimelineSpan]
    var hourWidth: CGFloat = 46
    var height: CGFloat = 28
    /// Fit every span into this width instead of `hourWidth` per hour.
    var totalWidth: CGFloat?
    /// When set, a tap selects a stretch (and a second tap clears it).
    var selected: Binding<Int?>?

    private var hourCount: Int { spans.reduce(0) { $0 + $1.len } }

    var body: some View {
        let perHour = totalWidth.map { $0 / CGFloat(max(hourCount, 1)) } ?? hourWidth
        HStack(spacing: 1) {
            ForEach(Array(spans.enumerated()), id: \.element.start) { index, span in
                UnevenRoundedRectangle(
                    topLeadingRadius: index == 0 ? height / 2 : 0,
                    bottomLeadingRadius: index == 0 ? height / 2 : 0,
                    bottomTrailingRadius: index == spans.count - 1 ? height / 2 : 0,
                    topTrailingRadius: index == spans.count - 1 ? height / 2 : 0
                )
                .fill(colors.timeline(span.group).gradient)
                .overlay {
                    if selected?.wrappedValue == index {
                        UnevenRoundedRectangle(
                            topLeadingRadius: index == 0 ? height / 2 : 0,
                            bottomLeadingRadius: index == 0 ? height / 2 : 0,
                            bottomTrailingRadius: index == spans.count - 1 ? height / 2 : 0,
                            topTrailingRadius: index == spans.count - 1 ? height / 2 : 0
                        )
                        .strokeBorder(.white, lineWidth: 2)
                    }
                }
                .opacity(selected?.wrappedValue.map { $0 == index ? 1 : 0.55 } ?? 1)
                .frame(width: max(perHour * CGFloat(span.len) - 1, 1), height: height)
                .contentShape(.rect)
                .onTapGesture {
                    guard let selected else { return }
                    withAnimation(.snappy) {
                        selected.wrappedValue = selected.wrappedValue == index ? nil : index
                    }
                }
                .overlay(alignment: .leading) {
                    if perHour * CGFloat(span.len) >= 70 {
                        Text(span.label)
                            .font(.caption2.weight(.semibold))
                            .foregroundStyle(colors.onTimeline(span.group))
                            .lineLimit(1)
                            .padding(.horizontal, index == 0 ? 12 : 8)
                    }
                }
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(
            spans.map { "\($0.label) for \($0.len) \($0.len == 1 ? "hour" : "hours")" }
                .joined(separator: ", "))
    }
}
