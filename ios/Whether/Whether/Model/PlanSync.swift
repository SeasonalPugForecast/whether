import Foundation
import WhetherKit

// The pure half of live shared plans: what a plan remembers about how it is
// shared, how changes from two people merge, how a change from iCloud lands in the
// plan list, and what changed (as data, for the change banner). No CloudKit here: this
// file is compiled into the test target (`AppSources/PlanSync.swift`), and `PlanCloud`
// (app only) turns CloudKit's records into these values.

/// Where a plan's CloudKit record lives, as plain strings (not `CKRecord.ID`, so a
/// `SavedPlan` stays `Codable` and the tests need no CloudKit).
nonisolated struct PlanRecordRef: Codable, Hashable, Sendable {
    /// The zone every shared plan lives in.
    static let zoneName = "Plans"
    /// The record type of a shared plan.
    static let recordType = "Plan"
    /// `CKCurrentUserDefaultName`: the "owner" of a zone in the signed-in person's own
    /// private database.
    static let me = "__defaultOwner__"

    /// The plan's id as text: one record per plan, named after it.
    var recordName: String
    var zoneName: String
    /// Who owns the zone: `me` for the owner's own, else the owner's CloudKit user record name.
    var zoneOwner: String

    init(recordName: String, zoneName: String = Self.zoneName, zoneOwner: String = Self.me) {
        self.recordName = recordName
        self.zoneName = zoneName
        self.zoneOwner = zoneOwner
    }

    /// The record of plan `id` in the signed-in person's own zone.
    static func owned(_ id: UUID) -> PlanRecordRef { PlanRecordRef(recordName: id.uuidString) }

    /// Whether the zone is the signed-in person's own (they are the owner).
    var isMine: Bool { zoneOwner == Self.me }
}

/// How a plan is shared through iCloud, kept in the plan. Only shared plans have it.
nonisolated struct PlanSharing: Codable, Hashable, Sendable {
    enum Role: String, Codable, Sendable {
        /// This person shared the plan (it is in their private database).
        case owner
        /// Someone shared the plan with this person (it is in the shared database).
        case participant
    }

    var role: Role
    /// For "Shared by Robin": the owner's name, when iCloud told it; `nil` shows as "someone".
    var ownerName: String?
    /// What a participant may do: edit (`true`) or only look (`false`, the default). The
    /// owner always can.
    var canEdit: Bool
    var record: PlanRecordRef

    init(role: Role, ownerName: String? = nil, canEdit: Bool, record: PlanRecordRef) {
        self.role = role
        self.ownerName = ownerName
        self.canEdit = role == .owner ? true : canEdit
        self.record = record
    }

    /// A plan this person has just shared.
    static func owned(_ id: UUID) -> PlanSharing {
        PlanSharing(role: .owner, canEdit: true, record: .owned(id))
    }

    var isOwner: Bool { role == .owner }
    /// Whether this person may change the plan's place, dates, name and years.
    var mayEdit: Bool { role == .owner || canEdit }

    private enum CodingKeys: String, CodingKey { case role, ownerName, canEdit, record }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let role = try c.decode(Role.self, forKey: .role)
        self.init(
            role: role, ownerName: try c.decodeIfPresent(String.self, forKey: .ownerName),
            // Read-only unless saved otherwise: the safe side if the key is missing.
            canEdit: try c.decodeIfPresent(Bool.self, forKey: .canEdit) ?? false,
            record: try c.decode(PlanRecordRef.self, forKey: .record))
    }
}

/// Whether live sharing (iCloud, `PlanCloud`) may be offered or touched at all. Creating the
/// container raises an Objective-C exception, which Swift can't catch, in a process without
/// the iCloud entitlement, so every way into `PlanCloud` checks this first
/// (`PlanCloud.isAvailable`, which passes the build's own conditions).
nonisolated enum LiveSharing {
    /// - `simulator`: a simulator build (unsigned: no entitlements).
    /// - `noICloud`: the Swift condition `NO_ICLOUD`, set by `cargo xtask ios remote
    ///   device|archive --no-weatherkit` (signed with the widget's entitlements: no iCloud).
    static func isAvailable(simulator: Bool, noICloud: Bool) -> Bool { !simulator && !noICloud }
}

