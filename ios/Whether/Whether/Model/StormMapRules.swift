import Foundation
import WhetherKit

/// How the Storms map draws the models: each model's dash, so color is never the only
/// thing telling the tracks apart, which point of a track marks a scrubber step, and
/// which models the chips leave out.
enum StormMapRules {
    /// A track point marks a step when it is at most this far from the step's time.
    static let nearestWithin: Int64 = 3 * 3_600

    /// The track's dash pattern in points, on and off, drawn with round caps (so a
    /// near-zero dash is a dot): ECMWF solid, GFS long dashes, GEM dots, ICON dash-dot,
    /// any other model short dashes. Empty is solid.
    static func dash(_ role: String) -> [Double] {
        switch role {
        case "ecmwf": []
        case "gfs": [14, 9]
        case "gem": [0.5, 9]
        case "icon": [16, 9, 0.5, 9]
        default: [5, 7]
        }
    }

    /// The storm's track is drawn thicker than the other lows'.
    static func lineWidth(isStorm: Bool) -> Double { isStorm ? 4.5 : 2 }

    /// The point of `track` at `time`: the one at that time, else the nearest within
    /// `nearestWithin` (the earlier one on a tie), else `nil` (the low isn't there yet,
    /// or is gone).
    static func point(of track: StormsView.Track, at time: Int64) -> StormsView.TrackPoint? {
        var best: StormsView.TrackPoint?
        for point in track.points {
            let off = abs(point.time - time)
            guard off <= nearestWithin else { continue }
            if let kept = best, abs(kept.time - time) <= off { continue }
            best = point
        }
        return best
    }

    /// The step to show at first: `wanted` (a launch's) clamped to the steps, else the first.
    static func startStep(_ wanted: Int?, count: Int) -> Int {
        guard count > 0 else { return 0 }
        return min(max(wanted ?? 0, 0), count - 1)
    }

    /// The tracks to draw: those of the models not hidden (by the chips' model id).
    static func shown(_ tracks: [StormsView.Track], hidden: Set<String>) -> [StormsView.Track] {
        tracks.filter { !hidden.contains($0.model) }
    }

    /// The ensembles to chart: those whose model family (role) has no hidden chip. An
    /// ensemble of another family (`other`) always shows.
    static func shown(
        _ ensembles: [StormsView.Ensemble], hidden: Set<String>, chips: [StormsView.ModelChip]
    ) -> [StormsView.Ensemble] {
        let hiddenRoles = Set(chips.filter { hidden.contains($0.model) && $0.role != "other" }.map(\.role))
        return ensembles.filter { !hiddenRoles.contains($0.role) }
    }

    /// `hidden` with `model` shown if it was hidden, hidden if it was shown.
    static func toggled(_ hidden: Set<String>, _ model: String) -> Set<String> {
        hidden.contains(model) ? hidden.subtracting([model]) : hidden.union([model])
    }

    /// Where a model's low label sits around its marker, so the four models' labels
    /// don't pile up when the lows are close: right and up, right and down, left and
    /// up, left and down, by the model's place among the chips.
    static func labelSlot(_ index: Int) -> (right: Bool, up: Bool) {
        let i = ((index % 4) + 4) % 4
        return (right: i < 2, up: i % 2 == 0)
    }
}
