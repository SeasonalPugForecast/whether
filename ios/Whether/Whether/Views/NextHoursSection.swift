import Charts
import SwiftUI
import WhetherKit

/// Dark Sky's next-hour graph, over 1 to 8 hours: how hard it will rain or
/// snow, a bar a step, against light, moderate and heavy lines.
struct NextHoursSection: View {
    @Environment(\.colorTheme) private var colors
    let nowcast: ViewModel.Nowcast
    @Binding var hours: Int
    /// Its own title and hours picker (not inside `RainSection`).
    var header = true

    static let choices = [1, 2, 4, 8]

    /// The scale's names follow Dynamic Type.
    @ScaledMetric(relativeTo: .caption2) private var levelSize: CGFloat = 10

    var body: some View {
        let nowcast = shown
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
                graph(nowcast)
            }
        }
        .sensoryFeedback(.selection, trigger: hours)
    }

    /// The nowcast drawn (screenshots: `-nowcastSample 60` draws a made-up spell of
    /// rain in 60 steps over the real nowcast's hours, its ticks moved to match, for
    /// the launch only; 32 steps are eight hours of quarter hours).
    private var shown: ViewModel.Nowcast {
        #if DEBUG
            let args = ProcessInfo.processInfo.arguments
            if let i = args.firstIndex(of: "-nowcastSample"), i + 1 < args.count,
                let count = Int(args[i + 1]), count > 0
            {
                var out = nowcast
                let real = Double(max(nowcast.intensity.count, 1))
                // Light rain building to a heavy burst, then easing off.
                out.intensity = (0..<count).map { step in
                    let t = (Double(step) + 0.5) / Double(count)
                    guard t > 0.08, t < 0.92 else { return 0 }
                    let burst = 0.15 + 0.72 * exp(-pow((t - 0.45) / 0.14, 2))
                    return Float(min(max(burst + 0.04 * sin(Double(step) * 1.3), 0), 1))
                }
                out.ticks = nowcast.ticks.map { tick in
                    var moved = tick
                    moved.step = Int((Double(tick.step) * Double(count) / real).rounded())
                    return moved
                }
                out.wet = true
                return out
            }
        #endif
        return nowcast
    }

    /// Step `step`'s bar, up from nothing to `value`; it leaves a little of its
    /// step either side, a gap between bars.
    private func bar(_ step: Int, _ value: Double) -> some ChartContent {
        let inset = 0.14
        let start: PlottableValue<Double> = .value("Time", Double(step) + inset)
        let end: PlottableValue<Double> = .value("Time", Double(step + 1) - inset)
        let bottom: PlottableValue<Double> = .value("Intensity", 0)
        let top: PlottableValue<Double> = .value("Intensity", value)
        return RectangleMark(xStart: start, xEnd: end, yStart: bottom, yEnd: top)
            .foregroundStyle(colors.rain)
            .cornerRadius(1.5)
    }

    /// The intensity as a bar a step (a minute, or a quarter hour), over the
    /// light, moderate and heavy lines.
    private func graph(_ nowcast: ViewModel.Nowcast) -> some View {
        let steps = Array(nowcast.intensity.enumerated())
        return Chart {
            ForEach(Array(nowcast.levels.enumerated()), id: \.offset) { index, _ in
                RuleMark(y: .value("Level", Double(index + 1) / 3))
                    // The text's color (a chart paints the `.primary` style in the tint).
                    .foregroundStyle(Color.primary.opacity(0.6))
                    .lineStyle(StrokeStyle(lineWidth: 1, dash: [4, 3]))
            }
            ForEach(steps, id: \.offset) { step, value in
                bar(step, Double(value))
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
                                .font(.system(size: levelSize, weight: .bold))
                                .tracking(0.4)
                                .foregroundStyle(.primary.opacity(0.85))
                                .frame(width: 160, alignment: .leading)
                                .position(x: frame.minX + 84, y: frame.minY + y - levelSize * 0.85)
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
            // `-nowcastSample` shows the nowcast's graph it fills.
            if ProcessInfo.processInfo.arguments.contains("-nowcastSample"), let nowcast {
                return Int(nowcast.hours)
            }
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
