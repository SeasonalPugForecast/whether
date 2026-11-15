import XCTest

@testable import WhetherKit

/// The storms view as the core writes it, and when the Storms tab fetches. The JSON
/// resources are the core's snapshots of its synthetic New York outlooks (a
/// nor'easter the models disagree on, in both unit systems, and a dry week), copied
/// byte for byte; the Rust test `storm_samples.rs` keeps them current.
final class StormsTests: XCTestCase {
    private func resource(_ name: String) throws -> String {
        let url = try XCTUnwrap(
            Bundle.module.url(forResource: name, withExtension: "json", subdirectory: "Resources"))
        return try String(contentsOf: url, encoding: .utf8)
    }

    func testTheNoreasterDecodes() throws {
        let view = try StormsView.decode(resource("storms-noreaster-imperial"))
        XCTAssertEqual(view.place, "New York")
        XCTAssertEqual(view.models.map(\.short), ["ECMWF", "GFS", "GEM", "ICON"])
        XCTAssertEqual(view.models.map(\.role), ["ecmwf", "gfs", "gem", "icon"])
        XCTAssertEqual(view.models.first?.run, "12z")
        XCTAssertNil(view.summary)
        let event = try XCTUnwrap(view.events.first)
        XCTAssertEqual(event.title, "Sat night – Sun night")
        XCTAssertEqual(event.start, 1_800_122_400)
        XCTAssertEqual(event.end, 1_800_273_600)
        let ecmwf = try XCTUnwrap(event.rows.first)
        XCTAssertEqual(ecmwf.snow.text, "13.9 in")
        XCTAssertEqual(ecmwf.precipitation.text, "1.72 in")
        XCTAssertEqual(ecmwf.gust.text, "52 mph")
        XCTAssertEqual(ecmwf.low.text, "28.92 inHg")
        XCTAssertEqual(ecmwf.lowAt, "Sun 1 PM")
        XCTAssertEqual(ecmwf.onset, "Sat 10 PM")
        XCTAssertEqual(event.agreement?.agreement, "low")
        XCTAssertEqual(event.agreement?.level, "poor")
        XCTAssertEqual(event.agreement?.label, "Low agreement")
        XCTAssertTrue(event.agreement?.sentence.hasPrefix("ECMWF brings the low") == true)
        XCTAssertEqual(event.ensembles.map(\.short), ["ECMWF ENS", "GEFS"])
        let plume = try XCTUnwrap(event.ensembles.first?.plume)
        XCTAssertEqual(event.ensembles.first?.members, 51)
        XCTAssertEqual(plume.members.count, 51)
        XCTAssertTrue(plume.members.allSatisfy { $0.count == plume.times.count })
        XCTAssertEqual(plume.p50.count, plume.times.count)
        XCTAssertEqual(plume.yMax, 20)
        XCTAssertEqual(plume.xLabels.map(\.text), ["Sun", "Mon"])
        XCTAssertEqual(plume.yLabels.map(\.text), ["0 in", "5 in", "10 in", "15 in", "20 in"])
        XCTAssertEqual(event.runToRun.map(\.short), ["ECMWF", "GFS"])
        let bars = try XCTUnwrap(event.runToRun.first?.bars)
        XCTAssertEqual(bars.map(\.label), ["Thu 12z", "Thu 18z", "Fri 00z", "Fri 06z", "Fri 12z"])
        XCTAssertEqual(bars.map(\.current), [false, false, false, false, true])
        XCTAssertEqual(bars.last?.runInit, 1_800_014_400)
        XCTAssertEqual(bars.last?.text, "13.9 in")
        XCTAssertFalse(view.tracks.isEmpty)
        XCTAssertTrue(view.tracks.contains(where: \.isStorm))
        XCTAssertFalse(view.tracks.first?.points.first?.pressure.text.isEmpty ?? true)
        XCTAssertTrue(view.steps.first?.label.hasSuffix("· Now") == true)
        XCTAssertTrue(view.attribution.joined().contains("Open-Meteo.com"))
    }

    func testTheMetricNoreasterDecodes() throws {
        let view = try StormsView.decode(resource("storms-noreaster-metric"))
        let row = try XCTUnwrap(view.events.first?.rows.first)
        XCTAssertEqual(row.snow.text, "35.2 cm")
        XCTAssertEqual(row.snow.unit, "cm")
        XCTAssertEqual(row.low.text, "979 hPa")
        XCTAssertEqual(row.onset, "Sat 22:00")
    }

