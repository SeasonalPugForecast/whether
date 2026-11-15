import MapKit
import SwiftUI
import WhetherKit

/// The models' low-pressure tracks on Apple's map: one translucent band per model in
/// its theme color with edges in its own dash (`StormMapRules.dash`), the storm wider
/// than the other lows, each model's low at the scrubber's step labelled with the core's pressure,
/// and the place's pin. The map is set up as the radar's (`RadarMap`).
struct StormsMap: UIViewRepresentable {
    let view: StormsView
    /// The scrubber's step: its time picks each low's marker.
    let step: Int
    /// The models the chips hide, by model id.
    let hidden: Set<String>
    /// What covers the map's edges (the chips, the scrubber and the drawer), so the
    /// tracks are framed in the part left open.
    let insets: UIEdgeInsets
    @Environment(\.colorTheme) private var colors
    @Environment(\.colorScheme) private var scheme

    private var center: CLLocationCoordinate2D {
        CLLocationCoordinate2D(latitude: view.latitude, longitude: view.longitude)
    }

    /// What the lines depend on: when it changes, they are drawn again.
    struct LinesKey: Equatable {
        var tracks: [StormsView.Track]
        var colors: [String: RGBA]
    }

    /// A model's color as the map's appearance shows it (gray for another model).
    private func rgba(_ role: String) -> RGBA {
        guard let role = ColorRole.model(role) else { return RGBA(rgb: 0x8E8E93) }
        return colors[role].resolved(dark: scheme == .dark)
    }

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeUIView(context: Context) -> MKMapView {
        let map = MKMapView()
        map.delegate = context.coordinator
        map.pointOfInterestFilter = .excludingAll
        // The radar's muted map: the tracks are what to look at.
        map.preferredConfiguration = RadarMap.configuration(.muted)
        map.cameraZoomRange = MKMapView.CameraZoomRange(
            minCenterCoordinateDistance: 40_000, maxCenterCoordinateDistance: 60_000_000)
        map.setRegion(
            MKCoordinateRegion(center: center, span: MKCoordinateSpan(latitudeDelta: 20, longitudeDelta: 20)),
            animated: false)
        let pin = MKPointAnnotation()
        pin.coordinate = center
        map.addAnnotation(pin)
        return map
    }

    func updateUIView(_ map: MKMapView, context: Context) {
        let coordinator = context.coordinator
        let tracks = StormMapRules.shown(view.tracks, hidden: hidden)
        let roles = Set(view.tracks.map(\.role))
        let key = LinesKey(tracks: tracks, colors: Dictionary(uniqueKeysWithValues: roles.map { ($0, rgba($0)) }))
        if coordinator.lines != key {
            coordinator.lines = key
            map.removeOverlays(map.overlays)
            // The other lows under the storm.
            let ordered = tracks.filter { !$0.isStorm } + tracks.filter(\.isStorm)
            map.addOverlays(
                ordered.compactMap { track in
                    let coordinates = track.points.map {
                        CLLocationCoordinate2D(latitude: Double($0.latitude), longitude: Double($0.longitude))
                    }
                    guard coordinates.count > 1 else { return nil }
                    let line = TrackLine(coordinates: coordinates, count: coordinates.count)
                    line.color = UIColor(key.colors[track.role] ?? RGBA(rgb: 0x8E8E93))
                    line.dash = StormMapRules.dash(track.role)
                    line.width = StormMapRules.bandWidth(isStorm: track.isStorm)
                    return line
                }, level: .aboveRoads)
        }
        // The lows at the step.
        let time = view.steps.indices.contains(step) ? view.steps[step].time : nil
        let order = view.models.map(\.model)
        let marks: [LowMark] = tracks.compactMap { track in
            guard let time, let point = StormMapRules.point(of: track, at: time) else { return nil }
            let mark = LowMark()
            mark.coordinate = CLLocationCoordinate2D(
                latitude: Double(point.latitude), longitude: Double(point.longitude))
            mark.text = "\(track.short) \(point.pressure.text)"
            let rgba = key.colors[track.role] ?? RGBA(rgb: 0x8E8E93)
            mark.color = UIColor(rgba)
            mark.colorHex = rgba.hex
            mark.storm = track.isStorm
            mark.dark = scheme == .dark
            mark.slot = StormMapRules.labelSlot(order.firstIndex(of: track.model) ?? 0)
            mark.accessibilityLabel = "\(track.short) low, \(point.pressure.text)"
            return mark
        }
        let shown = marks.map(\.signature)
        if coordinator.marks.map(\.signature) != shown {
            map.removeAnnotations(coordinator.marks)
            coordinator.marks = marks
            map.addAnnotations(marks)
        }
        // Frame the tracks and the place once, in the part of the map left open.
        if !coordinator.framed || coordinator.insets != insets {
            let wasFramed = coordinator.framed
            coordinator.framed = true
            coordinator.insets = insets
            let place = MKMapRect(origin: MKMapPoint(center), size: MKMapSize(width: 1, height: 1))
            let rect = map.overlays.reduce(place) { $0.union($1.boundingMapRect) }
            // At least about 1,200 km across, so a lone low or a calm week still shows the region.
            let minimum = MKMapPointsPerMeterAtLatitude(center.latitude) * 1_200_000
            let grown = rect.insetBy(
                dx: -max(0, minimum - rect.width) / 2 - rect.width * 0.06,
                dy: -max(0, minimum - rect.height) / 2 - rect.height * 0.06)
            map.setVisibleMapRect(grown, edgePadding: insets, animated: wasFramed)
        }
    }

