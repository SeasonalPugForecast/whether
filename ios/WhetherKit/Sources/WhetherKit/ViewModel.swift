import Foundation

/// The display contract from the Rust core (`whether_core::view::ViewModel`), as
/// `viewModel(...)` returns it. Every value is already formatted for the place's
/// units and the settings' clock: draw it, never format it.
///
/// Enum-like fields (`icon`, `level`, `theme`, `tempLevel`) stay strings, so a new
/// value from the core never breaks decoding; `WeatherSymbols` maps them.
public struct ViewModel: Decodable, Sendable, Hashable {
    public var tabs: [Tab]
    public var active: Int
    public var location: LocationHeader
    public var status: Status
    /// The data credit: always shown.
    /// The leading temperatures (big number, hours, highs and lows) are feels-like.
    public var feelsLikeLeads: Bool
    public var attribution: String
    public var theme: String
    public var isDay: Bool
    public var widgets: [String]
    public var current: Current?
    public var summary: String?
    public var hourly: Hourly?
    public var daily: Daily?
    public var details: [Detail]
    public var airQuality: AirQuality?
    public var sun: Sun?
    public var moon: Moon?
    public var nowcast: Nowcast?
    public var alerts: [Alert]
    public var radar: Radar?
    public var precipitation: Precipitation?
    /// The last days as observed, today's normals and records.
    public var history: History?

    /// Decodes the core's JSON.
    public static func decode(_ json: String) throws -> ViewModel {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return try decoder.decode(ViewModel.self, from: Data(json.utf8))
    }

    /// Builds and decodes the view for `locations`, showing `active`.
    public static func build(
        locations: [LocationEntry], active: Int, settings: Settings, now: Date = .now
    ) throws -> ViewModel {
        let json = try viewModel(
            locations: locations, active: UInt32(clamping: active), settings: settings,
            now: Int64(now.timeIntervalSince1970))
        return try decode(json)
    }
}

extension ViewModel {
    /// A formatted number: `text` to show, `value` (display units) to draw with.
    public struct Quantity: Decodable, Sendable, Hashable {
        public var value: Float?
        public var text: String
        public var unit: String
    }

    public struct Tab: Decodable, Sendable, Hashable {
        public var name: String
        public var temp: String
        public var icon: String
    }

    public struct LocationHeader: Decodable, Sendable, Hashable {
        public var name: String
        public var region: String?
        public var localTime: String
    }

    public struct Status: Decodable, Sendable, Hashable {
        /// "Updated 3 min ago".
        public var updated: String
        public var provider: String
        public var stale: Bool
        public var refreshing: Bool
        public var error: String?
        /// "Observed at Central Park, 8:55 AM" when a station measured
        /// the current conditions.
        public var observed: String?
    }

    public struct Current: Decodable, Sendable, Hashable {
        public var temperature: Quantity
        public var tempLevel: String
        public var feelsLike: Quantity
        /// "Feels like 57°".
        public var feelsLikeLabel: String
        public var high: Quantity
        public var low: Quantity
        // `condition` is left out: with precipitation it is an object
        // (`{"rain_showers": "light"}`); `conditionLabel` and `icon` say it.
        public var conditionLabel: String
        public var icon: String
    }

    public struct Wind: Decodable, Sendable, Hashable {
        public var speed: Quantity
        public var gust: Quantity
        public var directionDeg: Float?
        public var compass: String
        /// Points where the wind blows to.
        public var arrow: String
    }

