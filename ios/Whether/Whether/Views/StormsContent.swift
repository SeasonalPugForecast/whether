import Charts
import SwiftUI
import WhetherKit

/// The storms at one place, in the Storms drawer: for the storm picked its agreement,
/// each model's numbers, the ensembles' plumes and the run-to-run bars, and the data
/// credits. Every text is the core's.
struct StormsContent: View {
    let view: StormsView
    let stale: Bool
    let failure: String?
    /// The models the map's chips hide (by model id): their family's plumes go too.
    let hidden: Set<String>
    /// Takes the bottom of the agreement card (or of the summary) in this view: what the
    /// drawer shows when small.
    let peek: (CGFloat) -> Void
    /// Fetches the earlier runs for the storm shown.
    let loadPastRuns: (StormsView.Event) async -> Void
    @State private var picked = 0

    /// The coordinate space `peek` measures in.
    nonisolated static let space = "storms-content"

    /// What the earlier runs depend on: the storm's window and the runs shown.
    private struct PastRunsID: Hashable {
        var start: Int64
        var end: Int64
        var runs: [String]
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            if stale {
                Note(symbol: "clock.badge.exclamationmark", text: "This outlook is over 12 hours old.")
            }
            if let failure {
                Note(symbol: "exclamationmark.triangle", text: failure)
            }
            if view.events.isEmpty {
                if let summary = view.summary {
                    Text(summary)
                        .font(.callout)
                        .glassCard()
                        .modifier(PeekMark(peek: peek))
                }
            } else {
                if view.events.count > 1 { picker }
                let event = view.events[min(picked, view.events.count - 1)]
                EventDetail(
                    event: event,
                    ensembles: StormMapRules.shown(event.ensembles, hidden: hidden, chips: view.models),
                    peek: peek
                )
                .task(id: PastRunsID(start: event.start, end: event.end, runs: view.models.map(\.run))) {
                    await loadPastRuns(event)
                }
            }
            VStack(alignment: .leading, spacing: 2) {
                ForEach(view.attribution, id: \.self) { line in
                    Text(line)
                }
            }
            .font(.caption2)
            .foregroundStyle(.secondary)
            .frame(maxWidth: .infinity, alignment: .leading)
            .id("storms-credits")
        }
        .coordinateSpace(.named(Self.space))
    }

    /// The storms, when there is more than one.
    private var picker: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                ForEach(Array(view.events.enumerated()), id: \.offset) { index, event in
                    let selected = index == min(picked, view.events.count - 1)
                    Button {
                        withAnimation(.snappy) { picked = index }
                    } label: {
                        Text(event.title)
                            .font(.subheadline.weight(selected ? .semibold : .regular))
                            .padding(.horizontal, 12)
                            .padding(.vertical, 6)
                            .background(
                                selected ? AnyShapeStyle(.tint.opacity(0.3)) : AnyShapeStyle(.quaternary.opacity(0.5)),
                                in: .capsule)
                    }
                    .buttonStyle(.plain)
                    .accessibilityAddTraits(selected ? .isSelected : [])
                }
            }
        }
    }
}

/// Reports the bottom of the view it marks, in the content's coordinate space.
private struct PeekMark: ViewModifier {
    let peek: (CGFloat) -> Void

    func body(content: Content) -> some View {
        content.onGeometryChange(for: CGFloat.self) {
            $0.frame(in: .named(StormsContent.space)).maxY
        } action: {
            peek($0)
        }
    }
}

/// A line above the storms: the outlook is old, or couldn't be updated.
private struct Note: View {
    let symbol: String
    let text: String

    var body: some View {
        Label(text, systemImage: symbol)
            .font(.footnote)
            .foregroundStyle(.secondary)
    }
}

/// One storm: agreement, the models' numbers, the ensembles and the runs.
private struct EventDetail: View {
    let event: StormsView.Event
    /// The event's ensembles the chips leave shown.
    let ensembles: [StormsView.Ensemble]
    let peek: (CGFloat) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            if let agreement = event.agreement {
                VStack(alignment: .leading, spacing: 10) {
                    title
                    AgreementCard(agreement: agreement)
                }
                .modifier(PeekMark(peek: peek))
            } else {
                title.modifier(PeekMark(peek: peek))
            }
            ModelTable(rows: event.rows).glassCard()
            VStack(spacing: 14) {
                ForEach(ensembles) { ensemble in
                    PlumeCard(ensemble: ensemble).glassCard()
                }
            }
            .id("storms-plumes")
            if !event.runToRun.isEmpty {
                VStack(alignment: .leading, spacing: 16) {
                    SectionTitle(text: "Run to run")
                    ForEach(event.runToRun) { RunBars(runs: $0) }
                }
                .glassCard()
                .id("storms-runs")
            }
        }
    }

    private var title: some View {
        Text(event.title)
            .font(.headline)
            .accessibilityAddTraits(.isHeader)
    }
}

