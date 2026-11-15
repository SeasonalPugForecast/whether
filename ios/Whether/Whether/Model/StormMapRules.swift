import Foundation
import MapKit
import WhetherKit

/// How the Storms map draws the models: each model's dash, so color is never the only
/// thing telling the tracks apart, which point of a track marks a scrubber step, and
/// which models the chips leave out.
enum StormMapRules {
    /// A track point marks a step when it is at most this far from the step's time.
    static let nearestWithin: Int64 = 3 * 3_600

    /// The dash pattern of a track's edges in points, on and off, drawn with round caps
    /// (so a near-zero dash is a dot) at `edgeWidth`: ECMWF solid, GFS long dashes, GEM
    /// dots, ICON dash-dot; for a hurricane's guidance UKMET dash-dash-dot, HAFS short
    /// even dashes, the consensus long dashes with short gaps; any other model short
    /// dashes. NHC's official track is solid (and a thicker line, not a band). Empty is
    /// solid. The band's fill between the edges is always continuous.
    static func dash(_ role: String) -> [Double] {
        switch role {
        case "ecmwf", "official": []
        case "gfs": [7, 4]
        case "gem": [0.01, 3.5]
        case "icon": [8, 3.5, 0.01, 3.5]
        case "ukmet": [10, 4, 2, 4]
        case "hafs": [3, 3]
        case "consensus": [12, 3]
        default: [3, 3.5]
        }
    }

    /// A track is a band this wide in points, edge to edge: the storm's wider than the
    /// other lows'.
    static func bandWidth(isStorm: Bool) -> Double { isStorm ? 12 : 6 }

    /// The width of a band's two edges, in points, in the model's full color.
    nonisolated static let edgeWidth = 1.5

    /// The opacity of a band's fill in the model's color, so bands that cross show
    /// through each other.
    nonisolated static let fillAlpha = 0.28

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

    // MARK: Hurricanes

    /// A hurricane guidance model's band, edge to edge, in points: as narrow as a low
    /// that isn't the storm.
    nonisolated static let aidBandWidth = 6.0
    /// NHC's official forecast track: a solid line this wide, in points, wider than a
    /// band's edges.
    nonisolated static let officialWidth = 3.5
    /// The observed track: dots this wide, this far apart (on and off, in points).
    nonisolated static let pastWidth = 2.5
    nonisolated static let pastDash: [Double] = [0.01, 5]
    /// The forecast cone: filled at this opacity of `hurricane.cone`, under a crisp edge
    /// this wide in the full color.
    nonisolated static let coneFillAlpha = 0.18
    nonisolated static let coneEdgeWidth = 1.5
    /// A hurricane is near when its cone or track comes within this of the place, in
    /// meters: the map then frames it with the place.
    static let nearWithin = 3_000_000.0

    /// The official forecast points that get a labelled marker: one a day, every 24 hours
    /// after the first (within an hour), so the labels don't crowd. The first is the
    /// storm itself, marked with its name.
    static func dayMarks(_ official: [StormsView.ForecastPoint]) -> [StormsView.ForecastPoint] {
        guard let first = official.first else { return [] }
        let day: Int64 = 86_400, slack: Int64 = 3_600
        return official.filter { point in
            let after = point.time - first.time
            guard after >= day - slack else { return false }
            let off = after % day
            return off <= slack || off >= day - slack
        }
    }

    /// The hurricanes to draw: those not hidden (by NHC id), each without the guidance
    /// hidden (by aid code).
    static func shown(
        _ hurricanes: [StormsView.Hurricane], hidden: Set<String>, hiddenAids: Set<String>
    ) -> [StormsView.Hurricane] {
        hurricanes.filter { !hidden.contains($0.id) }.map { h in
            var h = h
            h.aids = h.aids.filter { !hiddenAids.contains($0.aid) }
            return h
        }
    }

    /// One chip per guidance model across the hurricanes, by aid code, in the core's order.
    static func aidChips(_ hurricanes: [StormsView.Hurricane]) -> [StormsView.Aid] {
        var seen: Set<String> = []
        return hurricanes.flatMap(\.aids).filter { seen.insert($0.aid).inserted }
    }

