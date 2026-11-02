import Foundation

/// A tile a `JiggleGrid` shows or hides.
protocol GridChoice: Identifiable, Hashable where ID: Hashable & CustomStringConvertible {
    var shown: Bool { get set }
}

extension Array {
    /// What SwiftUI's `move(fromOffsets:toOffset:)` does, without SwiftUI (so the
    /// order rules build and test outside the app): the elements at `source`
    /// go before the element at `destination` (an index in the array as it is now).
    mutating func relocate(fromOffsets source: IndexSet, toOffset destination: Int) {
        let moving = source.map { self[$0] }
        let before = source.filter { $0 < destination }.count
        for i in source.reversed() { remove(at: i) }
        insert(contentsOf: moving, at: destination - before)
    }
}

/// The order rules of an edited grid (the views only animate them).
enum GridOrder {
    /// Shows or hides `choice`: added back after the shown ones, hidden ones
    /// at the end.
    static func set<Choice: GridChoice>(_ choices: inout [Choice], _ choice: Choice, shown on: Bool) {
        guard let i = choices.firstIndex(where: { $0.id == choice.id }) else { return }
        var c = choices.remove(at: i)
        c.shown = on
        let last = choices.lastIndex(where: \.shown).map { $0 + 1 } ?? 0
        choices.insert(c, at: on ? last : choices.count)
    }

    /// Moves the dragged tile to where `target` is, as it passes over it.
    static func move<Choice: GridChoice>(
        _ choices: inout [Choice], dragging: Choice.ID, over target: Choice.ID
    ) {
        guard dragging != target,
            let from = choices.firstIndex(where: { $0.id == dragging }),
            let to = choices.firstIndex(where: { $0.id == target })
        else { return }
        choices.relocate(fromOffsets: IndexSet(integer: from), toOffset: to > from ? to + 1 : to)
    }
}

extension Preferences {
    /// `choices` plus the core's `kinds` it lacks, added hidden (offered in the Add row).
    static func addingMissing(_ choices: [DetailChoice], kinds: [String]) -> [DetailChoice] {
        let missing = kinds.filter { k in !choices.contains { $0.kind == k } }
        return choices + missing.map { DetailChoice(kind: $0, shown: false) }
    }
}

extension Preferences.DetailChoice {
    /// The card's SF Symbol, by its kind.
    var symbol: String {
        switch kind {
        case "wind": "wind"
        case "gusts": "tornado"
        case "humidity": "humidity"
        case "dew_point": "drop"
        case "pressure": "gauge.with.dots.needle.33percent"
        case "uv": "sun.max"
        case "visibility": "eye"
        case "cloud_cover": "cloud"
        case "precipitation": "umbrella"
        case "feels_like": "thermometer.medium"
        case "normals": "chart.line.uptrend.xyaxis"
        case "records": "trophy"
        default: "circle"
        }
    }

    /// A name for the card when the forecast is not at hand to give the core's
    /// own title: the kind with its underscores as spaces ("dew_point" is
    /// "Dew point").
    var fallbackTitle: String {
        let words = kind.replacingOccurrences(of: "_", with: " ")
        return words.prefix(1).uppercased() + words.dropFirst()
    }
}
