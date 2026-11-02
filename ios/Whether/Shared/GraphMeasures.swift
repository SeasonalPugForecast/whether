import CoreGraphics
import Foundation

/// The order rules of the measure lists (`Preferences.graphMeasures` and
/// `hourRows`): the shown measures as raw values, the first one leading (the
/// graph fills it; the 24 hours print it largest). Views only animate these.
/// `known` is every raw value there is (`GraphMeasure.allCases`).
nonisolated enum MeasureList {
    /// What shows when a list holds nothing usable.
    static let fallback = "temperature"

    /// `raw` without unknown or repeated values; the temperature when none is left.
    static func shown(_ raw: [String], known: [String]) -> [String] {
        var seen = Set<String>()
        let out = raw.filter { known.contains($0) && seen.insert($0).inserted }
        return out.isEmpty ? [fallback] : out
    }

    /// `id` first, the others as they were.
    static func fill(_ id: String, in list: [String]) -> [String] {
        guard let i = list.firstIndex(of: id) else { return list }
        var out = list
        out.insert(out.remove(at: i), at: 0)
        return out
    }

    /// `id` moved `by` places (negative: toward the start), stopping at the ends.
    static func move(_ id: String, by steps: Int, in list: [String]) -> [String] {
        guard let i = list.firstIndex(of: id) else { return list }
        var out = list
        let to = min(max(i + steps, 0), list.count - 1)
        out.insert(out.remove(at: i), at: to)
        return out
    }

    /// `id` in the place of `target` (what dropping one chip on another does):
    /// the others make room.
    static func place(_ id: String, at target: String, in list: [String]) -> [String] {
        guard id != target, list.contains(id), let to = list.firstIndex(of: target) else { return list }
        var out = list
        out.remove(at: out.firstIndex(of: id) ?? 0)
        out.insert(id, at: to)
        return out
    }

    /// `list` without `id`, unless it is the only one: one measure always stays.
    static func hide(_ id: String, in list: [String]) -> [String] {
        guard list.count > 1 else { return list }
        return list.filter { $0 != id }
    }

    /// `list` with `id` after the shown ones.
    static func show(_ id: String, in list: [String]) -> [String] {
        list.contains(id) ? list : list + [id]
    }

    /// The editor's rows: the shown measures in order, then the offered ones
    /// that are hidden, in their usual order.
    static func editorRows(shown: [String], offered: [String]) -> [(id: String, shown: Bool)] {
        shown.map { ($0, true) } + offered.filter { !shown.contains($0) }.map { ($0, false) }
    }

    /// The shown measures after the editor moved rows: only the shown ones
    /// count, as in a list of them alone (a hidden row dragged among them
    /// settles back below).
    static func moved(
        shown: [String], offered: [String], from source: IndexSet, to destination: Int
    ) -> [String] {
        // What SwiftUI's `move(fromOffsets:toOffset:)` does: the moved rows go
        // before the row at `destination` (an index in the rows as they are).
        var rows = editorRows(shown: shown, offered: offered).map(\.id)
        let moving = source.map { rows[$0] }
        let before = source.filter { $0 < destination }.count
        for i in source.reversed() { rows.remove(at: i) }
        rows.insert(contentsOf: moving, at: destination - before)
        let moved = rows.filter { shown.contains($0) }
        return moved.isEmpty ? shown : moved
    }

    /// Which scale a measure is drawn on: wind and gusts share one, and the
    /// feels-like and air temperatures (so they compare), when `shared`; else
    /// each its own.
    static func scale(of id: String, shared: Bool) -> String {
        guard shared else { return id }
        switch id {
        case "gusts": return "wind"
        case "actual": return "temperature"
        default: return id
        }
    }
}

/// How the Hourly graph labels and scales its lines (Settings → Display → Hourly graph).
struct GraphOptions: Codable, Equatable {
    /// A value label every this many hours; 0 for automatic (the graph picks by
    /// how wide the hours are).
    var valueEvery = 0
    /// Like measures share a scale (feels-like with air temperature, wind with
    /// gusts), or each has its own.
    var sharedScales = false

    /// The choices for `valueEvery`.
    static let everyChoices = [0, 1, 3, 6]

    init() {}

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let d = GraphOptions()
        let every = (try? c.decodeIfPresent(Int.self, forKey: .valueEvery)) ?? d.valueEvery
        valueEvery = Self.everyChoices.contains(every) ? every : d.valueEvery
        sharedScales = (try? c.decodeIfPresent(Bool.self, forKey: .sharedScales)) ?? d.sharedScales
    }
}

extension HourWindow {
    /// Every how many hours the graph labels its values. Automatic (0) thins
    /// them as the hours narrow, never under 3; a choice is kept as far as
    /// labels `labelWidth` points wide still fit, else thinned to round hours.
    static func valueStride(every: Int, step: CGFloat, labelWidth: CGFloat) -> Int {
        guard every > 0 else { return max(3, labelStride(step: step, spacing: 60)) }
        return max(every, labelStride(step: step, spacing: labelWidth))
    }
}
