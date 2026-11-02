import SwiftUI
import UniformTypeIdentifiers
import WhetherKit

extension UTType {
    /// A custom color theme sent between people: JSON in a `.whethertheme` file.
    /// Declared in `Whether-Info.plist` (exported, and openable by the app).
    nonisolated static let whetherTheme = UTType(exportedAs: "dev.savery.whether.theme")
}

/// A theme as the file that is shared: its name and JSON, plain values, so the system
/// can ask for the file from any thread (as `PlanFile`).
nonisolated struct ThemeFile: Transferable, Sendable {
    /// "My Aurora.whethertheme".
    let fileName: String
    /// The `SharedTheme` JSON.
    let json: Data

    static var transferRepresentation: some TransferRepresentation {
        FileRepresentation(exportedContentType: .whetherTheme) { file in
            // In a folder of its own, so two themes with one name don't meet.
            let folder = URL.temporaryDirectory.appending(
                path: UUID().uuidString, directoryHint: .isDirectory)
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
            let url = folder.appending(path: file.fileName, directoryHint: .notDirectory)
            try file.json.write(to: url, options: .atomic)
            return SentTransferredFile(url)
        }
    }
}

extension SharedTheme {
    /// The file to send; `nil` if it can't be written as JSON.
    var file: ThemeFile? {
        guard let json = try? data() else { return nil }
        return ThemeFile(fileName: fileName, json: json)
    }

    #if DEBUG
        /// For screenshots (`-receiveTheme sample`): someone's Aurora with warmer skies.
        static var sample: SharedTheme {
            SharedTheme(
                name: "Lake at dusk", base: "aurora",
                colors: [
                    .skyClearDayTop: ThemeColor(light: RGBA(rgb: 0x2F4A7A)),
                    .skyClearDayBottom: ThemeColor(light: RGBA(rgb: 0x8A4F6E)),
                    .skyClearNightTop: ThemeColor(light: RGBA(rgb: 0x10162E)),
                    .skyClearNightBottom: ThemeColor(light: RGBA(rgb: 0x3B2850)),
                    .accent: ThemeColor(light: RGBA(rgb: 0xE07A5F)),
                ])
        }
    #endif
}

/// Share a custom theme: the system's share sheet with a `.whethertheme` file.
struct ThemeShareButton: View {
    let theme: ColorTheme
    /// The button's text (in a menu); `nil` is the system's share icon.
    var title: String?

    var body: some View {
        let shared = SharedTheme(theme)
        if let file = shared.file {
            if let title {
                ShareLink(item: file, preview: SharePreview(shared.name)) {
                    Label(title, systemImage: "square.and.arrow.up")
                }
            } else {
                ShareLink(item: file, preview: SharePreview(shared.name))
            }
        }
    }
}

/// Colors someone sent: a sample in them, the preset they were made from, and Add or
/// Decline. When the same colors are already here, it says so and offers to use them.
struct ReceivedThemeSheet: View {
    @Bindable var store: WeatherStore
    @Environment(\.colorTheme) private var colors
    let shared: SharedTheme

    private var month: Int { Calendar.current.component(.month, from: .now) }

    var body: some View {
        let month = month
        let base = shared.basePreset(month: month)
        let same = shared.match(among: store.preferences.customThemes, month: month)
        // As it will paint here: completed from its preset, with this reader's color vision.
        let theme = shared.theme(among: [], month: month, id: "")
            .completed(from: base)
            .applying(store.preferences.colorVision)
        let current = store.view?.current
        VStack(alignment: .leading, spacing: 16) {
            Text(SharedTheme.headline).font(.title2.weight(.semibold))
            ThemeSample(
                theme: theme, sky: store.view?.theme ?? "clear_day", icon: current?.icon ?? "clear_day",
                temperature: current?.temperature.text)
            Label(
                shared.colors.isEmpty ? "\(base.name)'s colors" : "Made from \(base.name)",
                systemImage: "paintpalette"
            )
            .font(.callout)
            if let same {
                Text("These colors are already in your list as \(same.name).")
                    .font(.callout).foregroundStyle(.secondary)
            } else {
                Text("Add them to Your colors and paint with them. You can change them in Settings › Display › Colors.")
                    .font(.callout).foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
            VStack(spacing: 10) {
                Button {
                    withAnimation(.easeInOut(duration: 0.4)) { store.receive(shared) }
                } label: {
                    Text(same.map { "Use \($0.name)" } ?? "Add").frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent).controlSize(.large)
                Button("Decline", role: .cancel) { store.incomingTheme = nil }
                    .controlSize(.large)
            }
        }
        .padding(20)
        .tint(colors.color(.accent))
        .presentationDetents([.medium, .large])
    }
}
