import Charts
import SwiftUI
import WhetherKit

/// Dark Sky's next-hour graph, over 1 to 8 hours: how hard it will rain or
/// snow, against light, moderate and heavy lines.
struct NextHoursSection: View {
    @Environment(\.colorTheme) private var colors
    let nowcast: ViewModel.Nowcast
    @Binding var hours: Int
    /// Its own title and hours picker (not inside `RainSection`).
    var header = true

    static let choices = [1, 2, 4, 8]

    /// The scale's names follow Dynamic Type.
    @ScaledMetric(relativeTo: .caption2) private var levelSize: CGFloat = 9

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if header {
                HStack {
                    SectionTitle(text: "Next hours")
                    Spacer()
                    Picker("Hours", selection: $hours) {
                        ForEach(Self.choices, id: \.self) { Text("\($0) h").tag($0) }
                    }
                    .pickerStyle(.segmented)
                    .frame(maxWidth: 200)
                }
            }
            Text(nowcast.text)
                .font(.callout)
                .foregroundStyle(nowcast.wet ? colors.rain : .primary)
            if nowcast.wet {
                graph
            }
        }
        .sensoryFeedback(.selection, trigger: hours)
    }

    /// The intensity as a smooth area, each step at its middle.
    private var graph: some View {
        let steps = Array(nowcast.intensity.enumerated())
        return Chart {
            ForEach(steps, id: \.offset) { step, value in
                AreaMark(
                    x: .value("Time", Double(step) + 0.5),
                    y: .value("Intensity", Double(value))
                )
                .interpolationMethod(.monotone)
                .foregroundStyle(
                    LinearGradient(
                        colors: [colors.rain.opacity(0.5), colors.rain.opacity(0.08)],
                        startPoint: .top, endPoint: .bottom))
                LineMark(
                    x: .value("Time", Double(step) + 0.5),
                    y: .value("Intensity", Double(value))
                )
                .interpolationMethod(.monotone)
                .foregroundStyle(colors.rain)
                .lineStyle(StrokeStyle(lineWidth: 2, lineCap: .round))
            }
            ForEach(Array(nowcast.levels.enumerated()), id: \.offset) { index, level in
                RuleMark(y: .value("Level", Double(index + 1) / 3))
                    .foregroundStyle(.secondary.opacity(0.4))
                    .lineStyle(StrokeStyle(lineWidth: 0.5, dash: [3, 3]))
            }
        }
        // The scale's names over its lines (annotations go missing on glass).
        .chartOverlay { proxy in
            GeometryReader { geo in
                if let anchor = proxy.plotFrame {
                    let frame = geo[anchor]
                    ForEach(Array(nowcast.levels.enumerated()), id: \.offset) { index, level in
                        if let y = proxy.position(forY: Double(index + 1) / 3) {
                            Text(level.uppercased())
                                .font(.system(size: levelSize, weight: .semibold))
                                .foregroundStyle(.secondary)
                                .frame(width: 120, alignment: .leading)
                                .position(x: frame.minX + 64, y: frame.minY + y - 7)
                        }
                    }
                }
            }
            .allowsHitTesting(false)
        }
        .chartXScale(domain: 0...Double(max(nowcast.intensity.count, 1)))
        .chartYScale(domain: 0...1.12)
        .chartYAxis(.hidden)
        .chartXAxis {
            AxisMarks(values: nowcast.ticks.map { Double($0.step) }) { value in
                AxisGridLine().foregroundStyle(.quaternary)
                AxisValueLabel(anchor: .top) {
                    if let step = value.as(Double.self),
                        let tick = nowcast.ticks.first(where: { Double($0.step) == step })
                    {
                        Text(tick.label)
                    }
                }
            }
        }
        .frame(height: 130)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(nowcast.text)
    }
}

/// The Now tab's one precipitation panel: up to eight hours the nowcast (when
/// rain starts or stops, and its fine graph), beyond that the table of hours.
struct RainSection: View {
    let nowcast: ViewModel.Nowcast?
    let precipitation: ViewModel.Precipitation?
    /// The hours chosen; the nowcast's or the table's hours follow it.
    @Binding var span: Int
    @Binding var nowcastHours: Int
    @Binding var precipHours: Int

    private var choices: [Int] {
        (nowcast == nil ? [] : NextHoursSection.choices) + (precipitation == nil ? [] : [12, 24, 48])
    }

    /// The chosen span, or the nearest one there is.
    private var shown: Int {
        #if DEBUG
            // Screenshots: `-precipSample` shows the 12-hour table it fills.
            if ProcessInfo.processInfo.arguments.contains("-precipSample"), precipitation != nil { return 12 }
        #endif
        return choices.min { abs($0 - span) < abs($1 - span) } ?? span
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                SectionTitle(text: "Precipitation")
                Spacer()
                Picker(
                    "Hours",
                    selection: Binding(
                        get: { shown },
                        set: { hours in
                            span = hours
                            if hours <= 8 { nowcastHours = hours } else { precipHours = hours }
                        })
                ) {
                    ForEach(choices, id: \.self) { Text("Next \($0) h").tag($0) }
                }
                .pickerStyle(.menu)
                .font(.caption)
            }
            if shown <= 8, let nowcast {
                NextHoursSection(nowcast: nowcast, hours: $nowcastHours, header: false)
            } else if let precipitation {
                PrecipitationSection(precipitation: precipitation, hours: $precipHours, header: false)
            }
        }
    }
}
