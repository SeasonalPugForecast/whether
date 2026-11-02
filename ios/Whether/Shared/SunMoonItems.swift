import Foundation

/// One thing the Sun & moon card can show. The raw values are saved,
/// so they never change.
enum SunItem: String, ListedItem {
    case sunTimes, daylight, lightBar, astronomicalTwilight, nauticalTwilight, civilTwilight
    case goldenHour, solarNoon, moonPhase, moonIllumination, moonTimes

    /// Every item shows until hidden.
    nonisolated var shownByDefault: Bool { true }

    var title: String {
        switch self {
        case .sunTimes: "Sunrise and sunset"
        case .daylight: "Daylight length"
        case .lightBar: "The day's light bar"
        case .astronomicalTwilight: "Astronomical twilight"
        case .nauticalTwilight: "Nautical twilight"
        case .civilTwilight: "Civil twilight"
        case .goldenHour: "Golden hour"
        case .solarNoon: "Solar noon"
        case .moonPhase: "Moon phase"
        case .moonIllumination: "Moon illumination"
        case .moonTimes: "Moonrise and moonset"
        }
    }

    var symbol: String {
        switch self {
        case .sunTimes: "sunrise.fill"
        case .daylight: "sun.max"
        case .lightBar: "chart.bar.fill"
        // The same symbols as on the day's bar and beside its rows.
        case .astronomicalTwilight: SunMoonLayout.symbol(kind: "astronomical")
        case .nauticalTwilight: SunMoonLayout.symbol(kind: "nautical")
        case .civilTwilight: SunMoonLayout.symbol(kind: "civil")
        case .goldenHour: SunMoonLayout.symbol(kind: "golden")
        case .solarNoon: SunMoonLayout.symbol(kind: "noon")
        case .moonPhase: "moonphase.waxing.crescent"
        case .moonIllumination: "circle.lefthalf.filled"
        case .moonTimes: "moonrise.fill"
        }
    }

    /// The core's `SunEvent.kind` this item shows rows for, if it is one of the
    /// dawns and dusks ("astronomical", "nautical", "civil" or "golden").
    var eventKind: String? {
        switch self {
        case .astronomicalTwilight: "astronomical"
        case .nauticalTwilight: "nautical"
        case .civilTwilight: "civil"
        case .goldenHour: "golden"
        default: nil
        }
    }
}

/// One item's place in the card and whether it shows (the shared `ItemChoice`).
typealias SunItemChoice = ItemChoice<SunItem>

/// A dawn, dusk or golden-hour edge of the day, as the core gave it.
struct SunEdge: Equatable {
    var at: Int64
    var morning: Bool
    /// "astronomical", "nautical", "civil" or "golden".
    var kind: String
    var label: String
    var time: String
}

/// One moment of the day, as a row of the times list and a mark on the light bar.
/// `kind` is a twilight's ("astronomical", "nautical", "civil", "golden"), or
/// "sunrise", "sunset", "noon", "moonrise" or "moonset". `label` and `time` are
/// the core's words.
struct SunMark: Equatable {
    var kind: String
    var label: String
    var time: String
    var at: Int64

    init(kind: String, label: String, time: String, at: Int64) {
        self.kind = kind
        self.label = label
        self.time = time
        self.at = at
    }

    init(_ edge: SunEdge) {
        self.init(kind: edge.kind, label: edge.label, time: edge.time, at: edge.at)
    }
}

/// A mark placed on the bar: where along it (`x`, in points from the bar's left
/// edge, the symbol's centre), and in the strip above the bar or the one below.
struct PlacedMark: Equatable {
    var mark: SunMark
    var x: Double
    var above: Bool
}

/// A stretch of the day's light bar. `kind` is the twilight's, or "day" between
/// the golden hours.
struct LightStretch: Equatable {
    var start: Int64
    var end: Int64
    var kind: String
}

