import AppIntents
import WidgetKit

/// A saved place (or "The app's") as the widget's place picker offers it.
struct WidgetPlaceEntity: AppEntity {
    static let typeDisplayRepresentation: TypeDisplayRepresentation = "Place"
    static let defaultQuery = WidgetPlaceQuery()

    /// A saved place's id, or `WidgetPlaceOption.appsID`.
    var id: String
    var name: String
    var detail: String?

    var displayRepresentation: DisplayRepresentation {
        DisplayRepresentation(title: "\(name)", subtitle: detail.map { "\($0)" })
    }

    init(_ option: WidgetPlaceOption) {
        id = option.id
        name = option.name
        detail = option.detail
    }

    /// "The app's": the favourite place, else the one the app shows (the default).
    static let apps = WidgetPlaceEntity(.apps)
}

/// Lists "The app's", then the saved places in `places.json` (App Group).
struct WidgetPlaceQuery: EnumerableEntityQuery {
    func allEntities() async throws -> [WidgetPlaceEntity] {
        WidgetPlaceOption.options(places: WidgetData.places()).map(WidgetPlaceEntity.init)
    }

    func entities(for identifiers: [String]) async throws -> [WidgetPlaceEntity] {
        WidgetPlaceOption.options(for: identifiers, places: WidgetData.places())
            .map(WidgetPlaceEntity.init)
    }

    func defaultResult() async -> WidgetPlaceEntity? { .apps }
}

/// `WidgetContent` for Edit Widget.
enum WidgetContentOption: String, AppEnum {
    case overview, nextHour, nextHours, today

    static let typeDisplayRepresentation: TypeDisplayRepresentation = "Show"
    static let caseDisplayRepresentations: [WidgetContentOption: DisplayRepresentation] = [
        .overview: DisplayRepresentation(
            title: "Overview", subtitle: "Now, the next hours and the days"),
        .nextHour: DisplayRepresentation(title: "The next hour", subtitle: "Rain minute by minute"),
        .nextHours: DisplayRepresentation(
            title: "The next hours", subtitle: "More hours, with their chance of rain"),
        .today: DisplayRepresentation(
            title: "Today's high and low", subtitle: "With rain, wind and sun"),
    ]

    var content: WidgetContent {
        switch self {
        case .overview: .overview
        case .nextHour: .nextHour
        case .nextHours: .nextHours
        case .today: .today
        }
    }
}

/// Edit Widget: the place, and (medium and large) what to show. The defaults
/// reproduce the widget before it was configurable.
struct WeatherWidgetIntent: WidgetConfigurationIntent {
    static let title: LocalizedStringResource = "Weather"
    static let description = IntentDescription(
        "Choose the place, and what the medium and large widgets show.")

    @Parameter(title: "Place", default: WidgetPlaceEntity.apps)
    var place: WidgetPlaceEntity?

    @Parameter(title: "Show", default: .overview)
    var show: WidgetContentOption

    static var parameterSummary: some ParameterSummary {
        When(widgetFamily: .oneOf, [.systemMedium, .systemLarge]) {
            Summary("\(\.$place), showing \(\.$show)")
        } otherwise: {
            Summary("\(\.$place)")
        }
    }
}
