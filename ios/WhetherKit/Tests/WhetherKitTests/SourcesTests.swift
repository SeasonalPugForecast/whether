import XCTest

@testable import WhetherKit

/// The phone's ordered sources: the saved list, its migration from the
/// old switches, the order rules and the screen's status lines.
final class SourcesTests: XCTestCase {
    private func decode(_ json: String) throws -> Preferences {
        try JSONDecoder().decode(Preferences.self, from: Data(json.utf8))
    }

    private func row(_ id: String, _ on: Bool = true) -> Preferences.SourceRow {
        Preferences.SourceRow(provider: id, enabled: on)
    }

    private func ids(_ rows: [Preferences.SourceRow]) -> [String] { rows.map(\.provider) }

    // MARK: Sources that wait for the app to be in use

    func testAWidgetOrBackgroundFetchLeavesMetNorwayOut() {
        let settings = Preferences().settings(withKey: false)
        XCTAssertTrue(settings.metNorway)
        XCTAssertTrue(settings.sources.contains(SourceChoice(provider: "met-norway", enabled: true)))
        let background = settings.forBackground()
        XCTAssertFalse(background.metNorway)
        XCTAssertTrue(background.sources.contains(SourceChoice(provider: "met-norway", enabled: false)))
        XCTAssertEqual(
            background.sources.filter { $0.provider != "met-norway" },
            settings.sources.filter { $0.provider != "met-norway" }, "nothing else changes")
        XCTAssertEqual(background.sources.count, settings.sources.count)
        XCTAssertEqual(Sources.foregroundOnly, ["met-norway"])
    }

    // MARK: Preferences saved before the list

    func testANewInstallStartsWithTheCoresDefaultOrder() {
        let fresh = Preferences().sources
        XCTAssertEqual(fresh.map { SourceChoice(provider: $0.provider, enabled: $0.enabled) }, defaultSources())
        XCTAssertEqual(
            ids(fresh), ["nws", "open-meteo-models", "openweather", "pirate-weather", "open-meteo", "met-norway"])
        XCTAssertFalse(fresh[0].enabled, "the NWS is off for a new install")
    }