    public struct Hour: Decodable, Sendable, Hashable {
        public var time: Int64
        /// "Now", "1 PM".
        public var label: String
        /// Its day: "Today", "Tomorrow" or the weekday.
        public var day: String
        /// Its date, "Nov 2" (Settings → Dates).
        public var dateShort: String
        public var icon: String
        public var isDay: Bool
        public var temperature: Quantity
        public var tempLevel: String
        public var feelsLike: Quantity
        /// The air temperature, whichever leads.
        public var actual: Quantity
        /// Percent; show it with `chanceText`.
        public var precipProbability: UInt8?
        public var precipitation: Quantity
        public var wind: Wind
        public var humidity: UInt8?
        public var uv: Float?
        /// "62%".
        public var humidityText: String?
        /// "3".
        public var uvText: String?
        /// "Light rain".
        public var conditionLabel: String
        public var dewPoint: Quantity
        public var pressure: Quantity
        public var visibility: Quantity
        public var cloudCover: Quantity
    }

    public struct Hourly: Decodable, Sendable, Hashable {
        public var hours: [Hour]
        public var tempMin: Float
        public var tempMax: Float
        public var precipMax: Float
        public var nowIndex: Int
        /// The first 24 hours as stretches of one kind of weather.
        public var timeline: [TimelineSpan]
    }

    /// Hours `start ..< start + len` of `Hourly.hours`, all one kind of weather.
    public struct TimelineSpan: Decodable, Sendable, Hashable {
        public var start: Int
        public var len: Int
        /// "clear", "partly_cloudy", "cloudy", "fog", "drizzle", "rain",
        /// "heavy_rain", "snow", "storm".
        public var group: String
        /// "Light rain".
        public var label: String
    }

    public struct Day: Decodable, Sendable, Hashable {
        /// "Today", "Tue".
        public var label: String
        public var dateShort: String
        public var icon: String
        public var conditionLabel: String
        public var high: Quantity
        public var low: Quantity
        /// The air temperature's high and low, whichever leads.
        public var actualHigh: Quantity
        public var actualLow: Quantity
        public var tempLevelHigh: String
        public var precipProbability: UInt8?
        public var precipSum: Quantity
        /// Low and high within the period's range, 0…1.
        public var range: [Float]
        public var sunrise: String?
        public var sunset: String?
        public var uvMax: Float?
        public var windMax: Quantity
        /// The strongest gust.
        public var gustMax: Quantity?
        /// "Light drizzle in the early afternoon."
        public var precipText: String?
        /// The day in a forecaster's words: "Cold in the morning, then sunny and
        /// warm in the afternoon. Breezy."
        public var outlook: String?
        /// The day's hours, for expanding it (the app asks for them).
        public var hours: [Hour]
        /// Those hours as stretches of one kind of weather.
        public var timeline: [TimelineSpan]
        /// The date's normal high and low (text "—" without history).
        public var normalHigh: Quantity?
        public var normalLow: Quantity?
    }

    public struct Daily: Decodable, Sendable, Hashable {
        public var days: [Day]
        public var periodMin: Float
        public var periodMax: Float
    }

    public struct Detail: Decodable, Sendable, Hashable {
        public var kind: String
        public var title: String
        public var value: String
        public var caption: String?
        public var level: String?
        /// 0…1 for a gauge.
        public var gauge: Float?
        public var directionDeg: Float?
        public var trend: String?
    }

    public struct AirQuality: Decodable, Sendable, Hashable {
        public var scale: String
        public var index: Int
        public var categoryLabel: String
        public var level: String
        public var gauge: Float
        /// `[name, value]` pairs.
        public var pollutants: [[String]]
    }

    public struct Sun: Decodable, Sendable, Hashable {
        public var sunrise: String
        public var sunset: String
        public var daylight: String
        /// Through the day, 0…1.
        public var progress: Float?
        public var isDay: Bool
        /// Solar noon, "12:47 PM".
        public var solarNoon: String?
        /// The dawns, dusks and golden hours, in time order.
        public var twilight: [SunEvent]
        /// Today's sunrise and sunset with their instants, when the place has them.
        public var rise: RiseSet?
        public var set: RiseSet?
        /// Solar noon, "Solar noon" at "1:15 PM", with its instant.
        public var noon: RiseSet?