    /// The great-circle distance between two points, in meters.
    static func distance(_ a: (latitude: Double, longitude: Double), _ b: (latitude: Double, longitude: Double))
        -> Double
    {
        let r = 6_371_000.0, rad = Double.pi / 180
        let dLat = (b.latitude - a.latitude) * rad, dLon = (b.longitude - a.longitude) * rad
        let h =
            sin(dLat / 2) * sin(dLat / 2)
            + cos(a.latitude * rad) * cos(b.latitude * rad) * sin(dLon / 2) * sin(dLon / 2)
        return 2 * r * asin(min(1, h.squareRoot()))
    }

    /// Whether `hurricane`'s centre, official track or cone comes within `nearWithin`
    /// of the place.
    static func isNear(_ hurricane: StormsView.Hurricane, latitude: Double, longitude: Double) -> Bool {
        let place = (latitude: latitude, longitude: longitude)
        let points =
            [(latitude: hurricane.latitude, longitude: hurricane.longitude)]
            + hurricane.official.map { (latitude: $0.latitude, longitude: $0.longitude) }
            + hurricane.cone.map { (latitude: $0.latitude, longitude: $0.longitude) }
        return points.contains { distance($0, place) <= nearWithin }
    }

    // MARK: Across the antimeridian

    /// `longitude` moved by whole turns to within 180° of `reference`.
    static func longitude(_ longitude: Double, near reference: Double) -> Double {
        longitude - 360 * ((longitude - reference) / 360).rounded()
    }

    /// `longitudes` as one continuous run, each within 180° of the one before (the first as
    /// it is): a track from 179° E to 179° W goes on to 181° instead of back across the world.
    /// The core gives longitudes in -180..<180, so a cone or track across the antimeridian
    /// jumps by 360° there.
    static func continuous(_ longitudes: [Double]) -> [Double] {
        var out: [Double] = []
        out.reserveCapacity(longitudes.count)
        for lon in longitudes {
            out.append(out.last.map { longitude(lon, near: $0) } ?? lon)
        }
        return out
    }

    /// The map points of positions whose longitudes are already continuous (beyond ±180° where
    /// they cross the antimeridian), all moved by the same whole turns so the westernmost lies
    /// in the world map: those east of 180° get an `x` past `MKMapSize.world.width`. MapKit
    /// takes such a shape as one that spans the 180th meridian (`MKMapRect.spans180thMeridian`,
    /// whose part past the edge it wraps to the other side, `MKMapRect.remainder`) and draws it
    /// the short way.
    static func mapPoints(unwrapped positions: [(latitude: Double, longitude: Double)]) -> [MKMapPoint] {
        guard let west = positions.map(\.longitude).min() else { return [] }
        let turns = ((west + 180) / 360).rounded(.down)
        let world = MKMapSize.world.width
        return positions.map { position in
            let shifted = position.longitude - 360 * turns
            let wrapped = longitude(shifted, near: 0)
            var point = MKMapPoint(CLLocationCoordinate2D(latitude: position.latitude, longitude: wrapped))
            point.x += (shifted - wrapped) / 360 * world
            return point
        }
    }

    /// The map points of a line or ring, drawn the short way across the antimeridian.
    static func mapPoints(_ positions: [(latitude: Double, longitude: Double)]) -> [MKMapPoint] {
        let longitudes = continuous(positions.map(\.longitude))
        return mapPoints(unwrapped: zip(positions, longitudes).map { (latitude: $0.latitude, longitude: $1) })
    }

    /// The smallest map rect around `positions` the short way round from `reference` (the
    /// place's longitude): each taken within 180° of it, so a storm across the antimeridian
    /// from the place is framed beside it, not across the world.
    static func frame(_ positions: [(latitude: Double, longitude: Double)], around reference: Double) -> MKMapRect {
        let near = positions.map { (latitude: $0.latitude, longitude: longitude($0.longitude, near: reference)) }
        return mapPoints(unwrapped: near).reduce(MKMapRect.null) {
            $0.union(MKMapRect(origin: $1, size: MKMapSize(width: 1, height: 1)))
        }
    }

    /// What the map frames besides the place: the hurricane `focus` names alone (the
    /// Storm ahead card opened it), else the near hurricanes and the models' tracks.
    static func framing(
        _ hurricanes: [StormsView.Hurricane], focus: String?, latitude: Double, longitude: Double
    ) -> (hurricanes: [StormsView.Hurricane], tracks: Bool) {
        if let focus, let focused = hurricanes.first(where: { $0.id == focus }) {
            return ([focused], false)
        }
        return (hurricanes.filter { isNear($0, latitude: latitude, longitude: longitude) }, true)
    }
}
