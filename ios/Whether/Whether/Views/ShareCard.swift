import SwiftUI
import UniformTypeIdentifiers
import WhetherKit

/// The forecast as an image to send: the place, now, the summary and the next
/// five days on its sky.
struct ShareCard: View {
    let view: ViewModel

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .top) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(view.location.name).font(.title2.bold())
                    Text(view.location.localTime).font(.subheadline).foregroundStyle(.secondary)
                }
                Spacer()
                if let current = view.current {
                    ConditionIcon(icon: current.icon, size: 44)
                }
            }
            if let current = view.current {
                Text(current.temperature.text).font(.system(size: 76, weight: .thin))
                Text("\(current.conditionLabel) · H \(current.high.text)  L \(current.low.text)")
                    .font(.headline)
            }
            if let summary = view.summary {
                Text(summary).font(.callout)
            }
            if let days = view.daily?.days.prefix(5), !days.isEmpty {
                VStack(spacing: 8) {
                    ForEach(Array(days), id: \.dateShort) { day in
                        HStack(spacing: 10) {
                            Text(day.label).frame(width: 96, alignment: .leading)
                            ConditionIcon(icon: day.icon, size: 18).frame(width: 26)
                            Spacer()
                            Text(day.low.text).foregroundStyle(.secondary)
                            Text(day.high.text).frame(width: 40, alignment: .trailing)
                        }
                        .monospacedDigit()
                    }
                }
                .padding(14)
                .background(.white.opacity(0.12), in: .rect(cornerRadius: 16))
            }
            Text(view.attribution).font(.caption2).foregroundStyle(.secondary)
        }
        .padding(24)
        .frame(width: 380)
        .foregroundStyle(.white)
        .background(SkyBackground(theme: view.theme, animated: false))
        .environment(\.colorScheme, .dark)
    }
}

/// The share card as a PNG, rendered when it is shared.
struct ForecastImage: Transferable {
    let view: ViewModel
    /// The app's color theme: an image renderer doesn't inherit the environment.
    let colors: ColorTheme

    static var transferRepresentation: some TransferRepresentation {
        DataRepresentation(exportedContentType: .png) { item in
            try await MainActor.run { try item.png() }
        }
    }

    @MainActor func png() throws -> Data {
        let renderer = ImageRenderer(content: ShareCard(view: view).environment(\.colorTheme, colors))
        renderer.scale = 3
        guard let data = renderer.uiImage?.pngData() else { throw CocoaError(.fileWriteUnknown) }
        return data
    }
}
