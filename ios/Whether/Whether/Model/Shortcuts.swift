import AppIntents
import WhetherKit

/// "Will it rain?" for Siri and Shortcuts: the next hours and today's outlook at
/// the place shown, from the cached forecast (or a fresh one).
struct WillItRainIntent: AppIntent {
    static let title: LocalizedStringResource = "Will it rain?"
    static let description = IntentDescription(
        "The next hours' rain and today's outlook at the place shown in Seasonal Pug Forecast.")

    @MainActor
    func perform() async throws -> some IntentResult & ProvidesDialog {
        guard let view = await WidgetData.entry().view else {
            return .result(dialog: "Open Pug Forecast once to load the forecast.")
        }
        let said = [view.nowcast?.text, view.summary].compactMap { $0 }.joined(separator: " ")
        return .result(dialog: IntentDialog(stringLiteral: "\(view.location.name): \(said)"))
    }
}

struct WhetherShortcuts: AppShortcutsProvider {
    static var appShortcuts: [AppShortcut] {
        AppShortcut(
            intent: WillItRainIntent(),
            phrases: [
                "Will it rain in \(.applicationName)",
                "Ask \(.applicationName) about rain",
                "\(.applicationName) forecast",
            ],
            shortTitle: "Will it rain?",
            systemImageName: "cloud.rain")
    }
}