    /// A model's track, with how to draw it.
    nonisolated final class TrackLine: MKPolyline {
        var color = UIColor.gray
        /// The edges' dash pattern, in points.
        var dash: [Double] = []
        /// The band's width, edge to edge, in points.
        var width = 6.0
    }

    /// A track as a band: the outline of the line stroked at the band's width (round
    /// caps and joins, overlaps merged so the fill is even), filled in the model's color
    /// at `StormMapRules.fillAlpha` and stroked along it in the full color, dashed by
    /// the model. Each track is its own overlay, so where bands cross their fills blend.
    /// Its properties have defaults, so it keeps MapKit's initializers (MapKit calls
    /// `init(overlay:)` from `init(polyline:)`).
    nonisolated final class TrackBandRenderer: MKPolylineRenderer {
        private var color = UIColor.gray
        private var dash: [CGFloat] = []
        private var band: CGFloat = 6

        static func band(_ line: TrackLine) -> TrackBandRenderer {
            let renderer = TrackBandRenderer(polyline: line)
            renderer.color = line.color
            renderer.dash = line.dash.map { CGFloat($0) }
            renderer.band = CGFloat(line.width)
            // Tells MapKit how far the drawing reaches past the line.
            renderer.lineWidth = renderer.band + CGFloat(StormMapRules.edgeWidth)
            return renderer
        }

        override func draw(_ mapRect: MKMapRect, zoomScale: MKZoomScale, in context: CGContext) {
            if path == nil { createPath() }
            guard let line = path else { return }
            // Points on screen to map points at this zoom.
            let perPoint = 1 / zoomScale
            let outline = line.copy(
                strokingWithWidth: band * perPoint, lineCap: .round, lineJoin: .round, miterLimit: 10
            ).normalized()
            context.addPath(outline)
            context.setFillColor(color.withAlphaComponent(StormMapRules.fillAlpha).cgColor)
            context.fillPath()
            context.addPath(outline)
            context.setStrokeColor(color.cgColor)
            context.setLineWidth(CGFloat(StormMapRules.edgeWidth) * perPoint)
            context.setLineCap(.round)
            context.setLineJoin(.round)
            if !dash.isEmpty { context.setLineDash(phase: 0, lengths: dash.map { $0 * perPoint }) }
            context.strokePath()
        }
    }

