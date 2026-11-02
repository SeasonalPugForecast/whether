import Foundation
import WhetherKit

/// Why a shared plan can't be used. `userMessage` shows `friendlyMessage` as the headline.
nonisolated enum SharedPlanError: LocalizedError, FriendlyError, Equatable, Sendable {
    /// Its `v` isn't one this app reads: a newer version of the app made it.
    case needsNewerVersion
    /// The last day is before the first.
    case datesBackwards
    /// The last day is more than a month after the first: the editor and the lookup stop there.
    case tooLong
    /// Not a plan, or one with a place that can't exist.
    case unreadable
    /// The file couldn't be read at all (not a matter of what it holds).
    case unopenable

    var friendlyMessage: String {
        switch self {
        case .needsNewerVersion: "This plan needs a newer version of the app."
        case .datesBackwards: "This plan's dates are the wrong way round."
        case .tooLong: "This plan is longer than a month, which the app can't look up."
        case .unreadable: "That isn't a plan this app can read."
        case .unopenable: "That file couldn't be opened."
        }
    }

    var errorDescription: String? { friendlyMessage }
}

/// The place of a shared plan: what the other person needs to look it up. Not the
/// local list's `id`, nor whether it follows the phone or is pinned.
struct SharedPlace: Codable, Hashable {
    var name: String
    /// "New York, US".
    var detail: String?
    var latitude: Double
    var longitude: Double
    /// IANA zone.
    var timeZone: String

    init(name: String, detail: String?, latitude: Double, longitude: Double, timeZone: String) {
        self.name = name
        self.detail = detail
        self.latitude = latitude
        self.longitude = longitude
        self.timeZone = timeZone
    }

    /// `place` as saved: its coordinates are not rounded.
    init(_ place: AppPlace) {
        self.init(
            name: place.name, detail: place.detail, latitude: place.latitude,
            longitude: place.longitude, timeZone: place.timeZone)
    }

    /// A place for the local list, with a new id, fixed and not pinned.
    var appPlace: AppPlace {
        AppPlace(
            name: name, detail: detail, latitude: latitude, longitude: longitude,
            timeZone: timeZone)
    }
}

/// Where a plan that was shared stands among the plans on this device.
nonisolated enum PlanReceipt: Equatable, Sendable {
    /// No plan has its id.
    case new
    /// A plan with its id has the same name, place, dates and years.
    case same
    /// A plan with its id has something different.
    case differs
}

/// What accepting a shared plan does when a plan with its id is already there.
nonisolated enum ReceiveChoice: Equatable, Sendable {
    /// Add it; as a copy if that plan differs.
    case add
    /// Add it as a copy with a new id.
    case copy
    /// Replace that plan (its place and kept answer stay when only the name differs).
    case replace
}

/// A plan as it travels between people: a `.whetherplan` file or a CloudKit
/// record. Version 1 is JSON with these keys:
/// `v`, `id`, `name`, `place`, `from`, `to` (days, "2027-07-12"), `years`, `modifiedBy`
/// and `modifiedAt` (ISO 8601). Not in it: `looked` and the kept answers, which each
/// device looks up for itself.
///
/// It lives in the app's Model folder, not in `Shared/`, because it turns into a
/// `SavedPlan`, which the widget doesn't compile.
struct SharedPlan: Codable, Hashable, Identifiable {
    /// The payload version this app reads and writes.
    nonisolated static let version = 1
    /// `modifiedBy` in a copy: nobody's name is known.
    nonisolated static let someone = "Someone"
    /// The most days a trip runs after its first day: what the Plan editor allows.
    nonisolated static let longestTrip = 30

    var id: UUID
    var name: String
    var place: SharedPlace
    var from: PlanDay
    var to: PlanDay
    /// Years back from the last whole year (1–50).
    var years: Int
    var modifiedBy: String
    var modifiedAt: Date

    init(
        id: UUID, name: String, place: SharedPlace, from: PlanDay, to: PlanDay, years: Int,
        modifiedBy: String, modifiedAt: Date
    ) {
        self.id = id
        self.name = name
        self.place = place
        self.from = from
        self.to = to
        self.years = Self.clamped(years)
        self.modifiedBy = Self.person(modifiedBy)
        self.modifiedAt = Self.wholeSeconds(modifiedAt)
    }

