import XCTest

@testable import WhetherKit

/// Live shared plans, the pure half: what a plan remembers about its sharing, the
/// merge of two edits, applying iCloud's changes, and what changed. No CloudKit here:
/// `AppSources/PlanSync.swift` is a symlink to the app's file.
final class PlanSyncTests: XCTestCase {
    private let planID = UUID(uuidString: "E621E1F8-C36C-495A-93FC-0C247A3E6E5F")!  // INVARIANT: a valid UUID.
    private let moment = Date(timeIntervalSince1970: 1_792_000_000)

    private var shore: AppPlace {
        AppPlace(
            id: "local-id", name: "New York", detail: "New York, US", latitude: 40.7143,
            longitude: -74.00597, timeZone: "America/New_York", follows: true, pinned: true)
    }

    private var cape: AppPlace {
        AppPlace(
            id: "other", name: "Cape May", detail: "New Jersey, US", latitude: 38.935,
            longitude: -74.906, timeZone: "America/New_York")
    }

    private let july12 = PlanDay(year: 2027, month: 7, day: 12)
    private let july19 = PlanDay(year: 2027, month: 7, day: 19)
    private let july14 = PlanDay(year: 2027, month: 7, day: 14)
    private let july21 = PlanDay(year: 2027, month: 7, day: 21)

    private func participant(
        canEdit: Bool = false, owner: String? = "Robin", zoneOwner: String = "_owner1"
    ) -> PlanSharing {
        PlanSharing(
            role: .participant, ownerName: owner, canEdit: canEdit,
            record: PlanRecordRef(
                recordName: planID.uuidString, zoneName: "Plans", zoneOwner: zoneOwner))
    }

    private func plan(
        name: String = "Summer at the shore", place: AppPlace? = nil, from: PlanDay? = nil,
        to: PlanDay? = nil, years: Int = 30, looked: Bool = false, sharing: PlanSharing? = nil
    ) -> SavedPlan {
        SavedPlan(
            id: planID, name: name, place: place ?? shore, fromDay: from ?? july12,
            toDay: to ?? july19, years: years, looked: looked, sharing: sharing)
    }

    private func shared(
        _ plan: SavedPlan, by who: String = "Robin", at date: Date? = nil
    ) -> SharedPlan {
        SharedPlan(plan, by: who, at: date ?? moment)
    }

    // MARK: Remembering how a plan is shared

    func testASharedPlanSurvivesBeingSaved() throws {
        let original = plan(looked: true, sharing: participant(canEdit: true))
        let data = try JSONEncoder().encode([original])
        let read = try JSONDecoder().decode([SavedPlan].self, from: data)
        XCTAssertEqual(read, [original])
        let sharing = try XCTUnwrap(read.first?.sharing)
        XCTAssertEqual(sharing.role, .participant)
        XCTAssertEqual(sharing.ownerName, "Robin")
        XCTAssertTrue(sharing.canEdit)
        XCTAssertEqual(sharing.record.recordName, planID.uuidString)
        XCTAssertEqual(sharing.record.zoneName, "Plans")
        XCTAssertEqual(sharing.record.zoneOwner, "_owner1")
    }

    func testAPrivatePlansFileHasNoSharingKey() throws {
        let data = try JSONEncoder().encode(plan())
        let object = try XCTUnwrap(try JSONSerialization.jsonObject(with: data) as? [String: Any])
        XCTAssertNil(object["sharing"])
        XCTAssertNil(try JSONDecoder().decode(SavedPlan.self, from: data).sharing)
    }

    func testAnOldSaveWithoutSharingStillDecodes() throws {
        let old = Data(
            """
            {"id":"E621E1F8-C36C-495A-93FC-0C247A3E6E5F","name":"Summer",
             "place":{"id":"p","name":"New York","latitude":40.7143,"longitude":-74.00597,
                      "timeZone":"America/New_York"},
             "fromDay":"2027-07-12","toDay":"2027-07-19","years":10,"looked":true}
            """.utf8)
        let read = try JSONDecoder().decode(SavedPlan.self, from: old)
        XCTAssertNil(read.sharing)
        XCTAssertEqual(read.years, 10)
        XCTAssertTrue(read.looked)
    }

    func testAParticipantIsReadOnlyUnlessSavedOtherwise() throws {
        let text = Data(
            """
            {"role":"participant","record":{"recordName":"x","zoneName":"Plans","zoneOwner":"o"}}
            """.utf8)
        let sharing = try JSONDecoder().decode(PlanSharing.self, from: text)
        XCTAssertFalse(sharing.canEdit)
        XCTAssertFalse(sharing.mayEdit)
        XCTAssertNil(sharing.ownerName)
    }