extension PlanSharing {
    /// "Shared by Robin" (the owner's name, when iCloud told it), else "Shared by someone".
    var sharedBy: String { "Shared by \(ownerName ?? "someone")" }
}

extension SavedPlan {
    /// Whether this person may only look at the plan: shared with them without permission to
    /// change it. Its fields show as text, with "Make my own copy" and "Leave".
    var isReadOnlyForMe: Bool { sharing.map { !$0.mayEdit } ?? false }

    /// A private plan with this one's name, place, dates, years and answer: a new id, and not
    /// shared. What "Make my own copy" saves (the shared plan stays as it is).
    func privateCopy() -> SavedPlan {
        var copy = self
        copy.id = UUID()
        copy.sharing = nil
        return copy
    }
}

/// A person a plan is shared with, as the owner's sharing page lists them.
nonisolated struct PlanParticipant: Equatable, Identifiable, Sendable {
    /// CloudKit's id for the participant (stable across fetches).
    var id: String
    /// Their name as iCloud gives it; `nil` when it doesn't.
    var name: String?
    /// Whether they may edit (`false`: read-only, as invited).
    var canEdit: Bool
    /// Invited and not answered yet.
    var pending: Bool
}

/// The fields of a shared plan that two people can change at the same time. A place and a
/// pair of dates each change as one: half a place (one person's name, the other's
/// coordinates) or half a trip (a first day after the last) would be nonsense.
extension SharedPlan {
    /// The merge of two edits of one plan, field by field, **last writer wins**: `mine` is
    /// the edit being saved now, `theirs` what the server has since got, `base` what this
    /// device last saw on the server.
    ///
    /// Per field (name, place, dates, years): a field `mine` changed from `base` is kept as
    /// `mine` has it, even if `theirs` changed it too (this device saves last); otherwise
    /// the field is `theirs`, which includes a change only they made. `modifiedBy` and
    /// `modifiedAt` come from `mine` when it kept any field, else from `theirs`. So with
    /// nothing changed on this device the result is `theirs`, and with nothing changed by
    /// them it is `mine`.
    static func merged(base: SharedPlan, mine: SharedPlan, theirs: SharedPlan) -> SharedPlan {
        let name = mine.name != base.name
        let place = mine.place != base.place
        let dates = mine.from != base.from || mine.to != base.to
        let years = mine.years != base.years
        let mineKept = name || place || dates || years
        return SharedPlan(
            id: theirs.id, name: name ? mine.name : theirs.name,
            place: place ? mine.place : theirs.place, from: dates ? mine.from : theirs.from,
            to: dates ? mine.to : theirs.to, years: years ? mine.years : theirs.years,
            modifiedBy: mineKept ? mine.modifiedBy : theirs.modifiedBy,
            modifiedAt: mineKept ? mine.modifiedAt : theirs.modifiedAt)
    }

    /// Whether `other` differs in what people see: name, place, dates or years (not who or
    /// when).
    func differs(from other: SharedPlan) -> Bool {
        name != other.name || place != other.place || from != other.from || to != other.to
            || years != other.years
    }
}

extension SavedPlan {
    /// Whether the two plans differ in a shared field (name, place, dates, years). `looked`
    /// and `sharing` are this device's own, so a change only to them is not one.
    func sharedFieldsDiffer(from other: SavedPlan) -> Bool {
        SharedPlan(self, by: "", at: Date(timeIntervalSince1970: 0)).differs(
            from: SharedPlan(other, by: "", at: Date(timeIntervalSince1970: 0)))
    }

    /// This plan after the editor changed it to `edit`: the editor never changes how the
    /// plan is shared (`sharing` stays), and a participant who may only look keeps the
    /// shared fields as they are (only `looked` can change).
    func editing(_ edit: SavedPlan) -> SavedPlan {
        var result = edit
        result.sharing = sharing
        if let sharing, !sharing.mayEdit {
            result = self
            result.looked = edit.looked
        }
        return result
    }

