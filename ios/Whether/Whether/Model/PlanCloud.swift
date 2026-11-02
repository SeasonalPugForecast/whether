import CloudKit
import Foundation
import WhetherKit

/// Shared plans in iCloud: the CloudKit half of live sharing. The pure half
/// (what a plan remembers, the merge, applying changes, `CloudChange`) is `PlanSync.swift`.
///
/// **Never create one at launch.** `CKContainer(identifier:)` raises an Objective-C
/// exception, which Swift can't catch, in a process without the iCloud entitlement: the
/// simulator, the unit tests, and `--no-weatherkit` device builds (the widget's
/// entitlements). So `WeatherStore` creates it only when a plan is shared or the person
/// starts sharing one (`planCloud()`), and a normal launch, a screenshot run and the tests
/// never touch CloudKit. Anything that offers sharing should offer it only where iCloud
/// can work (`isSignedIn`, which doesn't touch CloudKit).
///
/// **Where things live.**
/// - The owner's private database, zone `Plans`: one record `Plan` per shared plan, named
///   after the plan's id, with a per-record `CKShare` (title = the plan's name, no public
///   access: people are invited). Private plans never go to iCloud (they stay in plans.json).
/// - A participant's plan lives in the owner's zone, which shows in their shared database.
///
/// **The record** has two fields: `payload`, the `SharedPlan` v1 JSON as text, and `v`, its
/// version as a number. One payload field (not a field per key) keeps the CloudKit schema
/// fixed while `SharedPlan` grows, and the app that can't read a newer `v` says so (it
/// checks `v` before the payload) instead of half-reading it. The text is readable in the
/// CloudKit dashboard.
///
/// **Bookkeeping.** Per record this keeps the server's change tag (the record's system
/// fields) and the last payload this device reconciled (the "base" of the merge), in
/// `plan-cloud.json` next to plans.json. Change tokens are in the App Group defaults.
@MainActor
final class PlanCloud {
    /// What a record holds, and the keys of its two fields.
    private nonisolated enum Field {
        static let payload = "payload"
        static let version = "v"
    }

    /// The CloudKit record of a shared plan, with the server's change tag, and the payload
    /// this device last saw there.
    private struct Entry: Codable {
        /// `CKRecord.encodeSystemFields`.
        var systemFields: Data
        var base: SharedPlan
    }

    /// `iCloud.<bundle id>`, the container the entitlements name.
    let container: CKContainer
    /// The name written into `modifiedBy` when this device edits a plan. `nil` is "Someone":
    /// iCloud gives a name only to a person who is discoverable. It is learned from the
    /// shares this person is in (`learnName`) and kept in the defaults.
    var myName: String? {
        didSet { defaults.set(myName, forKey: Self.nameKey) }
    }

    private let directory: URL
    private let defaults: UserDefaults
    private var entries: [String: Entry]
    private var zoneReady = false

    /// What a fetch learned and has not kept yet: change tokens, and the record (change tag and
    /// base) of each plan that came. `commit(skipping:)` keeps them once the changes were
    /// applied, so a plan whose update was left out (an edit of this device that is not sent
    /// yet) keeps its old base, and a fetch that was never applied is made again.
    private struct Staged {
        var entries: [String: Entry] = [:]
        var tokens: [String: CKServerChangeToken?] = [:]
        var forgotten: Set<String> = []

        mutating func merge(_ other: Staged) {
            entries.merge(other.entries) { _, new in new }
            tokens.merge(other.tokens) { _, new in new }
            forgotten.formUnion(other.forgotten)
        }
    }
    private var staged = Staged()

    private static let nameKey = "planCloudMyName"
    private static let tokensKey = "planCloudTokens"
    private static let privateToken = "private:Plans"
    private static let sharedDatabaseToken = "shared-db"

    /// Creates the container: see the type's note. `directory` and `defaults` are where the
    /// bookkeeping is kept.
    init(
        directory: URL = SharedContainer.directory, defaults: UserDefaults = SharedContainer.defaults
    ) {
        // INVARIANT: only reached where the iCloud entitlement exists (see the note above).
        container = CKContainer(identifier: "iCloud." + (Bundle.main.bundleIdentifier ?? ""))
        self.directory = directory
        self.defaults = defaults
        myName = defaults.string(forKey: Self.nameKey)
        entries =
            (try? Data(contentsOf: directory.appending(path: "plan-cloud.json")))
            .flatMap { try? SharedPlan.decoder.decode([String: Entry].self, from: $0) } ?? [:]
    }