    func testTheOwnerAlwaysMayEdit() throws {
        XCTAssertTrue(PlanSharing.owned(planID).canEdit)
        XCTAssertTrue(PlanSharing(role: .owner, canEdit: false, record: .owned(planID)).canEdit)
        let text = Data(
            """
            {"role":"owner","canEdit":false,"record":{"recordName":"x","zoneName":"Plans","zoneOwner":"__defaultOwner__"}}
            """.utf8)
        XCTAssertTrue(try JSONDecoder().decode(PlanSharing.self, from: text).mayEdit)
    }

    func testTheOwnersRecordIsInTheirOwnZone() {
        let owned = PlanSharing.owned(planID)
        XCTAssertEqual(owned.record.recordName, planID.uuidString)
        XCTAssertEqual(owned.record.zoneName, "Plans")
        XCTAssertTrue(owned.record.isMine)
        XCTAssertFalse(participant().record.isMine)
    }

    // MARK: Merging two edits

    private func edit(
        _ base: SharedPlan, name: String? = nil, place: SharedPlace? = nil, from: PlanDay? = nil,
        to: PlanDay? = nil, years: Int? = nil, by who: String, at date: Date
    ) -> SharedPlan {
        SharedPlan(
            id: base.id, name: name ?? base.name, place: place ?? base.place, from: from ?? base.from,
            to: to ?? base.to, years: years ?? base.years, modifiedBy: who, modifiedAt: date)
    }

    private var capePlace: SharedPlace { SharedPlace(cape) }
    private let later = Date(timeIntervalSince1970: 1_792_000_100)
    private let latest = Date(timeIntervalSince1970: 1_792_000_200)

    func testWhenOnlyTheyChangedAFieldItIsTheirs() {
        let base = shared(plan())
        let mine = edit(base, name: "Beach week", by: "Alex", at: later)
        let theirs = edit(base, from: july14, to: july21, by: "Robin", at: latest)
        let merged = SharedPlan.merged(base: base, mine: mine, theirs: theirs)
        XCTAssertEqual(merged.name, "Beach week")
        XCTAssertEqual(merged.from, july14)
        XCTAssertEqual(merged.to, july21)
        XCTAssertEqual(merged.place, base.place)
        XCTAssertEqual(merged.modifiedBy, "Alex")
        XCTAssertEqual(merged.modifiedAt, later)
    }

    func testWhenOnlyIChangedAFieldItIsMine() {
        let base = shared(plan())
        let mine = edit(base, place: capePlace, years: 12, by: "Alex", at: later)
        let theirs = edit(base, name: "Beach week", by: "Robin", at: latest)
        let merged = SharedPlan.merged(base: base, mine: mine, theirs: theirs)
        XCTAssertEqual(merged.place, capePlace)
        XCTAssertEqual(merged.years, 12)
        XCTAssertEqual(merged.name, "Beach week")
        XCTAssertEqual(merged.from, base.from)
    }

    func testWhenBothChangedAFieldTheLastWriterWinsAndThatIsMine() {
        let base = shared(plan())
        let mine = edit(base, name: "Mine", from: july14, to: july21, by: "Alex", at: later)
        let theirs = edit(base, name: "Theirs", from: july12, to: july21, by: "Robin", at: latest)
        let merged = SharedPlan.merged(base: base, mine: mine, theirs: theirs)
        XCTAssertEqual(merged.name, "Mine")
        XCTAssertEqual(merged.from, july14)
        XCTAssertEqual(merged.to, july21)
    }

    func testDatesAndPlaceMergeAsOneUnitEach() {
        let base = shared(plan())
        // I moved the first day only; they moved the last day only: mine keeps both of
        // mine, so the trip never gets a first day from one and a last from the other.
        let mine = edit(base, from: july14, by: "Alex", at: later)
        let theirs = edit(base, to: july21, by: "Robin", at: latest)
        let merged = SharedPlan.merged(base: base, mine: mine, theirs: theirs)
        XCTAssertEqual(merged.from, july14)
        XCTAssertEqual(merged.to, base.to)
        XCTAssertLessThanOrEqual(merged.from, merged.to)
        // A place is never half one and half the other.
        var renamed = base.place
        renamed.name = "Shore house"
        var moved = base.place
        moved.latitude = 39.0
        moved.longitude = -74.9
        let placeMerge = SharedPlan.merged(
            base: base, mine: edit(base, place: renamed, by: "Alex", at: later),
            theirs: edit(base, place: moved, by: "Robin", at: latest))
        XCTAssertEqual(placeMerge.place, renamed)
    }