/// What the card draws, top to bottom.
enum SunBlock: Equatable {
    /// Sunrise and sunset, and the daylight length, on one line.
    case header(sunTimes: Bool, daylight: Bool)
    case bar
    /// Times in one list, in time order: these kinds of edge, and solar noon.
    case times(kinds: Set<String>, noon: Bool)
    /// The moon's phase and how much is lit, on one line.
    case moon(phase: Bool, illumination: Bool)
    case moonTimes

    /// Whether this is a times list.
    var isTimes: Bool {
        if case .times = self { return true }
        return false
    }
}

enum SunMoonLayout {
    /// The card's blocks for the shown items, in order. Neighbouring items of a
    /// kind (header, times list, moon line) share a block; a different kind of
    /// item between them splits it in two. The times list is in time order,
    /// since dawns and dusks of different twilights interleave.
    static func blocks(_ choices: [SunItemChoice]) -> [SunBlock] {
        var out: [SunBlock] = []
        for item in choices.filter(\.shown).map(\.item) {
            switch item {
            case .sunTimes, .daylight:
                if case .header(var t, var d) = out.last {
                    if item == .sunTimes { t = true } else { d = true }
                    out[out.count - 1] = .header(sunTimes: t, daylight: d)
                } else {
                    out.append(.header(sunTimes: item == .sunTimes, daylight: item == .daylight))
                }
            case .lightBar:
                out.append(.bar)
            case .astronomicalTwilight, .nauticalTwilight, .civilTwilight, .goldenHour, .solarNoon:
                var kinds = Set<String>()
                var noon = false
                if case .times(let k, let n) = out.last { kinds = k; noon = n }
                if let kind = item.eventKind { kinds.insert(kind) } else { noon = true }
                let block = SunBlock.times(kinds: kinds, noon: noon)
                if case .times = out.last { out[out.count - 1] = block } else { out.append(block) }
            case .moonPhase, .moonIllumination:
                if case .moon(var p, var i) = out.last {
                    if item == .moonPhase { p = true } else { i = true }
                    out[out.count - 1] = .moon(phase: p, illumination: i)
                } else {
                    out.append(.moon(phase: item == .moonPhase, illumination: item == .moonIllumination))
                }
            case .moonTimes:
                out.append(.moonTimes)
            }
        }
        return out
    }

    /// The rows of a times list: the edges of `kinds`, solar noon when `showNoon`
    /// and the core gave it, and the `extra` marks (sunrise and sunset, the
    /// moon's), all in time order. Ties keep the edges first.
    static func rows(edges: [SunEdge], kinds: Set<String>, noon: SunMark?, showNoon: Bool, extra: [SunMark] = [])
        -> [SunMark]
    {
        var out = edges.filter { kinds.contains($0.kind) }.map(SunMark.init)
        if showNoon, let noon { out.append(noon) }
        out += extra
        // `sorted` is not stable; the index breaks ties.
        return out.enumerated().sorted { ($0.element.at, $0.offset) < ($1.element.at, $1.offset) }.map(\.element)
    }

    /// The SF Symbol for a kind of moment: on the bar, beside the row, and on the
    /// item's switch. Distinct shapes, so the colors are not the only cue.
    static func symbol(kind: String) -> String {
        switch kind {
        case "astronomical": "sparkles"
        case "nautical": "sailboat.fill"
        case "civil": "sun.haze.fill"
        case "golden": "sun.horizon.fill"
        case "sunrise": "sunrise.fill"
        case "sunset": "sunset.fill"
        case "noon": "sun.max.fill"
        case "moonrise": "moonrise.fill"
        case "moonset": "moonset.fill"
        default: "circle.fill"
        }
    }

    /// The marks the light bar carries: every shown moment inside the bar's span.
    /// `twilight` are the edges of the twilights not hidden (golden always), and
    /// `extra` whatever else is shown (sunrise, sunset, noon, the moon's).
    static func barMarks(edges: [SunEdge], hiding hidden: Set<String>, extra: [SunMark], span: ClosedRange<Int64>)
        -> [SunMark]
    {
        let twilight = edges.filter { !hidden.contains($0.kind) }.map(SunMark.init)
        return (twilight + extra).filter { span.contains($0.at) }.sorted { $0.at < $1.at }
    }