    func testTheDryWeekDecodes() throws {
        let view = try StormsView.decode(resource("storms-dry-week"))
        XCTAssertEqual(view.events, [])
        XCTAssertEqual(view.tracks, [])
        XCTAssertTrue(view.summary?.hasPrefix("No storm in the next 10 days.") == true)
        XCTAssertFalse(view.steps.isEmpty)
        XCTAssertFalse(view.attribution.isEmpty)
    }

    // MARK: Refresh rules

    private let runs = StormOutlookRuns(
        fetchedAt: 1_800_039_600,
        points: [
            .init(model: "ecmwf_ifs025", run: .init(start: 1_800_014_400)),
            .init(model: "gfs_seamless", run: .init(start: 1_800_014_400)),
            .init(model: "gem_seamless", run: .init(start: 1_800_014_400)),
            .init(model: "icon_seamless", run: nil),
        ])

    func testTheOutlookGivesItsRunsAndWhenItWasFetched() throws {
        let json = #"""
            {"place":{"name":"New York"},"points":[
              {"model":"ecmwf_ifs025","run":{"model":"ecmwf_ifs025","init":1800014400,"available":1800036000},"times":[]},
              {"model":"icon_seamless","run":null,"times":[]}],
             "ensembles":[],"grids":[],"past_runs":[],"fetched_at":1800039600}
            """#
        let decoded = try StormOutlookRuns.decode(json)
        XCTAssertEqual(decoded.fetchedAt, 1_800_039_600)
        XCTAssertEqual(decoded.latestInit(of: "ecmwf_ifs025"), 1_800_014_400)
        XCTAssertNil(decoded.latestInit(of: "icon_seamless"))
        XCTAssertNil(decoded.latestInit(of: "gfs_seamless"))
    }

    func testAnOutlookIsStaleAfterTwelveHours() {
        let fetched: Int64 = 1_800_039_600
        XCTAssertFalse(StormsRules.isStale(fetchedAt: fetched, now: fetched))
        XCTAssertFalse(StormsRules.isStale(fetchedAt: fetched, now: fetched + 12 * 3_600))
        XCTAssertTrue(StormsRules.isStale(fetchedAt: fetched, now: fetched + 12 * 3_600 + 1))
    }

    func testRunTimesAreReadAtMostEveryFifteenMinutes() {
        let now: Int64 = 1_800_039_600
        XCTAssertTrue(StormsRules.isDue(lastChecked: nil, now: now), "never read")
        XCTAssertFalse(StormsRules.isDue(lastChecked: now - 60, now: now))
        XCTAssertFalse(StormsRules.isDue(lastChecked: now - 15 * 60 + 1, now: now))
        XCTAssertTrue(StormsRules.isDue(lastChecked: now - 15 * 60, now: now))
        XCTAssertTrue(StormsRules.isDue(lastChecked: now + 600, now: now), "the clock moved back")
    }

    func testEarlierRunsAreAskedForEcmwfAndGfsOverTheEventsWindow() throws {
        let view = try StormsView.decode(resource("storms-noreaster-imperial"))
        let event = try XCTUnwrap(view.events.first)
        let requests = StormsRules.pastRunsRequests(for: event, models: view.models, runs: runs)
        XCTAssertEqual(requests.map(\.model), ["ecmwf_ifs025", "gfs_seamless"])
        XCTAssertTrue(requests.allSatisfy { $0.latestInit == 1_800_014_400 })
        XCTAssertTrue(requests.allSatisfy { $0.windowStart == event.start && $0.windowEnd == event.end })
        // The same key as the core's for one run, so a set is fixed for good by it.
        XCTAssertEqual(
            requests[0].key,
            pastRunKey(
                model: "ecmwf_ifs025", runInit: 1_800_014_400, windowStart: event.start,
                windowEnd: event.end))
        // A model whose run is unknown is skipped.
        var unknown = runs
        unknown.points[1].run = nil
        XCTAssertEqual(
            StormsRules.pastRunsRequests(for: event, models: view.models, runs: unknown).map(\.model),
            ["ecmwf_ifs025"])
    }

    func testANewRunOrAnotherWindowIsAnotherSet() {
        let a = PastRunsRequest(model: "gfs_seamless", latestInit: 100, windowStart: 1_000, windowEnd: 2_000)
        var newer = a
        newer.latestInit = 200
        var later = a
        later.windowEnd = 3_000
        var other = a
        other.model = "ecmwf_ifs025"
        XCTAssertEqual(Set([a.key, newer.key, later.key, other.key]).count, 4)
        XCTAssertEqual(a.key, PastRunsRequest(model: "gfs_seamless", latestInit: 100, windowStart: 1_000, windowEnd: 2_000).key)
    }

    func testOnlyMissingSetsAreFetched() {
        let ecmwf = PastRunsRequest(model: "ecmwf_ifs025", latestInit: 1, windowStart: 2, windowEnd: 3)
        let gfs = PastRunsRequest(model: "gfs_seamless", latestInit: 1, windowStart: 2, windowEnd: 3)
        let both = [ecmwf, gfs]
        XCTAssertEqual(StormsRules.toFetch(both, kept: [], failed: [], running: []), both)
        XCTAssertEqual(StormsRules.toFetch(both, kept: [ecmwf.key], failed: [], running: []), [gfs])
        XCTAssertEqual(StormsRules.toFetch(both, kept: [], failed: [gfs.key], running: []), [ecmwf])
        XCTAssertEqual(StormsRules.toFetch(both, kept: [], failed: [], running: [ecmwf.key, gfs.key]), [])
    }

    func testKeptSetsJoinIntoOneList() throws {
        XCTAssertNil(StormsRules.joined([]))
        XCTAssertNil(StormsRules.joined(["[]", " [ ] \n", "not json"]))
        XCTAssertEqual(StormsRules.joined([#"[{"a":1}]"#]), #"[{"a":1}]"#)
        let joined = try XCTUnwrap(StormsRules.joined([#"[{"a":1},{"a":2}]"#, "[]", "[{\"a\":3}]\n"]))
        XCTAssertEqual(joined, #"[{"a":1},{"a":2},{"a":3}]"#)
        let decoded = try JSONSerialization.jsonObject(with: Data(joined.utf8)) as? [[String: Int]]
        XCTAssertEqual(decoded?.compactMap { $0["a"] }, [1, 2, 3])
    }

    // MARK: The map

    func testEachModelHasItsOwnDash() {
        let roles = ["ecmwf", "gfs", "gem", "icon", "other"]
        let dashes = roles.map(StormMapRules.dash)
        XCTAssertEqual(dashes.first, [], "ECMWF is solid")
        XCTAssertEqual(Set(dashes.map { $0.map(String.init(describing:)).joined(separator: ",") }).count, roles.count)
        for dash in dashes { XCTAssertEqual(dash.count % 2, 0, "on and off in pairs: \(dash)") }
        XCTAssertGreaterThan(StormMapRules.lineWidth(isStorm: true), StormMapRules.lineWidth(isStorm: false))
    }

    func testALowMarksTheStepAtItsTimeOrTheNearestWithinThreeHours() throws {
        let view = try StormsView.decode(resource("storms-noreaster-imperial"))
        let track = try XCTUnwrap(view.tracks.first { $0.role == "ecmwf" && $0.isStorm })
        let first = try XCTUnwrap(track.points.first)
        let second = track.points[1]
        XCTAssertEqual(StormMapRules.point(of: track, at: first.time), first)
        XCTAssertEqual(StormMapRules.point(of: track, at: second.time - 2 * 3_600), second)
        XCTAssertEqual(StormMapRules.point(of: track, at: first.time + 3 * 3_600), first, "a tie takes the earlier")
        XCTAssertNil(StormMapRules.point(of: track, at: first.time - 4 * 3_600), "before the low forms")
        let last = try XCTUnwrap(track.points.last)
        XCTAssertNil(StormMapRules.point(of: track, at: last.time + 3 * 3_600 + 1), "after it is gone")
        // Some step marks every storm track.
        for storm in view.tracks where storm.isStorm {
            XCTAssertTrue(view.steps.contains { StormMapRules.point(of: storm, at: $0.time) != nil }, storm.short)
        }
    }

    func testChipsHideTracksAndTheirFamilysPlumes() throws {
        let view = try StormsView.decode(resource("storms-noreaster-imperial"))
        let event = try XCTUnwrap(view.events.first)
        var hidden = StormMapRules.toggled([], "gfs_seamless")
        XCTAssertEqual(hidden, ["gfs_seamless"])
        XCTAssertFalse(StormMapRules.shown(view.tracks, hidden: hidden).contains { $0.role == "gfs" })
        XCTAssertTrue(StormMapRules.shown(view.tracks, hidden: hidden).contains { $0.role == "ecmwf" })
        XCTAssertEqual(
            StormMapRules.shown(event.ensembles, hidden: hidden, chips: view.models).map(\.short), ["ECMWF ENS"])
        hidden = StormMapRules.toggled(hidden, "gfs_seamless")
        XCTAssertEqual(hidden, [])
        XCTAssertEqual(StormMapRules.shown(view.tracks, hidden: hidden), view.tracks)
        XCTAssertEqual(StormMapRules.shown(event.ensembles, hidden: hidden, chips: view.models), event.ensembles)
    }

    func testTheScrubberStartsAtTheFirstStepOrTheOneAsked() {
        XCTAssertEqual(StormMapRules.startStep(nil, count: 37), 0)
        XCTAssertEqual(StormMapRules.startStep(9, count: 37), 9)
        XCTAssertEqual(StormMapRules.startStep(99, count: 37), 36)
        XCTAssertEqual(StormMapRules.startStep(-3, count: 37), 0)
        XCTAssertEqual(StormMapRules.startStep(4, count: 0), 0)
    }

    func testTheFourModelsLabelsGoFourWays() {
        let slots = (0..<4).map(StormMapRules.labelSlot)
        XCTAssertEqual(Set(slots.map { "\($0.right)\($0.up)" }).count, 4)
        XCTAssertEqual(StormMapRules.labelSlot(4).right, StormMapRules.labelSlot(0).right)
    }

    // MARK: Storm ahead

    func testTheStormAheadCardDecodes() throws {
        let card = try StormAhead.decode(resource("storm-ahead-noreaster-imperial"))
        XCTAssertEqual(card.title, "Storm ahead · Sat night – Sun night")
        XCTAssertEqual(card.line, "Up to 14 in of snow · models disagree")
        XCTAssertEqual(card.level, "poor")
        XCTAssertEqual(card.agreement, "Low agreement")
        XCTAssertEqual(card.start, 1_800_122_400)
        XCTAssertEqual(card.end, 1_800_273_600)
        let alone = try StormAhead.decode(
            #"{"title":"Storm ahead · Mon","line":"Gusts to 52 mph","level":"moderate","agreement":null,"start":1,"end":2}"#)
        XCTAssertNil(alone.agreement)
    }

    func testTheCardGoesOnceItsStormHasEnded() throws {
        let json = try resource("storm-ahead-noreaster-imperial")
        let card = try StormAhead.decode(json)
        XCTAssertEqual(StormAheadRules.shown(card, now: card.start - 86_400), card, "before it")
        XCTAssertEqual(StormAheadRules.shown(card, now: card.start + 3_600), card, "during it")
        XCTAssertNil(StormAheadRules.shown(card, now: card.end), "over")
        XCTAssertNil(StormAheadRules.shown(card, now: card.end + 1))
        XCTAssertEqual(StormAheadRules.shown(json: json, now: card.start), card)
        XCTAssertNil(StormAheadRules.shown(json: nil, now: card.start))
        XCTAssertNil(StormAheadRules.shown(json: "not json", now: card.start))
    }

    func testAFailedCheckKeepsTheLastCardAndAnAnswerReplacesIt() {
        XCTAssertEqual(StormAheadRules.kept("old", after: .unanswered), "old")
        XCTAssertNil(StormAheadRules.kept(nil, after: .unanswered))
        XCTAssertEqual(StormAheadRules.kept("old", after: .answered("new")), "new")
        XCTAssertNil(StormAheadRules.kept("old", after: .answered(nil)), "no storm now: the card goes")
    }

    func testOnlyThePlaceShownIsChecked() {
        XCTAssertTrue(StormAheadRules.checks("name:New York", shown: "name:New York"))
        XCTAssertFalse(StormAheadRules.checks("name:Brooklyn", shown: "name:New York"))
        XCTAssertFalse(StormAheadRules.checks("name:New York", shown: nil))
    }

    func testTheCardOpensTheTabWhenShownElseTheScreen() {
        XCTAssertTrue(StormAheadRules.opensTab(visible: [.hourly, .storms]))
        XCTAssertFalse(StormAheadRules.opensTab(visible: [.hourly, .days, .radar, .plan]))
    }

    func testEachLevelHasItsOwnSymbol() {
        let levels = ["good", "fair", "moderate", "poor", "very_poor"]
        XCTAssertEqual(Set(levels.map(StormAheadRules.symbol)).count, levels.count)
        XCTAssertEqual(StormAheadRules.symbol("dire"), "hurricane")
    }

    func testTheOutlookFileIsSharedByPlace() {
        let dir = URL(fileURLWithPath: "/tmp/storms", isDirectory: true)
        XCTAssertEqual(StormsRules.outlookURL("name:New York", in: dir).lastPathComponent, "outlook-name_New_York.json")
        XCTAssertEqual(StormsRules.outlookURL("name:New York", in: dir), StormsRules.outlookURL("name:New York", in: dir))
    }

    func testFileNamesAreSafe() {
        XCTAssertEqual(StormsRules.fileName("name:New York", prefix: "outlook"), "outlook-name_New_York.json")
        XCTAssertEqual(
            StormsRules.fileName("ecmwf_ifs025@1800014400:1800122400-1800273600", prefix: "runs"),
            "runs-ecmwf_ifs025_1800014400_1800122400-1800273600.json")
    }
}
