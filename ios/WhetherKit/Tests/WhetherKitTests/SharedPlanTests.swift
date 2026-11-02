import XCTest

@testable import WhetherKit

/// The plan as it is shared. `AppSources/SharedPlan.swift` is a symlink to
/// the app's file, compiled into this target.
final class SharedPlanTests: XCTestCase {
    private let planID = UUID(uuidString: "E621E1F8-C36C-495A-93FC-0C247A3E6E5F")!  // INVARIANT: a valid UUID.
    /// Whole seconds: the text keeps no more.
    private let moment = Date(timeIntervalSince1970: 1_792_000_000)

    private var shore: AppPlace {
        AppPlace(
            id: "local-id", name: "New York", detail: "New York, US", latitude: 40.7143,
            longitude: -74.00597, timeZone: "America/New_York", follows: true, pinned: true)
    }

    private func plan(
        place: AppPlace? = nil, from: PlanDay = PlanDay(year: 2027, month: 7, day: 12),
        to: PlanDay = PlanDay(year: 2027, month: 7, day: 19), years: Int = 30, looked: Bool = false
    ) -> SavedPlan {
        SavedPlan(
            id: planID, name: "Summer at the shore", place: place ?? shore, fromDay: from, toDay: to,
            years: years, looked: looked)
    }

    private func shared(_ plan: SavedPlan? = nil) -> SharedPlan {
        SharedPlan(plan ?? self.plan(), by: "Robin", at: moment)
    }

    /// A v1 payload as text, with the parts the tests change.
    private func json(
        v: Int = 1, from: String = "2027-07-12", to: String = "2027-07-19", years: Int = 30,
        latitude: Double = 40.7143
    ) -> Data {
        Data(
            """
            {"v":\(v),"id":"E621E1F8-C36C-495A-93FC-0C247A3E6E5F","name":"Summer",
             "place":{"name":"New York","latitude":\(latitude),"longitude":-74.00597,
                      "timeZone":"America/New_York"},
             "from":"\(from)","to":"\(to)","years":\(years),
             "modifiedBy":"Robin","modifiedAt":"2026-10-28T12:00:00Z"}
            """.utf8)
    }

    // MARK: The payload

    func testAPlanSurvivesBeingSharedAndRead() throws {
        let original = shared()
        let read = try SharedPlan(data: try original.data())
        XCTAssertEqual(read, original)
        XCTAssertEqual(read.modifiedAt, moment)
        XCTAssertEqual(read.modifiedBy, "Robin")
    }

    func testThePayloadHasTheDocumentedKeysAndDays() throws {
        let object = try XCTUnwrap(
            try JSONSerialization.jsonObject(with: try shared().data()) as? [String: Any])
        XCTAssertEqual(
            Set(object.keys), ["v", "id", "name", "place", "from", "to", "years", "modifiedBy", "modifiedAt"])
        XCTAssertEqual(object["v"] as? Int, 1)
        XCTAssertEqual(object["id"] as? String, planID.uuidString)
        XCTAssertEqual(object["from"] as? String, "2027-07-12")
        XCTAssertEqual(object["to"] as? String, "2027-07-19")
        XCTAssertEqual(object["years"] as? Int, 30)
        XCTAssertEqual(object["modifiedBy"] as? String, "Robin")
        XCTAssertEqual(object["modifiedAt"] as? String, isoText(moment))
        let place = try XCTUnwrap(object["place"] as? [String: Any])
        XCTAssertEqual(Set(place.keys), ["name", "detail", "latitude", "longitude", "timeZone"])
        XCTAssertEqual(place["name"] as? String, "New York")
        XCTAssertEqual(place["timeZone"] as? String, "America/New_York")
        XCTAssertEqual(place["latitude"] as? Double, 40.7143)
    }

    private func isoText(_ date: Date) -> String {
        ISO8601DateFormatter().string(from: date)
    }