    /// Where the marks go along a bar `width` points wide that runs over `span`.
    /// Sun moments (sunrise, sunset, noon) are placed first, then the twilights,
    /// then the moon's, so a crowd gives way to the sun. Each goes in its own
    /// strip, above the bar for the sun and moon and below for the twilights;
    /// when another symbol is closer than `gap` points in that strip it tries
    /// the other, and when both are taken it is left out (its row says it).
    /// A symbol's centre stays `gap / 2` from the bar's ends.
    static func placeMarks(_ marks: [SunMark], span: ClosedRange<Int64>, width: Double, gap: Double = 15)
        -> [PlacedMark]
    {
        guard width > gap, span.upperBound > span.lowerBound else { return [] }
        func priority(_ kind: String) -> Int {
            switch kind {
            case "sunrise", "sunset": 0
            case "noon": 1
            case "moonrise", "moonset": 3
            default: 2
            }
        }
        func prefersAbove(_ kind: String) -> Bool {
            ["sunrise", "sunset", "noon", "moonrise", "moonset"].contains(kind)
        }
        let length = Double(span.upperBound - span.lowerBound)
        var taken: [Bool: [Double]] = [true: [], false: []]
        var out: [PlacedMark] = []
        let ordered = marks.enumerated().sorted {
            (priority($0.element.kind), $0.element.at, $0.offset) < (priority($1.element.kind), $1.element.at, $1.offset)
        }.map(\.element)
        for mark in ordered {
            let raw = Double(mark.at - span.lowerBound) / length * width
            let x = min(max(raw, gap / 2), width - gap / 2)
            let first = prefersAbove(mark.kind)
            for above in [first, !first] where taken[above]!.allSatisfy({ abs($0 - x) >= gap }) {
                taken[above]!.append(x)
                out.append(PlacedMark(mark: mark, x: x, above: above))
                break
            }
        }
        return out.sorted { $0.mark.at < $1.mark.at }
    }

    /// The twilights whose rows are hidden, by the core's `kind`. Golden is not
    /// here: its edges bound the bright day on the bar, so they stay.
    static func hiddenTwilights(_ choices: [SunItemChoice]) -> Set<String> {
        Set(
            choices.filter { !$0.shown }.compactMap(\.item.eventKind).filter { $0 != "golden" })
    }

    /// The light bar's stretches: each pair of neighbouring edges is one, in the
    /// color of the twilight a morning one starts in and an evening one ends in,
    /// and "day" between the golden hours. A hidden twilight's stretch joins its
    /// neighbour: a morning one extends the stretch before it, an evening one the
    /// stretch after it, and at the ends of the day it goes, so the bar starts at
    /// the first twilight left (civil dawn when astronomical and nautical are hidden).
    static func lightBar(edges: [SunEdge], hiding hidden: Set<String>) -> [LightStretch] {
        struct Part {
            var stretch: LightStretch
            var morning: Bool
        }
        let parts = zip(edges, edges.dropFirst()).map { a, b -> Part in
            let day = a.kind == "golden" && b.kind == "golden"
            let kind = day ? "day" : (a.morning ? a.kind : b.kind)
            return Part(stretch: LightStretch(start: a.at, end: b.at, kind: kind), morning: a.morning)
        }
        // Morning stretches, forward: a hidden one extends the last kept one.
        var kept: [Part] = []
        for part in parts {
            guard hidden.contains(part.stretch.kind) else { kept.append(part); continue }
            if part.morning {
                if !kept.isEmpty { kept[kept.count - 1].stretch.end = part.stretch.end }
            } else {
                kept.append(part)
            }
        }
        // Evening stretches, backward: a hidden one extends the next kept one.
        var out: [LightStretch] = []
        for part in kept.reversed() {
            if hidden.contains(part.stretch.kind), !part.morning {
                if !out.isEmpty { out[out.count - 1].start = part.stretch.start }
            } else {
                out.append(part.stretch)
            }
        }
        return out.reversed()
    }
}
