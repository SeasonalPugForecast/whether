import MapKit
import SwiftUI
import WhetherKit

/// Apple's map with the NEXRAD frames as tile overlays; only frame `index`
/// shows, so stepping through the loop is instant once the tiles are in. Frame `index`
/// loads first, the others after it.
struct RadarMap: UIViewRepresentable {
    let center: CLLocationCoordinate2D
    let frames: [RadarFrame]
    let index: Int
    var interactive = true
    /// Degrees of latitude shown at first.
    var span: CLLocationDegrees = 3.5
    /// Apple's satellite Earth zoomed out to a globe (Dark Sky had one), the
    /// radar still on it.
    var globe = false
    /// The highest zoom level the tiles exist at; the map stretches the last one
    /// beyond it.
    var maximumZ = 20
    /// The map under the radar.
    var style = RadarOptions.MapStyle.muted
    /// How strongly the frame shown is drawn.
    var opacity = RadarOptions.defaultOpacity
    /// Whether the loop's other frames load too, after the one shown; the Now tab's card
    /// shows only the latest.
    var loops = true
    /// Called with the frames' templates once all of their tiles are in (or the wait for
    /// them ran out): the loop can play.
    var onLoopLoaded: (([String]) -> Void)? = nil
    @Environment(\.colorTheme) private var colors
    @Environment(\.colorVision) private var vision
    @Environment(\.showsPatterns) private var patterns

    /// The colors NEXRAD's tiles are repainted with: the theme's radar steps, so they
    /// match the legend; NOAA's own colors (`nil`) under the gradient legend.
    private var ramp: [RGBA]? {
        RadarPalette.repaintRamp(colors, vision: vision, patterns: patterns)
    }

    /// The map configuration for a style. The map has no points of interest: the
    /// radar is what to look at.
    static func configuration(_ style: RadarOptions.MapStyle) -> MKMapConfiguration {
        // Realistic elevation: zooming far out turns the map into a globe.
        switch style {
        case .standard:
            let c = MKStandardMapConfiguration(elevationStyle: .realistic, emphasisStyle: .default)
            c.pointOfInterestFilter = .excludingAll
            return c
        case .muted:
            let c = MKStandardMapConfiguration(elevationStyle: .realistic, emphasisStyle: .muted)
            c.pointOfInterestFilter = .excludingAll
            return c
        case .satellite:
            return MKImageryMapConfiguration(elevationStyle: .realistic)
        case .hybrid:
            let c = MKHybridMapConfiguration(elevationStyle: .realistic)
            c.pointOfInterestFilter = .excludingAll
            return c
        }
    }

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeUIView(context: Context) -> MKMapView {
        let map = MKMapView()
        map.delegate = context.coordinator
        map.isUserInteractionEnabled = interactive
        map.pointOfInterestFilter = .excludingAll
        map.preferredConfiguration = Self.configuration(style)
        context.coordinator.style = style
        context.coordinator.span = span
        context.coordinator.opacity = opacity
        map.setRegion(
            MKCoordinateRegion(
                center: center,
                span: MKCoordinateSpan(latitudeDelta: span, longitudeDelta: span)),
            animated: false)
        // The composite is drawn up to county scale.
        map.cameraZoomRange = MKMapView.CameraZoomRange(
            minCenterCoordinateDistance: 40_000,
            maxCenterCoordinateDistance: interactive ? 60_000_000 : 4_000_000)
        let pin = MKPointAnnotation()
        pin.coordinate = center
        map.addAnnotation(pin)
        return map
    }

    func updateUIView(_ map: MKMapView, context: Context) {
        let coordinator = context.coordinator
        let templates = frames.map(\.tileTemplate)
        let ramp = ramp
        coordinator.loops = loops
        coordinator.onLoopLoaded = onLoopLoaded
        if coordinator.templates != templates || coordinator.ramp != ramp {
            coordinator.load(templates, ramp: ramp, showing: index, maximumZ: maximumZ, on: map)
        }
        coordinator.opacity = opacity
        coordinator.show(index)
        if coordinator.style != style || coordinator.globe != globe {
            coordinator.style = style
            if !globe { map.preferredConfiguration = Self.configuration(style) }
        }
        if coordinator.globe == globe, coordinator.span != span {
            // The starting zoom changed (the options): fly to it.
            coordinator.span = span
            if !globe {
                map.setRegion(
                    MKCoordinateRegion(
                        center: center, span: MKCoordinateSpan(latitudeDelta: span, longitudeDelta: span)),
                    animated: true)
            }
        }
        if coordinator.globe != globe {
            coordinator.globe = globe
            if globe {
                map.preferredConfiguration = MKImageryMapConfiguration(elevationStyle: .realistic)
                // As far out as MapKit goes (it stops there): the whole sphere
                // in space, a little wider than a phone held upright.
                map.setCamera(
                    MKMapCamera(lookingAtCenter: center, fromDistance: 44_000_000, pitch: 0, heading: 0),
                    animated: true)
            } else {
                coordinator.span = span
                map.setRegion(
                    MKCoordinateRegion(
                        center: center, span: MKCoordinateSpan(latitudeDelta: span, longitudeDelta: span)),
                    animated: true)
            }
        }
    }