    func testWithNothingChangedHereTheResultIsTheirs() {
        let base = shared(plan())
        let mine = edit(base, by: "Alex", at: later)
        let theirs = edit(base, name: "Beach week", years: 5, by: "Robin", at: latest)
        XCTAssertEqual(SharedPlan.merged(base: base, mine: mine, theirs: theirs), theirs)
    }

    func testWithNothingChangedByThemTheResultIsMine() {
        let base = shared(plan())
        let mine = edit(base, name: "Beach week", from: july14, to: july21, by: "Alex", at: later)
        let theirs = edit(base, by: "Robin", at: latest)
        XCTAssertEqual(SharedPlan.merged(base: base, mine: mine, theirs: theirs), mine)
    }

    // MARK: Applying what iCloud sent

    func testANameOnlyChangeKeepsTheKeptAnswer() throws {
        let local = plan(looked: true, sharing: participant())
        let incoming = shared(plan(name: "Beach week"))
        let result = PlanSync.apply(
            [.updated(incoming, participant())], to: [local])
        let applied = try XCTUnwrap(result.plans.first)
        XCTAssertEqual(applied.name, "Beach week")
        XCTAssertTrue(applied.looked)
        XCTAssertEqual(applied.place, shore, "the local place keeps its id and flags")
        let change = try XCTUnwrap(result.applied.changed.first)
        XCTAssertTrue(change.renamed)
        XCTAssertFalse(change.clearsAnswer)
    }

    func testMovedDatesClearTheKeptAnswer() throws {
        let local = plan(looked: true, sharing: participant())
        let incoming = shared(plan(from: july14, to: july21))
        let result = PlanSync.apply([.updated(incoming, participant())], to: [local])
        let applied = try XCTUnwrap(result.plans.first)
        XCTAssertEqual(applied.fromDay, july14)
        XCTAssertEqual(applied.toDay, july21)
        XCTAssertFalse(applied.looked)
        XCTAssertTrue(try XCTUnwrap(result.applied.changed.first).clearsAnswer)
    }

    func testAnUnchangedUpdateChangesNothing() throws {
        let local = plan(looked: true, sharing: participant())
        let result = PlanSync.apply(
            [.updated(shared(plan(), by: "Someone"), participant())], to: [local])
        XCTAssertEqual(result.plans, [local])
        XCTAssertTrue(result.applied.isEmpty)
    }

    func testTheSharingStaysAndItsPermissionRefreshes() throws {
        let local = plan(sharing: participant(canEdit: false, owner: "Robin"))
        // The owner let this person edit; iCloud sent no name this time.
        let now = participant(canEdit: true, owner: nil)
        let result = PlanSync.apply([.updated(shared(plan()), now)], to: [local])
        let sharing = try XCTUnwrap(result.plans.first?.sharing)
        XCTAssertTrue(sharing.canEdit)
        XCTAssertEqual(sharing.ownerName, "Robin", "a missing name keeps the one known")
        XCTAssertEqual(sharing.record, local.sharing?.record)
        XCTAssertTrue(result.applied.isEmpty, "a permission is not a change to the trip")
    }

    func testAPlanNotHereYetIsAddedWithItsSharing() throws {
        let result = PlanSync.apply(
            [.updated(shared(plan(looked: true)), participant())], to: [])
        let added = try XCTUnwrap(result.plans.first)
        XCTAssertEqual(added.id, planID)
        XCTAssertFalse(added.looked, "each device looks up its own answer")
        XCTAssertEqual(added.sharing, participant())
        XCTAssertEqual(result.applied.added, [planID])
        XCTAssertTrue(result.applied.changed.isEmpty)
    }

    func testAnEditNotSentYetIsNotOverwritten() {
        let local = plan(name: "My unsent name", sharing: participant(canEdit: true))
        let incoming = shared(plan(name: "Theirs", from: july14, to: july21))
        let result = PlanSync.apply(
            [.updated(incoming, participant(canEdit: true))], to: [local], holding: [planID])
        XCTAssertEqual(result.plans, [local])
        XCTAssertTrue(result.applied.isEmpty)
    }

