import Foundation

// MARK: What a day row shows

/// One thing a day row can show: it comes from `ViewModel.Day`. The
/// chance of rain and the low–high bar sit on the day's own line; the rest
/// follow underneath, in the list's order.
enum DayRowItem: String, ListedItem {
    case precipChance, tempBar, outlook, airTemps, humidity, wind, uv

    var title: String {
        switch self {
        case .precipChance: "Chance of rain"
        case .tempBar: "The low–high bar"
        case .outlook: "The forecaster's line"
        case .airTemps: "Air temperature"
        case .humidity: "Humidity"
        case .wind: "Wind"
        case .uv: "UV index"
        }
    }

    var symbol: String {
        switch self {
        case .precipChance: "cloud.rain"
        case .tempBar: "thermometer.medium"
        case .outlook: "text.quote"
        case .airTemps: "thermometer.variable"
        case .humidity: "humidity"
        case .wind: "wind"
        case .uv: "sun.max"
        }
    }

    /// The rows are as they were before the choice existed.
    nonisolated var shownByDefault: Bool {
        switch self {
        case .precipChance, .tempBar, .outlook: true
        case .airTemps, .humidity, .wind, .uv: false
        }
    }
}

typealias DayRowChoice = ItemChoice<DayRowItem>

/// A small fact under a day's line.
enum DayFact: Hashable {
    case air, humidity, wind, uv
}

/// A line under a day's own line.
enum DayRowLine: Equatable {
    /// The day in a forecaster's words.
    case outlook
    /// Neighbouring facts share one line.
    case facts([DayFact])
}

/// What a day row draws.
struct DayRowLayout: Equatable {
    var chance: Bool
    var bar: Bool
    var lines: [DayRowLine]

    /// The row for the shown items. The chance and the bar keep their places on
    /// the day's own line; the others stack underneath in order, and facts next
    /// to each other (air, humidity, wind, UV) share a line.
    static func make(_ choices: [DayRowChoice]) -> DayRowLayout {
        var layout = DayRowLayout(chance: false, bar: false, lines: [])
        for item in choices.filter(\.shown).map(\.item) {
            let fact: DayFact
            switch item {
            case .precipChance:
                layout.chance = true
                continue
            case .tempBar:
                layout.bar = true
                continue
            case .outlook:
                layout.lines.append(.outlook)
                continue
            case .airTemps: fact = .air
            case .humidity: fact = .humidity
            case .wind: fact = .wind
            case .uv: fact = .uv
            }
            if case .facts(let facts) = layout.lines.last {
                layout.lines[layout.lines.count - 1] = .facts(facts + [fact])
            } else {
                layout.lines.append(.facts([fact]))
            }
        }
        return layout
    }
}

enum DayFacts {
    /// The facts' line: each as the core wrote it with a label, in order, `nil`
    /// ones (a day without UV) left out, and `nil` when none is left.
    static func line(
        _ facts: [DayFact], air: String?, humidity: String?, wind: String?, uv: String?
    ) -> String? {
        let parts = facts.compactMap { fact -> String? in
            switch fact {
            case .air: air.map { "Air \($0)" }
            case .humidity: humidity.map { "Humidity \($0)" }
            case .wind: wind.map { "Wind \($0)" }
            case .uv: uv.map { "UV \($0)" }
            }
        }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }
}

// MARK: What an opened day shows

/// One thing an opened day can show. The stretches and the hours
/// share one strip, drawn where the first of them is.
enum DayDetailItem: String, ListedItem {
    case stretches, hours, totals, sun, airTemps, normals

    var title: String {
        switch self {
        case .stretches: "Stretches of weather"
        case .hours: "The hours"
        case .totals: "Precipitation, wind and humidity"
        case .sun: "Sunrise and sunset"
        case .airTemps: "Air high and low"
        case .normals: "Normal high and low"
        }
    }

    var symbol: String {
        switch self {
        case .stretches: "rectangle.split.3x1"
        case .hours: "clock"
        case .totals: "drop"
        case .sun: "sunrise"
        case .airTemps: "thermometer.variable"
        case .normals: "chart.xyaxis.line"
        }
    }

    nonisolated var shownByDefault: Bool { true }
}

typealias DayDetailChoice = ItemChoice<DayDetailItem>

/// A caption line of an opened day.
enum DayDetailLine: Hashable {
    case totals, sun, airTemps, normals
}

enum DayDetailBlock: Equatable {
    /// The strip: the coloured stretches (with the one tapped), the hours under
    /// them (with the one slid to), or both.
    case timeline(stretches: Bool, hours: Bool)
    /// Caption lines next to each other.
    case lines([DayDetailLine])
}

enum DayDetailLayout {
    /// The opened day's blocks for the shown items, in order. The strip sits
    /// where the first of its two items is; caption lines next to each other
    /// share a block.
    static func blocks(_ choices: [DayDetailChoice]) -> [DayDetailBlock] {
        var out: [DayDetailBlock] = []
        var strip: Int?
        for item in choices.filter(\.shown).map(\.item) {
            let line: DayDetailLine
            switch item {
            case .stretches, .hours:
                let stretches = item == .stretches
                if let at = strip, case .timeline(let s, let h) = out[at] {
                    out[at] = .timeline(stretches: s || stretches, hours: h || !stretches)
                } else {
                    strip = out.count
                    out.append(.timeline(stretches: stretches, hours: !stretches))
                }
                continue
            case .totals: line = .totals
            case .sun: line = .sun
            case .airTemps: line = .airTemps
            case .normals: line = .normals
            }
            if case .lines(let lines) = out.last {
                out[out.count - 1] = .lines(lines + [line])
            } else {
                out.append(.lines([line]))
            }
        }
        return out
    }
}