    /// Whether this build can use iCloud at all: not a simulator build, and not a device build
    /// made `--no-weatherkit` (the Swift condition `NO_ICLOUD`, which `cargo xtask ios remote`
    /// sets there). **Check it before anything that could create a `PlanCloud`**; where it is
    /// `false` the app offers no live sharing (copies as `.whetherplan` files still work).
    nonisolated static var isAvailable: Bool {
        #if targetEnvironment(simulator)
            let simulator = true
        #else
            let simulator = false
        #endif
        #if NO_ICLOUD
            let noICloud = true
        #else
            let noICloud = false
        #endif
        return LiveSharing.isAvailable(simulator: simulator, noICloud: noICloud)
    }

    // MARK: Sharing

    /// Starts sharing `plan`: saves it as a record in the private zone with a share for it
    /// (people are added by invitation, e.g. `UICloudSharingController(share:container:)`),
    /// and returns the share. A plan already shared gets its existing share back. The caller
    /// keeps `PlanSharing.owned(plan.id)` in the plan.
    func share(_ plan: SavedPlan) async throws -> CKShare {
        try await guarded {
            try await requireAccount()
            try await ensureZone()
            let ref = PlanRecordRef.owned(plan.id)
            let id = recordID(ref)
            let existing = try await fetchRecord(id, in: privateDB)
            if let shareID = existing?.share?.recordID,
                let share = try await fetchRecord(shareID, in: privateDB) as? CKShare
            {
                learnName(from: share)
                return share
            }
            let payload = SharedPlan(plan, by: name)
            let record = existing ?? CKRecord(recordType: PlanRecordRef.recordType, recordID: id)
            try fill(record, with: payload)
            let share = CKShare(rootRecord: record)
            share[CKShare.SystemFieldKey.title] = plan.name as CKRecordValue
            share.publicPermission = .none
            let saved = try await modify(privateDB, saving: [record, share], deleting: [])
            remember(saved[id] ?? record, base: payload)
            let result = (saved[share.recordID] as? CKShare) ?? share
            learnName(from: result)
            return result
        }
    }

    /// The share of a plan this person shared, for the sharing controller. Throws
    /// `.notShared` if its record is gone.
    func existingShare(for plan: SavedPlan) async throws -> CKShare {
        try await guarded {
            try await requireAccount()
            guard let ref = plan.sharing?.record else { throw PlanCloudError.notShared }
            let db = database(for: ref)
            guard let record = try await fetchRecord(recordID(ref), in: db),
                let shareID = record.share?.recordID,
                let share = try await fetchRecord(shareID, in: db) as? CKShare
            else { throw PlanCloudError.notShared }
            learnName(from: share)
            return share
        }
    }

    /// The people `plan` is shared with, for the owner's sharing page.
    func participants(of plan: SavedPlan) async throws -> [PlanParticipant] {
        let share = try await existingShare(for: plan)
        return share.participants.filter { $0.role != .owner }.map { person in
            PlanParticipant(
                id: person.participantID, name: Self.name(of: person),
                canEdit: person.permission == .readWrite, pending: person.acceptanceStatus == .pending)
        }
    }

    /// Lets one person edit `plan` or only look at it (the owner's "Can edit" switch).
    func setCanEdit(_ canEdit: Bool, for participant: String, in plan: SavedPlan) async throws {
        try await guarded {
            guard plan.sharing?.isOwner == true else { throw PlanCloudError.readOnly }
            let share = try await existingShare(for: plan)
            guard let person = share.participants.first(where: { $0.participantID == participant })
            else { throw PlanCloudError.notShared }
            person.permission = canEdit ? .readWrite : .readOnly
            _ = try await modify(privateDB, saving: [share], deleting: [])
        }
    }

    /// Ends sharing (the owner): deletes the share and the record, so nobody sees the plan
    /// any more. The plan stays on this device as a private plan: the caller clears its
    /// `sharing`.
    func stopSharing(_ sharing: PlanSharing) async throws {
        try await guarded {
            guard sharing.isOwner else { throw PlanCloudError.notShared }
            try await requireAccount()
            let id = recordID(sharing.record)
            if let record = try await fetchRecord(id, in: privateDB) {
                let ids = [record.share?.recordID, id].compactMap { $0 }
                _ = try await modify(privateDB, saving: [], deleting: ids)
            }
            forget(sharing.record.recordName)
        }
    }