    /// This plan with `shared`, which came from iCloud, applied: the shared fields are
    /// `shared`'s, `looked` stays only if the trip is the same (`SharedPlan.savedPlan`), and
    /// `sharing` is `incoming`, keeping the owner's name if iCloud sent none this time.
    /// `change` says what moved; `nil` when nothing anyone can see did.
    func applying(_ shared: SharedPlan, sharing incoming: PlanSharing) -> (
        plan: SavedPlan, change: PlanChange?
    ) {
        var plan = shared.savedPlan(keeping: self)
        var sharing = incoming
        if sharing.ownerName == nil { sharing.ownerName = self.sharing?.ownerName }
        plan.sharing = sharing
        return (plan, PlanChange(from: self, to: plan, by: shared.modifiedBy))
    }

    /// This plan (the copy on this device **now**) after `sent`, an earlier copy of it, went to
    /// iCloud and `stored` is what iCloud holds afterwards. Normally `sent` is still this plan
    /// and the result is `stored` (their fields where they changed something); when the person
    /// edited the plan **while it was being sent**, `stored` is merged with that edit the way a
    /// save does (`SharedPlan.merged`: `sent` is the base), so what is kept locally is what the
    /// next save will agree with, and the edit is not undone by the next merge.
    ///
    /// - `resend`: the merge still differs from `stored`, so a newer edit must go out. A change
    ///   only to `looked` (this device's own) needs no send.
    /// - `change`: what the merge changed under this person, by whoever `theirs` (the version
    ///   the save merged with, when it conflicted) was written by; `nil` when nothing moved.
    func afterSend(of sent: SavedPlan, stored: SharedPlan, mergedWith theirs: SharedPlan?) -> SendOutcome
    {
        guard let sharing else { return SendOutcome(plan: self, resend: false, change: nil) }
        let epoch = Date(timeIntervalSince1970: 0)
        let merged = SharedPlan.merged(
            base: SharedPlan(sent, by: "", at: epoch), mine: SharedPlan(self, by: "", at: epoch),
            theirs: stored)
        let plan = applying(merged, sharing: sharing).plan
        return SendOutcome(
            plan: plan, resend: merged.differs(from: stored),
            change: PlanChange(from: self, to: plan, by: (theirs ?? stored).modifiedBy))
    }
}

/// What `SavedPlan.afterSend` decided.
struct SendOutcome: Equatable {
    var plan: SavedPlan
    var resend: Bool
    var change: PlanChange?
}

extension PlanSharing {
    /// What to tell the person when iCloud says a plan is not theirs to send any more: they
    /// were removed (a participant), or the plan is gone (the owner's own record).
    func lostText(planName: String) -> String {
        isOwner ? "\(planName) isn't shared any more" : "\(planName) is no longer shared with you"
    }

    /// The plan this sharing is of: the record is named after its id.
    var planID: UUID? { UUID(uuidString: record.recordName) }
}

/// What changed in a plan when someone else's edit arrived: plain data, so the banner
/// and the notification write the sentence ("Robin moved the trip to Jul 12–19").
struct PlanChange: Equatable {
    var planID: UUID
    /// Who made the change (`SharedPlan.modifiedBy`).
    var who: String
    var oldName: String
    var newName: String
    var oldPlace: SharedPlace
    var newPlace: SharedPlace
    var oldFrom: PlanDay
    var oldTo: PlanDay
    var newFrom: PlanDay
    var newTo: PlanDay
    var oldYears: Int
    var newYears: Int

    /// `nil` when `old` and `new` agree on name, place, dates and years.
    init?(from old: SavedPlan, to new: SavedPlan, by who: String) {
        guard new.sharedFieldsDiffer(from: old) else { return nil }
        planID = new.id
        self.who = who
        oldName = old.name
        newName = new.name
        oldPlace = SharedPlace(old.place)
        newPlace = SharedPlace(new.place)
        oldFrom = old.fromDay
        oldTo = old.toDay
        newFrom = new.fromDay
        newTo = new.toDay
        oldYears = old.years
        newYears = new.years
    }

    var renamed: Bool { oldName != newName }
    var movedDates: Bool { oldFrom != newFrom || oldTo != newTo }
    var movedPlace: Bool { oldPlace != newPlace }
    var changedYears: Bool { oldYears != newYears }
    /// Whether the kept answer was for something else (it is cleared): the dates, the place
    /// or the years moved. A rename alone leaves it.
    var clearsAnswer: Bool { movedDates || movedPlace || changedYears }
}

