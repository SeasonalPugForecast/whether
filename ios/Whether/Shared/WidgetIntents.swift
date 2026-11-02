import AppIntents
import WidgetKit

/// The widget's button: shows the next saved place there (the app keeps its own).
struct NextPlaceIntent: AppIntent {
    static let title: LocalizedStringResource = "Next place"
    static let description = IntentDescription("Shows the next saved place in the Seasonal Pug Forecast widget.")

    @MainActor
    func perform() async throws -> some IntentResult {
        let defaults = SharedContainer.defaults
        let places = WidgetData.places()
        let current = WidgetData.chosenIndex(
            places: places, defaults: defaults, favouriteID: Preferences.load().favouritePlaceID)
        if let next = WidgetData.nextID(places: places, after: current) {
            defaults.set(next, forKey: WidgetData.placeKey)
        }
        return .result()
    }
}