    /// Leaves a plan shared with this person (a participant): deletes the share from the
    /// shared database. The plan stays on this device as a private copy: the caller clears
    /// its `sharing`.
    func leave(_ sharing: PlanSharing) async throws {
        try await guarded {
            guard !sharing.isOwner else { throw PlanCloudError.notShared }
            try await requireAccount()
            if let shareID = try await fetchRecord(recordID(sharing.record), in: sharedDB)?.share?.recordID {
                _ = try await modify(sharedDB, saving: [], deleting: [shareID])
            }
            forget(sharing.record.recordName)
        }
    }

    /// Accepts an invitation the system handed to the app
    /// (`windowScene(_:userDidAcceptCloudKitShareWith:)`). The plan then arrives with the
    /// next `fetchChanges`.
    func accept(_ metadata: CKShare.Metadata) async throws {
        try await guarded {
            try await requireAccount()
            _ = try await container.accept(metadata)
        }
    }

    /// The plan inside an invitation, to show before accepting it: from the metadata's root
    /// record when the system sent it, else fetched (`shouldFetchRootRecord`). `nil` when it
    /// can't be read (offline, or a newer version wrote it): the sheet then says only that a
    /// plan was shared.
    func preview(_ metadata: CKShare.Metadata) async -> SharedPlan? {
        if let record = metadata.rootRecord, let plan = try? payload(of: record) { return plan }
        guard let url = metadata.share.url,
            let record = await fetchRootRecord(of: url, in: container)
        else { return nil }
        return try? payload(of: record)
    }