    func testARemovedRecordLeavesAPrivatePlan() throws {
        let local = plan(looked: true, sharing: participant())
        let result = PlanSync.apply([.removed(planID: planID)], to: [local])
        let kept = try XCTUnwrap(result.plans.first)
        XCTAssertNil(kept.sharing)
        XCTAssertEqual(kept.name, local.name)
        XCTAssertTrue(kept.looked)
        XCTAssertEqual(result.applied.unshared, [planID])
    }

    func testARemovedRecordOfAPrivatePlanIsIgnored() {
        // Our own stop-sharing comes back as a deletion once the plan is private.
        let result = PlanSync.apply([.removed(planID: planID)], to: [plan()])
        XCTAssertEqual(result.plans, [plan()])
        XCTAssertTrue(result.applied.isEmpty)
    }

    func testARemovedZoneUnsharesOnlyItsPlans() throws {
        let other = SavedPlan(
            id: UUID(), name: "Other", place: cape, fromDay: july12, toDay: july19,
            sharing: participant(zoneOwner: "_owner2"))
        let local = [plan(sharing: participant(zoneOwner: "_owner1")), other]
        let result = PlanSync.apply(
            [.zoneRemoved(zoneName: "Plans", zoneOwner: "_owner1")], to: local)
        XCTAssertNil(result.plans[0].sharing)
        XCTAssertNotNil(result.plans[1].sharing)
        XCTAssertEqual(result.applied.unshared, [planID])
    }

    func testAnUnreadableRecordIsReportedNotApplied() {
        let local = plan(sharing: participant())
        let result = PlanSync.apply(
            [.unreadable(recordName: "abc", .needsNewerVersion)], to: [local])
        XCTAssertEqual(result.plans, [local])
        XCTAssertEqual(result.applied.unreadable, ["abc"])
    }

    // MARK: What the editor may change

    func testTheEditorNeverChangesHowAPlanIsShared() {
        let stored = plan(sharing: .owned(planID))
        var edited = stored
        edited.name = "Renamed"
        edited.sharing = nil
        let result = stored.editing(edited)
        XCTAssertEqual(result.name, "Renamed")
        XCTAssertEqual(result.sharing, .owned(planID))
    }

    func testAReadOnlyParticipantChangesOnlyTheKeptAnswer() {
        let stored = plan(sharing: participant(canEdit: false))
        var edited = stored
        edited.name = "Mine now"
        edited.fromDay = july14
        edited.looked = true
        let result = stored.editing(edited)
        XCTAssertEqual(result.name, stored.name)
        XCTAssertEqual(result.fromDay, stored.fromDay)
        XCTAssertTrue(result.looked)
    }

    func testAParticipantWhoMayEditChangesTheFields() {
        let stored = plan(sharing: participant(canEdit: true))
        var edited = stored
        edited.name = "Mine now"
        XCTAssertEqual(stored.editing(edited).name, "Mine now")
    }

    func testTheKeptAnswerAloneIsNotASharedChange() {
        let a = plan(looked: false, sharing: .owned(planID))
        var b = a
        b.looked = true
        XCTAssertFalse(b.sharedFieldsDiffer(from: a))
        b.years = 12
        XCTAssertTrue(b.sharedFieldsDiffer(from: a))
    }

    // MARK: What changed

    func testTheSummaryOfMovedDates() throws {
        let change = try XCTUnwrap(
            PlanChange(from: plan(), to: plan(from: july14, to: july21), by: "Robin"))
        XCTAssertEqual(change.planID, planID)
        XCTAssertEqual(change.who, "Robin")
        XCTAssertTrue(change.movedDates)
        XCTAssertFalse(change.movedPlace)
        XCTAssertFalse(change.renamed)
        XCTAssertFalse(change.changedYears)
        XCTAssertTrue(change.clearsAnswer)
        XCTAssertEqual(change.oldFrom, july12)
        XCTAssertEqual(change.newFrom, july14)
        XCTAssertEqual(change.oldTo, july19)
        XCTAssertEqual(change.newTo, july21)
    }

    func testTheSummaryOfAnotherPlace() throws {
        let change = try XCTUnwrap(PlanChange(from: plan(), to: plan(place: cape), by: "Robin"))
        XCTAssertTrue(change.movedPlace)
        XCTAssertFalse(change.movedDates)
        XCTAssertEqual(change.oldPlace.name, "New York")
        XCTAssertEqual(change.newPlace.name, "Cape May")
        XCTAssertTrue(change.clearsAnswer)
    }