/// How closely the models agree, colored by the core's level.
private struct AgreementCard: View {
    let agreement: StormsView.Agreement
    @Environment(\.colorTheme) private var colors

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Capsule()
                .fill(colors.level(agreement.level))
                .frame(width: 5)
            VStack(alignment: .leading, spacing: 4) {
                Text(agreement.label).font(.headline)
                if !agreement.sentence.isEmpty {
                    Text(agreement.sentence)
                        .font(.callout)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
        .glassCard()
        .accessibilityElement(children: .combine)
    }
}

/// Each model's snow, precipitation, gust, lowest pressure and onset: a table when it
/// fits, else a block per model.
private struct ModelTable: View {
    let rows: [StormsView.ModelRow]

    var body: some View {
        ViewThatFits(in: .horizontal) {
            table
            stacked
        }
    }

    private var table: some View {
        Grid(alignment: .leading, horizontalSpacing: 10, verticalSpacing: 10) {
            GridRow {
                Text("Model")
                Text("Snow")
                Text("Precip")
                Text("Gust")
                Text("Low")
            }
            .font(.caption.weight(.semibold))
            .foregroundStyle(.secondary)
            ForEach(rows) { row in
                GridRow(alignment: .firstTextBaseline) {
                    // When it starts under the name, when the pressure bottoms out under it.
                    VStack(alignment: .leading, spacing: 1) {
                        ModelName(short: row.short, role: row.role)
                        Text("Starts \(row.onset)").font(.caption2).foregroundStyle(.secondary)
                    }
                    Text(row.snow.text)
                    Text(row.precipitation.text)
                    Text(row.gust.text)
                    VStack(alignment: .leading, spacing: 1) {
                        Text(row.low.text)
                        Text(row.lowAt).font(.caption2).foregroundStyle(.secondary)
                    }
                }
                .font(.footnote)
                .lineLimit(1)
            }
        }
        // VoiceOver reads a model per row, not a cell at a time.
        .accessibilityRepresentation {
            VStack {
                ForEach(rows) { Text(Self.spoken($0)) }
            }
        }
    }

    private var stacked: some View {
        VStack(alignment: .leading, spacing: 12) {
            ForEach(rows) { row in
                VStack(alignment: .leading, spacing: 4) {
                    ModelName(short: row.short, role: row.role)
                    Text("Snow \(row.snow.text) · Precip \(row.precipitation.text) · Gust \(row.gust.text)")
                    Text("Low \(row.low.text), \(row.lowAt) · Starts \(row.onset)")
                }
                .font(.footnote)
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(Self.spoken(row))
            }
        }
    }

    /// What VoiceOver reads for a model: its name, then each value as the core wrote it.
    static func spoken(_ row: StormsView.ModelRow) -> String {
        "\(row.short): snow \(row.snow.text), precipitation \(row.precipitation.text), "
            + "gust \(row.gust.text), lowest pressure \(row.low.text) at \(row.lowAt), "
            + "starting \(row.onset)"
    }
}

/// A model's dot and short name.
private struct ModelName: View {
    let short: String
    let role: String
    @Environment(\.colorTheme) private var colors

    var body: some View {
        HStack(spacing: 5) {
            Circle().fill(colors.model(role)).frame(width: 7, height: 7)
            Text(short).fontWeight(.semibold)
        }
    }
}

/// An ensemble's members as thin lines, the 10th to 90th percentile as a band and the
/// median bold, on the core's axes.
private struct PlumeCard: View {
    let ensemble: StormsView.Ensemble
    @Environment(\.colorTheme) private var colors

    private struct Point: Identifiable {
        var time: Double
        var value: Double
        var id: Double { time }
    }

    private struct Band: Identifiable {
        var time: Double
        var low: Double
        var high: Double
        var id: Double { time }
    }

    /// The points of a series that have a value.
    private func points(_ series: [Float?]) -> [Point] {
        zip(plume.times, series).compactMap { time, value in
            value.map { Point(time: Double(time), value: Double($0)) }
        }
    }

    private var plume: StormsView.Plume { ensemble.plume }

    private var bands: [Band] {
        zip(plume.times, zip(plume.p10, plume.p90)).compactMap { time, pair in
            guard let low = pair.0, let high = pair.1 else { return nil }
            return Band(time: Double(time), low: Double(low), high: Double(high))
        }
    }

