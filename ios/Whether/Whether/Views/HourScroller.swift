import SwiftUI

/// A strip of hours that scrolls like any list: it follows the finger, a flick
/// carries on with the system's momentum, and it settles with an hour under the
/// needle, which chooses it. The needle starts on the first hour at the left
/// edge, moves to the middle as the strip scrolls and stays there, then moves on
/// to the last hour at the right edge (`HourTrack`), so the strip always fills
/// the window. Tap an hour to bring the needle to it; pinch to show fewer or
/// more hours (`step` within `zoom`). Setting `selected` from outside
/// (VoiceOver, screenshots) scrolls to that hour.
struct HourScroller<Marker: View, Strip: View>: View {
    let count: Int
    @Binding var selected: Int
    /// One hour's width.
    @Binding var step: CGFloat
    let zoom: ClosedRange<CGFloat>
    /// Drawn behind the strip at the needle, one hour wide.
    @ViewBuilder var marker: () -> Marker
    /// The hours side by side, `step` wide each, `count * step` in all.
    @ViewBuilder var strip: () -> Strip

    @State private var position = ScrollPosition(idType: Int.self)
    /// The scroll offset now.
    @State private var offset: CGFloat = 0
    /// The hour width when a pinch began; the needle's hour stays put meanwhile.
    @State private var pinchBase: CGFloat?
    /// The hour a tap or a choice from outside is scrolling to: chosen at once,
    /// and not unchosen by the hours passing under the needle on the way. A
    /// touch, or the scroll coming to rest, ends it.
    @State private var target: Int?
    /// Scrolled to the chosen hour on appearing: until then the scroll view's
    /// first reports (offset 0, at rest) come before it and choose nothing.
    @State private var placed = false

    var body: some View {
        GeometryReader { geo in
            let track = HourTrack(step: step, viewport: geo.size.width, count: count)
            ScrollView(.horizontal, showsIndicators: false) {
                strip()
                    .frame(width: track.content, height: geo.size.height, alignment: .leading)
            }
            .contentMargins(0, for: .scrollContent)
            .scrollPosition($position)
            .scrollTargetBehavior(HourSnap(step: step, count: count))
            .background(alignment: .leading) {
                marker()
                    .frame(width: step)
                    .offset(x: track.needle(atOffset: offset) - step / 2)
            }
            .onScrollGeometryChange(for: CGFloat.self) { $0.contentOffset.x } action: { _, x in
                offset = x
                // While pinching the widths change under the needle; its hour
                // stays. On the way to a tapped hour it is already chosen.
                guard placed, pinchBase == nil, target == nil else { return }
                let hour = track.hour(atOffset: x)
                if hour != selected { selected = hour }
            }
            .onTapGesture { location in
                scroll(to: track.hour(atTap: location.x, offset: offset), in: track, animated: true)
            }
            .simultaneousGesture(
                MagnifyGesture()
                    .onChanged { value in
                        let base = pinchBase ?? step
                        pinchBase = base
                        // Never so narrow that the needle could not reach the middle.
                        let lowest = max(zoom.lowerBound, track.narrowest)
                        step = HourWindow.zoomed(
                            base, by: value.magnification, within: lowest...max(lowest, zoom.upperBound))
                    }
                    .onEnded { _ in pinchBase = nil }
            )
            .onScrollPhaseChange { _, phase in
                switch phase {
                case .tracking, .interacting:
                    // The finger takes over from an animated scroll.
                    target = nil
                case .idle:
                    // At rest: the hour under the needle is the chosen one.
                    target = nil
                    let hour = track.hour(atOffset: offset)
                    if placed, pinchBase == nil, hour != selected { selected = hour }
                default:
                    break
                }
            }
            .onChange(of: selected) { _, hour in
                // Chosen from outside: bring the needle to it.
                guard hour != target, track.hour(atOffset: offset) != hour else { return }
                scroll(to: hour, in: track, animated: true)
            }
            .onChange(of: track) { scroll(to: selected, in: track, animated: false) }
            .onAppear {
                scroll(to: selected, in: track, animated: false)
                placed = true
            }
        }
    }

    private func scroll(to hour: Int, in track: HourTrack, animated: Bool) {
        let hour = HourWindow.clamp(hour, count: count)
        let x = track.offset(of: hour)
        if animated {
            target = hour
            if selected != hour { selected = hour }
            withAnimation(.snappy) { position.scrollTo(x: x) }
        } else {
            position.scrollTo(x: x)
        }
    }
}

/// Settles a scroll with an hour's middle under the needle, after the momentum
/// the system gave it.
nonisolated struct HourSnap: ScrollTargetBehavior {
    let step: CGFloat
    let count: Int

    func updateTarget(_ target: inout ScrollTarget, context: TargetContext) {
        let track = HourTrack(step: step, viewport: context.containerSize.width, count: count)
        target.rect.origin.x = track.settle(target.rect.origin.x)
    }
}