    func testTheSummaryOfANameOnlyChange() throws {
        let change = try XCTUnwrap(
            PlanChange(from: plan(), to: plan(name: "Beach week"), by: "Robin"))
        XCTAssertTrue(change.renamed)
        XCTAssertEqual(change.oldName, "Summer at the shore")
        XCTAssertEqual(change.newName, "Beach week")
        XCTAssertFalse(change.movedDates || change.movedPlace || change.changedYears)
        XCTAssertFalse(change.clearsAnswer)
    }

    func testTheSummaryOfMoreYears() throws {
        let change = try XCTUnwrap(PlanChange(from: plan(), to: plan(years: 10), by: "Robin"))
        XCTAssertTrue(change.changedYears)
        XCTAssertTrue(change.clearsAnswer)
        XCTAssertEqual(change.newYears, 10)
    }

    func testNothingChangedIsNoSummary() {
        XCTAssertNil(PlanChange(from: plan(), to: plan(looked: true), by: "Robin"))
    }

    // MARK: What changed, as sentences' inputs

    private func notice(
        _ new: SavedPlan, old: SavedPlan? = nil, by who: String = "Robin", me: String? = nil
    ) throws -> PlanChangeNotice? {
        try XCTUnwrap(PlanChange(from: old ?? plan(), to: new, by: who)).notice(ignoring: me)
    }

    func testMovedDatesAreAnnouncedWithTheNewDates() throws {
        let result = try XCTUnwrap(notice(plan(from: july14, to: july21)))
        XCTAssertEqual(result.who, "Robin")
        XCTAssertEqual(result.planName, "Summer at the shore")
        XCTAssertEqual(result.kind, .movedDates(from: july14, to: july21))
        XCTAssertEqual(result.planID, planID)
    }

    func testAMovedPlaceIsAnnouncedWithItsName() throws {
        XCTAssertEqual(try notice(plan(place: cape))?.kind, .movedPlace(to: "Cape May"))
    }

    func testARenameKeepsTheOldNameAndAnnouncesTheNewOne() throws {
        let result = try XCTUnwrap(notice(plan(name: "Beach week")))
        XCTAssertEqual(result.kind, .renamed(from: "Summer at the shore"))
        XCTAssertEqual(result.planName, "Beach week")
    }

    func testOnlyTheYearsOrSeveralThingsAreJustChanged() throws {
        XCTAssertEqual(try notice(plan(years: 10))?.kind, .changed)
        XCTAssertEqual(try notice(plan(from: july14, to: july21, years: 10))?.kind, .changed)
        XCTAssertEqual(try notice(plan(place: cape, from: july14, to: july21))?.kind, .changed)
        XCTAssertEqual(try notice(plan(name: "Beach week", place: cape))?.kind, .changed)
    }

    func testNoNameOrSomeoneIsAnonymous() throws {
        XCTAssertNil(try notice(plan(years: 10), by: "Someone")?.who)
        XCTAssertNil(try notice(plan(years: 10), by: "")?.who)
        XCTAssertNil(try notice(plan(years: 10), by: "  ")?.who)
        XCTAssertEqual(try notice(plan(years: 10), by: " Robin ")?.who, "Robin")
    }

    func testWhatThisPersonChangedIsNotAnnounced() throws {
        XCTAssertNil(try notice(plan(years: 10), by: "Sam", me: "Sam"))
        XCTAssertNotNil(try notice(plan(years: 10), by: "Robin", me: "Sam"))
        // Without a name of their own, nobody is "me": "Someone" is announced.
        XCTAssertNotNil(try notice(plan(years: 10), by: "Someone", me: nil))
        XCTAssertNotNil(try notice(plan(years: 10), by: "Someone", me: ""))
    }

    func testOneNoticePerPlanInOrderLeavingOutMyOwn() throws {
        let other = UUID()
        func change(_ id: UUID, _ new: SavedPlan, by who: String) throws -> PlanChange {
            var old = plan()
            old.id = id
            var new = new
            new.id = id
            return try XCTUnwrap(PlanChange(from: old, to: new, by: who))
        }
        let changes = [
            try change(planID, plan(place: cape), by: "Robin"),
            try change(other, plan(years: 5), by: "Alex"),
            try change(planID, plan(from: july14, to: july21), by: "Robin"),
        ]
        let notices = PlanChange.notices(for: changes, ignoring: nil)
        XCTAssertEqual(notices.map(\.planID), [planID, other])
        XCTAssertEqual(notices[0].kind, .movedDates(from: july14, to: july21), "the last change")
        XCTAssertEqual(notices[1].who, "Alex")
        let mine = PlanChange.notices(for: changes, ignoring: "Alex")
        XCTAssertEqual(mine.map(\.planID), [planID])
        XCTAssertTrue(PlanChange.notices(for: [], ignoring: nil).isEmpty)
    }

