import Foundation
import WhetherKit

/// A calendar day: year, month and day, with no time and no time zone. It is
/// what a trip's dates mean, so it reads the same wherever the phone is.
/// Saved as the text "2027-07-12".
nonisolated struct PlanDay: Codable, Hashable, Sendable, Comparable {
    var year: Int
    var month: Int
    var day: Int

    init(year: Int, month: Int, day: Int) {
        self.year = year
        self.month = month
        self.day = day
    }

    /// The day `date` falls on in `calendar`.
    init(_ date: Date, in calendar: Calendar = .current) {
        let c = calendar.dateComponents([.year, .month, .day], from: date)
        self.init(year: c.year ?? 2000, month: c.month ?? 1, day: c.day ?? 1)
    }

    /// The start of this day in `calendar`.
    func date(in calendar: Calendar = .current) -> Date {
        // Noon exists on every day, even where midnight is skipped by the clocks.
        let noon = calendar.date(
            from: DateComponents(year: year, month: month, day: day, hour: 12)) ?? .now
        return calendar.startOfDay(for: noon)
    }

    static func < (a: PlanDay, b: PlanDay) -> Bool {
        (a.year, a.month, a.day) < (b.year, b.month, b.day)
    }

    /// Whether the day is on the Gregorian calendar: month 1–12, and a day the month has
    /// (29 February only in a leap year).
    var exists: Bool {
        guard (1...9999).contains(year), (1...12).contains(month), day >= 1 else { return false }
        let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
        let lengths = [31, leap ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
        return day <= lengths[month - 1]
    }

    /// The days from this one to `other` (negative if `other` is earlier). Counted on the
    /// Gregorian calendar in UTC, so no time zone or clock change moves it.
    func days(to other: PlanDay) -> Int {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = .gmt
        func noon(_ d: PlanDay) -> Date? {
            calendar.date(from: DateComponents(year: d.year, month: d.month, day: d.day, hour: 12))
        }
        guard let a = noon(self), let b = noon(other) else { return 0 }
        return calendar.dateComponents([.day], from: a, to: b).day ?? 0
    }

    init(from decoder: Decoder) throws {
        let text = try decoder.singleValueContainer().decode(String.self)
        let parts = text.split(separator: "-", omittingEmptySubsequences: false)
        guard parts.count == 3, let y = Int(parts[0]), let m = Int(parts[1]), let d = Int(parts[2]),
            PlanDay(year: y, month: m, day: d).exists
        else {
            throw DecodingError.dataCorrupted(
                .init(codingPath: decoder.codingPath, debugDescription: "Not a day: \(text)"))
        }
        self.init(year: y, month: m, day: d)
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.singleValueContainer()
        try c.encode("\(Self.padded(year, 4))-\(Self.padded(month, 2))-\(Self.padded(day, 2))")
    }

    private static func padded(_ n: Int, _ width: Int) -> String {
        let text = String(n)
        return String(repeating: "0", count: max(0, width - text.count)) + text
    }
}

/// A saved trip plan: its name, place, dates and the years to look back over.
struct SavedPlan: Codable, Hashable, Identifiable {
    var id = UUID()
    var name: String
    var place: AppPlace
    /// The first and last day of the trip: calendar days, not instants, so
    /// they don't move when the phone changes time zone.
    var fromDay: PlanDay
    var toDay: PlanDay
    /// Years back from the last whole year (1–50).
    var years = 30
    /// Looked up before: its answer is kept, so it shows when the plan opens.
    var looked = false
    /// How the plan is shared through iCloud; `nil` for a private plan, which is
    /// every plan until someone is invited. Kept by the store and the cloud, never by
    /// the editor (`WeatherStore.update` keeps what is stored).
    var sharing: PlanSharing?

    init(
        id: UUID = UUID(), name: String, place: AppPlace, fromDay: PlanDay, toDay: PlanDay,
        years: Int = 30, looked: Bool = false, sharing: PlanSharing? = nil
    ) {
        self.id = id
        self.name = name
        self.place = place
        self.fromDay = fromDay
        self.toDay = toDay
        self.years = years
        self.looked = looked
        self.sharing = sharing
    }

    /// The start of the first day, in the current calendar; setting it keeps its day.
    var from: Date {
        get { fromDay.date() }
        set { fromDay = PlanDay(newValue) }
    }

    /// The start of the last day, in the current calendar; setting it keeps its day.
    var to: Date {
        get { toDay.date() }
        set { toDay = PlanDay(newValue) }
    }

    /// A plan for `place` a month from now, a week long.
    static func new(at place: AppPlace, number: Int) -> SavedPlan {
        let c = Calendar.current
        let from = c.date(byAdding: .day, value: 30, to: .now) ?? .now
        let to = c.date(byAdding: .day, value: 6, to: from) ?? from
        return SavedPlan(
            name: number == 1 ? "New plan" : "New plan \(number)", place: place,
            fromDay: PlanDay(from, in: c), toDay: PlanDay(to, in: c))
    }

    /// The trip as the core asks it, looking back from `lastYear`.
    func trip(lastYear: Int) -> Trip {
        Trip(
            fromYear: Int32(fromDay.year), fromMonth: UInt8(fromDay.month), fromDay: UInt8(fromDay.day),
            toYear: Int32(toDay.year), toMonth: UInt8(toDay.month), toDay: UInt8(toDay.day),
            firstYear: Int32(lastYear - years + 1), lastYear: Int32(lastYear))
    }

    // MARK: Saving

    /// Decoding a plan saved by an older version, which kept `from` and `to` as
    /// instants, turns them into days in this calendar (the current one if absent).
    nonisolated static let calendarKey = CodingUserInfoKey(rawValue: "planCalendar")!  // INVARIANT: the raw value is not empty.

    private enum CodingKeys: String, CodingKey {
        case id, name, place, fromDay, toDay, years, looked, sharing
        /// Saved by an older version: instants, read once into days.
        case from, to
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let calendar = decoder.userInfo[Self.calendarKey] as? Calendar ?? .current
        func day(_ key: CodingKeys, old: CodingKeys) throws -> PlanDay {
            if let found = try? c.decode(PlanDay.self, forKey: key) { return found }
            return PlanDay(try c.decode(Date.self, forKey: old), in: calendar)
        }
        id = try c.decodeIfPresent(UUID.self, forKey: .id) ?? UUID()
        name = try c.decode(String.self, forKey: .name)
        place = try c.decode(AppPlace.self, forKey: .place)
        fromDay = try day(.fromDay, old: .from)
        toDay = try day(.toDay, old: .to)
        years = try c.decodeIfPresent(Int.self, forKey: .years) ?? 30
        looked = try c.decodeIfPresent(Bool.self, forKey: .looked) ?? false
        sharing = try c.decodeIfPresent(PlanSharing.self, forKey: .sharing)
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(id, forKey: .id)
        try c.encode(name, forKey: .name)
        try c.encode(place, forKey: .place)
        try c.encode(fromDay, forKey: .fromDay)
        try c.encode(toDay, forKey: .toDay)
        try c.encode(years, forKey: .years)
        try c.encode(looked, forKey: .looked)
        // Absent for a private plan, so its file reads as it always did.
        try c.encodeIfPresent(sharing, forKey: .sharing)
    }
}
