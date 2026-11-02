import SwiftUI
import WhetherKit

/// The next hours' rain and snow: a bar by intensity, the chance and the amount.
struct PrecipitationSection: View {
    @Environment(\.colorTheme) private var colors
    @Environment(\.showsPatterns) private var patterns
    @Environment(\.colorScheme) private var scheme
    let precipitation: ViewModel.Precipitation
    @Binding var hours: Int
    /// Its own title and hours picker (not inside `RainSection`).
    var header = true
    /// A dry table shows its sentence; the hours open on request.
    @State private var showDryHours = false

    static let choices = WeatherStore.precipHourChoices

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if header {
                HStack {
                    SectionTitle(text: "Precipitation")
                    Spacer()
                    Picker("Hours", selection: $hours) {
                        ForEach(Self.choices, id: \.self) { hours in
                            Text("\(hours) h").tag(hours)
                        }
                    }
                    .pickerStyle(.segmented)
                    .frame(maxWidth: 210)
                }
            }
            let precipitation = shown
            if let dry = precipitation.dry, !showDryHours {
                HStack {
                    Text(dry).font(.callout)
                    Spacer()
                    Button("Show hours") { withAnimation(.snappy) { showDryHours = true } }
                        .font(.caption)
                }
            } else {
                VStack(spacing: 4) {
                    ForEach(precipitation.hours.indices, id: \.self) { i in
                        let hours = precipitation.hours
                        // Snow's sign begins each run of frozen hours.
                        row(hours[i], startsSnow: hours[i].frozen && (i == 0 || !hours[i - 1].frozen))
                    }
                }
                Text(precipitation.dry ?? footer)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
    }

    /// The hours shown: the forecast's (screenshots: `-precipSample`, a made-up spell
    /// of rain turning to snow over the real hours, for the launch only).
    private var shown: ViewModel.Precipitation {
        var out = precipitation
        #if DEBUG
            if ProcessInfo.processInfo.arguments.contains("-precipSample") {
                let sample: [(Float, Bool)] = [
                    (0.2, false), (0.45, false), (0.7, false), (0.55, true), (0.4, true), (0.3, true),
                    (0.15, false), (0.1, false), (0.35, true), (0.25, true), (0.1, true), (0.05, true),
                ]
                for i in out.hours.indices where i < sample.count {
                    (out.hours[i].intensity, out.hours[i].frozen) = sample[i]
                }
                out.dry = nil
            }
        #endif
        return out
    }

    private var footer: String {
        let precipitation = shown
        var text = "Total \(precipitation.total.text)"
        if let chance = precipitation.maxChance {
            text += " · highest chance \(chance)"
        }
        return text
    }

    private func row(_ hour: ViewModel.PrecipHour, startsSnow: Bool) -> some View {
        let role: ColorRole = hour.frozen ? .precipSnow : .precipRain
        return HStack(spacing: 10) {
            Text(hour.label)
                .font(.caption)
                .foregroundStyle(.secondary)
                .frame(width: 50, alignment: .leading)
            if patterns {
                // Snow's symbol where a run of frozen hours begins.
                Image(systemName: "snowflake")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .frame(width: 12)
                    .opacity(startsSnow ? 1 : 0)
                    .accessibilityHidden(true)
            }
            GeometryReader { geo in
                let fill = CGFloat(min(max(hour.intensity, 0), 1))
                ZStack(alignment: .leading) {
                    Capsule().fill(.quaternary)
                    if fill > 0 {
                        Capsule()
                            .fill(colors.color(role))
                            .overlay {
                                // Snow, sleet and freezing rain are hatched: not by color alone.
                                if patterns && hour.frozen {
                                    Hatch()
                                        .stroke(colors.hatch(on: role, dark: scheme == .dark), lineWidth: 1.2)
                                        .clipShape(.capsule)
                                }
                            }
                            .frame(width: max(geo.size.height, geo.size.width * fill))
                    }
                }
            }
            .frame(height: 8)
            Text(hour.chance ?? "")
                .font(.caption)
                .foregroundStyle(colors.rain)
                .frame(width: 40, alignment: .trailing)
            Text(hour.amount.text)
                .font(.caption)
                .frame(width: 62, alignment: .trailing)
        }
        .monospacedDigit()
        .frame(height: 18)
        .accessibilityElement(children: .combine)
    }
}