    var body: some View {
        let color = colors.model(ensemble.role)
        VStack(alignment: .leading, spacing: 6) {
            SectionTitle(text: "\(ensemble.short) · \(plume.title)")
            Text(ensemble.text)
                .font(.footnote)
                .foregroundStyle(.secondary)
            chart(color)
                .frame(height: 170)
                .accessibilityElement(children: .ignore)
                .accessibilityLabel("\(ensemble.short), \(plume.title)")
                .accessibilityValue(
                    "\(ensemble.text). 10th percentile \(ensemble.p10.text), median \(ensemble.p50.text), "
                        + "90th percentile \(ensemble.p90.text)")
        }
    }

    private func chart(_ color: Color) -> some View {
        let members = Array(plume.members.enumerated())
        let median = points(plume.p50)
        return Chart {
            ForEach(bands) { band in
                AreaMark(
                    x: .value("Time", band.time),
                    yStart: .value("10th percentile", band.low),
                    yEnd: .value("90th percentile", band.high)
                )
                .foregroundStyle(color.opacity(0.22))
            }
            ForEach(members, id: \.offset) { index, member in
                ForEach(points(member)) { point in
                    LineMark(
                        x: .value("Time", point.time), y: .value("Total", point.value),
                        series: .value("Member", index)
                    )
                    .foregroundStyle(color.opacity(0.18))
                    .lineStyle(StrokeStyle(lineWidth: 0.6))
                }
            }
            ForEach(median) { point in
                LineMark(
                    x: .value("Time", point.time), y: .value("Total", point.value),
                    series: .value("Member", -1)
                )
                .foregroundStyle(color)
                .lineStyle(StrokeStyle(lineWidth: 2.5, lineCap: .round))
            }
        }
        .chartLegend(.hidden)
        .chartXScale(domain: xDomain)
        .chartYScale(domain: 0...Double(max(plume.yMax, 0.001)))
        .chartXAxis {
            AxisMarks(values: plume.xLabels.map { Double($0.time) }) { value in
                AxisGridLine()
                AxisTick()
                AxisValueLabel {
                    if plume.xLabels.indices.contains(value.index) {
                        Text(plume.xLabels[value.index].text)
                    }
                }
            }
        }
        .chartYAxis {
            AxisMarks(position: .leading, values: plume.yLabels.map { Double($0.value) }) { value in
                AxisGridLine()
                AxisValueLabel {
                    if plume.yLabels.indices.contains(value.index) {
                        Text(plume.yLabels[value.index].text)
                    }
                }
            }
        }
    }

    private var xDomain: ClosedRange<Double> {
        guard let first = plume.times.first, let last = plume.times.last, last > first else {
            return 0...1
        }
        return Double(first)...Double(last)
    }
}

/// One model's totals over its recent runs, the current run's bar emphasised.
private struct RunBars: View {
    let runs: StormsView.RunToRun
    @Environment(\.colorTheme) private var colors

    private struct Bar: Identifiable {
        var index: Int
        var bar: StormsView.RunBar
        var id: Int { index }
    }

    var body: some View {
        let color = colors.model(runs.role)
        let bars = runs.bars.enumerated().map { Bar(index: $0.offset, bar: $0.element) }
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 5) {
                Circle().fill(color).frame(width: 7, height: 7)
                Text("\(runs.short) · \(runs.title)").font(.subheadline.weight(.semibold))
            }
            Chart(bars) { item in
                BarMark(
                    x: .value("Run", item.bar.label),
                    y: .value(runs.title, Double(item.bar.value))
                )
                .foregroundStyle(color.opacity(item.bar.current ? 1 : 0.4))
            }
            .chartXScale(domain: runs.bars.map(\.label))
            .chartYScale(domain: 0...Double(max(runs.max, 0.001)))
            .chartYAxis(.hidden)
            // Each run's total over its label (annotations go missing on glass).
            .chartXAxis {
                AxisMarks(values: runs.bars.map(\.label)) { value in
                    AxisValueLabel {
                        if runs.bars.indices.contains(value.index) {
                            let bar = runs.bars[value.index]
                            VStack(spacing: 1) {
                                Text(bar.text)
                                    .font(.caption2.weight(bar.current ? .bold : .regular))
                                    .foregroundStyle(bar.current ? .primary : .secondary)
                                Text(bar.label).font(.caption2).foregroundStyle(.secondary)
                            }
                        }
                    }
                }
            }
            .frame(height: 130)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel("\(runs.short), \(runs.title), run to run")
            .accessibilityValue(runs.bars.map { "\($0.label) \($0.text)" }.joined(separator: ", "))
        }
    }
}