    func testPreferencesWithoutTheListGiveTodaysChain() throws {
        // Nothing saved but a few unrelated keys: the old defaults.
        let plain = try decode(#"{"units":"metric"}"#)
        XCTAssertEqual(
            ids(plain.sources), ["nws", "open-meteo-models", "openweather", "pirate-weather", "open-meteo", "met-norway"])
        XCTAssertEqual(plain.sources.map(\.enabled), [false, false, true, true, true, true])
        XCTAssertFalse(plain.nws)
        XCTAssertTrue(plain.metNorway)
        // Every old switch becomes its row.
        let all = try decode(#"{"nws":true,"models":["gfs_hrrr"],"oneCall":true,"metNorway":false}"#)
        XCTAssertEqual(all.sources.map(\.enabled), [true, true, true, true, true, false])
        XCTAssertTrue(all.nws)
        XCTAssertFalse(all.metNorway)
        // The "NOAA blend" switch, which came before the model lists, turns the models row on.
        let blend = try decode(#"{"noaaBlend":true}"#)
        XCTAssertTrue(blend.enabled(Sources.models))
        XCTAssertFalse(try decode(#"{"noaaBlend":false}"#).enabled(Sources.models))
        // An empty list counts as none saved.
        XCTAssertEqual(try decode(#"{"sources":[],"nws":true}"#).sources.map(\.enabled), [true, false, true, true, true, true])
    }

    func testTheCoreGetsTheMigratedListAndTheSwitchesAgree() throws {
        let prefs = try decode(#"{"nws":true,"models":["gfs_hrrr"]}"#)
        let s = prefs.settings(withKey: false)
        XCTAssertEqual(
            s.sources.map(\.provider),
            ["nws", "open-meteo-models", "openweather", "pirate-weather", "open-meteo", "met-norway"])
        XCTAssertEqual(s.sources.map(\.enabled), [true, true, true, true, true, true])
        XCTAssertTrue(s.nws)
        XCTAssertEqual(s.models, ["gfs_hrrr"])
        XCTAssertEqual(s.keys, [:], "no keys without the Keychain read")
        XCTAssertNil(s.openweatherKey)
    }

    func testASavedListWinsOverTheOldSwitches() throws {
        let prefs = try decode(
            """
            {"nws":true,"metNorway":true,"sources":[
              {"provider":"openweather","enabled":true},
              {"provider":"nws","enabled":false},
              {"provider":"open-meteo-models","enabled":true},
              {"provider":"open-meteo","enabled":true},
              {"provider":"met-norway","enabled":false}]}
            """)
        // The list saved before Pirate Weather gets its row, after the others.
        XCTAssertEqual(
            ids(prefs.sources), ["openweather", "nws", "open-meteo-models", "pirate-weather", "open-meteo", "met-norway"])
        XCTAssertFalse(prefs.nws, "the row, not the old switch")
        XCTAssertFalse(prefs.metNorway)
    }

    func testTheListSurvivesSaving() throws {
        var prefs = Preferences()
        prefs.sources = [
            row("openweather"), row("open-meteo-models"), row("nws", true), row("pirate-weather", false), row("open-meteo"),
            row("met-norway", false),
        ]
        let again = try JSONDecoder().decode(Preferences.self, from: JSONEncoder().encode(prefs))
        XCTAssertEqual(again.sources, prefs.sources)
        // The old switches are not saved any more: the rows carry them.
        let json = try XCTUnwrap(String(data: JSONEncoder().encode(prefs), encoding: .utf8))
        XCTAssertFalse(json.contains("\"nws\":"))
        XCTAssertFalse(json.contains("\"metNorway\":"))
    }

    func testTheOldSwitchesWriteTheirRows() {
        var prefs = Preferences()
        prefs.nws = true
        prefs.metNorway = false
        XCTAssertTrue(prefs.enabled(Sources.nws))
        XCTAssertFalse(prefs.enabled(Sources.metNorway))
        prefs.set(Sources.openWeather, enabled: false)
        XCTAssertFalse(prefs.sources.first { $0.provider == "openweather" }?.enabled ?? true)
    }

    // MARK: Order rules

    func testCompleteKeepsEachOnceAndEndsWithOpenMeteoThenMetNorway() {
        let list = [row("openweather"), row("openweather", false), row("open-meteo", false), row("nws", true)]
        let done = Sources.complete(list)
        // One row per id (the first counts), the missing movable row added, Open-Meteo on, MET last.
        XCTAssertEqual(
            ids(done), ["openweather", "nws", "open-meteo-models", "pirate-weather", "open-meteo", "met-norway"])
        XCTAssertTrue(done[0].enabled)
        XCTAssertTrue(done.first { $0.provider == "open-meteo" }?.enabled ?? false, "never switched off")
        XCTAssertEqual(done.last?.provider, "met-norway")
    }

    func testAnIdFromANewerBuildIsKeptButNotOffered() {
        let list = [
            row("visual-crossing"), row("openweather"), row("nws"), row("open-meteo-models"), row("pirate-weather"),
        ]
        let done = Sources.complete(list)
        XCTAssertEqual(
            ids(done), ["openweather", "nws", "open-meteo-models", "pirate-weather", "visual-crossing", "open-meteo", "met-norway"])
        XCTAssertEqual(ids(Sources.orderable(done)), ["openweather", "nws", "open-meteo-models", "pirate-weather"])
    }

    func testMovingReordersTheMovableRowsOnly() {
        var rows = Sources.complete(Preferences.defaultSources)
        // OpenWeather (index 2) before the models (index 1).
        Sources.move(&rows, fromOffsets: IndexSet(integer: 2), toOffset: 1)
        XCTAssertEqual(
            ids(rows), ["nws", "openweather", "open-meteo-models", "pirate-weather", "open-meteo", "met-norway"])
        // Pirate Weather (index 3) to the front.
        Sources.move(&rows, fromOffsets: IndexSet(integer: 3), toOffset: 0)
        XCTAssertEqual(
            ids(rows), ["pirate-weather", "nws", "openweather", "open-meteo-models", "open-meteo", "met-norway"])
        // To the end of the movable rows: Open-Meteo stays after them.
        Sources.move(&rows, fromOffsets: IndexSet(integer: 0), toOffset: 4)
        XCTAssertEqual(
            ids(rows), ["nws", "openweather", "open-meteo-models", "pirate-weather", "open-meteo", "met-norway"])
    }

    func testAKeyedProviderIsOneCatalogEntry() {
        XCTAssertEqual(Sources.keyedProviders, ["openweather", "pirate-weather"])
        XCTAssertNotNil(Sources.info("openweather")?.signUp)
        XCTAssertEqual(Sources.info("pirate-weather")?.name, "Pirate Weather")
        XCTAssertEqual(Sources.info("pirate-weather")?.signUp?.host, "pirate-weather.apiable.io")
        XCTAssertNil(Sources.info("visual-crossing"))
    }

    // MARK: What the screen says

    func testStatusLines() {
        var facts = Sources.Facts()
        facts.dailyCalls = 500
        // OpenWeather: no key, a key, One Call.
        XCTAssertEqual(Sources.status(row("openweather"), facts), "Needs a key")
        facts.hasKey["openweather"] = true
        XCTAssertEqual(Sources.status(row("openweather"), facts), "Key saved")
        facts.oneCall = true
        facts.callsToday = 12
        XCTAssertEqual(Sources.status(row("openweather"), facts), "12 of 500 One Call calls used today")
        XCTAssertEqual(Sources.status(row("openweather", false), facts), "Off")
        // Models: none, a preset, models of a desktop config.
        XCTAssertTrue(Sources.status(row("open-meteo-models"), facts).hasPrefix("No models chosen"))
        facts.models = ["gfs_hrrr"]
        facts.presetName = "NOAA blend"
        XCTAssertEqual(Sources.status(row("open-meteo-models"), facts), "NOAA blend")
        facts.presetName = nil
        facts.models = ["a", "b"]
        XCTAssertEqual(Sources.status(row("open-meteo-models"), facts), "a → b")
        XCTAssertEqual(Sources.status(row("nws"), facts), "On for US places")
        // The fallbacks.
        XCTAssertEqual(Sources.status(row("open-meteo", false), facts), "Always on, and always last")
        XCTAssertEqual(Sources.status(row("met-norway"), facts), "Needs the app's contact address")
        facts.hasContact = true
        XCTAssertEqual(Sources.status(row("met-norway"), facts), "On, after Open-Meteo")
        XCTAssertEqual(Sources.status(row("met-norway", false), facts), "Off")
    }

    func testTheSummaryListsWhatWouldServeInOrder() {
        var facts = Sources.Facts()
        var rows = Sources.complete(Preferences.defaultSources)
        // The defaults, no key, no models, no contact: only Open-Meteo.
        XCTAssertEqual(Sources.summary(rows, facts), "Open-Meteo")
        facts.hasKey["openweather"] = true
        facts.models = ["gfs_hrrr"]
        facts.hasContact = true
        XCTAssertEqual(Sources.summary(rows, facts), "Forecast models → OpenWeather → Open-Meteo → MET Norway")
        Sources.move(&rows, fromOffsets: IndexSet(integer: 2), toOffset: 0)
        rows[1].enabled = true  // the NWS
        rows[2].enabled = false  // the models
        XCTAssertEqual(Sources.summary(rows, facts), "OpenWeather → NWS → Open-Meteo → MET Norway")
    }

    func testTheFactsReadTheKeychainForEachKeyedProvider() {
        var prefs = Preferences()
        prefs.oneCall = true
        prefs.dailyCalls = 300
        let facts = prefs.sourceFacts(callsToday: 7)
        XCTAssertEqual(Set(facts.hasKey.keys), Set(Sources.keyedProviders))
        XCTAssertEqual(facts.callsToday, 7)
        XCTAssertEqual(facts.dailyCalls, 300)
        XCTAssertTrue(facts.oneCall)
    }

    // MARK: Pirate Weather

    func testPreferencesSavedBeforePirateWeatherKeepTheirOrderAndGainTheRow() throws {
        // A list saved by the build that had the Sources screen but not this source.
        let prefs = try decode(
            """
            {"dailyCalls":300,"sources":[
              {"provider":"openweather","enabled":true},
              {"provider":"nws","enabled":true},
              {"provider":"open-meteo-models","enabled":false},
              {"provider":"open-meteo","enabled":true},
              {"provider":"met-norway","enabled":false}]}
            """)
        // Their order and switches are kept; the new row follows the others, on:
        // it serves only with a key, so today's chain does not change.
        XCTAssertEqual(
            ids(prefs.sources), ["openweather", "nws", "open-meteo-models", "pirate-weather", "open-meteo", "met-norway"])
        XCTAssertEqual(prefs.sources.map(\.enabled), [true, true, false, true, true, false])
        XCTAssertEqual(prefs.dailyCalls, 300)
        XCTAssertEqual(prefs.pirateWeatherCalls, 9000, "the core's default budget")
        // Without a key saved the core gets no key for it.
        let settings = prefs.settings(withKey: false)
        XCTAssertEqual(settings.keys, [:])
        XCTAssertEqual(settings.pirateWeatherMonthlyCalls, 9000)
        XCTAssertTrue(settings.sources.contains { $0.provider == "pirate-weather" && $0.enabled })
    }

    func testTheMonthlyLimitIsSavedAndReachesTheCore() throws {
        var prefs = Preferences()
        XCTAssertEqual(prefs.pirateWeatherCalls, 9000)
        prefs.pirateWeatherCalls = 4000
        let again = try JSONDecoder().decode(Preferences.self, from: JSONEncoder().encode(prefs))
        XCTAssertEqual(again.pirateWeatherCalls, 4000)
        XCTAssertEqual(again.settings(withKey: false).pirateWeatherMonthlyCalls, 4000)
        // Saved before the limit existed.
        XCTAssertEqual(try decode(#"{"units":"metric"}"#).pirateWeatherCalls, 9000)
    }

    func testPirateWeatherStatusLinesAndSummary() {
        var facts = Sources.Facts()
        XCTAssertEqual(Sources.status(row("pirate-weather"), facts), "Needs a key")
        XCTAssertFalse(Sources.serves(row("pirate-weather"), facts))
        facts.hasKey["pirate-weather"] = true
        XCTAssertEqual(Sources.status(row("pirate-weather"), facts), "Key saved", "until the count is known")
        facts.monthUsage["pirate-weather"] = "12 of 9000 calls used this month"
        XCTAssertEqual(Sources.status(row("pirate-weather"), facts), "12 of 9000 calls used this month")
        XCTAssertTrue(Sources.serves(row("pirate-weather"), facts))
        XCTAssertEqual(Sources.status(row("pirate-weather", false), facts), "Off")
        XCTAssertFalse(Sources.serves(row("pirate-weather", false), facts))
        // In the order: after OpenWeather by default, wherever the user puts it.
        var rows = Sources.complete(Preferences.defaultSources)
        facts.hasKey["openweather"] = true
        XCTAssertEqual(Sources.summary(rows, facts), "OpenWeather → Pirate Weather → Open-Meteo")
        Sources.move(&rows, fromOffsets: IndexSet(integer: 3), toOffset: 0)
        XCTAssertEqual(Sources.summary(rows, facts), "Pirate Weather → OpenWeather → Open-Meteo")
    }

    func testTheMonthTextComesFromTheCore() {
        XCTAssertEqual(
            monthUsageText(usageJson: nil, provider: "pirate-weather", limit: 9000, now: 1_792_843_200),
            "0 of 9000 calls used this month")
        XCTAssertEqual(
            monthUsageText(usageJson: nil, provider: "pirate-weather", limit: 0, now: 0),
            "All 0 calls used this month: the next source serves until it ends")
    }
}