    /// The root record of the share at `url`, which an invitation's metadata may come without
    /// (it is only fetched when asked for: `shouldFetchRootRecord`).
    private nonisolated func fetchRootRecord(of url: URL, in container: CKContainer) async -> CKRecord? {
        let found = Found()
        await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
            let operation = CKFetchShareMetadataOperation(shareURLs: [url])
            operation.shouldFetchRootRecord = true
            operation.rootRecordDesiredKeys = [Field.payload, Field.version]
            operation.perShareMetadataResultBlock = { _, result in
                if case .success(let metadata) = result { found.record = metadata.rootRecord }
            }
            operation.fetchShareMetadataResultBlock = { _ in continuation.resume() }
            container.add(operation)
        }
        return found.record
    }

    /// Where `fetchRootRecord`'s callbacks leave the record (they run one after the other).
    private nonisolated final class Found: @unchecked Sendable {
        var record: CKRecord?
    }

    /// Takes this person's own name from a share they are in (as owner or as invited person),
    /// when iCloud gives one, for `modifiedBy`.
    private func learnName(from share: CKShare) {
        if let name = Self.name(of: share.currentUserParticipant), name != myName { myName = name }
    }

    // MARK: Subscriptions

    private static let subscribedKey = "planCloudSubscribed"
    private static let privateSubscriptionID = "plans-private"
    private static let sharedSubscriptionID = "plans-shared"

    /// Makes sure iCloud pushes to this device when a shared plan changes: a database
    /// subscription on the private database (plans this person shared, changed on another
    /// device or by someone allowed to edit) and one on the shared database (plans shared with
    /// them), both silent (`shouldSendContentAvailable`: the app wakes and fetches, and the
    /// app says what changed, not iCloud).
    ///
    /// Created once: `planCloudSubscribed` in the App Group defaults remembers it, and the
    /// subscriptions are saved without asking first. After that, a call checks that they
    /// are still there and saves again any iCloud lost (`CKError.unknownItem`: it was
    /// deleted, or the account was reset). The store calls it once per launch.
    func ensureSubscriptions() async throws {
        try await guarded {
            try await requireAccount()
            let known = defaults.bool(forKey: Self.subscribedKey)
            try await ensureSubscription(Self.privateSubscriptionID, in: privateDB, verify: known)
            try await ensureSubscription(Self.sharedSubscriptionID, in: sharedDB, verify: known)
            defaults.set(true, forKey: Self.subscribedKey)
        }
    }

    private func ensureSubscription(
        _ id: CKSubscription.ID, in db: CKDatabase, verify: Bool
    ) async throws {
        if verify {
            do {
                _ = try await db.subscription(for: id)
                return
            } catch let error as CKError where error.code == .unknownItem {
                // Lost: saved again below.
            }
        }
        let subscription = CKDatabaseSubscription(subscriptionID: id)
        let info = CKSubscription.NotificationInfo()
        // Silent: no alert, sound or badge, only a wake-up for the fetch.
        info.shouldSendContentAvailable = true
        subscription.notificationInfo = info
        let result = try await db.modifySubscriptions(saving: [subscription], deleting: [])
        for (_, saved) in result.saveResults { _ = try saved.get() }
    }

    // MARK: Saving

    /// What `save` leaves: the plan iCloud holds now, and the version it was merged with.
    struct Saved {
        /// What iCloud holds after the save (this edit, with their fields where they changed
        /// something this edit didn't).
        var stored: SharedPlan
        /// The version found on the server when someone had changed the plan first (`nil` when
        /// nobody had): who wrote it is who changed the plan under this person.
        var mergedWith: SharedPlan?
    }

    /// Sends an edit of `plan` by the owner or a participant who may edit, and returns what
    /// the server holds afterwards.
    ///
    /// The save is `.ifServerRecordUnchanged`. If someone changed the record first, the
    /// server's version is fetched and merged with this edit (`SharedPlan.merged`: each field
    /// this device changed from what it last saw keeps this device's value, the others are
    /// theirs) and saved once more; losing that race too is `.conflict`. The result may then
    /// differ from `plan` (their fields): the caller applies it (`SavedPlan.afterSend`).
    @discardableResult
    func save(_ plan: SavedPlan) async throws -> Saved {
        guard let sharing = plan.sharing else { throw PlanCloudError.notShared }
        guard sharing.mayEdit else { throw PlanCloudError.readOnly }
        return try await guarded {
            try await requireAccount()
            let db = database(for: sharing.record)
            let id = recordID(sharing.record)
            let mine = SharedPlan(plan, by: name)
            var (record, base) = try await working(id, in: db)
            var toSave = mine
            var mergedWith: SharedPlan?
            for attempt in 0..<2 {
                try fill(record, with: toSave)
                do {
                    let saved = try await modify(db, saving: [record], deleting: [])
                    remember(saved[id] ?? record, base: toSave)
                    return Saved(stored: toSave, mergedWith: mergedWith)
                } catch let error as CKError where error.code == .serverRecordChanged && attempt == 0 {
                    guard let server = error.serverRecord else { throw error }
                    let theirs = try payload(of: server)
                    toSave = .merged(base: base, mine: mine, theirs: theirs)
                    mergedWith = theirs
                    record = server
                    base = theirs
                    if !toSave.differs(from: theirs) {
                        // Nothing of this edit is left: theirs is the plan.
                        remember(server, base: theirs)
                        return Saved(stored: theirs, mergedWith: theirs)
                    }
                }
            }
            throw PlanCloudError.conflict
        }
    }

    /// A plan as iCloud has it now, with how it is shared (permission included), for a person
    /// whose save was refused: their permission changed, so what they edited is not what is
    /// kept. `nil` when its record is gone. Remembers the record as the new base.
    func current(_ sharing: PlanSharing) async throws -> CloudChange? {
        try await guarded {
            try await requireAccount()
            let db = database(for: sharing.record)
            guard let record = try await fetchRecord(recordID(sharing.record), in: db) else {
                return nil
            }
            let payload = try self.payload(of: record)
            let now = try await sharingState(of: record, in: db, sharesInBatch: [:])
            remember(record, base: payload)
            return .updated(payload, now)
        }
    }

    // MARK: Fetching

    /// What a fetch found. A failure of one database doesn't hide what the other gave: the
    /// changes are applied, then the failure is shown.
    struct Fetched {
        var changes: [CloudChange] = []
        var failure: Error?
    }

    /// What changed in iCloud since the last fetch, in both databases: the owner's private
    /// zone (plans this person shared, including from another device) and the shared
    /// database (plans shared with this person, after their zones' changes).
    ///
    /// Nothing is kept until `commit(skipping:)`, which the caller makes after applying the
    /// changes. A database that failed keeps its token (it is fetched again next time) and
    /// gives no changes; the other is committed. Throws only when there is no account.
    ///
    /// A change token iCloud has expired restarts that zone from the beginning, which misses
    /// deletions, so a plan removed in that gap is found only when its record is fetched
    /// next. A private zone that doesn't exist (never made, or deleted) is no private changes.
    func fetchChanges() async throws -> Fetched {
        try await guarded {
            try await requireAccount()
            var fetched = Fetched()
            var staged = Staged()
            do {
                var part = Staged()
                fetched.changes += try await privateChanges(&part)
                staged.merge(part)
            } catch {
                fetched.failure = Self.friendly(error)
            }
            do {
                var part = Staged()
                fetched.changes += try await sharedChanges(&part)
                staged.merge(part)
            } catch {
                fetched.failure = fetched.failure ?? Self.friendly(error)
            }
            self.staged = staged
            return fetched
        }
    }

    /// Keeps what the last fetch learned: the change tokens, and the record and base of each
    /// plan that came, **except** the plans in `skipping` (updates the app didn't apply: an
    /// edit not sent yet, or a deleted plan). Their base stays what this device last saw, so
    /// the save that follows merges with a base the local copy knew.
    func commit(skipping: Set<UUID> = []) {
        let left = Set(skipping.map(\.uuidString))
        for (key, token) in staged.tokens { setToken(token, for: key) }
        for (name, entry) in staged.entries where !left.contains(name) { entries[name] = entry }
        for name in staged.forgotten { entries[name] = nil }
        staged = Staged()
        persist()
    }

    private func privateChanges(_ staged: inout Staged) async throws -> [CloudChange] {
        guard
            let fetched = try await zoneChanges(
                privateDB, zone: zoneID, key: Self.privateToken, tokens: &staged.tokens)
        else { return [] }
        return try await changes(from: fetched, in: privateDB, staged: &staged)
    }

    private func sharedChanges(_ staged: inout Staged) async throws -> [CloudChange] {
        var token = savedToken(Self.sharedDatabaseToken)
        var changed = Set<CKRecordZone.ID>()
        var gone = Set<CKRecordZone.ID>()
        while true {
            do {
                let result = try await sharedDB.databaseChanges(since: token)
                changed.formUnion(result.modifications.map(\.zoneID))
                gone.formUnion(result.deletions.map(\.zoneID))
                token = result.changeToken
                if !result.moreComing { break }
            } catch let error as CKError where error.code == .changeTokenExpired && token != nil {
                token = nil
                changed = []
                gone = []
            }
        }
        staged.tokens[Self.sharedDatabaseToken] = token
        var out: [CloudChange] = []
        for zone in gone where zone.zoneName == PlanRecordRef.zoneName {
            out.append(.zoneRemoved(zoneName: zone.zoneName, zoneOwner: zone.ownerName))
            staged.tokens[Self.zoneKey(zone)] = .some(nil)
        }
        for zone in changed.subtracting(gone) where zone.zoneName == PlanRecordRef.zoneName {
            guard
                let fetched = try await zoneChanges(
                    sharedDB, zone: zone, key: Self.zoneKey(zone), tokens: &staged.tokens)
            else { continue }
            out += try await changes(from: fetched, in: sharedDB, staged: &staged)
        }
        return out
    }

    /// Whether `error` says a zone isn't there: it was never made, or the person deleted it.
    /// A batch's `.partialFailure` that holds only such errors counts.
    nonisolated static func isZoneGone(_ error: CKError) -> Bool {
        switch error.code {
        case .zoneNotFound, .userDeletedZone: return true
        case .partialFailure:
            let inner = (error.partialErrorsByItemID ?? [:]).values.compactMap { $0 as? CKError }
            return !inner.isEmpty && inner.allSatisfy { isZoneGone($0) }
        default: return false
        }
    }

    /// The records and deletions of one zone since its token (`nil` if the zone doesn't exist:
    /// its token is reset, and for the private zone it is made again when next needed).
    private func zoneChanges(
        _ db: CKDatabase, zone: CKRecordZone.ID, key: String,
        tokens: inout [String: CKServerChangeToken?], restart: Bool = false
    ) async throws -> ZoneChanges? {
        var token: CKServerChangeToken? = restart ? nil : savedToken(key)
        var fetched = ZoneChanges()
        while true {
            do {
                let result = try await db.recordZoneChanges(inZoneWith: zone, since: token)
                for (_, modification) in result.modificationResultsByID {
                    fetched.records.append(try modification.get().record)
                }
                fetched.deleted += result.deletions.filter { $0.recordType == PlanRecordRef.recordType }
                    .map(\.recordID)
                token = result.changeToken
                if !result.moreComing { break }
            } catch let error as CKError where error.code == .changeTokenExpired && token != nil {
                token = nil
                fetched = ZoneChanges()
            } catch let error as CKError where Self.isZoneGone(error) {
                if !restart { tokens[key] = .some(nil) }
                if db.databaseScope == .private { zoneReady = false }
                return nil
            }
        }
        if !restart { tokens[key] = token }
        return fetched
    }

    private struct ZoneChanges {
        var records: [CKRecord] = []
        var deleted: [CKRecord.ID] = []
    }

    /// How a plan is shared with this person, from its record: the owner's own (they may
    /// edit), or a participant's, with their permission and the owner's name from the share
    /// (from `sharesInBatch` if it is there, else fetched).
    private func sharingState(
        of record: CKRecord, in db: CKDatabase, sharesInBatch: [CKRecord.ID: CKShare]
    ) async throws -> PlanSharing {
        var share: CKShare?
        if let shareID = record.share?.recordID {
            if let inBatch = sharesInBatch[shareID] {
                share = inBatch
            } else {
                share = try await fetchRecord(shareID, in: db) as? CKShare
            }
        }
        if let share { learnName(from: share) }
        guard db.databaseScope != .private else {
            return PlanSharing(role: .owner, canEdit: true, record: ref(for: record.recordID))
        }
        return PlanSharing(
            role: .participant, ownerName: share.flatMap { Self.name(of: $0.owner) },
            canEdit: share?.currentUserParticipant?.permission == .readWrite,
            record: ref(for: record.recordID))
    }

    /// The `CloudChange`s for a zone's records. A plan's permission and owner come from its
    /// share: from the batch if it is in it, else fetched.
    private func changes(
        from fetched: ZoneChanges, in db: CKDatabase, staged: inout Staged
    ) async throws -> [CloudChange] {
        let mine = db.databaseScope == .private
        var shares: [CKRecord.ID: CKShare] = [:]
        for case let share as CKShare in fetched.records { shares[share.recordID] = share }
        var out: [CloudChange] = []
        var planIDs = Set<CKRecord.ID>()
        for record in fetched.records where record.recordType == PlanRecordRef.recordType {
            planIDs.insert(record.recordID)
            let payload: SharedPlan
            do {
                payload = try self.payload(of: record)
            } catch let error as SharedPlanError {
                out.append(.unreadable(recordName: record.recordID.recordName, error))
                continue
            }
            let sharing = try await sharingState(of: record, in: db, sharesInBatch: shares)
            out.append(.updated(payload, sharing))
            staged.entries[record.recordID.recordName] = Entry(
                systemFields: Self.archive(record), base: payload)
        }
        // A share that changed (the owner let a person edit, or took it away) says nothing in
        // its plan's record: read the zone's plans again to get their new permission, for the
        // plans this batch didn't already bring (whichever other plans it did).
        if !mine {
            for zone in Set(fetched.records.filter { $0 is CKShare }.map(\.recordID.zoneID)) {
                var ignored: [String: CKServerChangeToken?] = [:]
                guard
                    let all = try await zoneChanges(
                        db, zone: zone, key: Self.zoneKey(zone), tokens: &ignored, restart: true)
                else { continue }
                // Only its plans (its shares are why we are here), not the ones just done.
                let plans = all.records.filter {
                    $0.recordType == PlanRecordRef.recordType && !planIDs.contains($0.recordID)
                }
                if !plans.isEmpty {
                    out += try await changes(
                        from: ZoneChanges(records: plans), in: db, staged: &staged)
                }
            }
        }
        for id in fetched.deleted {
            guard let plan = UUID(uuidString: id.recordName) else { continue }
            out.append(.removed(planID: plan))
            staged.forgotten.insert(id.recordName)
            staged.entries[id.recordName] = nil
        }
        return out
    }

    // MARK: CloudKit helpers

    private var privateDB: CKDatabase { container.privateCloudDatabase }
    private var sharedDB: CKDatabase { container.sharedCloudDatabase }
    private var zoneID: CKRecordZone.ID {
        CKRecordZone.ID(zoneName: PlanRecordRef.zoneName, ownerName: CKCurrentUserDefaultName)
    }
    private var name: String { myName ?? SharedPlan.someone }

    private func database(for ref: PlanRecordRef) -> CKDatabase { ref.isMine ? privateDB : sharedDB }

    private func recordID(_ ref: PlanRecordRef) -> CKRecord.ID {
        CKRecord.ID(
            recordName: ref.recordName,
            zoneID: CKRecordZone.ID(
                zoneName: ref.zoneName, ownerName: ref.isMine ? CKCurrentUserDefaultName : ref.zoneOwner))
    }

    private func ref(for id: CKRecord.ID) -> PlanRecordRef {
        let owner = id.zoneID.ownerName
        return PlanRecordRef(
            recordName: id.recordName, zoneName: id.zoneID.zoneName,
            zoneOwner: owner == CKCurrentUserDefaultName ? PlanRecordRef.me : owner)
    }

    private static func zoneKey(_ zone: CKRecordZone.ID) -> String {
        "shared:\(zone.ownerName):\(zone.zoneName)"
    }

    /// iCloud as a person's name: "Robin"; `nil` when it didn't say.
    private static func name(of person: CKShare.Participant?) -> String? {
        name(of: person?.userIdentity.nameComponents)
    }

    private static func name(of components: PersonNameComponents?) -> String? {
        guard let components else { return nil }
        let text = PersonNameComponentsFormatter.localizedString(from: components, style: .short)
        return text.isEmpty ? nil : text
    }

    /// Who sent an invitation: "Robin"; `nil` when iCloud didn't say.
    static func ownerName(of metadata: CKShare.Metadata) -> String? {
        name(of: metadata.ownerIdentity.nameComponents)
    }

    /// Throws `.noAccount` unless an iCloud account is available (`.offline` if it can't be
    /// told).
    private func requireAccount() async throws {
        switch try await container.accountStatus() {
        case .available: return
        case .couldNotDetermine: throw PlanCloudError.offline
        default: throw PlanCloudError.noAccount
        }
    }

    private func ensureZone() async throws {
        guard !zoneReady else { return }
        let result = try await privateDB.modifyRecordZones(
            saving: [CKRecordZone(zoneID: zoneID)], deleting: [])
        for (_, saved) in result.saveResults { _ = try saved.get() }
        zoneReady = true
    }

    /// The record, or `nil` if there isn't one (or its zone is gone).
    private func fetchRecord(_ id: CKRecord.ID, in db: CKDatabase) async throws -> CKRecord? {
        do {
            return try await db.record(for: id)
        } catch let error as CKError where error.code == .unknownItem || error.code == .zoneNotFound {
            return nil
        }
    }

    /// Saves and deletes in one go, and gives back what was saved. Throws the first real
    /// error: in an atomic save a failed record makes the others fail with
    /// `.batchRequestFailed`, which says nothing.
    private func modify(
        _ db: CKDatabase, saving: [CKRecord], deleting: [CKRecord.ID]
    ) async throws -> [CKRecord.ID: CKRecord] {
        let result = try await db.modifyRecords(
            saving: saving, deleting: deleting, savePolicy: .ifServerRecordUnchanged, atomically: true)
        var saved: [CKRecord.ID: CKRecord] = [:]
        var errors: [Error] = []
        for (id, outcome) in result.saveResults {
            switch outcome {
            case .success(let record): saved[id] = record
            case .failure(let error): errors.append(error)
            }
        }
        for (_, outcome) in result.deleteResults {
            if case .failure(let error) = outcome { errors.append(error) }
        }
        if let error = errors.first(where: { ($0 as? CKError)?.code != .batchRequestFailed }) ?? errors.first {
            throw error
        }
        return saved
    }

    /// The record to change and the payload this device last saw in it: from the bookkeeping,
    /// else fetched (then what the server has is the base).
    private func working(_ id: CKRecord.ID, in db: CKDatabase) async throws -> (CKRecord, SharedPlan) {
        if let entry = entries[id.recordName], let record = Self.record(from: entry.systemFields) {
            return (record, entry.base)
        }
        guard let record = try await fetchRecord(id, in: db) else { throw PlanCloudError.notShared }
        return (record, try payload(of: record))
    }

    private func fill(_ record: CKRecord, with plan: SharedPlan) throws {
        record[Field.payload] = String(decoding: try plan.data(), as: UTF8.self) as CKRecordValue
        record[Field.version] = SharedPlan.version as CKRecordValue
    }

    /// The plan in a record. A record of a newer version is `.needsNewerVersion` whatever
    /// its payload says.
    private func payload(of record: CKRecord) throws -> SharedPlan {
        if let version = record[Field.version] as? Int, version > SharedPlan.version {
            throw SharedPlanError.needsNewerVersion
        }
        guard let text = record[Field.payload] as? String else { throw SharedPlanError.unreadable }
        return try SharedPlan(data: Data(text.utf8))
    }

    // MARK: Bookkeeping

    private func remember(_ record: CKRecord, base: SharedPlan) {
        entries[record.recordID.recordName] = Entry(
            systemFields: Self.archive(record), base: base)
        persist()
    }

    /// Drops what is kept about a plan's record (it is no longer shared with this person).
    func forget(_ ref: PlanRecordRef) { forget(ref.recordName) }

    private func forget(_ recordName: String) {
        guard entries.removeValue(forKey: recordName) != nil else { return }
        persist()
    }

    private func persist() {
        if let data = try? SharedPlan.encoder.encode(entries) {
            try? data.write(to: directory.appending(path: "plan-cloud.json"), options: .atomic)
        }
    }

    /// The record's system fields (its change tag, not the plan), to save against later.
    private static func archive(_ record: CKRecord) -> Data {
        let coder = NSKeyedArchiver(requiringSecureCoding: true)
        record.encodeSystemFields(with: coder)
        coder.finishEncoding()
        return coder.encodedData
    }

    private static func record(from data: Data) -> CKRecord? {
        guard let coder = try? NSKeyedUnarchiver(forReadingFrom: data) else { return nil }
        coder.requiresSecureCoding = true
        defer { coder.finishDecoding() }
        return CKRecord(coder: coder)
    }

    private func savedToken(_ key: String) -> CKServerChangeToken? {
        guard let data = (defaults.dictionary(forKey: Self.tokensKey) ?? [:])[key] as? Data else {
            return nil
        }
        return try? NSKeyedUnarchiver.unarchivedObject(ofClass: CKServerChangeToken.self, from: data)
    }

    private func setToken(_ token: CKServerChangeToken?, for key: String) {
        var all = defaults.dictionary(forKey: Self.tokensKey) ?? [:]
        if let token,
            let data = try? NSKeyedArchiver.archivedData(withRootObject: token, requiringSecureCoding: true)
        {
            all[key] = data
        } else {
            all[key] = nil
        }
        defaults.set(all, forKey: Self.tokensKey)
    }

    // MARK: Errors

    /// Runs `body`, turning CloudKit's and the network's errors into `PlanCloudError`.
    private func guarded<T>(_ body: () async throws -> T) async throws -> T {
        do {
            return try await body()
        } catch {
            throw Self.friendly(error)
        }
    }

    /// `error` as something `userMessage` words well: no account, offline, quota, read-only;
    /// anything else CloudKit refused is `.unavailable`. Errors that aren't CloudKit's pass.
    nonisolated static func friendly(_ error: Error) -> Error {
        if error is PlanCloudError || error is SharedPlanError { return error }
        if error is URLError { return PlanCloudError.offline }
        guard let error = error as? CKError else { return error }
        switch error.code {
        case .notAuthenticated, .accountTemporarilyUnavailable, .managedAccountRestricted:
            return PlanCloudError.noAccount
        case .networkUnavailable, .networkFailure:
            return PlanCloudError.offline
        case .quotaExceeded:
            return PlanCloudError.quotaExceeded
        case .permissionFailure:
            return PlanCloudError.readOnly
        case .unknownItem, .zoneNotFound:
            return PlanCloudError.notShared
        case .serverRecordChanged:
            return PlanCloudError.conflict
        case .partialFailure:
            let inner = error.partialErrorsByItemID?.values.first { ($0 as? CKError)?.code != .batchRequestFailed }
            return inner.map { friendly($0) } ?? PlanCloudError.unavailable
        default:
            return PlanCloudError.unavailable
        }
    }
}