    func testThePlaceOnTheDeviceStaysOnTheDevice() throws {
        let object = try XCTUnwrap(
            try JSONSerialization.jsonObject(with: try shared().data()) as? [String: Any])
        let place = try XCTUnwrap(object["place"] as? [String: Any])
        XCTAssertNil(place["id"], "the local list's id")
        XCTAssertNil(place["follows"], "following this phone means nothing to another")
        XCTAssertNil(place["pinned"])
        XCTAssertNil(object["looked"], "each device looks up its own answer")
    }

    func testACopyWithoutANameIsFromSomeone() {
        XCTAssertEqual(SharedPlan(plan(), by: "", at: moment).modifiedBy, "Someone")
        XCTAssertEqual(SharedPlan(plan(), by: "  \n", at: moment).modifiedBy, "Someone")
    }

    func testTheModifiedTimeKeepsWholeSecondsSoACopyEqualsTheOriginal() throws {
        let fraction = SharedPlan(plan(), by: "Robin", at: moment.addingTimeInterval(0.75))
        XCTAssertEqual(fraction.modifiedAt, moment)
        XCTAssertEqual(try SharedPlan(data: try fraction.data()), fraction)
    }

    // MARK: Reading

    func testAVersionOtherThanOneIsRefusedInFriendlyWords() throws {
        for v in [0, 2, 3, 99] {
            XCTAssertThrowsError(try SharedPlan(data: json(v: v)), "v\(v)") { error in
                XCTAssertEqual(error as? SharedPlanError, .needsNewerVersion)
                XCTAssertEqual(
                    userMessage(error).title, "This plan needs a newer version of the app.")
                XCTAssertNil(userMessage(error).detail)
                XCTAssertEqual(
                    error.localizedDescription, "This plan needs a newer version of the app.")
            }
        }
        // A newer version may keep its fields elsewhere: only `v` is read first.
        let future = Data(#"{"v":2,"plan":{"somewhere":"else"}}"#.utf8)
        XCTAssertThrowsError(try SharedPlan(data: future)) {
            XCTAssertEqual($0 as? SharedPlanError, .needsNewerVersion)
        }
    }

    func testYearsAreClampedToOneThroughFifty() throws {
        XCTAssertEqual(try SharedPlan(data: json(years: 0)).years, 1)
        XCTAssertEqual(try SharedPlan(data: json(years: -5)).years, 1)
        XCTAssertEqual(try SharedPlan(data: json(years: 12)).years, 12)
        XCTAssertEqual(try SharedPlan(data: json(years: 50)).years, 50)
        XCTAssertEqual(try SharedPlan(data: json(years: 500)).years, 50)
        XCTAssertEqual(shared(plan(years: 90)).years, 50, "made from a plan too")
    }

    func testDatesTheWrongWayRoundAreRefused() throws {
        XCTAssertThrowsError(try SharedPlan(data: json(from: "2027-07-19", to: "2027-07-12"))) { error in
            XCTAssertEqual(error as? SharedPlanError, .datesBackwards)
            XCTAssertEqual(userMessage(error).title, "This plan's dates are the wrong way round.")
        }
        let oneDay = try SharedPlan(data: json(from: "2027-07-12", to: "2027-07-12"))
        XCTAssertEqual(oneDay.from, oneDay.to, "a one-day trip is fine")
    }

    func testATripOverAMonthLongIsRefusedInFriendlyWords() throws {
        // The editor stops at 30 days after the first day.
        XCTAssertEqual(
            try SharedPlan(data: json(from: "2027-07-12", to: "2027-08-11")).to,
            PlanDay(year: 2027, month: 8, day: 11))
        XCTAssertThrowsError(try SharedPlan(data: json(from: "2027-07-12", to: "2027-08-12"))) { error in
            XCTAssertEqual(error as? SharedPlanError, .tooLong)
            XCTAssertEqual(
                userMessage(error).title,
                "This plan is longer than a month, which the app can't look up.")
            XCTAssertNil(userMessage(error).detail)
        }
        // Counted in days, whatever the length of the months or the year.
        XCTAssertNoThrow(try SharedPlan(data: json(from: "2027-02-01", to: "2027-03-03")))
        XCTAssertThrowsError(try SharedPlan(data: json(from: "2027-02-01", to: "2027-03-04"))) {
            XCTAssertEqual($0 as? SharedPlanError, .tooLong)
        }
        XCTAssertNoThrow(try SharedPlan(data: json(from: "2028-02-01", to: "2028-03-02")))
        XCTAssertThrowsError(try SharedPlan(data: json(from: "2028-02-01", to: "2028-03-03"))) {
            XCTAssertEqual($0 as? SharedPlanError, .tooLong)
        }
        XCTAssertNoThrow(try SharedPlan(data: json(from: "2027-12-20", to: "2028-01-19")))
        XCTAssertThrowsError(try SharedPlan(data: json(from: "2027-12-20", to: "2028-01-20"))) {
            XCTAssertEqual($0 as? SharedPlanError, .tooLong)
        }
        XCTAssertThrowsError(try SharedPlan(data: json(from: "2027-07-12", to: "2029-07-12"))) {
            XCTAssertEqual($0 as? SharedPlanError, .tooLong)
        }
    }

    func testADayThatDoesNotExistIsNotAPlan() {
        XCTAssertThrowsError(try SharedPlan(data: json(from: "2027-02-31", to: "2027-03-05"))) {
            XCTAssertEqual($0 as? SharedPlanError, .unreadable)
        }
        XCTAssertThrowsError(try SharedPlan(data: json(from: "2027-02-20", to: "2027-02-29"))) {
            XCTAssertEqual($0 as? SharedPlanError, .unreadable)
        }
        XCTAssertNoThrow(try SharedPlan(data: json(from: "2028-02-20", to: "2028-02-29")))
    }

    func testAFileThatCouldNotBeOpenedHasFriendlyWords() {
        XCTAssertEqual(userMessage(SharedPlanError.unopenable).title, "That file couldn't be opened.")
        XCTAssertEqual(SharedPlanError.unopenable.localizedDescription, "That file couldn't be opened.")
    }

    func testSomethingThatIsNotAPlanIsRefused() {
        for bad in [
            "not json", "{}", #"{"v":1}"#,
        ] {
            XCTAssertThrowsError(try SharedPlan(data: Data(bad.utf8)), bad) {
                XCTAssertEqual($0 as? SharedPlanError, .unreadable)
            }
        }
        XCTAssertThrowsError(try SharedPlan(data: json(from: "tomorrow"))) {
            XCTAssertEqual($0 as? SharedPlanError, .unreadable)
        }
        XCTAssertThrowsError(try SharedPlan(data: json(latitude: 91))) {
            XCTAssertEqual($0 as? SharedPlanError, .unreadable, "no such place")
        }
    }

    func testAPayloadWithoutWhoChangedItIsFromSomeone() throws {
        let text = String(decoding: json(), as: UTF8.self)
            .replacingOccurrences(of: #""modifiedBy":"Robin","#, with: "")
        XCTAssertEqual(try SharedPlan(data: Data(text.utf8)).modifiedBy, "Someone")
    }

    // MARK: Saving it here

    func testWithoutALocalCopyThePlanIsNotLooked() {
        let saved = shared().savedPlan(keeping: nil)
        XCTAssertFalse(saved.looked)
        XCTAssertEqual(saved.id, planID)
        XCTAssertEqual(saved.name, "Summer at the shore")
        XCTAssertEqual(saved.fromDay, PlanDay(year: 2027, month: 7, day: 12))
        XCTAssertEqual(saved.toDay, PlanDay(year: 2027, month: 7, day: 19))
        XCTAssertEqual(saved.years, 30)
        XCTAssertEqual(saved.place.name, "New York")
        XCTAssertEqual(saved.place.latitude, 40.7143)
        XCTAssertFalse(saved.place.follows)
        XCTAssertFalse(saved.place.pinned)
    }

    func testUnchangedDatesKeepWhatWasLookedUpAndThePlaceAsItIsHere() {
        let local = plan(looked: true)
        let saved = shared(local).savedPlan(keeping: local)
        XCTAssertTrue(saved.looked)
        XCTAssertEqual(saved.place, local.place, "its id, and that it follows or is pinned, stay")
        XCTAssertEqual(saved, local)
    }

    func testChangedDatesPlaceOrYearsClearWhatWasLookedUp() {
        let local = plan(looked: true)
        let later = shared(plan(to: PlanDay(year: 2027, month: 7, day: 20)))
        XCTAssertFalse(later.savedPlan(keeping: local).looked, "the last day moved")
        let earlier = shared(plan(from: PlanDay(year: 2027, month: 7, day: 11)))
        XCTAssertFalse(earlier.savedPlan(keeping: local).looked, "the first day moved")
        let longer = shared(plan(years: 10))
        XCTAssertFalse(longer.savedPlan(keeping: local).looked, "other years")
        let elsewhere = AppPlace(
            name: "Cape May", detail: "New Jersey, US", latitude: 38.93, longitude: -74.91,
            timeZone: "America/New_York")
        let moved = shared(plan(place: elsewhere)).savedPlan(keeping: local)
        XCTAssertFalse(moved.looked, "another place")
        XCTAssertEqual(moved.place.name, "Cape May")
        XCTAssertNotEqual(moved.place.id, local.place.id, "a new place has its own id")
    }

    func testTheSharedIdWinsAndAnUnlookedLocalStaysUnlooked() {
        var local = plan()
        local.id = UUID()
        let saved = shared().savedPlan(keeping: local)
        XCTAssertEqual(saved.id, planID)
        XCTAssertFalse(saved.looked)
    }

    func testRenamingAPlanKeepsItsAnswer() {
        let local = plan(looked: true)
        var renamed = local
        renamed.name = "Shore week"
        let saved = shared(renamed).savedPlan(keeping: local)
        XCTAssertEqual(saved.name, "Shore week")
        XCTAssertTrue(saved.looked, "the name doesn't change the answer")
    }

    // MARK: Receiving

    func testAPlanNobodyHasIsNew() {
        let other = plan()
        var someone = other
        someone.id = UUID()
        XCTAssertEqual(shared().receipt(among: []), .new)
        XCTAssertEqual(shared().receipt(among: [someone]), .new)
        let result = shared().accepted(.add, into: [someone])
        XCTAssertEqual(result.plans.map(\.id), [someone.id, planID])
        XCTAssertEqual(result.open, planID)
        XCTAssertFalse(result.plans[1].looked)
    }

    func testAPlanAlreadyHereJustTheSameIsOnlyOpened() {
        let mine = plan(looked: true)
        XCTAssertEqual(shared().receipt(among: [mine]), .same)
        let result = shared().accepted(.add, into: [mine])
        XCTAssertEqual(result.plans, [mine], "nothing changes, and its answer stays")
        XCTAssertEqual(result.open, planID)
    }

    func testADifferentNameOrTripIsNotTheSame() {
        var renamed = plan()
        renamed.name = "Shore week"
        XCTAssertEqual(shared().receipt(among: [renamed]), .differs)
        let later = plan(to: PlanDay(year: 2027, month: 7, day: 20))
        XCTAssertEqual(shared().receipt(among: [later]), .differs)
        XCTAssertEqual(shared().receipt(among: [plan(years: 10)]), .differs)
    }

    func testADifferentPlanWithTheSameIdIsAddedAsACopy() {
        let mine = plan(to: PlanDay(year: 2027, month: 7, day: 20), looked: true)
        for choice in [ReceiveChoice.add, .copy] {
            let result = shared().accepted(choice, into: [mine])
            XCTAssertEqual(result.plans.count, 2)
            XCTAssertEqual(result.plans[0], mine, "mine is kept as it was")
            let copy = result.plans[1]
            XCTAssertNotEqual(copy.id, planID, "a copy has a new id")
            XCTAssertEqual(result.open, copy.id)
            XCTAssertEqual(copy.toDay, PlanDay(year: 2027, month: 7, day: 19))
            XCTAssertEqual(copy.name, "Summer at the shore")
            XCTAssertFalse(copy.looked)
        }
    }

    func testReplacingKeepsTheSpotAndTheId() {
        let mine = plan(to: PlanDay(year: 2027, month: 7, day: 20), looked: true)
        var after = plan()
        after.id = UUID()
        let result = shared().accepted(.replace, into: [mine, after])
        XCTAssertEqual(result.plans.count, 2)
        XCTAssertEqual(result.plans[0].id, planID)
        XCTAssertEqual(result.plans[0].toDay, PlanDay(year: 2027, month: 7, day: 19))
        XCTAssertFalse(result.plans[0].looked, "the dates changed")
        XCTAssertEqual(result.plans[1], after)
        XCTAssertEqual(result.open, planID)
    }

    func testReplacingAPlanNobodyHasAddsIt() {
        let result = shared().accepted(.replace, into: [])
        XCTAssertEqual(result.plans.map(\.id), [planID])
    }

    func testTheSheetNamesWhoSharedIt() {
        XCTAssertEqual(shared().headline, "Robin shared a plan")
        let anonymous = SharedPlan(plan(), by: SharedPlan.someone, at: moment)
        XCTAssertEqual(anonymous.headline, "A plan was shared with you")
    }

    // MARK: The inbox

    func testOnlyAFileInTheInboxIsInIt() throws {
        let fm = FileManager.default
        let root = fm.temporaryDirectory.appending(
            path: "inbox-\(UUID().uuidString)", directoryHint: .isDirectory)
        defer { try? fm.removeItem(at: root) }
        let inbox = root.appending(path: "Inbox", directoryHint: .isDirectory)
        let lookalike = root.appending(path: "Inbox2", directoryHint: .isDirectory)
        for folder in [inbox, lookalike] {
            try fm.createDirectory(at: folder, withIntermediateDirectories: true)
        }
        let inside = inbox.appending(path: "Shore.whetherplan")
        let beside = root.appending(path: "Shore.whetherplan")
        let near = lookalike.appending(path: "Shore.whetherplan")
        for file in [inside, beside, near] { try Data("{}".utf8).write(to: file) }

        XCTAssertTrue(SharedPlan.isInside(inside, inbox: inbox))
        XCTAssertFalse(SharedPlan.isInside(beside, inbox: inbox), "a file next to the inbox")
        XCTAssertFalse(SharedPlan.isInside(near, inbox: inbox), "a folder that starts the same")
        XCTAssertFalse(SharedPlan.isInside(inbox, inbox: inbox), "the folder itself")
        XCTAssertFalse(
            SharedPlan.isInside(inbox.appending(path: "../Shore.whetherplan"), inbox: inbox),
            "a way out by dots")
        // A link in the inbox to a file elsewhere is not the inbox's to delete.
        let link = inbox.appending(path: "Link.whetherplan")
        try fm.createSymbolicLink(at: link, withDestinationURL: beside)
        XCTAssertFalse(SharedPlan.isInside(link, inbox: inbox))
        // The same folder written another way (the inbox as a link to the real one).
        let alias = root.appending(path: "Alias", directoryHint: .isDirectory)
        try fm.createSymbolicLink(at: alias, withDestinationURL: inbox)
        XCTAssertTrue(SharedPlan.isInside(alias.appending(path: "Shore.whetherplan"), inbox: inbox))
    }

    // MARK: The file

    func testTheFileIsNamedAfterThePlan() {
        XCTAssertEqual(shared().fileName, "Summer at the shore.whetherplan")
    }

    func testAFileNameHoldsNoSeparatorsOrLeadingDots() {
        XCTAssertEqual(SharedPlan.fileStem("Pat's 60th: Cape May/NJ"), "Pat's 60th- Cape May-NJ")
        XCTAssertEqual(SharedPlan.fileStem("  ..hidden \n"), "hidden")
        XCTAssertEqual(SharedPlan.fileStem("a\\b*c?d\"e<f>g|h"), "a-b-c-d-e-f-g-h")
        XCTAssertEqual(SharedPlan.fileStem(""), "Plan")
        XCTAssertEqual(SharedPlan.fileStem(" ... "), "Plan")
        XCTAssertEqual(SharedPlan.fileStem(String(repeating: "x", count: 100)).count, 60)
        XCTAssertEqual(SharedPlan.fileStem("Été à Paris"), "Été à Paris", "letters stay")
    }
}
