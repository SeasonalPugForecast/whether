import XCTest

@testable import WhetherKit

/// The storms view as the core writes it, and when the Storms tab fetches. The JSON
/// resources are the core's snapshots of its synthetic New York outlooks (a
/// nor'easter the models disagree on, in both unit systems, and a dry week) and of
/// Josephine from NHC's recordings, copied byte for byte; the Rust test
/// `storm_samples.rs` keeps them current.
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
        XCTAssertEqual(view.hurricanes, [], "no hurricanes key: none")
    }

    // MARK: Hurricanes

    /// Josephine (AL10 2026) from NHC's recorded decks, seen from New York: the core's
    /// snapshots, copied byte for byte.
    func testJosephineDecodes() throws {
        let view = try StormsView.decode(resource("storms-josephine-imperial"))
        XCTAssertEqual(view.events, [])
        XCTAssertEqual(view.tracks, [])
        XCTAssertEqual(view.steps, [])
        let h = try XCTUnwrap(view.hurricanes.first)
        XCTAssertEqual(view.hurricanes.count, 1)
        XCTAssertEqual(h.id, "al102026")
        XCTAssertEqual(h.name, "Josephine")
        XCTAssertEqual(h.title, "Subtropical Storm Josephine")
        XCTAssertEqual(h.classification, "STS")
        XCTAssertNil(h.category)
        XCTAssertEqual(h.level, "poor")
        XCTAssertEqual(h.basin, "atlantic")
        XCTAssertEqual(h.latitude, 39.5, accuracy: 0.001)
        XCTAssertEqual(h.longitude, -66.8, accuracy: 0.001)
        XCTAssertEqual(h.wind.text, "65 mph")
        XCTAssertEqual(h.pressure.text, "29.18 inHg")
        XCTAssertEqual(h.movement, "NE at 13 mph")
        XCTAssertEqual(h.advisory, "Advisory Thu 5:00 PM")
        XCTAssertEqual(h.advisoryTime, 1_792_702_800)
        XCTAssertEqual(h.advisoryUrl, "https://www.nhc.noaa.gov/text/MIATCPAT1.shtml")
        XCTAssertNil(h.reaches, "New York is far behind her")
        XCTAssertEqual(
            h.official.map(\.label), ["Thu 2 PM", "Thu 5 PM", "Fri 2 AM", "Fri 2 PM", "Sat 2 AM", "Sat 2 PM", "Sun 2 AM"])
        XCTAssertEqual(h.official[2].wind.text, "75 mph")
        XCTAssertEqual(h.aids.map(\.short), ["GFS", "GEM", "UKMET", "HAFS", "Consensus"])
        XCTAssertEqual(h.aids.map(\.role), ["gfs", "gem", "ukmet", "hafs", "consensus"])
        XCTAssertEqual(h.aids.map(\.id), ["AVNI", "CMCI", "UKXI", "HFAI", "TVCN"])
        for aid in h.aids {
            XCTAssertTrue(zip(aid.points, aid.points.dropFirst()).allSatisfy { $0.time < $1.time }, aid.short)
        }
        XCTAssertEqual(h.past.count, 17)
        XCTAssertEqual(h.past.last?.latitude ?? 0, 39.5, accuracy: 0.001)
        XCTAssertEqual(h.cone.count, 33)
        XCTAssertEqual(view.attribution.last, "Hurricanes: NOAA National Hurricane Center")
    }

    func testJosephineInMetric() throws {
        let h = try XCTUnwrap(StormsView.decode(resource("storms-josephine-metric")).hurricanes.first)
        XCTAssertEqual(h.wind.text, "100 km/h")
        XCTAssertEqual(h.wind.unit, "km/h")
        XCTAssertEqual(h.pressure.text, "988 hPa")
    }

    func testAHurricanesCardNamesIt() throws {
        let card = try StormAhead.decode(resource("storms-josephine-storm-ahead"))
        XCTAssertEqual(card.title, "Subtropical Storm Josephine")
        XCTAssertEqual(card.line, "The forecast cone reaches Test Point Fri")
        XCTAssertEqual(card.level, "poor")
        XCTAssertNil(card.agreement)
        XCTAssertEqual(card.hurricane, "al102026")
        let sample = try StormAhead.decode(resource("storm-ahead-hurricane"))
        XCTAssertEqual(sample.title, "Hurricane Sample")
        XCTAssertEqual(sample.line, "The forecast cone reaches New York Sun")
        XCTAssertEqual(sample.level, "very_poor")
        XCTAssertEqual(sample.hurricane, "al992027")
        XCTAssertNil(try StormAhead.decode(resource("storm-ahead-noreaster-imperial")).hurricane, "a storm from the models")
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

    func testAFailedSetIsTriedAgainAfterFifteenMinutes() {
        let now: Int64 = 1_800_039_600
        let failed: [String: Int64] = ["a": now - 60, "b": now - 15 * 60 + 1, "c": now - 15 * 60, "d": now + 600]
        let left = StormsRules.stillFailed(failed, now: now)
        XCTAssertEqual(Set(left.keys), ["a", "b"], "c is old enough, d failed in the future")
        XCTAssertEqual(left["a"], now - 60, "the failure time is kept")
        XCTAssertEqual(StormsRules.stillFailed([:], now: now), [:])
        let ecmwf = PastRunsRequest(model: "ecmwf_ifs025", latestInit: 1, windowStart: 2, windowEnd: 3)
        let waiting = StormsRules.stillFailed([ecmwf.key: now - 60], now: now)
        XCTAssertEqual(StormsRules.toFetch([ecmwf], kept: [], failed: Set(waiting.keys), running: []), [])
        let due = StormsRules.stillFailed([ecmwf.key: now - 15 * 60], now: now)
        XCTAssertEqual(StormsRules.toFetch([ecmwf], kept: [], failed: Set(due.keys), running: []), [ecmwf])
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
        let roles = ["ecmwf", "gfs", "gem", "icon", "ukmet", "hafs", "consensus", "other"]
        let dashes = roles.map(StormMapRules.dash)
        XCTAssertEqual(dashes.first, [], "ECMWF is solid")
        XCTAssertEqual(StormMapRules.dash("official"), [], "so is NHC's official track")
        XCTAssertEqual(StormMapRules.dash("ukmet"), [10, 4, 2, 4])
        XCTAssertEqual(StormMapRules.dash("hafs"), [3, 3])
        XCTAssertEqual(StormMapRules.dash("consensus"), [12, 3])
        XCTAssertEqual(Set(dashes.map { $0.map(String.init(describing:)).joined(separator: ",") }).count, roles.count)
        for dash in dashes { XCTAssertEqual(dash.count % 2, 0, "on and off in pairs: \(dash)") }
        // Each gap leaves room between the round caps of the edge's strokes.
        for dash in dashes {
            for gap in stride(from: 1, to: dash.count, by: 2) {
                XCTAssertGreaterThan(dash[gap], StormMapRules.edgeWidth * 1.5, "\(dash)")
            }
        }
    }

    func testTracksAreTranslucentBandsWithEdges() {
        let storm = StormMapRules.bandWidth(isStorm: true)
        let other = StormMapRules.bandWidth(isStorm: false)
        XCTAssertGreaterThan(storm, other, "the storm's band is wider")
        // Both edges leave a fill between them.
        XCTAssertGreaterThan(other, 2 * StormMapRules.edgeWidth + 2)
        XCTAssertGreaterThan(StormMapRules.fillAlpha, 0.15)
        XCTAssertLessThan(StormMapRules.fillAlpha, 0.4)
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

    // MARK: Hurricanes on the map

    func testTheOfficialTrackIsMarkedOnceADay() throws {
        let h = try XCTUnwrap(StormsView.decode(resource("storms-josephine-imperial")).hurricanes.first)
        // Josephine's forecast runs 0, 3, 12, 24, 36, 48 and 60 hours out; the start is the storm itself.
        XCTAssertEqual(StormMapRules.dayMarks(h.official).map(\.label), ["Fri 2 PM", "Sat 2 PM"])
        XCTAssertEqual(StormMapRules.dayMarks([]), [])
        XCTAssertEqual(StormMapRules.dayMarks(Array(h.official.prefix(1))), [])
        // An hour off still counts as the day.
        var late = h.official
        late[3].time += 3_600
        XCTAssertEqual(StormMapRules.dayMarks(late).map(\.label), ["Fri 2 PM", "Sat 2 PM"])
        late[3].time += 1
        XCTAssertEqual(StormMapRules.dayMarks(late).map(\.label), ["Sat 2 PM"])
    }

    func testTheHurricaneLayerIsDrawnAsDesigned() {
        XCTAssertLessThan(StormMapRules.aidBandWidth, StormMapRules.bandWidth(isStorm: true))
        XCTAssertGreaterThan(StormMapRules.aidBandWidth, 2 * StormMapRules.edgeWidth + 2)
        XCTAssertGreaterThan(StormMapRules.officialWidth, StormMapRules.edgeWidth * 2, "thicker than a band's edge")
        XCTAssertEqual(StormMapRules.coneFillAlpha, 0.18, accuracy: 0.03)
        XCTAssertEqual(StormMapRules.coneEdgeWidth, 1.5)
        XCTAssertLessThan(StormMapRules.pastDash[0], 0.1, "dots")
        XCTAssertGreaterThan(StormMapRules.pastDash[1], StormMapRules.pastWidth)
    }

    func testChipsHideHurricanesAndTheirGuidance() throws {
        let view = try StormsView.decode(resource("storms-josephine-imperial"))
        XCTAssertEqual(StormMapRules.aidChips(view.hurricanes).map(\.aid), ["AVNI", "CMCI", "UKXI", "HFAI", "TVCN"])
        XCTAssertEqual(StormMapRules.shown(view.hurricanes, hidden: [], hiddenAids: []), view.hurricanes)
        XCTAssertEqual(StormMapRules.shown(view.hurricanes, hidden: ["al102026"], hiddenAids: []), [])
        let shown = StormMapRules.shown(view.hurricanes, hidden: [], hiddenAids: ["UKXI", "TVCN"])
        XCTAssertEqual(shown.first?.aids.map(\.short), ["GFS", "GEM", "HAFS"])
        XCTAssertEqual(shown.first?.official, view.hurricanes.first?.official, "the official track stays")
        // Two storms with the same guidance: one chip each.
        var second = try XCTUnwrap(view.hurricanes.first)
        second.id = "al112026"
        XCTAssertEqual(StormMapRules.aidChips(view.hurricanes + [second]).count, 5)
    }

    func testANearHurricaneIsFramedWithThePlace() throws {
        let view = try StormsView.decode(resource("storms-josephine-imperial"))
        let h = try XCTUnwrap(view.hurricanes.first)
        // New York to Josephine's centre: about 620 km.
        let apart = StormMapRules.distance((view.latitude, view.longitude), (h.latitude, h.longitude))
        XCTAssertEqual(apart, 625_000, accuracy: 25_000)
        XCTAssertTrue(StormMapRules.isNear(h, latitude: view.latitude, longitude: view.longitude))
        XCTAssertFalse(StormMapRules.isNear(h, latitude: 34.05, longitude: -118.24), "Los Angeles")
        let near = StormMapRules.framing(view.hurricanes, focus: nil, latitude: view.latitude, longitude: view.longitude)
        XCTAssertEqual(near.hurricanes.map(\.id), ["al102026"])
        XCTAssertTrue(near.tracks)
        let far = StormMapRules.framing(view.hurricanes, focus: nil, latitude: 34.05, longitude: -118.24)
        XCTAssertEqual(far.hurricanes, [])
        // The Storm ahead card's hurricane alone, near or not.
        let focused = StormMapRules.framing(view.hurricanes, focus: "al102026", latitude: 34.05, longitude: -118.24)
        XCTAssertEqual(focused.hurricanes.map(\.id), ["al102026"])
        XCTAssertFalse(focused.tracks)
        let gone = StormMapRules.framing(view.hurricanes, focus: "al992026", latitude: 34.05, longitude: -118.24)
        XCTAssertEqual(gone.hurricanes, [])
        XCTAssertTrue(gone.tracks)
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
        let visible: [Preferences.OverviewSection] = [.now, .alerts, .storm]
        XCTAssertTrue(StormAheadRules.checks("name:New York", shown: "name:New York", visible: visible))
        XCTAssertFalse(StormAheadRules.checks("name:Brooklyn", shown: "name:New York", visible: visible))
        XCTAssertFalse(StormAheadRules.checks("name:New York", shown: nil, visible: visible))
    }

    func testAHiddenCardIsNotChecked() {
        var prefs = Preferences()
        let visible = { prefs.sections.filter(\.shown).map(\.section) }
        XCTAssertTrue(StormAheadRules.checks("name:New York", shown: "name:New York", visible: visible()))
        prefs.hide(.storm)
        XCTAssertFalse(StormAheadRules.checks("name:New York", shown: "name:New York", visible: visible()))
        prefs.show(.storm)
        XCTAssertTrue(StormAheadRules.checks("name:New York", shown: "name:New York", visible: visible()))
        XCTAssertFalse(StormAheadRules.checks("name:New York", shown: "name:New York", visible: [.now, .alerts]))
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

    func testAHurricanesCardShowsTheHurricane() throws {
        let hurricane = try StormAhead.decode(resource("storms-josephine-storm-ahead"))
        XCTAssertEqual(StormAheadRules.symbol(for: hurricane), "hurricane")
        let models = try StormAhead.decode(resource("storm-ahead-noreaster-imperial"))
        XCTAssertEqual(StormAheadRules.symbol(for: models), StormAheadRules.symbol("poor"))
    }

    func testTheCheckKeepsItsOwnFileBesideTheOutlook() {
        let dir = URL(fileURLWithPath: "/tmp/storms", isDirectory: true)
        let outlook = StormsRules.outlookURL("name:New York", in: dir)
        let ahead = StormsRules.aheadURL("name:New York", in: dir)
        XCTAssertEqual(outlook.lastPathComponent, "outlook-name_New_York.json")
        XCTAssertEqual(ahead.lastPathComponent, "ahead-name_New_York.json")
        XCTAssertEqual(ahead.deletingLastPathComponent(), outlook.deletingLastPathComponent())
        XCTAssertNotEqual(ahead, outlook)
    }

    private func outlook(fetchedAt: Int64, grids: String = "[]") -> String {
        #"{"place":{"name":"New York"},"points":[{"model":"ecmwf_ifs025","run":null,"times":[]}],"#
            + #""ensembles":[],"grids":\#(grids),"past_runs":[],"fetched_at":\#(fetchedAt)}"#
    }

    func testTheCheckStartsFromTheNewerOutlook() {
        let full = outlook(fetchedAt: 1_800_039_600, grids: #"[{"model":"ecmwf_ifs025"}]"#)
        let older = outlook(fetchedAt: 1_800_039_600 - 3_600)
        let newer = outlook(fetchedAt: 1_800_039_600 + 3_600)
        // A fresh full outlook from the Storms tab saves the check its request.
        XCTAssertEqual(StormAheadRules.cachedOutlook(full: full, ahead: older), full)
        XCTAssertEqual(StormAheadRules.cachedOutlook(full: full, ahead: newer), newer)
        XCTAssertEqual(StormAheadRules.cachedOutlook(full: full, ahead: outlook(fetchedAt: 1_800_039_600)), full, "a tie")
        // Only one kept, or one that doesn't read.
        XCTAssertEqual(StormAheadRules.cachedOutlook(full: nil, ahead: older), older)
        XCTAssertEqual(StormAheadRules.cachedOutlook(full: full, ahead: nil), full)
        XCTAssertEqual(StormAheadRules.cachedOutlook(full: "{", ahead: older), older)
        XCTAssertEqual(StormAheadRules.cachedOutlook(full: full, ahead: "not json"), full)
        XCTAssertNil(StormAheadRules.cachedOutlook(full: nil, ahead: nil))
        XCTAssertNil(StormAheadRules.cachedOutlook(full: "{", ahead: "["))
    }

    func testFileNamesAreSafe() {
        XCTAssertEqual(StormsRules.fileName("name:New York", prefix: "outlook"), "outlook-name_New_York.json")
        XCTAssertEqual(
            StormsRules.fileName("ecmwf_ifs025@1800014400:1800122400-1800273600", prefix: "runs"),
            "runs-ecmwf_ifs025_1800014400_1800122400-1800273600.json")
    }
}
