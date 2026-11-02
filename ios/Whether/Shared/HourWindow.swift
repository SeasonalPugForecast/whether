import CoreGraphics

/// A strip of hours scrolled like any list under a needle. The needle starts on
/// the first hour at the left edge and moves to the middle as the strip
/// scrolls, stays there, and moves on to the last hour at the right edge at the
/// end: the strip always fills its window, and the hour under the needle is the
/// one chosen.
nonisolated struct HourTrack: Equatable {
    /// One hour's width.
    let step: CGFloat
    /// The window's width.
    let viewport: CGFloat
    let count: Int

    /// The whole strip's width.
    var content: CGFloat { CGFloat(count) * step }

    /// How far the strip scrolls.
    var maxOffset: CGFloat { max(content - viewport, 0) }

    /// How far the needle travels: from the first hour's middle to the window's.
    var lead: CGFloat { max(min((viewport - step) / 2, maxOffset / 2), 0) }

    /// How far the strip scrolls while the needle travels (at each end): twice
    /// the needle's way, so the hour under it speeds up gently, never past
    /// about 1.75 times the strip's own speed (less room makes it quicker).
    var ramp: CGFloat { min(2 * lead, maxOffset / 2) }

    /// Where the needle is in the window with the strip scrolled `offset`: eased
    /// in and out (smoothstep) on its way to and from the middle.
    func needle(atOffset offset: CGFloat) -> CGFloat {
        let o = min(max(offset, 0), maxOffset)
        guard ramp > 0 else { return step / 2 }
        let ease = { (t: CGFloat) -> CGFloat in
            let t = min(max(t, 0), 1)
            return t * t * (3 - 2 * t)
        }
        return step / 2 + lead * ease(o / ramp) + lead * ease((o - (maxOffset - ramp)) / ramp)
    }

    /// The point of the strip under the needle (it only grows as the strip scrolls).
    func cursor(atOffset offset: CGFloat) -> CGFloat {
        min(max(offset, 0), maxOffset) + needle(atOffset: offset)
    }

    /// The hour under the needle.
    func hour(atOffset offset: CGFloat) -> Int {
        HourWindow.clamp(Int((cursor(atOffset: offset) / max(step, 1)).rounded(.down)), count: count)
    }

    /// The scroll offset that puts hour `index`'s middle under the needle.
    func offset(of index: Int) -> CGFloat {
        let x = (CGFloat(HourWindow.clamp(index, count: count)) + 0.5) * step
        // In the middle the needle stays put: one point of strip per point.
        if x >= ramp + step / 2 + lead, x <= maxOffset - ramp + step / 2 + lead {
            return x - step / 2 - lead
        }
        // On its way: `cursor` only grows, so halve the range until it is found.
        var low: CGFloat = 0
        var high = maxOffset
        for _ in 0..<40 {
            let mid = (low + high) / 2
            if cursor(atOffset: mid) < x { low = mid } else { high = mid }
        }
        return (low + high) / 2
    }

    /// Where a scroll that would come to rest at `offset` settles instead: with
    /// the nearest hour's middle under the needle.
    func settle(_ offset: CGFloat) -> CGFloat {
        let nearest = Int(((cursor(atOffset: offset) - step / 2) / max(step, 1)).rounded())
        return self.offset(of: nearest)
    }

    /// The hour under a tap at `x` in the window.
    func hour(atTap x: CGFloat, offset: CGFloat) -> Int {
        HourWindow.clamp(Int(((min(max(offset, 0), maxOffset) + x) / max(step, 1)).rounded(.down)), count: count)
    }

    /// The narrowest hour that still lets the needle reach the middle and leave
    /// it again: the strip at least two windows less an hour wide.
    var narrowest: CGFloat { 2 * viewport / CGFloat(max(count + 1, 1)) }
}

nonisolated enum HourWindow {
    /// An hour's width after a pinch of `scale` that began at `base`, within `range`.
    static func zoomed(_ base: CGFloat, by scale: CGFloat, within range: ClosedRange<CGFloat>) -> CGFloat {
        min(max(base * scale, range.lowerBound), range.upperBound)
    }

    /// Every how many hours a label goes, so labels at `step` per hour are at
    /// least `spacing` apart: 1, 2, 3, 4, 6, 12 or 24, so they fall on round hours.
    static func labelStride(step: CGFloat, spacing: CGFloat) -> Int {
        let needed = Int((spacing / max(step, 1)).rounded(.up))
        return [1, 2, 3, 4, 6, 12, 24].first { $0 >= needed } ?? 24
    }

    static func clamp(_ index: Int, count: Int) -> Int {
        min(max(index, 0), max(count - 1, 0))
    }
}