        public init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            sunrise = try c.decode(String.self, forKey: .sunrise)
            sunset = try c.decode(String.self, forKey: .sunset)
            daylight = try c.decode(String.self, forKey: .daylight)
            progress = try c.decodeIfPresent(Float.self, forKey: .progress)
            isDay = try c.decode(Bool.self, forKey: .isDay)
            solarNoon = try c.decodeIfPresent(String.self, forKey: .solarNoon)
            twilight = try c.decodeIfPresent([SunEvent].self, forKey: .twilight) ?? []
            rise = try c.decodeIfPresent(RiseSet.self, forKey: .rise)
            set = try c.decodeIfPresent(RiseSet.self, forKey: .set)
            noon = try c.decodeIfPresent(RiseSet.self, forKey: .noon)
        }

        private enum CodingKeys: String, CodingKey {
            case sunrise, sunset, daylight, progress, isDay, solarNoon, twilight, rise, set, noon
        }
    }

    /// A sunrise, sunset, moonrise or moonset: "Sunrise" at "6:49 AM", with its instant.
    public struct RiseSet: Decodable, Sendable, Hashable {
        public var label: String
        public var time: String
        public var at: Int64
    }

    /// One moment of the day's light: "Civil dawn" at "6:49 AM".
    public struct SunEvent: Decodable, Sendable, Hashable {
        public var label: String
        public var time: String
        public var at: Int64
        public var morning: Bool
        /// "astronomical", "nautical", "civil" or "golden".
        public var kind: String
    }

    public struct Moon: Decodable, Sendable, Hashable {
        public var phase: Float
        public var phaseLabel: String
        public var icon: String
        public var moonrise: String?
        public var moonset: String?
        /// "62% illuminated".
        public var illumination: String?
        /// Today's moonrise and moonset with their instants, when the moon has them.
        public var rise: RiseSet?
        public var set: RiseSet?
    }

    public struct Nowcast: Decodable, Sendable, Hashable {
        /// "Rain starting in ~45 min."
        public var text: String
        /// Precipitation per 15 minutes, in millimetres.
        public var seriesMm: [Float]
        /// How hard each step falls, 0…1: a third is light, two thirds moderate.
        public var intensity: [Float]
        /// "Light", "Moderate", "Heavy": the lines at a third, two thirds and the top.
        public var levels: [String]
        /// Time axis labels; step 0 is now, `seriesMm.count` the end.
        public var ticks: [Tick]
        /// Hours covered.
        public var hours: UInt8
        /// Minutes per step: 15, or 1 for an hour of Apple's minutes.
        public var stepMinutes: UInt8
        /// Whether any step has precipitation.
        public var wet: Bool

        public struct Tick: Decodable, Sendable, Hashable {
            public var step: Int
            /// "Now", "30 min", "2 h".
            public var label: String
        }
    }

    public struct Alert: Decodable, Sendable, Hashable, Identifiable {
        public var id: String?
        public var title: String
        public var body: String
        public var level: String
        public var period: String
        public var end: Int64
        /// Who issued it ("NWS Upton NY").
        public var source: String?
        /// A page with the alert's details, when the source gives one.
        public var url: String?

        /// Whether the alert is Apple Weather's (its id starts `apple:`), which
        /// WeatherKit requires crediting.
        public var isApple: Bool { id?.hasPrefix("apple:") == true }
    }

    /// One hour of the precipitation table.
    public struct PrecipHour: Decodable, Sendable, Hashable {
        public var time: Int64
        /// "Now", "3 PM".
        public var label: String
        /// "40%".
        public var chance: String?
        public var chanceValue: UInt8?
        public var amount: Quantity
        /// How hard it falls, 0…1 (1 is heavy rain).
        public var intensity: Float
        /// Snow, sleet or freezing rain.
        public var frozen: Bool
    }

    /// The next hours' rain and snow.
    public struct Precipitation: Decodable, Sendable, Hashable {
        public var hours: [PrecipHour]
        public var total: Quantity
        /// "70%".
        public var maxChance: String?
        /// "No precipitation in the next 12 hours." when every hour is dry.
        public var dry: String?
    }

    /// The history section.
    public struct History: Decodable, Sendable, Hashable {
        /// Observed days, oldest first; days without a report show "—".
        public var days: [PastDay]
        /// Today's normals, records and chance of rain or snow.
        public var today: Climate?
        /// "0.15 in this month, normal 0.60 in".
        public var monthToDate: MonthToDate?
        /// "4° warmer than yesterday".
        public var yesterdayDelta: String?
        /// "NY City Central Park, 4.8 mi · Normals 1991–2020".
        public var credit: String
    }

    public struct PastDay: Decodable, Sendable, Hashable {
        /// "Yesterday", "Sat".
        public var label: String
        /// "Oct 3".
        public var dateShort: String
        public var high: Quantity
        public var low: Quantity
        /// "Trace" when too little to measure.
        public var precipitation: Quantity
        /// The high against normal: "+5°".
        public var departure: Quantity
        public var normalHigh: Quantity
        public var normalLow: Quantity
        /// The low against normal.
        public var lowDeparture: Quantity?
        public var normalPrecip: Quantity?
        /// Snowfall, when any fell.
        public var snow: Quantity?
        /// The date's records: "94° in 1941".
        public var recordHigh: String?
        public var recordLow: String?
        /// "5° warmer than normal. 0.25 in of rain."
        public var summary: String?
    }

    public struct Climate: Decodable, Sendable, Hashable {
        public var normalHigh: Quantity
        public var normalLow: Quantity
        /// "Normal 69° / 55°".
        public var normalText: String
        /// "94° in 1941".
        public var recordHigh: String?
        public var recordLow: String?
        /// "Rain or snow on this date in 23% of years".
        public var precipChance: String?
    }

    public struct MonthToDate: Decodable, Sendable, Hashable {
        public var precipitation: Quantity
        public var normal: Quantity
        public var text: String
    }

    public struct Radar: Decodable, Sendable, Hashable {
        public var frames: [String]
        public var attribution: String
    }
}

