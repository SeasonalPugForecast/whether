import AppIntents

// Edit Widget's choices. App Intents saves each widget's choice by its raw
// value, so these never change (WidgetTimelineTests).

/// `WidgetContent` for Edit Widget.
enum WidgetContentOption: String, AppEnum, CaseIterable {
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
enum WidgetBackdropOption: String, AppEnum, CaseIterable {
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
enum WidgetTemperatureOption: String, AppEnum, CaseIterable {
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
