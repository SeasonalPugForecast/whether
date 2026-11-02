import UIKit

/// The season's pug on the home screen (meteorological seasons, northern
/// hemisphere): spring is the primary icon, the others are alternates.
enum Season: String, CaseIterable, Codable {
    case spring, summer, fall, winter

    /// The alternate icon's name; `nil` for the primary ("AppIcon", spring).
    var alternateIconName: String? {
        switch self {
        case .spring: nil
        case .summer: "AppIcon-Summer"
        case .fall: "AppIcon-Fall"
        case .winter: "AppIcon-Winter"
        }
    }

    static func current(date: Date = .now, calendar: Calendar = .current) -> Season {
        switch calendar.component(.month, from: date) {
        case 3...5: .spring
        case 6...8: .summer
        case 9...11: .fall
        default: .winter
        }
    }
}

extension Preferences.AppIconChoice {
    /// The season whose icon shows now.
    var season: Season {
        switch self {
        case .seasonal: .current()
        case .spring: .spring
        case .summer: .summer
        case .fall: .fall
        case .winter: .winter
        }
    }
}

/// Puts `choice`'s icon on the home screen when it is not there already. iOS
/// confirms every change with its own alert, so this runs only when the icon
/// actually changes (a new season, or a new choice).
@MainActor
func applyAppIcon(_ choice: Preferences.AppIconChoice) {
    let app = UIApplication.shared
    let name = choice.season.alternateIconName
    guard app.supportsAlternateIcons, app.alternateIconName != name else { return }
    app.setAlternateIconName(name) { _ in }
}