/// The kind of sentence that announces a `PlanChange`, with the parts it is made of.
/// The words themselves ("Robin moved the trip to Jul 12–19") are composed in the app, around
/// the dates text the plan list already writes (`planDates`); this is the decision of
/// which sentence, and what goes in it.
enum PlanChangeKind: Equatable {
    /// Only the dates moved: "Robin moved Summer at the shore to Jul 12–Jul 19, 2027".
    case movedDates(from: PlanDay, to: PlanDay)
    /// Only the place moved: "Robin moved Summer at the shore to Cape May".
    case movedPlace(to: String)
    /// Only the name changed (the plan's old name): "Robin renamed Summer to Beach week".
    case renamed(from: String)
    /// Only the years looked back over changed, or more than one thing did: "Robin changed
    /// Summer at the shore".
    case changed
}

/// One announcement for a plan someone else changed: the banner and the notification show it.
struct PlanChangeNotice: Equatable, Identifiable {
    var planID: UUID
    /// Who changed it; `nil` when iCloud gave no name (it reads "Someone").
    var who: String?
    /// The plan's name now.
    var planName: String
    var kind: PlanChangeKind

    var id: UUID { planID }
}

extension PlanChange {
    /// Which sentence announces this change: one thing moved says what (dates, place, name);
    /// the years alone, or several things at once, just say the plan changed.
    var kind: PlanChangeKind {
        let things = [renamed, movedDates, movedPlace, changedYears].filter { $0 }.count
        guard things == 1 else { return .changed }
        if movedDates { return .movedDates(from: newFrom, to: newTo) }
        if movedPlace { return .movedPlace(to: newPlace.name) }
        if renamed { return .renamed(from: oldName) }
        return .changed
    }

    /// The name to announce: `nil` for "Someone" or no name.
    var speaker: String? {
        let name = who.trimmingCharacters(in: .whitespacesAndNewlines)
        return name.isEmpty || name == SharedPlan.someone ? nil : name
    }

    /// The announcement for this change, or `nil` when `myName` made it (a change this person
    /// made themselves, which came back from iCloud with another device's merge, is not news).
    func notice(ignoring myName: String?) -> PlanChangeNotice? {
        if let myName, !myName.isEmpty, speaker == myName { return nil }
        return PlanChangeNotice(planID: planID, who: speaker, planName: newName, kind: kind)
    }

    /// The announcements for what a sync changed, one per plan (a plan changed twice in one
    /// sync is announced as its last change), in the order the plans first appear, leaving out
    /// what `myName` changed.
    static func notices(for changes: [PlanChange], ignoring myName: String?) -> [PlanChangeNotice] {
        var order: [UUID] = []
        var latest: [UUID: PlanChangeNotice] = [:]
        for change in changes {
            guard let notice = change.notice(ignoring: myName) else {
                // The latest change of the plan is this person's: nothing to say about it.
                latest[change.planID] = nil
                continue
            }
            if latest[change.planID] == nil && !order.contains(change.planID) {
                order.append(change.planID)
            }
            latest[change.planID] = notice
        }
        return order.compactMap { latest[$0] }
    }
}

/// One thing that happened to shared plans in iCloud since the last fetch.
enum CloudChange: Equatable {
    /// A plan is new or changed, with how it is shared now (permission included).
    case updated(SharedPlan, PlanSharing)
    /// The plan's record is gone: it is no longer shared (the owner stopped, or this person
    /// left). The plan stays on this device as a private plan.
    case removed(planID: UUID)
    /// A whole shared zone is gone (the owner stopped sharing every plan in it, or this
    /// person was removed): its plans stay as private plans.
    case zoneRemoved(zoneName: String, zoneOwner: String)
    /// A record that this version of the app can't read (a newer one wrote it).
    case unreadable(recordName: String, SharedPlanError)
}

/// What `PlanSync.apply` did, for the banner and the plan list.
struct AppliedChanges: Equatable {
    /// Plans that changed under this person, with what moved.
    var changed: [PlanChange] = []
    /// Plans that arrived for the first time (shared with this person, or from the owner's
    /// other device).
    var added: [UUID] = []
    /// Plans that are no longer shared and stay as private plans.
    var unshared: [UUID] = []
    /// Records this version can't read (their record names).
    var unreadable: [String] = []
    /// Plans whose update was left out on purpose (an edit not sent yet, or the plan was
    /// deleted here and iCloud still has it): the merge base of each stays as it was.
    var skipped: Set<UUID> = []

