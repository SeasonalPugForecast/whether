import Foundation

/// How the radar looks and moves: the map under it, how fast the loop
/// plays, how strong the rain is drawn, and how far in it opens. Saved in
/// `Preferences.radar`; every field decodes with its default, so an older save,
/// or a value this version does not know, keeps the rest.
struct RadarOptions: Codable, Equatable {
    /// The map under the radar. The default is the muted standard map the radar
    /// always had.
    enum MapStyle: String, Codable, CaseIterable, Identifiable {
        case standard, muted, satellite, hybrid
        var id: Self { self }

        var title: String {
            switch self {
            case .standard: "Standard"
            case .muted: "Muted"
            case .satellite: "Satellite"
            case .hybrid: "Hybrid"
            }
        }

        var symbol: String {
            switch self {
            case .standard: "map"
            case .muted: "map.fill"
            case .satellite: "globe.americas.fill"
            case .hybrid: "square.2.layers.3d"
            }
        }
    }

    /// How fast the loop steps.
    enum LoopSpeed: String, Codable, CaseIterable, Identifiable {
        case slow, normal, fast
        var id: Self { self }

        var title: String {
            switch self {
            case .slow: "Slow"
            case .normal: "Normal"
            case .fast: "Fast"
            }
        }

        /// Time between two frames.
        var frameMilliseconds: Int {
            switch self {
            case .slow: 1000
            case .normal: 600
            case .fast: 300
            }
        }
    }

    /// How far in the radar opens.
    enum StartZoom: String, Codable, CaseIterable, Identifiable {
        case city, region, state
        var id: Self { self }

        var title: String {
            switch self {
            case .city: "City"
            case .region: "Region"
            case .state: "State"
            }
        }

        /// Degrees of latitude shown at first on the full screen (the region is
        /// about 150 km across: county scale, as it always was).
        var span: Double {
            switch self {
            case .city: 0.5
            case .region: 1.4
            case .state: 4.0
            }
        }

        /// The same on the Now tab's card, which shows a wider area.
        var cardSpan: Double {
            switch self {
            case .city: 1.0
            case .region: 2.5
            case .state: 6.0
            }
        }
    }

    /// The opacity the radar always had.
    static let defaultOpacity = 0.75
    /// Below this the rain can hardly be seen.
    static let opacityRange = 0.2...1.0

    var style = MapStyle.muted
    var speed = LoopSpeed.normal
    /// How strongly the rain is drawn over the map, within `opacityRange`.
    var opacity = RadarOptions.defaultOpacity
    var startZoom = StartZoom.region

    init() {}

    /// Saved options; a field that is missing, unknown or out of range keeps its default.
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let d = RadarOptions()
        style = (try? c.decodeIfPresent(MapStyle.self, forKey: .style)) ?? d.style
        speed = (try? c.decodeIfPresent(LoopSpeed.self, forKey: .speed)) ?? d.speed
        let saved = try? c.decodeIfPresent(Double.self, forKey: .opacity)
        opacity = saved.map(Self.clamped) ?? d.opacity
        startZoom = (try? c.decodeIfPresent(StartZoom.self, forKey: .startZoom)) ?? d.startZoom
    }

    /// `value` within `opacityRange` (a number that is not one reads as the default).
    static func clamped(_ value: Double) -> Double {
        guard value.isFinite else { return defaultOpacity }
        return min(max(value, opacityRange.lowerBound), opacityRange.upperBound)
    }

    /// Options for a launch's arguments (`-radarstyle hybrid -radarspeed fast
    /// -radaropacity 0.4 -radarzoom state`), over `self`: for screenshots, which
    /// never save them. An argument that does not parse changes nothing.
    func overridden(by args: [String]) -> RadarOptions {
        func value(_ flag: String) -> String? {
            args.firstIndex(of: flag).flatMap { args.indices.contains($0 + 1) ? args[$0 + 1] : nil }
        }
        var out = self
        if let s = value("-radarstyle").flatMap(MapStyle.init(rawValue:)) { out.style = s }
        if let s = value("-radarspeed").flatMap(LoopSpeed.init(rawValue:)) { out.speed = s }
        if let s = value("-radaropacity").flatMap(Double.init) { out.opacity = Self.clamped(s) }
        if let s = value("-radarzoom").flatMap(StartZoom.init(rawValue:)) { out.startZoom = s }
        return out
    }
}
