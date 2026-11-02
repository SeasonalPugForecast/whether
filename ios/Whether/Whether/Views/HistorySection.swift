import SwiftUI
import WhetherKit

/// History, one day: today's normals, records and chance of rain
/// or snow, today against yesterday, and yesterday in full (its high, low and
/// rain against normal, and its date's records). The Plan tab looks further
/// back.
struct HistorySection: View {
    @Environment(\.colorTheme) private var colors
    let history: ViewModel.History

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            SectionTitle(text: "History")
            if let today = history.today {
                VStack(alignment: .leading, spacing: 3) {
                    Text(today.normalText).font(.headline)
                    if let high = today.recordHigh, let low = today.recordLow {
                        Label("Records \(high) · \(low)", systemImage: "trophy")
                            .font(.callout)
                    }
                    if let chance = today.precipChance {
                        Label(chance, systemImage: "umbrella").font(.callout).foregroundStyle(colors.rain)
                    }
                }
            }
            if let delta = history.yesterdayDelta {
                Text(delta).font(.callout).foregroundStyle(.secondary)
            }
            if let yesterday = history.days.last, yesterday.summary != nil {
                Divider()
                row(yesterday)
                details(yesterday)
            }
            Text(history.credit).font(.caption2).foregroundStyle(.secondary)
        }
    }

    /// One observed day: its high and low, the high against normal in a warm or
    /// cold pill, and its precipitation.
    private func row(_ day: ViewModel.PastDay) -> some View {
        HStack(spacing: 10) {
            VStack(alignment: .leading, spacing: 0) {
                Text(day.label)
                Text(day.dateShort).font(.caption2).foregroundStyle(.secondary)
            }
            .frame(width: 78, alignment: .leading)
            Text(day.high.text).frame(width: 40, alignment: .trailing)
            Text(day.low.text).foregroundStyle(.secondary).frame(width: 40, alignment: .trailing)
            departure(day.departure).frame(width: 52)
            Spacer(minLength: 0)
            Text(day.precipitation.text)
                .font(.caption)
                .foregroundStyle(colors.rain)
                .lineLimit(1)
                .minimumScaleFactor(0.8)
        }
        .monospacedDigit()
        .padding(.vertical, 7)
        .contentShape(.rect)
        .accessibilityElement(children: .combine)
    }

    /// An opened day: what it was like, then its high, low and rain each
    /// against normal, any snow, and the date's records.
    private func details(_ day: ViewModel.PastDay) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            if let summary = day.summary {
                Text(summary).font(.callout)
            }
            Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 4) {
                fact("High", day.high.text, normal: day.normalHigh, departure: day.departure)
                fact("Low", day.low.text, normal: day.normalLow, departure: day.lowDeparture)
                fact("Rain", day.precipitation.text, normal: day.normalPrecip, departure: nil)
                if let snow = day.snow {
                    GridRow {
                        Text("Snow").foregroundStyle(.secondary)
                        Text(snow.text)
                    }
                }
                if day.recordHigh != nil || day.recordLow != nil {
                    GridRow {
                        Text("Records").foregroundStyle(.secondary)
                        Text([day.recordHigh, day.recordLow].compactMap { $0 }.joined(separator: " · "))
                            .gridCellColumns(3)
                    }
                }
            }
            .font(.caption)
            .monospacedDigit()
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.bottom, 10)
        .transition(.opacity.combined(with: .move(edge: .top)))
    }

    /// One measure of an opened day: its value, the normal and the difference.
    @ViewBuilder private func fact(
        _ title: String, _ value: String, normal: ViewModel.Quantity?, departure: ViewModel.Quantity?
    ) -> some View {
        GridRow {
            Text(title).foregroundStyle(.secondary)
            Text(value)
            Text(normal?.value == nil ? "" : "normal \(normal!.text)").foregroundStyle(.secondary)
            Text(departure?.value == nil ? "" : departure!.text)
        }
    }

    /// "+5°" on red, "-3°" on blue: colored text alone fades into the skies.
    @ViewBuilder private func departure(_ d: ViewModel.Quantity) -> some View {
        let value = d.value ?? 0
        Text(d.text)
            .font(.caption.weight(.semibold))
            .padding(.horizontal, 7)
            .padding(.vertical, 2)
            .background(
                value > 0
                    ? colors.color(.tempAboveNormal).opacity(0.75)
                    : value < 0 ? colors.color(.tempBelowNormal).opacity(0.8) : .clear,
                in: .capsule)
            .accessibilityLabel(
                d.value == nil ? "" : "\(d.text) against normal")
    }
}
