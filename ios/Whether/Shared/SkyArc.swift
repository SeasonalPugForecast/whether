import Foundation

/// Where the sun and moon are on the Now tab's arc (the "sky" behind the top
/// summary): from rising to setting each crosses the width, low at the edges and
/// highest in the middle. The times are the core's instants for the place shown,
/// so its time zone is already in them. Pure arithmetic, no formatting.
enum SkyArc {
    /// A body's place on the arc.
    struct Place: Equatable {
        /// 0 at its rising, 1 at its setting: the share of the width crossed.
        var along: Double
        /// 0 on the horizon, 1 at the highest point: a sine of `along`.
        var height: Double
        /// 0…1, so a body fades in as it rises and out as it sets, not pops.
        var fade: Double
    }

    /// The moon rises about 50 minutes later each day: a day of the moon is 24 h 50 min.
    static let lunarDay: Int64 = 89_400
    /// How long the moon is up, when only one of its two times is known (half a lunar day).
    static let moonUp: Int64 = 44_700

    /// The place `progress` (0…1 from rising to setting) of the way across.
    static func place(progress p: Double) -> Place {
        let height = sin(min(max(p, 0), 1) * .pi)
        return Place(along: p, height: height, fade: min(1, height * 6))
    }

    /// The sun at `now` (unix seconds), between today's `rise` and `set`; nil before
    /// and after, and without both times (a polar day or night).
    static func sun(now: Int64, rise: Int64?, set: Int64?) -> Place? {
        guard let rise, let set, set > rise, (rise...set).contains(now) else { return nil }
        return place(progress: Double(now - rise) / Double(set - rise))
    }

    /// The stretches of time around today's date the moon is up. Both times, rise
    /// first: one stretch. Set first (the moon rose yesterday, and rises again
    /// today): the end of yesterday's and the start of today's, each completed by a
    /// lunar day's shift. One time: half a lunar day from it.
    static func moonWindows(rise: Int64?, set: Int64?) -> [ClosedRange<Int64>] {
        switch (rise, set) {
        case (let rise?, let set?) where set > rise:
            return [rise...set]
        case (let rise?, let set?):
            return [(rise - lunarDay)...set, rise...(set + lunarDay)]
        case (let rise?, nil):
            return [rise...(rise + moonUp)]
        case (nil, let set?):
            return [(set - moonUp)...set]
        case (nil, nil):
            return []
        }
    }

    /// The moon at `now`, while it is up.
    static func moon(now: Int64, rise: Int64?, set: Int64?) -> Place? {
        for window in moonWindows(rise: rise, set: set) where window.contains(now) {
            return place(progress: Double(now - window.lowerBound) / Double(window.upperBound - window.lowerBound))
        }
        return nil
    }

    /// A screenshot's clock: `fraction` of the way from sunrise (0) to sunset (1);
    /// below 0 is before sunrise, above 1 after sunset.
    static func now(fraction: Double, rise: Int64, set: Int64) -> Int64 {
        rise + Int64((fraction * Double(set - rise)).rounded())
    }

    /// A body's centre in a `width` × `height` area: `inset` from the sides at the
    /// horizon, and from `horizon` (the y of the horizon) up to `peak` at its highest.
    static func point(
        _ place: Place, width: Double, horizon: Double, peak: Double, inset: Double
    ) -> (x: Double, y: Double) {
        (inset + place.along * max(width - 2 * inset, 0), horizon - place.height * (horizon - peak))
    }
}