    /// `plan` as `name` changed it at `date`; `Someone` when there is no name.
    init(_ plan: SavedPlan, by name: String, at date: Date = .now) {
        self.init(
            id: plan.id, name: plan.name, place: SharedPlace(plan.place), from: plan.fromDay,
            to: plan.toDay, years: plan.years, modifiedBy: name, modifiedAt: date)
    }

    /// Whether `local` has this plan's place, dates and years.
    private func sameTrip(as local: SavedPlan) -> Bool {
        SharedPlace(local.place) == place && local.fromDay == from && local.toDay == to
            && local.years == years
    }

    /// The plan to save on this device. It keeps the shared `id`. When `local` (this
    /// device's copy) has the same place, dates and years, it keeps `local`'s place
    /// and `looked`; otherwise the place is a new one and `looked` is `false`, since
    /// the kept answer was for other dates.
    func savedPlan(keeping local: SavedPlan?) -> SavedPlan {
        let unchanged = local.map(sameTrip(as:)) ?? false
        return SavedPlan(
            id: id, name: name, place: unchanged ? (local?.place ?? place.appPlace) : place.appPlace,
            fromDay: from, toDay: to, years: years, looked: unchanged && (local?.looked ?? false))
    }

    // MARK: Receiving

    /// The sheet's title: who sent it, or that nobody's name is known.
    var headline: String { Self.headline(from: modifiedBy) }

    /// The sheet's title for a plan from `name` (`nil` or "Someone" when nobody's name is known).
    static func headline(from name: String?) -> String {
        guard let name, !name.isEmpty, name != someone else { return "A plan was shared with you" }
        return "\(name) shared a plan"
    }

    /// Where this plan stands among `plans`: not there, there just the same (name, place,
    /// dates and years), or there with something different.
    func receipt(among plans: [SavedPlan]) -> PlanReceipt {
        guard let mine = plans.first(where: { $0.id == id }) else { return .new }
        return mine.name == name && sameTrip(as: mine) ? .same : .differs
    }

    /// `plans` after accepting this plan as `choice` says, and the plan to open. A new
    /// plan is added and one already there just the same is only opened; one with the same
    /// id and something different is added as a copy with a new id, unless `.replace` (which
    /// keeps the plan's `sharing`, and for a plan this person may only look at is a copy too).
    /// Nothing already in `plans` is lost except by `.replace`.
    func accepted(_ choice: ReceiveChoice, into plans: [SavedPlan]) -> (plans: [SavedPlan], open: UUID) {
        var plans = plans
        let index = plans.firstIndex { $0.id == id }
        switch (receipt(among: plans), choice) {
        case (.new, _):
            plans.append(savedPlan(keeping: nil))
            return (plans, id)
        case (.same, _):
            return (plans, id)
        case (.differs, .replace) where canReplace(among: plans):
            // A plan that is shared stays shared: the editor's rule (`SavedPlan.editing`) keeps
            // `sharing`.
            if let index { plans[index] = plans[index].editing(savedPlan(keeping: plans[index])) }
            return (plans, id)
        case (.differs, _):
            // `.add` and `.copy`, and `.replace` of a plan this person may only look at (it is
            // not theirs to replace): a copy with a new id.
            var copy = savedPlan(keeping: nil)
            copy.id = UUID()
            plans.append(copy)
            return (plans, copy.id)
        }
    }

    /// Whether "Replace mine" is offered: a plan with this id is there and this person may
    /// change it (not a plan shared with them to look at, which only a copy can be added
    /// next to).
    func canReplace(among plans: [SavedPlan]) -> Bool {
        guard let mine = plans.first(where: { $0.id == id }) else { return false }
        return !mine.isReadOnlyForMe
    }

    /// For `.replace` of a plan that is **shared** (and this person may edit): the plan to
    /// hand to `WeatherStore.update`, which keeps its `sharing` and sends the change to iCloud
    /// like any other edit. `nil` for everything else (`accepted` does that).
    func replacementForSharedPlan(_ choice: ReceiveChoice, among plans: [SavedPlan]) -> SavedPlan? {
        guard choice == .replace, receipt(among: plans) == .differs,
            let mine = plans.first(where: { $0.id == id }), mine.sharing?.mayEdit == true
        else { return nil }
        return savedPlan(keeping: mine)
    }

    // MARK: File

    /// The file a copy travels as: the name without what a file name can't hold, and
    /// `.whetherplan`.
    var fileName: String { Self.fileStem(name) + ".whetherplan" }