    func testPreferencesWithoutThePlanChangeSwitchTurnItOn() throws {
        func decode(_ json: String) throws -> Preferences {
            try JSONDecoder().decode(Preferences.self, from: Data(json.utf8))
        }
        XCTAssertTrue(Preferences().planChangeNotices)
        XCTAssertTrue(try decode(#"{"alertNotifications":false}"#).planChangeNotices)
        XCTAssertFalse(try decode(#"{"planChangeNotices":false}"#).planChangeNotices)
        var saved = Preferences()
        saved.planChangeNotices = false
        let back = try JSONDecoder().decode(Preferences.self, from: JSONEncoder().encode(saved))
        XCTAssertFalse(back.planChangeNotices)
    }

    // MARK: Errors

    func testNoAccountSaysToSignInToICloud() {
        XCTAssertEqual(PlanCloudError.noAccount.friendlyMessage, "Sign in to iCloud to share plans.")
        XCTAssertEqual(userMessage(PlanCloudError.noAccount).title, "Sign in to iCloud to share plans.")
    }

    func testOfflineUsesTheNoConnectionWording() {
        XCTAssertEqual(userMessage(PlanCloudError.offline).title, "No connection")
    }

    func testEveryCloudErrorHasWordsOfItsOwn() {
        let all: [PlanCloudError] = [
            .noAccount, .offline, .quotaExceeded, .readOnly, .notShared, .conflict, .unavailable,
            .notInThisBuild,
        ]
        XCTAssertEqual(Set(all.map(\.friendlyMessage)).count, all.count)
        for error in all { XCTAssertEqual(error.errorDescription, error.friendlyMessage) }
    }

    // MARK: Invite, accept, leave

    func testLiveSharingIsOfferedOnlyWhereICloudCanWork() {
        XCTAssertTrue(LiveSharing.isAvailable(simulator: false, noICloud: false))
        // The simulator and the tests build without the entitlement.
        XCTAssertFalse(LiveSharing.isAvailable(simulator: true, noICloud: false))
        // `--no-weatherkit` device and archive builds set NO_ICLOUD.
        XCTAssertFalse(LiveSharing.isAvailable(simulator: false, noICloud: true))
        XCTAssertFalse(LiveSharing.isAvailable(simulator: true, noICloud: true))
        XCTAssertEqual(
            userMessage(PlanCloudError.notInThisBuild).title,
            "Sharing a plan with other people isn't available in this build of the app.")
    }

    func testOnlyAParticipantWithoutPermissionIsReadOnly() {
        XCTAssertFalse(plan().isReadOnlyForMe)
        XCTAssertFalse(plan(sharing: .owned(planID)).isReadOnlyForMe)
        XCTAssertFalse(plan(sharing: participant(canEdit: true)).isReadOnlyForMe)
        XCTAssertTrue(plan(sharing: participant(canEdit: false)).isReadOnlyForMe)
    }

    func testAnOwnCopyIsPrivateWithANewIdAndKeepsTheRest() {
        let original = plan(looked: true, sharing: participant())
        let copy = original.privateCopy()
        XCTAssertNotEqual(copy.id, original.id)
        XCTAssertNil(copy.sharing)
        XCTAssertEqual(copy.name, original.name)
        XCTAssertEqual(copy.place, original.place)
        XCTAssertEqual(copy.fromDay, original.fromDay)
        XCTAssertEqual(copy.toDay, original.toDay)
        XCTAssertEqual(copy.years, original.years)
        XCTAssertTrue(copy.looked)
        // The shared plan itself is untouched.
        XCTAssertEqual(original.sharing, participant())
    }

    func testWhoSharedAPlanReadsAsSharedBy() {
        XCTAssertEqual(participant(owner: "Robin").sharedBy, "Shared by Robin")
        XCTAssertEqual(participant(owner: nil).sharedBy, "Shared by someone")
    }

    func testAnInvitationsTitleNamesWhoSharedItWhenKnown() {
        XCTAssertEqual(SharedPlan.headline(from: "Robin"), "Robin shared a plan")
        XCTAssertEqual(SharedPlan.headline(from: nil), "A plan was shared with you")
        XCTAssertEqual(SharedPlan.headline(from: SharedPlan.someone), "A plan was shared with you")
        XCTAssertEqual(SharedPlan.headline(from: ""), "A plan was shared with you")
    }

    // MARK: After a send (review fixes)

    /// What `sent` becomes on the server when a person who last saw `base` saved it and
    /// somebody changed `theirs` first: the merge a `PlanCloud.save` stores.
    private func stored(base: SavedPlan, sent: SavedPlan, theirs: SharedPlan) -> SharedPlan {
        SharedPlan.merged(base: shared(base), mine: shared(sent, by: "Me"), theirs: theirs)
    }

    func testAnEditMadeDuringASendIsMergedWithWhatTheServerKeptNotLost() throws {
        let sharing = participant(canEdit: true)
        let base = plan(sharing: sharing)
        // Sent: dates moved. Meanwhile Robin changed the years, and the save merged them.
        let sent = plan(from: july14, to: july21, sharing: sharing)
        let theirs = shared(plan(years: 10, sharing: sharing), by: "Robin", at: later)
        let kept = stored(base: base, sent: sent, theirs: theirs)
        XCTAssertEqual(kept.years, 10)
        // While it was going, the person renamed the plan.
        let now = plan(name: "Beach week", from: july14, to: july21, sharing: sharing)
        let outcome = now.afterSend(of: sent, stored: kept, mergedWith: theirs)
        XCTAssertEqual(outcome.plan.years, 10, "Robin's change is not undone by the next save")
        XCTAssertEqual(outcome.plan.name, "Beach week", "the edit made meanwhile stays")
        XCTAssertEqual(outcome.plan.fromDay, july14)
        XCTAssertTrue(outcome.resend, "the rename still has to go")
        // The next save's merge, with `kept` as the base, keeps Robin's years.
        let next = SharedPlan.merged(base: kept, mine: shared(outcome.plan), theirs: kept)
        XCTAssertEqual(next.years, 10)
        XCTAssertEqual(next.name, "Beach week")
    }

    func testTheChangeMergedDuringASendIsAttributedToWhoMadeIt() throws {
        let sharing = participant(canEdit: true)
        let sent = plan(name: "Mine", sharing: sharing)
        let theirs = shared(plan(years: 10, sharing: sharing), by: "Robin", at: later)
        let kept = stored(base: plan(sharing: sharing), sent: sent, theirs: theirs)
        let outcome = sent.afterSend(of: sent, stored: kept, mergedWith: theirs)
        let change = try XCTUnwrap(outcome.change)
        XCTAssertEqual(change.who, "Robin")
        XCTAssertTrue(change.changedYears)
        XCTAssertFalse(outcome.resend, "nothing of this device is left to send")
        XCTAssertEqual(
            PlanChange.notices(for: [change], ignoring: "Me").map(\.who), ["Robin"],
            "announced to this person, who is not Robin")
        XCTAssertTrue(PlanChange.notices(for: [change], ignoring: "Robin").isEmpty)
    }

    func testAPlainSendChangesNothingAndNeedsNoResend() {
        let sent = plan(looked: true, sharing: .owned(planID))
        let outcome = sent.afterSend(of: sent, stored: shared(sent), mergedWith: nil)
        XCTAssertEqual(outcome.plan, sent)
        XCTAssertFalse(outcome.resend)
        XCTAssertNil(outcome.change)
    }

    func testAChangeToTheKeptAnswerAloneDuringASendNeedsNoResend() {
        let sent = plan(sharing: .owned(planID))
        var now = sent
        now.looked = true
        let outcome = now.afterSend(of: sent, stored: shared(sent), mergedWith: nil)
        XCTAssertFalse(outcome.resend, "looked is this device's own: nothing to send")
        XCTAssertTrue(outcome.plan.looked)
    }

    func testAPlanMadePrivateDuringASendIsLeftAlone() {
        let sent = plan(sharing: .owned(planID))
        let now = plan(name: "Mine now")
        let outcome = now.afterSend(of: sent, stored: shared(sent), mergedWith: nil)
        XCTAssertEqual(outcome.plan, now)
        XCTAssertFalse(outcome.resend)
    }

    // MARK: Plans deleted here, and updates left out

    func testAnUpdateOfAPlanDeletedHereDoesNotBringItBack() {
        let incoming = shared(plan())
        let result = PlanSync.apply(
            [.updated(incoming, participant())], to: [], ended: [planID])
        XCTAssertTrue(result.plans.isEmpty)
        XCTAssertTrue(result.applied.added.isEmpty)
        XCTAssertEqual(result.applied.skipped, [planID])
    }

    func testAnUpdateComesBackOnceTheStopWorked() {
        let incoming = shared(plan())
        let result = PlanSync.apply([.updated(incoming, participant())], to: [], ended: [])
        XCTAssertEqual(result.plans.map(\.id), [planID])
    }

    func testAHeldPlansUpdateIsReportedAsSkippedSoItsBaseStaysBehind() {
        let local = plan(name: "Unsent", sharing: participant(canEdit: true))
        let result = PlanSync.apply(
            [.updated(shared(plan(name: "Theirs")), participant(canEdit: true))], to: [local],
            holding: [planID])
        XCTAssertEqual(result.applied.skipped, [planID])
        let applied = PlanSync.apply(
            [.updated(shared(plan(name: "Theirs")), participant(canEdit: true))], to: [local])
        XCTAssertTrue(applied.applied.skipped.isEmpty)
    }

    func testASharingTombstoneNamesItsPlan() {
        XCTAssertEqual(participant().planID, planID)
        XCTAssertEqual(PlanSharing.owned(planID).planID, planID)
    }

    func testTheWordsWhenAPlanIsNoLongerSharedWithYou() {
        XCTAssertEqual(
            participant().lostText(planName: "Summer"), "Summer is no longer shared with you")
        XCTAssertEqual(
            PlanSharing.owned(planID).lostText(planName: "Summer"), "Summer isn't shared any more")
    }

    // MARK: Replacing a plan that is shared

    func testReplacingASharedPlanKeepsItShared() throws {
        let mine = plan(name: "Mine", sharing: participant(canEdit: true))
        let incoming = shared(plan(name: "Robin's"))
        XCTAssertTrue(incoming.canReplace(among: [mine]))
        let result = incoming.accepted(.replace, into: [mine])
        XCTAssertEqual(result.plans.count, 1)
        XCTAssertEqual(result.plans[0].name, "Robin's")
        XCTAssertEqual(result.plans[0].sharing, participant(canEdit: true))
        XCTAssertEqual(result.open, planID)
    }

    func testReplacingASharedPlanGoesThroughTheEditor() throws {
        let mine = plan(name: "Mine", sharing: participant(canEdit: true))
        let incoming = shared(plan(name: "Robin's"))
        let replacement = try XCTUnwrap(
            incoming.replacementForSharedPlan(.replace, among: [mine]))
        XCTAssertEqual(replacement.name, "Robin's")
        // The editor (`update`) keeps the sharing whatever the replacement says.
        XCTAssertEqual(mine.editing(replacement).sharing, participant(canEdit: true))
        XCTAssertTrue(mine.editing(replacement).sharedFieldsDiffer(from: mine), "it will be sent")
    }

    func testThereIsNoSharedReplacementForPrivatePlansOrOtherChoices() {
        let incoming = shared(plan(name: "Robin's"))
        XCTAssertNil(incoming.replacementForSharedPlan(.replace, among: [plan(name: "Mine")]))
        XCTAssertNil(incoming.replacementForSharedPlan(.replace, among: []))
        let mine = plan(name: "Mine", sharing: participant(canEdit: true))
        XCTAssertNil(incoming.replacementForSharedPlan(.copy, among: [mine]))
        XCTAssertNil(incoming.replacementForSharedPlan(.add, among: [mine]))
    }

    func testAReadOnlyParticipantCanOnlyAddACopy() throws {
        let mine = plan(name: "Mine", sharing: participant(canEdit: false))
        let incoming = shared(plan(name: "Robin's"))
        XCTAssertFalse(incoming.canReplace(among: [mine]))
        XCTAssertNil(incoming.replacementForSharedPlan(.replace, among: [mine]))
        let result = incoming.accepted(.replace, into: [mine])
        XCTAssertEqual(result.plans.count, 2, "replace is a copy here")
        XCTAssertEqual(result.plans[0], mine, "their plan is untouched")
        XCTAssertNotEqual(result.plans[1].id, planID)
        XCTAssertNil(result.plans[1].sharing)
        XCTAssertEqual(result.open, result.plans[1].id)
    }

    func testReplacingIsOnlyOfferedForAPlanThatIsThere() {
        let incoming = shared(plan())
        XCTAssertFalse(incoming.canReplace(among: []))
        XCTAssertTrue(incoming.canReplace(among: [plan(name: "Mine")]))
        XCTAssertTrue(incoming.canReplace(among: [plan(sharing: .owned(planID))]))
    }
}