    final class Coordinator: NSObject, MKMapViewDelegate {
        var templates: [String] = []
        var ramp: [RGBA]?
        var overlays: [MKTileOverlay] = []
        var renderers: [ObjectIdentifier: MKTileOverlayRenderer] = [:]
        var shown = 0
        var globe = false
        var style = RadarOptions.MapStyle.muted
        var span: CLLocationDegrees = 0
        var opacity = RadarOptions.defaultOpacity
        var loops = true
        var onLoopLoaded: (([String]) -> Void)?
        /// The frames not on the map yet, and what tells when their tiles are in.
        private var waiting: [MKTileOverlay] = []
        private var waitingLoads: TileLoads?
        private weak var map: MKMapView?
        /// Counts the loads, so what an earlier one started (its timers, its tiles
        /// arriving late) does nothing once another took its place, even with the same
        /// frames (a new ramp).
        private var generation = 0

        /// Puts the frames of `templates` on `map`: frame `index` at once, the others once
        /// its tiles are in (or after `RadarLoopRules.latestWait`), so they don't compete
        /// with it for the connections. Without `loops` the others never load.
        func load(_ templates: [String], ramp: [RGBA]?, showing index: Int, maximumZ: Int, on map: MKMapView) {
            map.removeOverlays(map.overlays)
            renderers = [:]
            self.map = map
            self.templates = templates
            self.ramp = ramp
            generation += 1
            let current = generation
            let first = min(max(index, 0), max(templates.count - 1, 0))
            let firstLoads = TileLoads { [weak self] in self?.addWaiting(generation: current) }
            let loopLoads = TileLoads { [weak self] in
                guard let self, self.generation == current else { return }
                self.onLoopLoaded?(templates)
            }
            overlays = templates.enumerated().map { i, template in
                let overlay = RadarTileOverlay(
                    urlTemplate: template, ramp: RadarPalette.recolors(template: template) ? ramp : nil,
                    loads: i == first ? firstLoads : loopLoads)
                overlay.canReplaceMapContent = false
                overlay.maximumZ = maximumZ
                return overlay
            }
            shown = first
            waiting = RadarLoopRules.deferred(count: overlays.count, shown: first).map { overlays[$0] }
            waitingLoads = loopLoads
            guard overlays.indices.contains(first) else { return }
            map.addOverlay(overlays[first], level: .aboveRoads)
            guard loops, !waiting.isEmpty else { return }
            Task { [weak self] in
                try? await Task.sleep(for: RadarLoopRules.latestWait)
                self?.addWaiting(generation: current)
            }
        }

        /// Adds the frames still waiting, while load `generation` is the map's; the loop
        /// counts as loaded once their tiles are in, or after `RadarLoopRules.loopWait`.
        func addWaiting(generation: Int) {
            guard loops, generation == self.generation, !waiting.isEmpty, let map else { return }
            let frames = waiting
            waiting = []
            map.addOverlays(frames, level: .aboveRoads)
            let loads = waitingLoads
            Task { [weak self] in
                try? await Task.sleep(for: RadarLoopRules.loopWait)
                guard self?.generation == generation else { return }
                loads?.settle()
            }
        }

        func show(_ index: Int) {
            // Another frame than the first (dragged to): the others are wanted now.
            if index != shown { addWaiting(generation: generation) }
            shown = index
            for (i, overlay) in overlays.enumerated() {
                renderers[ObjectIdentifier(overlay)]?.alpha = i == index ? opacity : 0
            }
        }

        func mapView(_ mapView: MKMapView, rendererFor overlay: any MKOverlay) -> MKOverlayRenderer {
            guard let tiles = overlay as? MKTileOverlay else { return MKOverlayRenderer(overlay: overlay) }
            let renderer = MKTileOverlayRenderer(tileOverlay: tiles)
            renderer.alpha = overlays.firstIndex(of: tiles) == shown ? opacity : 0
            renderers[ObjectIdentifier(tiles)] = renderer
            return renderer
        }
    }
}
