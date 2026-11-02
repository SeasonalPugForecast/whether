import SwiftUI
import WhetherKit

/// The place, one big number, the condition, and what happens next.
struct NowSection: View {
    let view: ViewModel
    let current: ViewModel.Current
    @Environment(\.feelsLikeLeads) private var feelsLeads

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(alignment: .center, spacing: 18) {
                ConditionIcon(icon: current.icon, size: 54, animated: true)
                VStack(alignment: .leading, spacing: 0) {
                    if feelsLeads { FeelsLikeTag() }
                    Text(current.temperature.text)
                        .font(.system(size: 76, weight: .thin))
                        .monospacedDigit()
                    Text(current.conditionLabel).font(.title3)
                }
            }
            Text(otherLine(view, current, feelsLeads: feelsLeads))
                .font(.subheadline)
                .foregroundStyle(.secondary)
            if let summary = view.summary {
                Text(summary).font(.body)
            }
        }
    }
}

/// The centered layout's top: the icon, one big number, the condition, the
/// other temperature with today's high and low, and the summary, all centered.
struct CenteredNowSection: View {
    let view: ViewModel
    let current: ViewModel.Current
    @Environment(\.feelsLikeLeads) private var feelsLeads

    var body: some View {
        VStack(spacing: 4) {
            ConditionIcon(icon: current.icon, size: 46, animated: true)
            if feelsLeads { FeelsLikeTag() }
            Text(current.temperature.text)
                .font(.system(size: 96, weight: .thin))
                .monospacedDigit()
                .padding(.leading, 18)  // the degree sign hangs right
            Text(current.conditionLabel).font(.title3.weight(.medium))
            Text(otherLine(view, current, feelsLeads: feelsLeads))
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
            if let summary = view.summary {
                Text(summary)
                    .font(.callout)
                    .multilineTextAlignment(.center)
                    .padding(.top, 8)
                    .padding(.horizontal, 12)
            }
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 18)
    }
}

/// "FEELS LIKE" over a feels-like big number.
struct FeelsLikeTag: View {
    var body: some View {
        Text("FEELS LIKE")
            .font(.caption2.weight(.semibold))
            .tracking(0.8)
            .foregroundStyle(.secondary)
            .accessibilityLabel("Feels like")
    }
}

/// Under the big number: the other temperature and the humidity now, then
/// today's high and low, said to be feels-like when they are.
func otherLine(_ view: ViewModel, _ current: ViewModel.Current, feelsLeads: Bool) -> String {
    // The humidity as the core wrote it: the detail card's, else this hour's.
    let humidity =
        view.details.first { $0.kind == "humidity" }?.value ?? view.hourly?.hours.first?.humidityText
    let now = [current.feelsLikeLabel, humidity.map { "Humidity \($0)" }].compactMap { $0 }
        .joined(separator: " · ")
    return "\(now)\n\(feelsLeads ? "Feels-like " : "")H \(current.high.text)  L \(current.low.text)"
}
