import Foundation

/// The storms screen from the Rust core (`whether_core::view::StormsVm`), as
/// `stormsView(...)` returns it: several models side by side for the coming storms
/// at one place. Every value is already formatted; numbers come along only to place
/// marks on charts and maps.
///
/// Enum-like fields (`role`, `agreement`, `level`) stay strings, so a new value from
/// the core never breaks decoding.
public struct StormsView: Decodable, Sendable, Hashable {
    /// "New York".
    public var place: String
    public var latitude: Double
    public var longitude: Double
    /// One chip per deterministic model, in display order.
    public var models: [ModelChip]
    /// "No storm in the next 10 days. …" when there is no event.
    public var summary: String?
    /// The coming storms, in time order.
    public var events: [Event]
    /// Low-pressure tracks for the map, per model.
    public var tracks: [Track]
    /// The time scrubber's steps, oldest first.
    public var steps: [Step]
    /// Active tropical cyclones from NHC, in the core's order; empty when there are none
    /// (the core leaves the key out then, and earlier views lack it).
    public var hurricanes: [Hurricane]
    /// The data credits: always shown.
    public var attribution: [String]

    private enum CodingKeys: String, CodingKey {
        case place, latitude, longitude, models, summary, events, tracks, steps, hurricanes, attribution
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        place = try c.decode(String.self, forKey: .place)
        latitude = try c.decode(Double.self, forKey: .latitude)
        longitude = try c.decode(Double.self, forKey: .longitude)
        models = try c.decode([ModelChip].self, forKey: .models)
        summary = try c.decodeIfPresent(String.self, forKey: .summary)
        events = try c.decode([Event].self, forKey: .events)
        tracks = try c.decode([Track].self, forKey: .tracks)
        steps = try c.decode([Step].self, forKey: .steps)
        hurricanes = try c.decodeIfPresent([Hurricane].self, forKey: .hurricanes) ?? []
        attribution = try c.decode([String].self, forKey: .attribution)
    }

    /// Decodes the core's JSON.
    public static func decode(_ json: String) throws -> StormsView {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return try decoder.decode(StormsView.self, from: Data(json.utf8))
    }
}

extension StormsView {
    public typealias Quantity = ViewModel.Quantity

    /// A model and the run shown.
    public struct ModelChip: Decodable, Sendable, Hashable, Identifiable {
        /// The Open-Meteo id, `ecmwf_ifs025`.
        public var model: String
        /// "ECMWF IFS 0.25°".
        public var label: String
        /// "ECMWF".
        public var short: String
        /// `ecmwf`, `gfs`, `gem`, `icon` or `other`: its color and dash.
        public var role: String
        /// "12z".
        public var run: String
        public var id: String { model }
    }

    /// One coming storm at the place.
    public struct Event: Decodable, Sendable, Hashable {
        /// "Sat night – Sun night".
        public var title: String
        /// The window's start and end, Unix seconds.
        public var start: Int64
        public var end: Int64
        /// One row per model.
        public var rows: [ModelRow]
        /// `nil` when fewer than two models can be compared.
        public var agreement: Agreement?
        public var ensembles: [Ensemble]
        /// The totals over each model's recent runs (empty until they are fetched).
        public var runToRun: [RunToRun]
    }

    /// One model's numbers for an event.
    public struct ModelRow: Decodable, Sendable, Hashable, Identifiable {
        public var model: String
        public var short: String
        public var role: String
        public var snow: Quantity
        /// Rain and snow as water.
        public var precipitation: Quantity
        public var gust: Quantity
        /// The lowest pressure at the place, and when: "Sun 1 PM".
        public var low: Quantity
        public var lowAt: String
        /// When it starts: "Sat 10 PM".
        public var onset: String
        public var id: String { model }
    }

    /// How closely the models agree.
    public struct Agreement: Decodable, Sendable, Hashable {
        /// `high`, `medium` or `low`.
        public var agreement: String
        /// The core's `Level`, to color it.
        public var level: String
        /// "Low agreement".
        public var label: String
        /// "ECMWF brings the low within 55 mi of New York; GFS keeps it 210 mi away."
        public var sentence: String
    }

    /// One ensemble's spread for an event.
    public struct Ensemble: Decodable, Sendable, Hashable, Identifiable {
        public var model: String
        /// "ECMWF ENS".
        public var short: String
        public var role: String
        /// Members, the control included.
        public var members: UInt32
        /// "Most members: 6–18 in (median 12 in)".
        public var text: String
        public var p10: Quantity
        public var p50: Quantity
        public var p90: Quantity
        public var plume: Plume
        public var id: String { model }
    }

    /// The members' running totals as a chart, in display units: every series has one
    /// value per entry of `times` (`nil` where a member has none).
    public struct Plume: Decodable, Sendable, Hashable {
        /// "Snow".
        public var title: String
        /// "in".
        public var unit: String
        /// Unix seconds, ascending.
        public var times: [Int64]
        /// `[member][time]`.
        public var members: [[Float?]]
        public var p10: [Float?]
        public var p50: [Float?]
        public var p90: [Float?]
        /// The top of the y axis.
        public var yMax: Float
        public var xLabels: [TimeTick]
        public var yLabels: [ValueTick]
    }

    /// A label at a time on a chart's x axis: "Sun".
    public struct TimeTick: Decodable, Sendable, Hashable {
        public var time: Int64
        public var text: String
    }

    /// A label at a value on a chart's y axis: "5 in".
    public struct ValueTick: Decodable, Sendable, Hashable {
        public var value: Float
        public var text: String
    }

