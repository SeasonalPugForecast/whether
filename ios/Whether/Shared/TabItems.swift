import Foundation

// MARK: The app's tabs

/// A tab the person using the app can hide and reorder. The Now tab is not one: it
/// always shows, and first. The raw values are saved, so they never change.
enum AppTabItem: String, ListedItem {
    case hourly, days, radar, plan

    var title: String {
        switch self {
        case .hourly: "Hourly"
        case .days: "Days"
        case .radar: "Radar"
        case .plan: "Plan"
        }
    }

    var symbol: String {
        switch self {
        case .hourly: "clock"
        case .days: "calendar"
        case .radar: "globe.americas.fill"
        case .plan: "suitcase"
        }
    }

    nonisolated var shownByDefault: Bool { true }
}

typealias TabChoice = ItemChoice<AppTabItem>

enum TabLayout {
    /// The tabs after Now, in the order saved, without the hidden.
    static func visible(_ choices: [TabChoice]) -> [AppTabItem] {
        choices.shownItems()
    }

    /// The tab to show when `current` (nil: Now) is hidden: Now.
    static func keeping(_ current: AppTabItem?, among visible: [AppTabItem]) -> AppTabItem? {
        current.flatMap { visible.contains($0) ? $0 : nil }
    }
}

// MARK: The cards of the Hourly and Days tabs

/// A card of the Hourly tab, top to bottom in the order saved.
enum HourlyCard: String, ListedItem {
    case graph, hour, sun

    var title: String {
        switch self {
        case .graph: "Hour by hour"
        case .hour: "The hour in full"
        case .sun: "Sun & moon"
        }
    }

    var symbol: String {
        switch self {
        case .graph: "chart.xyaxis.line"
        case .hour: "thermometer.medium"
        case .sun: "sun.horizon"
        }
    }

    nonisolated var shownByDefault: Bool { true }
}

/// A card of the Days tab.
enum DaysCard: String, ListedItem {
    case days, history

    var title: String {
        switch self {
        case .days: "Next days"
        case .history: "History"
        }
    }

    var symbol: String {
        switch self {
        case .days: "calendar"
        case .history: "clock.arrow.circlepath"
        }
    }

    nonisolated var shownByDefault: Bool { true }
}

typealias HourlyCardChoice = ItemChoice<HourlyCard>
typealias DaysCardChoice = ItemChoice<DaysCard>

extension Array {
    /// The shown choices' items, in order.
    func shownItems<I: ListedItem>() -> [I] where Element == ItemChoice<I> {
        filter(\.shown).map(\.item)
    }
}