/// The vacation planner's view of a trip (`whether_core::view::PlanVm`), as
/// `planView(...)` returns it.
public struct PlanView: Decodable, Sendable, Hashable {
    /// "Jul 12 – Jul 19, as in 1995–2024".
    public var title: String
    public var days: [Day]
    /// "On average 88° / 66°".
    public var average: String?
    /// "Rain or snow on 33% of days".
    public var wetDays: String?
    /// "NY City Central Park, 4.8 mi · 1995–2024".
    public var credit: String
    /// The data credit: always shown.
    public var attribution: String

    public struct Day: Decodable, Sendable, Hashable {
        /// The trip date's weekday: "Mon".
        public var label: String
        /// "Jul 12".
        public var dateShort: String
        /// The typical weather (sun, some cloud, showers or storms, rain, snow).
        public var icon: String
        public var high: ViewModel.Quantity
        public var low: ViewModel.Quantity
        public var precipitation: ViewModel.Quantity
        /// "34%".
        public var precipChance: ViewModel.Quantity
        /// "94° in 2011".
        public var hottest: String?
        public var coldest: String?
        /// The hottest high and coldest low as numbers, for the graph.
        public var highest: ViewModel.Quantity
        public var lowest: ViewModel.Quantity
        /// Average low and high on the trip's scale, 0…1 (-1 unknown).
        public var range: [Float]
        /// Coldest low and hottest high on that scale.
        public var extremes: [Float]
    }

    /// Decodes the core's JSON.
    public static func decode(_ json: String) throws -> PlanView {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return try decoder.decode(PlanView.self, from: Data(json.utf8))
    }
}

extension ViewModel.Hour {
    /// Its day and date as the core wrote them: "Tomorrow, Nov 2".
    public var dayAndDate: String { [day, dateShort].filter { !$0.isEmpty }.joined(separator: ", ") }
}