    /// One model's totals over its recent runs.
    public struct RunToRun: Decodable, Sendable, Hashable, Identifiable {
        public var model: String
        public var short: String
        public var role: String
        /// "Snow".
        public var title: String
        /// Oldest first; the last is the current run.
        public var bars: [RunBar]
        /// The top of the bars' scale.
        public var max: Float
        public var id: String { model }
    }

    /// One run's bar.
    public struct RunBar: Decodable, Sendable, Hashable {
        /// "Fri 06z".
        public var label: String
        /// The run's start, Unix seconds (`init` in the JSON).
        public var runInit: Int64
        public var value: Float
        /// "11.0 in".
        public var text: String
        /// The run the rest of the screen shows.
        public var current: Bool

        private enum CodingKeys: String, CodingKey {
            case label, runInit = "init", value, text, current
        }
    }

    /// A low's track in one model.
    public struct Track: Decodable, Sendable, Hashable {
        public var model: String
        public var short: String
        public var role: String
        /// The storm at the place; the others are context.
        public var isStorm: Bool
        public var points: [TrackPoint]
    }

    public struct TrackPoint: Decodable, Sendable, Hashable {
        public var latitude: Float
        public var longitude: Float
        public var time: Int64
        /// "978 hPa".
        public var pressure: Quantity
    }

    /// One step of the time scrubber: "Sun 7 PM · +72 h".
    public struct Step: Decodable, Sendable, Hashable {
        public var time: Int64
        public var label: String
    }

    /// An active tropical cyclone from NHC: its advisory, NHC's official forecast
    /// track, the guidance models' tracks, the observed track and the forecast cone.
    public struct Hurricane: Decodable, Sendable, Hashable, Identifiable {
        /// NHC's id: `al102026`.
        public var id: String
        /// "Josephine".
        public var name: String
        /// "Subtropical Storm Josephine", "Hurricane Sample · Category 2".
        public var title: String
        /// NHC's classification code: `HU`, `TS`, `STS`, `PTC`, …
        public var classification: String
        /// The Saffir–Simpson category of a hurricane (1–5).
        public var category: Int?
        /// The core's `Level`, to color it.
        public var level: String
        /// `atlantic`, `east_pacific` or `central_pacific`.
        public var basin: String
        /// The centre now.
        public var latitude: Double
        public var longitude: Double
        /// The maximum sustained wind: "65 mph".
        public var wind: Quantity
        /// The central pressure: "29.18 inHg".
        public var pressure: Quantity
        /// "NE at 13 mph".
        public var movement: String
        /// The wind, pressure and movement in one line: "Winds 65 mph · 29.18 inHg · NE at 13
        /// mph"; `nil` from a core without it.
        public var strength: String?
        /// "Advisory Thu 5:00 PM".
        public var advisory: String
        /// When the advisory was issued, Unix seconds.
        public var advisoryTime: Int64
        /// NHC's advisory page.
        public var advisoryUrl: String?
        /// "The forecast cone reaches New York Sun", when it does.
        public var reaches: String?
        /// NHC's official forecast track, oldest first.
        public var official: [ForecastPoint]
        /// The guidance models' tracks, in display order.
        public var aids: [Aid]
        /// The observed track, oldest first.
        public var past: [MapPoint]
        /// The forecast cone: a counter-clockwise ring, not closed.
        public var cone: [Coordinate]
    }

    /// A point of NHC's official forecast track.
    public struct ForecastPoint: Decodable, Sendable, Hashable {
        public var latitude: Double
        public var longitude: Double
        public var time: Int64
        /// The marker's label: "Thu 2 PM".
        public var label: String
        /// The forecast maximum wind: "75 mph".
        public var wind: Quantity
    }

    /// One guidance model's track for a cyclone.
    public struct Aid: Decodable, Sendable, Hashable, Identifiable {
        /// The ATCF aid code: `AVNI`.
        public var aid: String
        /// "GFS", "UKMET", "Consensus".
        public var short: String
        /// `gfs`, `gem`, `ecmwf`, `ukmet`, `hafs`, `consensus` or `other`: its color and dash.
        public var role: String
        public var points: [MapPoint]
        public var id: String { aid }
    }

    /// A track position at a time.
    public struct MapPoint: Decodable, Sendable, Hashable {
        public var latitude: Double
        public var longitude: Double
        public var time: Int64
    }

    /// A position on the map.
    public struct Coordinate: Decodable, Sendable, Hashable {
        public var latitude: Double
        public var longitude: Double
    }
}

/// The Now tab's "Storm ahead" card from the Rust core (`whether_core::view::StormAheadVm`),
/// as `stormAhead(...)` returns it in `viewJson`: a significant storm coming within days.
/// Every text is already formatted.
public struct StormAhead: Decodable, Sendable, Hashable {
    /// "Storm ahead · Sat night – Sun night".
    public var title: String
    /// "Up to 14 in of snow · models disagree".
    public var line: String
    /// The core's `Level` (the agreement's), to color the card: `poor`, …
    public var level: String
    /// "Low agreement"; `nil` when fewer than two models can be compared.
    public var agreement: String?
    /// The storm's window, Unix seconds.
    public var start: Int64
    public var end: Int64

    /// The NHC id of the hurricane the card warns of (`al102026`); `nil` for a storm
    /// from the models (the core leaves it out then, and earlier cards lack it).
    public var hurricane: String?

    public init(
        title: String, line: String, level: String, agreement: String?, start: Int64, end: Int64,
        hurricane: String? = nil
    ) {
        self.title = title
        self.line = line
        self.level = level
        self.agreement = agreement
        self.start = start
        self.end = end
        self.hurricane = hurricane
    }

    /// Decodes the core's JSON.
    public static func decode(_ json: String) throws -> StormAhead {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return try decoder.decode(StormAhead.self, from: Data(json.utf8))
    }
}
