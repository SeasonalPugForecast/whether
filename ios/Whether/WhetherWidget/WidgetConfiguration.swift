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

/// `WidgetBackdrop` for Edit Widget.
enum WidgetBackdropOption: String, AppEnum {
    case sky, plain

    static let typeDisplayRepresentation: TypeDisplayRepresentation = "Background"
    static let caseDisplayRepresentations: [WidgetBackdropOption: DisplayRepresentation] = [
        .sky: DisplayRepresentation(title: "Sky", subtitle: "The weather's colors, as in the app"),
        .plain: DisplayRepresentation(title: "Plain", subtitle: "Light or dark, as the phone is"),
    ]

    var backdrop: WidgetBackdrop {
        switch self {
        case .sky: .sky
        case .plain: .plain
        }
    }
}

/// `WidgetTemperature` for Edit Widget.
enum WidgetTemperatureOption: String, AppEnum {
    case app, actual, feelsLike

    static let typeDisplayRepresentation: TypeDisplayRepresentation = "Temperature"
    static let caseDisplayRepresentations: [WidgetTemperatureOption: DisplayRepresentation] = [
        .app: DisplayRepresentation(title: "The app's", subtitle: "As Settings choose"),
        .actual: DisplayRepresentation(title: "Actual"),
        .feelsLike: DisplayRepresentation(title: "Feels like"),
    ]

    var temperature: WidgetTemperature {
        switch self {
        case .app: .app
        case .actual: .actual
        case .feelsLike: .feelsLike
        }
    }
}

/// Edit Widget: the place, (medium and large) what to show, the background and
/// the temperature. A widget placed before a choice existed gets its default:
/// the app's place, the overview, the sky and the app's temperature.
struct WeatherWidgetIntent: WidgetConfigurationIntent {
    static let title: LocalizedStringResource = "Weather"
    static let description = IntentDescription(
        "Choose the place, what the medium and large widgets show, the background and the temperature.")

    @Parameter(title: "Place", default: WidgetPlaceEntity.apps)
    var place: WidgetPlaceEntity?

    @Parameter(title: "Show", default: .overview)
    var show: WidgetContentOption

    @Parameter(title: "Background", default: .sky)
    var background: WidgetBackdropOption

    @Parameter(title: "Temperature", default: .app)
    var temperature: WidgetTemperatureOption

    static var parameterSummary: some ParameterSummary {
        When(widgetFamily: .oneOf, [.systemMedium, .systemLarge]) {
            Summary("\(\.$place), showing \(\.$show)") {
                \.$background
                \.$temperature
            }
        } otherwise: {
            // Lock Screen widgets have no background of their own.
            When(widgetFamily: .equalTo, .systemSmall) {
                Summary("\(\.$place)") {
                    \.$background
                    \.$temperature
                }
            } otherwise: {
                Summary("\(\.$place)") {
                    \.$temperature
                }
            }
        }
    }

    /// The choices, as the widget's data reads them.
    var setup: WidgetSetup {
        WidgetSetup(
            placeID: place?.id, content: show.content, backdrop: background.backdrop,
            temperature: temperature.temperature)
    }
}