    /// A model's low at the scrubber's step, labelled with its pressure.
    nonisolated final class LowMark: MKPointAnnotation {
        var text = ""
        var color = UIColor.gray
        var colorHex = ""
        var storm = false
        /// The map is dark: a dark plate with white text.
        var dark = false
        var slot = (right: true, up: true)

        /// What tells two marks apart for redrawing.
        var signature: String {
            "\(text)|\(coordinate.latitude)|\(coordinate.longitude)|\(colorHex)|\(slot.right)\(slot.up)|\(dark)"
        }
    }

    final class Coordinator: NSObject, MKMapViewDelegate {
        var lines: LinesKey?
        var marks: [LowMark] = []
        var framed = false
        var insets = UIEdgeInsets.zero

        func mapView(_ mapView: MKMapView, rendererFor overlay: any MKOverlay) -> MKOverlayRenderer {
            guard let line = overlay as? TrackLine else { return MKOverlayRenderer(overlay: overlay) }
            return TrackBandRenderer.band(line)
        }

        func mapView(_ mapView: MKMapView, viewFor annotation: any MKAnnotation) -> MKAnnotationView? {
            guard let mark = annotation as? LowMark else { return nil }  // the place: the usual pin
            let view = LowMarkView(annotation: mark, reuseIdentifier: nil)
            return view
        }
    }

    /// A low: a dot in the model's color at the low's center, and beside it a label
    /// with the model's name and the pressure on a plate that reads on any map.
    final class LowMarkView: MKAnnotationView {
        override init(annotation: (any MKAnnotation)?, reuseIdentifier: String?) {
            super.init(annotation: annotation, reuseIdentifier: reuseIdentifier)
            guard let mark = annotation as? LowMark else { return }
            displayPriority = .required
            collisionMode = .none
            zPriority = mark.storm ? .max : .defaultUnselected
            isAccessibilityElement = true
            accessibilityLabel = mark.accessibilityLabel

            let dotSize: CGFloat = mark.storm ? 14 : 10
            let label = UILabel()
            label.text = mark.text
            label.font = UIFont.preferredFont(forTextStyle: .caption1).bold()
            label.adjustsFontForContentSizeCategory = false
            let ground = mark.dark ? UIColor.black : UIColor.white
            label.textColor = mark.dark ? .white : .black
            let textSize = label.intrinsicContentSize
            let plate = UIView()
            plate.backgroundColor = ground.withAlphaComponent(mark.dark ? 0.78 : 0.9)
            plate.layer.cornerRadius = 7
            plate.layer.borderWidth = 2
            plate.layer.borderColor = mark.color.cgColor
            let plateSize = CGSize(width: textSize.width + 14, height: textSize.height + 6)

            // The plate sits beside the dot, toward its slot, with a gap.
            let gap: CGFloat = 4
            let width = dotSize + gap + plateSize.width
            let height = dotSize / 2 + gap + plateSize.height + dotSize / 2
            frame = CGRect(x: 0, y: 0, width: width, height: height)
            let dotX = mark.slot.right ? 0 : width - dotSize
            let dotY = mark.slot.up ? height - dotSize : 0
            let plateX = mark.slot.right ? dotSize + gap : 0
            let plateY = mark.slot.up ? 0 : height - plateSize.height
            let dot = UIView(frame: CGRect(x: dotX, y: dotY, width: dotSize, height: dotSize))
            dot.backgroundColor = mark.color
            dot.layer.cornerRadius = dotSize / 2
            dot.layer.borderWidth = 2
            dot.layer.borderColor = ground.cgColor
            plate.frame = CGRect(origin: CGPoint(x: plateX, y: plateY), size: plateSize)
            label.frame = plate.bounds.insetBy(dx: 7, dy: 3)
            plate.addSubview(label)
            addSubview(plate)
            addSubview(dot)
            // The dot's center on the low.
            centerOffset = CGPoint(x: width / 2 - (dotX + dotSize / 2), y: height / 2 - (dotY + dotSize / 2))
        }

        required init?(coder: NSCoder) { nil }
    }
}

extension UIFont {
    /// The same font, bold.
    fileprivate func bold() -> UIFont {
        fontDescriptor.withSymbolicTraits(.traitBold).map { UIFont(descriptor: $0, size: 0) } ?? self
    }
}
