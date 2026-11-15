import MapKit
import SwiftUI
import WhetherKit

/// The models' low-pressure tracks on Apple's map: one translucent band per model in
/// its theme color with edges in its own dash (`StormMapRules.dash`), the storm wider
/// than the other lows, each model's low at the scrubber's step labelled with the core's pressure,
/// and the place's pin. Active hurricanes add NHC's forecast cone under everything, the
/// guidance models as narrower bands, the observed track dotted, NHC's official track
/// solid with a marker a day, and the storm's symbol with its name. The map is set up as
/// the radar's (`RadarMap`).
struct StormsMap: UIViewRepresentable {
    let view: StormsView
    /// The scrubber's step: its time picks each low's marker.
    let step: Int
    /// The models the chips hide, by model id.
    let hidden: Set<String>
    /// The hurricanes the chips hide, by NHC id, and the guidance models, by aid code.
    let hiddenHurricanes: Set<String>
    let hiddenAids: Set<String>
    /// The hurricane to frame alone with the place (the Storm ahead card's), by NHC id.
    let focus: String?
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
        var hurricanes: [StormsView.Hurricane]
        /// By model role, `cone`, and `level.<name>` for the hurricanes' levels.
        var colors: [String: RGBA]
        var dark: Bool
    }

    private static let gray = RGBA(rgb: 0x8E8E93)

    /// A model's color as the map's appearance shows it (gray for another model).
    private func rgba(_ role: String) -> RGBA {
        guard let role = ColorRole.model(role) else { return Self.gray }
        return colors[role].resolved(dark: scheme == .dark)
    }

    /// The palette the lines and marks are drawn with.
    private func palette() -> [String: RGBA] {
        let dark = scheme == .dark
        var out: [String: RGBA] = [:]
        let roles = view.tracks.map(\.role) + view.hurricanes.flatMap { $0.aids.map(\.role) } + ["official"]
        for role in roles { out[role] = rgba(role) }
        out["cone"] = colors[.hurricaneCone].resolved(dark: dark)
        for h in view.hurricanes {
            out["level.\(h.level)"] = ColorRole.level(h.level).map { colors[$0].resolved(dark: dark) } ?? Self.gray
        }
        return out
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
        let hurricanes = StormMapRules.shown(view.hurricanes, hidden: hiddenHurricanes, hiddenAids: hiddenAids)
        let key = LinesKey(tracks: tracks, hurricanes: hurricanes, colors: palette(), dark: scheme == .dark)
        func color(_ name: String) -> UIColor { UIColor(key.colors[name] ?? Self.gray) }
        if coordinator.lines != key {
            coordinator.lines = key
            map.removeOverlays(map.overlays)
            // The cones under everything, then the other lows under the storm, then each
            // hurricane's observed track, its guidance and NHC's official track on top.
            let cones: [MKOverlay] = hurricanes.compactMap { h in
                let ring = h.cone.map { CLLocationCoordinate2D(latitude: $0.latitude, longitude: $0.longitude) }
                guard ring.count > 2 else { return nil }
                let cone = ConeShape(coordinates: ring, count: ring.count)
                cone.color = color("cone")
                return cone
            }
            let ordered = tracks.filter { !$0.isStorm } + tracks.filter(\.isStorm)
            let lows: [MKOverlay] = ordered.compactMap { track in
                band(
                    track.points.map {
                        CLLocationCoordinate2D(latitude: Double($0.latitude), longitude: Double($0.longitude))
                    }, role: track.role, color: color(track.role),
                    width: StormMapRules.bandWidth(isStorm: track.isStorm))
            }
            var storms: [MKOverlay] = []
            for h in hurricanes {
                let past = h.past.map { CLLocationCoordinate2D(latitude: $0.latitude, longitude: $0.longitude) }
                if past.count > 1 {
                    let line = PlainLine(coordinates: past, count: past.count)
                    line.color = color("official").withAlphaComponent(0.85)
                    line.width = StormMapRules.pastWidth
                    line.dash = StormMapRules.pastDash
                    storms.append(line)
                }
                for aid in h.aids {
                    let points = aid.points.map {
                        CLLocationCoordinate2D(latitude: $0.latitude, longitude: $0.longitude)
                    }
                    if let line = band(points, role: aid.role, color: color(aid.role), width: StormMapRules.aidBandWidth) {
                        storms.append(line)
                    }
                }
                let official = h.official.map { CLLocationCoordinate2D(latitude: $0.latitude, longitude: $0.longitude) }
                if official.count > 1 {
                    let line = PlainLine(coordinates: official, count: official.count)
                    line.color = color("official")
                    line.width = StormMapRules.officialWidth
                    storms.append(line)
                }
            }
            map.addOverlays(cones + lows + storms, level: .aboveRoads)
            // Each hurricane's symbol and name, and a marker a day on the official track.
            map.removeAnnotations(coordinator.storms)
            coordinator.storms = hurricanes.flatMap { h -> [MKAnnotation] in
                let days: [MKAnnotation] = StormMapRules.dayMarks(h.official).map { point in
                    let mark = DayMark()
                    mark.coordinate = CLLocationCoordinate2D(latitude: point.latitude, longitude: point.longitude)
                    mark.text = point.label
                    mark.color = color("official")
                    mark.dark = key.dark
                    mark.accessibilityLabel = "\(h.name), official forecast, \(point.label), \(point.wind.text)"
                    return mark
                }
                let storm = HurricaneMark()
                storm.coordinate = CLLocationCoordinate2D(latitude: h.latitude, longitude: h.longitude)
                storm.text = h.title
                storm.color = color("level.\(h.level)")
                storm.dark = key.dark
                storm.accessibilityLabel = "\(h.title), \(h.wind.text), \(h.movement)"
                return days + [storm]
            }
            map.addAnnotations(coordinator.storms)
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
            let rgba = key.colors[track.role] ?? Self.gray
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
        // Frame the tracks, the near hurricanes and the place once (or the hurricane the
        // Storm ahead card opened), in the part of the map left open.
        if !coordinator.framed || coordinator.insets != insets || coordinator.focus != focus {
            let wasFramed = coordinator.framed
            coordinator.framed = true
            coordinator.insets = insets
            coordinator.focus = focus
            let framing = StormMapRules.framing(
                view.hurricanes, focus: focus, latitude: view.latitude, longitude: view.longitude)
            var points = [MKMapPoint(center)]
            if framing.tracks {
                points += tracks.flatMap(\.points).map {
                    MKMapPoint(CLLocationCoordinate2D(latitude: Double($0.latitude), longitude: Double($0.longitude)))
                }
            }
            for h in framing.hurricanes {
                points += h.cone.map { MKMapPoint(CLLocationCoordinate2D(latitude: $0.latitude, longitude: $0.longitude)) }
                points += h.official.map {
                    MKMapPoint(CLLocationCoordinate2D(latitude: $0.latitude, longitude: $0.longitude))
                }
            }
            let rect = points.reduce(MKMapRect.null) {
                $0.union(MKMapRect(origin: $1, size: MKMapSize(width: 1, height: 1)))
            }
            // At least about 1,200 km across, so a lone low or a calm week still shows the region.
            let minimum = MKMapPointsPerMeterAtLatitude(center.latitude) * 1_200_000
            let grown = rect.insetBy(
                dx: -max(0, minimum - rect.width) / 2 - rect.width * 0.06,
                dy: -max(0, minimum - rect.height) / 2 - rect.height * 0.06)
            map.setVisibleMapRect(grown, edgePadding: insets, animated: wasFramed)
        }
    }

    /// A track as a band in its model's color and dash, or `nil` for fewer than two points.
    private func band(_ coordinates: [CLLocationCoordinate2D], role: String, color: UIColor, width: Double)
        -> TrackLine?
    {
        guard coordinates.count > 1 else { return nil }
        let line = TrackLine(coordinates: coordinates, count: coordinates.count)
        line.color = color
        line.dash = StormMapRules.dash(role)
        line.width = width
        return line
    }

    /// A model's track, with how to draw it.
    nonisolated final class TrackLine: MKPolyline {
        var color = UIColor.gray
        /// The edges' dash pattern, in points.
        var dash: [Double] = []
        /// The band's width, edge to edge, in points.
        var width = 6.0
    }

    /// A line of one width: NHC's official track (solid) or a hurricane's observed one
    /// (dotted).
    nonisolated final class PlainLine: MKPolyline {
        var color = UIColor.gray
        var width = 2.0
        /// On and off, in points; empty is solid.
        var dash: [Double] = []
    }

    /// NHC's forecast cone.
    nonisolated final class ConeShape: MKPolygon {
        var color = UIColor.gray
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

    /// A hurricane now: its symbol and name.
    nonisolated final class HurricaneMark: MKPointAnnotation {
        var text = ""
        var color = UIColor.gray
        var dark = false
    }

    /// A day on NHC's official track, labelled with its time.
    nonisolated final class DayMark: MKPointAnnotation {
        var text = ""
        var color = UIColor.gray
        var dark = false
    }

    final class Coordinator: NSObject, MKMapViewDelegate {
        var lines: LinesKey?
        var marks: [LowMark] = []
        /// The hurricanes' symbols and day markers.
        var storms: [MKAnnotation] = []
        var framed = false
        var insets = UIEdgeInsets.zero
        var focus: String?

        func mapView(_ mapView: MKMapView, rendererFor overlay: any MKOverlay) -> MKOverlayRenderer {
            if let line = overlay as? TrackLine { return TrackBandRenderer.band(line) }
            if let cone = overlay as? ConeShape {
                let renderer = MKPolygonRenderer(polygon: cone)
                renderer.fillColor = cone.color.withAlphaComponent(StormMapRules.coneFillAlpha)
                renderer.strokeColor = cone.color
                renderer.lineWidth = StormMapRules.coneEdgeWidth
                renderer.lineJoin = .round
                return renderer
            }
            if let line = overlay as? PlainLine {
                let renderer = MKPolylineRenderer(polyline: line)
                renderer.strokeColor = line.color
                renderer.lineWidth = line.width
                renderer.lineCap = .round
                renderer.lineJoin = .round
                if !line.dash.isEmpty { renderer.lineDashPattern = line.dash.map { NSNumber(value: $0) } }
                return renderer
            }
            return MKOverlayRenderer(overlay: overlay)
        }

        func mapView(_ mapView: MKMapView, viewFor annotation: any MKAnnotation) -> MKAnnotationView? {
            if let mark = annotation as? LowMark { return LowMarkView(annotation: mark, reuseIdentifier: nil) }
            if let mark = annotation as? HurricaneMark {
                return LabelledMarkView(
                    annotation: mark, text: mark.text, color: mark.color, dark: mark.dark,
                    symbol: "hurricane", size: 30, priority: .max)
            }
            if let mark = annotation as? DayMark {
                return LabelledMarkView(
                    annotation: mark, text: mark.text, color: mark.color, dark: mark.dark, symbol: nil, size: 11,
                    priority: .defaultSelected)
            }
            return nil  // the place: the usual pin
        }
    }

    /// The plate a mark's label sits on: reads on any map, edged in the mark's color.
    fileprivate static func plate(_ text: String, color: UIColor, dark: Bool, font: UIFont) -> UIView {
        let label = UILabel()
        label.text = text
        label.font = font
        label.adjustsFontForContentSizeCategory = false
        label.textColor = dark ? .white : .black
        let size = label.intrinsicContentSize
        let plate = UIView(frame: CGRect(x: 0, y: 0, width: size.width + 14, height: size.height + 6))
        plate.backgroundColor = (dark ? UIColor.black : UIColor.white).withAlphaComponent(dark ? 0.78 : 0.9)
        plate.layer.cornerRadius = 7
        plate.layer.borderWidth = 2
        plate.layer.borderColor = color.cgColor
        label.frame = plate.bounds.insetBy(dx: 7, dy: 3)
        plate.addSubview(label)
        return plate
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
            let ground = mark.dark ? UIColor.black : UIColor.white
            let plate = StormsMap.plate(
                mark.text, color: mark.color, dark: mark.dark,
                font: UIFont.preferredFont(forTextStyle: .caption1).bold())
            let plateSize = plate.bounds.size

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
            plate.frame.origin = CGPoint(x: plateX, y: plateY)
            addSubview(plate)
            addSubview(dot)
            // The dot's center on the low.
            centerOffset = CGPoint(x: width / 2 - (dotX + dotSize / 2), y: height / 2 - (dotY + dotSize / 2))
        }

        required init?(coder: NSCoder) { nil }
    }

    /// A hurricane's symbol (or a day's dot) in its color on the map's ground, centred
    /// on the point, with its label on a plate to the right and below, away from a track
    /// heading north or east.
    final class LabelledMarkView: MKAnnotationView {
        init(
            annotation: any MKAnnotation, text: String, color: UIColor, dark: Bool, symbol: String?,
            size: CGFloat, priority: MKAnnotationViewZPriority
        ) {
            super.init(annotation: annotation, reuseIdentifier: nil)
            displayPriority = .required
            collisionMode = .none
            zPriority = priority
            isAccessibilityElement = true
            accessibilityLabel = (annotation as? NSObject)?.accessibilityLabel ?? text
            let ground = dark ? UIColor.black : UIColor.white
            let font = UIFont.preferredFont(forTextStyle: symbol == nil ? .caption2 : .footnote).bold()
            let plate = StormsMap.plate(text, color: color, dark: dark, font: font)
            let plateSize = plate.bounds.size
            let gap: CGFloat = 3
            let width = size + gap + plateSize.width
            let height = max(size, size / 2 + plateSize.height)
            frame = CGRect(x: 0, y: 0, width: width, height: height)
            let dot = UIView(frame: CGRect(x: 0, y: 0, width: size, height: size))
            dot.layer.cornerRadius = size / 2
            dot.layer.borderWidth = 2
            if let symbol {
                dot.backgroundColor = ground.withAlphaComponent(0.92)
                dot.layer.borderColor = color.cgColor
                let image = UIImageView(
                    image: UIImage(
                        systemName: symbol, withConfiguration: UIImage.SymbolConfiguration(pointSize: size * 0.55, weight: .bold)))
                image.tintColor = color
                image.contentMode = .center
                image.frame = dot.bounds
                dot.addSubview(image)
            } else {
                dot.backgroundColor = color
                dot.layer.borderColor = ground.cgColor
            }
            plate.frame.origin = CGPoint(x: size + gap, y: size / 2)
            addSubview(plate)
            addSubview(dot)
            // The dot's center on the point.
            centerOffset = CGPoint(x: width / 2 - size / 2, y: height / 2 - size / 2)
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