    var isEmpty: Bool {
        changed.isEmpty && added.isEmpty && unshared.isEmpty && unreadable.isEmpty
    }
}

enum PlanSync {
    /// `plans` after `changes` from iCloud, and what happened.
    ///
    /// - `.updated` for a plan not on this device adds it (with its sharing); for one that is,
    ///   `SavedPlan.applying` (keeps `looked` only when the trip is unchanged).
    /// - A plan in `holding` has an edit of this device not sent yet: its incoming update is
    ///   left out, and the pending save merges it (`SharedPlan.merged`) when it goes.
    /// - A plan in `ended` was deleted on this device and iCloud may not have heard yet (the
    ///   stop or leave is retried): an update of it is dropped, or it would come back.
    /// - `.removed` and `.zoneRemoved` clear `sharing`: the plan stays, private.
    static func apply(
        _ changes: [CloudChange], to plans: [SavedPlan], holding: Set<UUID> = [],
        ended: Set<UUID> = []
    ) -> (plans: [SavedPlan], applied: AppliedChanges) {
        var plans = plans
        var applied = AppliedChanges()
        for change in changes {
            switch change {
            case .updated(let shared, let sharing):
                if ended.contains(shared.id) {
                    applied.skipped.insert(shared.id)
                    continue
                }
                guard let i = plans.firstIndex(where: { $0.id == shared.id }) else {
                    var plan = shared.savedPlan(keeping: nil)
                    plan.sharing = sharing
                    plans.append(plan)
                    applied.added.append(plan.id)
                    continue
                }
                if holding.contains(shared.id) {
                    applied.skipped.insert(shared.id)
                    continue
                }
                let result = plans[i].applying(shared, sharing: sharing)
                plans[i] = result.plan
                if let change = result.change { applied.changed.append(change) }
            case .removed(let id):
                if let i = plans.firstIndex(where: { $0.id == id }), plans[i].sharing != nil {
                    plans[i].sharing = nil
                    applied.unshared.append(id)
                }
            case .zoneRemoved(let zone, let owner):
                for i in plans.indices {
                    guard let ref = plans[i].sharing?.record, ref.zoneName == zone,
                        ref.zoneOwner == owner
                    else { continue }
                    plans[i].sharing = nil
                    applied.unshared.append(plans[i].id)
                }
            case .unreadable(let name, _):
                applied.unreadable.append(name)
            }
        }
        return (plans, applied)
    }
}

/// Why a shared plan couldn't be sent, fetched or left. `userMessage` shows
/// `friendlyMessage` as the headline.
nonisolated enum PlanCloudError: LocalizedError, FriendlyError, Equatable, Sendable {
    /// Not signed in to iCloud (or iCloud is off or restricted for the app).
    case noAccount
    /// The phone is offline, or iCloud couldn't be reached.
    case offline
    /// The person's iCloud storage is full.
    case quotaExceeded
    /// A participant who may only look tried to change the plan.
    case readOnly
    /// The plan isn't shared, or its record can't be found any more.
    case notShared
    /// Someone else kept changing the plan at the same moment; the retry lost too.
    case conflict
    /// Anything else iCloud refused.
    case unavailable
    /// This build of the app has no iCloud (`LiveSharing.isAvailable`): a simulator build, or
    /// a device build signed without it.
    case notInThisBuild

    var friendlyMessage: String {
        switch self {
        case .noAccount: "Sign in to iCloud to share plans."
        case .offline: "No connection"
        case .quotaExceeded: "Your iCloud storage is full."
        case .readOnly: "You can look at this plan, but its owner hasn't let you change it."
        case .notShared: "This plan isn't shared any more."
        case .conflict: "Someone else changed this plan at the same time. Try again."
        case .unavailable: "iCloud isn't answering right now."
        case .notInThisBuild: "Sharing a plan with other people isn't available in this build of the app."
        }
    }

    var errorDescription: String? { friendlyMessage }
}