    /// `name` safe as a file name: separators become "-", control characters spaces, no
    /// leading dots, at most 60 characters, "Plan" when nothing is left.
    static func fileStem(_ name: String) -> String {
        let separators = CharacterSet(charactersIn: "/\\:*?\"<>|")
        let blanks = CharacterSet.controlCharacters.union(.newlines)
        var scalars = String.UnicodeScalarView()
        for scalar in name.unicodeScalars {
            if separators.contains(scalar) {
                scalars.append("-")
            } else if blanks.contains(scalar) {
                scalars.append(" ")
            } else {
                scalars.append(scalar)
            }
        }
        func trimmed(_ text: String) -> String {
            var text = text.trimmingCharacters(in: .whitespacesAndNewlines)
            while text.hasPrefix(".") { text.removeFirst() }
            return text.trimmingCharacters(in: .whitespacesAndNewlines)
        }
        let stem = trimmed(String(trimmed(String(scalars)).prefix(60)))
        return stem.isEmpty ? "Plan" : stem
    }

    // MARK: Data

    /// The JSON writer: sorted keys, dates as ISO 8601.
    nonisolated static var encoder: JSONEncoder {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        encoder.dateEncodingStrategy = .iso8601
        return encoder
    }

    /// The JSON reader: dates as ISO 8601 (whole seconds).
    nonisolated static var decoder: JSONDecoder {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return decoder
    }

    /// The plan as the JSON that is shared.
    func data() throws -> Data {
        try Self.encoder.encode(self)
    }

    /// A plan from shared JSON. Throws `SharedPlanError`: a newer version, backwards
    /// dates, a trip over a month long, or something that isn't a plan.
    init(data: Data) throws {
        do {
            self = try Self.decoder.decode(Self.self, from: data)
        } catch is DecodingError {
            throw SharedPlanError.unreadable
        }
    }

    /// Whether `file` lies inside `inbox`, the folder where the system copies files opened
    /// from other apps (`Documents/Inbox`). Compared by resolved paths, so a link out of
    /// the folder, or a file beside it, is not inside. The folder itself is not.
    nonisolated static func isInside(_ file: URL, inbox: URL) -> Bool {
        let folder = inbox.resolvingSymlinksInPath().standardizedFileURL.pathComponents
        let path = file.resolvingSymlinksInPath().standardizedFileURL.pathComponents
        return path.count > folder.count && Array(path.prefix(folder.count)) == folder
    }

    private static func clamped(_ years: Int) -> Int { min(max(years, 1), 50) }

    /// ISO 8601 text keeps whole seconds only: so does the date here, or a copy
    /// wouldn't equal the original.
    private static func wholeSeconds(_ date: Date) -> Date {
        Date(timeIntervalSince1970: date.timeIntervalSince1970.rounded(.down))
    }

    private static func person(_ name: String) -> String {
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        return name.isEmpty ? someone : name
    }

    // MARK: Coding

    private enum CodingKeys: String, CodingKey {
        case v, id, name, place, from, to, years, modifiedBy, modifiedAt
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        // First of all: a newer version may keep its fields elsewhere.
        guard try c.decode(Int.self, forKey: .v) == Self.version else {
            throw SharedPlanError.needsNewerVersion
        }
        let place = try c.decode(SharedPlace.self, forKey: .place)
        guard (-90.0...90.0).contains(place.latitude), (-180.0...180.0).contains(place.longitude)
        else { throw SharedPlanError.unreadable }
        let from = try c.decode(PlanDay.self, forKey: .from)
        let to = try c.decode(PlanDay.self, forKey: .to)
        guard from <= to else { throw SharedPlanError.datesBackwards }
        guard from.days(to: to) <= Self.longestTrip else { throw SharedPlanError.tooLong }
        self.init(
            id: try c.decode(UUID.self, forKey: .id), name: try c.decode(String.self, forKey: .name),
            place: place, from: from, to: to,
            years: try c.decode(Int.self, forKey: .years),
            modifiedBy: try c.decodeIfPresent(String.self, forKey: .modifiedBy) ?? Self.someone,
            modifiedAt: try c.decode(Date.self, forKey: .modifiedAt))
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(Self.version, forKey: .v)
        try c.encode(id, forKey: .id)
        try c.encode(name, forKey: .name)
        try c.encode(place, forKey: .place)
        try c.encode(from, forKey: .from)
        try c.encode(to, forKey: .to)
        try c.encode(years, forKey: .years)
        try c.encode(modifiedBy, forKey: .modifiedBy)
        try c.encode(modifiedAt, forKey: .modifiedAt)
    }
}
