import XCTest

@testable import WhetherKit

/// The app's own logic (preferences, grids, plans). The sources in
/// `AppSources/` are symlinks to the app's files, compiled into this target.
final class AppLogicTests: XCTestCase {
    private func decode(_ json: String) throws -> Preferences {
        try JSONDecoder().decode(Preferences.self, from: Data(json.utf8))
    }

    // MARK: Preferences saved by an older version

    func testOldPreferencesKeepTheirValuesAndDefaultTheRest() throws {
        // Saved before most fields existed: only a few keys.
        let old = try decode(#"{"units":"metric","days":5,"oneCall":true}"#)
        let fresh = Preferences()
        XCTAssertEqual(old.units, .metric)
        XCTAssertEqual(old.days, 5)
        XCTAssertTrue(old.oneCall)
        XCTAssertEqual(old.clock, fresh.clock)
        XCTAssertEqual(old.dates, .monthNameDay, "Nov 1, as before")
        XCTAssertEqual(fresh.layout, .centered, "centered by default")
        XCTAssertEqual(try decode(#"{"layout":"darkSky"}"#).layout, .darkSky, "a saved choice stays")
        XCTAssertEqual(old.sections, fresh.sections)
        XCTAssertEqual(old.hourTiles, fresh.hourTiles)
        XCTAssertEqual(old.detailCards, fresh.detailCards)
        XCTAssertEqual(old.dailyCalls, fresh.dailyCalls)
    }

    func testTheDateChoiceReachesTheCore() throws {
        var p = Preferences()
        XCTAssertEqual(p.settings(withKey: false).dateStyle, .monthNameDay)
        p.dates = .compact
        XCTAssertEqual(p.settings(withKey: false).dateStyle, .compact)
        XCTAssertEqual(try decode(#"{"dates":"dayMonthDots"}"#).dates, .dayMonthDots)
        XCTAssertEqual(try decode(#"{"dates":"lunar"}"#).dates, .monthNameDay, "unknown reads as the default")
        for choice in Preferences.DateChoice.allCases {
            XCTAssertEqual(Preferences.DateChoice(choice.style), choice)
        }
    }

    func testColorsDefaultToWhetherAndASavedChoiceStays() throws {
        // Saved before Colors: no colorTheme, colorVision or patterns keys.
        let old = try decode(#"{"units":"metric","background":"plain"}"#)
        XCTAssertEqual(old.colorTheme, "whether")
        XCTAssertEqual(old.colorVision, .typical)
        XCTAssertEqual(old.patterns, .automatic)
        XCTAssertEqual(old.colorTheme(), ColorPresets.whether, "nothing changes for an old save")
        let saved = try decode(#"{"colorTheme":"aurora","colorVision":"red_green","patterns":"on"}"#)
        XCTAssertEqual(saved.colorTheme, "aurora")
        XCTAssertEqual(saved.colorVision, .redGreen)
        XCTAssertEqual(saved.patterns, .on)
        XCTAssertEqual(saved.colorTheme().id, "aurora", "the app and the widget paint the choice")
        let again = try JSONDecoder().decode(Preferences.self, from: JSONEncoder().encode(saved))
        XCTAssertEqual(again, saved)
        // Values a later version might save read as the defaults, and keep the rest.
        let later = try decode(#"{"colorTheme":"mine","colorVision":"x_ray","patterns":"sometimes","days":5}"#)
        XCTAssertEqual(later.colorVision, .typical)
        XCTAssertEqual(later.patterns, .automatic)
        XCTAssertEqual(later.days, 5)
        XCTAssertEqual(later.colorTheme, "mine", "kept: it may name a custom theme")
        XCTAssertEqual(later.colorTheme(), ColorPresets.whether, "an id it doesn't know paints Whether")
        // Seasons follows the month.
        var seasons = Preferences()
        seasons.colorTheme = "seasons"
        let october = try XCTUnwrap(Calendar(identifier: .gregorian).date(from: DateComponents(year: 2026, month: 10, day: 30)))
        XCTAssertEqual(seasons.colorTheme(now: october, calendar: Calendar(identifier: .gregorian)).id, "seasons.fall")
    }

    func testColorVisionReplacesOnlyTheInformationalColors() {
        var prefs = Preferences()
        prefs.colorTheme = "aurora"
        prefs.colorVision = .redGreen
        let theme = prefs.colorTheme()
        XCTAssertEqual(theme.id, "aurora", "any preset, with the reader's ramps")
        XCTAssertEqual(theme[.levelPoor], ColorPresets.redGreen[.levelPoor])
        XCTAssertEqual(theme[.radar3], ColorPresets.redGreen[.radar3])
        let aurora = try! XCTUnwrap(ColorPresets.preset("aurora", month: 10))
        XCTAssertEqual(theme[.skyClearDayTop], aurora[.skyClearDayTop], "the skies stay the preset's")
        XCTAssertEqual(theme[.accent], aurora[.accent])
        prefs.colorVision = .typical
        XCTAssertEqual(prefs.colorTheme(), aurora, "typical vision: the preset as it is")
    }

    func testPatternsFollowTheSettingAndDifferentiateWithoutColor() {
        var prefs = Preferences()
        let whether = ColorPresets.whether
        XCTAssertFalse(prefs.showsPatterns(differentiateWithoutColor: false, theme: whether))
        XCTAssertTrue(
            prefs.showsPatterns(differentiateWithoutColor: true, theme: whether), "Automatic follows iOS")
        prefs.patterns = .on
        XCTAssertTrue(prefs.showsPatterns(differentiateWithoutColor: false, theme: whether))
        prefs.patterns = .off
        XCTAssertFalse(prefs.showsPatterns(differentiateWithoutColor: true, theme: whether))
        let graphite = try! XCTUnwrap(ColorPresets.preset("graphite", month: 10))
        XCTAssertTrue(prefs.showsPatterns(differentiateWithoutColor: false, theme: graphite), "Graphite always")
        prefs.colorVision = .noColor
        XCTAssertTrue(prefs.showsPatterns(differentiateWithoutColor: false, theme: whether), "no color always")
    }

    func testTheColorsGalleryListsEveryPresetOnceWithItsSavedID() {
        let gallery = ColorPresets.gallery(month: 1)
        XCTAssertEqual(gallery.map(\.base), ColorPresets.ids, "in order; a card saves its base")
        XCTAssertEqual(gallery.first, ColorPresets.whether, "Whether first")
        XCTAssertEqual(gallery.first { $0.base == "seasons" }?.id, "seasons.winter", "Seasons as the month's look")
        XCTAssertEqual(Set(gallery.map(\.id)).count, gallery.count, "ids are unique for the grid")
        XCTAssertEqual(ColorPresets.gallery(month: 7, southern: true).first { $0.base == "seasons" }?.id, "seasons.winter")
    }

    func testFeelsLikeIsOnByDefaultAndASavedChoiceStays() throws {
        XCTAssertTrue(Preferences().feelsLike, "on for new installs")
        XCTAssertTrue(try decode("{}").feelsLike, "no saved value: the default")
        XCTAssertTrue(try decode(#"{"feelsLike":true}"#).feelsLike)
        XCTAssertFalse(try decode(#"{"feelsLike":false}"#).feelsLike, "a saved off stays off")
    }

    func testUnitsPickedOneByOneDefaultToAutomaticAndSurviveSaving() throws {
        let old = try decode(#"{"units":"metric"}"#)
        XCTAssertTrue(old.unitOverrides.isAutomatic, "saved without the key: Automatic")
        XCTAssertEqual(old.settings(withKey: false).unitOverrides, UnitOverrides(
            temperature: nil, wind: nil, pressure: nil, precipitation: nil, distance: nil))
        var prefs = Preferences()
        prefs.unitOverrides.temperature = "celsius"
        prefs.unitOverrides.wind = "mph"
        let again = try JSONDecoder().decode(Preferences.self, from: JSONEncoder().encode(prefs))
        XCTAssertEqual(again.unitOverrides, prefs.unitOverrides)
        // The UK: °C with mph, in the core's settings.
        let s = again.settings(withKey: false)
        XCTAssertEqual(s.unitOverrides.temperature, .celsius)
        XCTAssertEqual(s.unitOverrides.wind, .mph)
        XCTAssertNil(s.unitOverrides.pressure)
        // The labels are the core's symbols.
        XCTAssertEqual(again.unitOverrides.summary(), "°C, mph")
        XCTAssertEqual(unitLabels().wind.map(\.label), ["km/h", "mph", "m/s", "kn", "Bft"])
        // A unit the core no longer lists reads as Automatic.
        let stale = try decode(#"{"unitOverrides":{"wind":"furlongs","pressure":"inHg"}}"#)
        XCTAssertNil(stale.settings(withKey: false).unitOverrides.wind)
        XCTAssertEqual(stale.settings(withKey: false).unitOverrides.pressure, .inHg)
    }

    func testADesktopUnitsTableBecomesPicks() throws {
        let imported = try importConfig(
            text: """
                [[location]]
                name = "Home"
                latitude = 51.5
                longitude = -0.13
                [general]
                units = "custom"
                [units]
                temperature = "celsius"
                wind = "kn"
                """)
        XCTAssertEqual(imported.units, .metric)
        let picks = Preferences.UnitPicks(try XCTUnwrap(imported.unitOverrides))
        XCTAssertEqual(picks.temperature, "celsius")
        XCTAssertEqual(picks.wind, "knots")
        XCTAssertNil(picks.pressure)
        XCTAssertEqual(picks.summary(), "°C, kn")
    }

    func testRefreshIntervalDefaultsAndDecodes() throws {
        XCTAssertEqual(Preferences().refreshMinutes, 15)
        XCTAssertEqual(try decode(#"{"units":"metric"}"#).refreshMinutes, 15, "saved without the key")
        let saved = try decode(#"{"refreshMinutes":60}"#)
        XCTAssertEqual(saved.refreshMinutes, 60)
        XCTAssertEqual(saved.fetchDueAfter, 40 * 60, "two thirds of the interval")
        XCTAssertEqual(saved.backgroundInterval, 60 * 60)
        XCTAssertEqual(Preferences().fetchDueAfter, 10 * 60, "the old fixed threshold at 15 minutes")
        XCTAssertEqual(Preferences().backgroundInterval, 15 * 60)
        XCTAssertEqual(Preferences.refreshChoices, [15, 30, 60])
    }

    func testMetNorwayBackupDefaultsOnAndDecodes() throws {
        XCTAssertTrue(Preferences().metNorway)
        XCTAssertTrue(try decode(#"{"units":"metric"}"#).metNorway, "saved without the key: the default")
        let off = try decode(#"{"metNorway":false}"#)
        XCTAssertFalse(off.metNorway, "a saved off stays off")
        XCTAssertFalse(off.settings(withKey: false).metNorway, "the core gets the switch")
        XCTAssertTrue(Preferences().settings(withKey: false).metNorway)
        // The core never offers a contact for a placeholder URL (an API contact).
        if let contact = apiContact() {
            XCTAssertFalse(contact.contains(".invalid"))
        }
    }

    func testTheCoreMarksDataStaleByTheChosenInterval() throws {
        XCTAssertEqual(Preferences().settings(withKey: false).refreshMinutes, 15)
        let hour = try decode(#"{"refreshMinutes":60}"#)
        XCTAssertEqual(hour.settings(withKey: false).refreshMinutes, 60)
    }

    func testTheCoreSpeaksThePhonesLanguage() {
        // The phone's language, region for the locale.
        let german = Settings.phone(locale: Locale(identifier: "de_DE"))
        XCTAssertEqual(german.language, "de")
        XCTAssertEqual(german.locale, "de_DE")
        XCTAssertEqual(Settings.phone(locale: Locale(identifier: "en_GB")).language, "en")
    }

    func testAnUnreadableFieldDoesNotLoseTheOthers() throws {
        let prefs = try decode(#"{"units":"furlongs","days":3}"#)
        XCTAssertEqual(prefs.units, Preferences().units)
        XCTAssertEqual(prefs.days, 3)
    }

    func testTheOldNoaaBlendSwitchBecomesModels() throws {
        let prefs = try decode(#"{"noaaBlend":true}"#)
        XCTAssertFalse(prefs.models.isEmpty)
        XCTAssertEqual(try decode(#"{"noaaBlend":false}"#).models, [])
    }

    func testPreferencesRoundTrip() throws {
        var prefs = Preferences()
        prefs.units = .imperial
        prefs.hide(.history)
        let back = try JSONDecoder().decode(Preferences.self, from: JSONEncoder().encode(prefs))
        XCTAssertEqual(back, prefs)
    }

    // MARK: Sections

    func testAlertsAreForcedShownEvenIfSavedHidden() throws {
        let prefs = try decode(
            #"{"sections":[{"section":"alerts","shown":false},{"section":"now","shown":false},{"section":"radar","shown":false}]}"#
        )
        XCTAssertTrue(prefs.sections.first { $0.section == .alerts }!.shown)
        XCTAssertTrue(prefs.sections.first { $0.section == .now }!.shown)
        XCTAssertFalse(prefs.sections.first { $0.section == .radar }!.shown)
        // Sections the old list lacked come back, once each.
        XCTAssertEqual(Set(prefs.sections.map(\.section)), Set(Preferences.offeredSections))
        XCTAssertEqual(prefs.sections.count, Preferences.offeredSections.count)
    }

    func testRadarStartsHiddenAndSunMovedToTheHourlyTab() {
        let fresh = Preferences()
        XCTAssertEqual(fresh.sections.last, Preferences.SectionChoice(section: .radar, shown: false))
        XCTAssertFalse(fresh.sections.contains { $0.section == .sun })
        XCTAssertTrue(shownBeforeHidden(fresh.sections))
        // A list saved with the sun keeps its order without it.
        let old = try? decode(
            #"{"sections":[{"section":"sun","shown":true},{"section":"now","shown":true},{"section":"radar","shown":true}]}"#
        )
        XCTAssertEqual(old?.sections.prefix(2).map(\.section), [.now, .radar])
        XCTAssertFalse(old?.sections.contains { $0.section == .sun } ?? true)
        XCTAssertEqual(old?.appIcon, .seasonal)
    }

    func testAlertsAndNowCannotBeHidden() {
        var prefs = Preferences()
        prefs.hide(.alerts)
        prefs.hide(.now)
        XCTAssertTrue(prefs.sections.filter { $0.section == .alerts || $0.section == .now }.allSatisfy(\.shown))
        XCTAssertFalse(Preferences.hideable(.alerts))
        XCTAssertTrue(Preferences.hideable(.radar))
    }

    private func shownBeforeHidden(_ sections: [Preferences.SectionChoice]) -> Bool {
        let flags = sections.map(\.shown)
        return flags == flags.sorted { $0 && !$1 }
    }

    func testHideShowAndMoveKeepShownSectionsFirst() {
        var prefs = Preferences()
        prefs.show(.radar)
        prefs.hide(.radar)
        prefs.hide(.history)
        XCTAssertTrue(shownBeforeHidden(prefs.sections))
        XCTAssertEqual(prefs.sections.filter { !$0.shown }.map(\.section), [.radar, .history])
        prefs.show(.history)
        let shown = prefs.sections.filter(\.shown).map(\.section)
        XCTAssertEqual(shown.last, .history, "shown again after the shown ones")
        XCTAssertEqual(prefs.sections.last?.section, .radar)
        prefs.moveShown(from: IndexSet(integer: 0), to: 3)
        XCTAssertEqual(prefs.sections.last?.section, .radar, "hidden stay at the end")
        XCTAssertEqual(prefs.sections.filter(\.shown).count, shown.count)
    }

    // MARK: Grids

    private struct Tile: GridChoice {
        var id: String
        var shown = true
        var description: String { id }
    }

    private func tiles(_ spec: String) -> [Tile] {
        spec.split(separator: " ").map { Tile(id: String($0.dropLast()), shown: $0.hasSuffix("+")) }
    }

    private func spec(_ tiles: [Tile]) -> String {
        tiles.map { $0.id + ($0.shown ? "+" : "-") }.joined(separator: " ")
    }

    func testHidingSendsATileToTheEndAndShowingPutsItAfterTheShownOnes() {
        var grid = tiles("a+ b+ c+ d-")
        GridOrder.set(&grid, grid[0], shown: false)
        XCTAssertEqual(spec(grid), "b+ c+ d- a-")
        GridOrder.set(&grid, grid[2], shown: true)
        XCTAssertEqual(spec(grid), "b+ c+ d+ a-")
        GridOrder.set(&grid, Tile(id: "zzz"), shown: true)
        XCTAssertEqual(spec(grid), "b+ c+ d+ a-", "an unknown tile changes nothing")
    }

    func testDraggingMovesATileOverAnother() {
        var grid = tiles("a+ b+ c+ d+")
        GridOrder.move(&grid, dragging: "a", over: "c")
        XCTAssertEqual(spec(grid), "b+ c+ a+ d+")
        GridOrder.move(&grid, dragging: "d", over: "b")
        XCTAssertEqual(spec(grid), "d+ b+ c+ a+")
        GridOrder.move(&grid, dragging: "b", over: "b")
        GridOrder.move(&grid, dragging: "nope", over: "b")
        XCTAssertEqual(spec(grid), "d+ b+ c+ a+")
    }

    func testRelocateMatchesSwiftUIsMove() {
        var a = [0, 1, 2, 3, 4]
        a.relocate(fromOffsets: IndexSet(integer: 1), toOffset: 4)
        XCTAssertEqual(a, [0, 2, 3, 1, 4])
        a.relocate(fromOffsets: IndexSet(integer: 4), toOffset: 0)
        XCTAssertEqual(a, [4, 0, 2, 3, 1])
        var b = [0, 1, 2, 3, 4]
        b.relocate(fromOffsets: IndexSet([0, 2]), toOffset: 5)
        XCTAssertEqual(b, [1, 3, 4, 0, 2])
        b.relocate(fromOffsets: IndexSet(integer: 2), toOffset: 2)
        XCTAssertEqual(b, [1, 3, 4, 0, 2])
    }

    func testDetailCardsTheCoreGainsAreAddedHidden() {
        let cards = Preferences.defaultDetails
        let more = Preferences.addingMissing(cards, kinds: ["wind", "tide", "sunshine"])
        XCTAssertEqual(more.count, cards.count + 2)
        XCTAssertEqual(more.suffix(2).map(\.kind), ["tide", "sunshine"])
        XCTAssertTrue(more.suffix(2).allSatisfy { !$0.shown })
        XCTAssertEqual(Preferences.addingMissing(cards, kinds: ["wind"]), cards)
    }

    // MARK: Place identity

    func testOldPlacesWithoutIdsGetIdsFromTheirNames() throws {
        // `places.json` as versions before this change saved it (no `id`).
        let json = """
            [{"name":"New York","detail":"New York, US","latitude":40.7143,
              "longitude":-74.00597,"timeZone":"America/New_York"},
             {"name":"Biel/Bienne","latitude":47.14,"longitude":7.25,"timeZone":"Europe/Zurich"}]
            """
        let places = try JSONDecoder().decode([AppPlace].self, from: Data(json.utf8))
        XCTAssertEqual(places.map(\.id), ["name:New York", "name:Biel/Bienne"])
        XCTAssertEqual(places[0].detail, "New York, US")
        XCTAssertNil(places[1].detail)
        XCTAssertEqual(places[1].timeZone, "Europe/Zurich")
    }

    func testAPlaceKeepsItsIdThroughEncoding() throws {
        let place = somewhere
        XCTAssertNotEqual(place.id, "name:Somewhere")
        let back = try JSONDecoder().decode(AppPlace.self, from: JSONEncoder().encode(place))
        XCTAssertEqual(back, place)
    }

    func testCacheNamesAreSafe() {
        let dir = URL(filePath: "/tmp/caches", directoryHint: .isDirectory)
        let place = AppPlace(
            id: "name:Biel/Bienne", name: "Biel/Bienne", detail: nil, latitude: 47.14,
            longitude: 7.25, timeZone: "Europe/Zurich")
        let url = place.cacheURL(in: dir)
        XCTAssertEqual(url.lastPathComponent, "name_Biel_Bienne.json")
        XCTAssertEqual(url.deletingLastPathComponent().standardizedFileURL, dir.standardizedFileURL)
    }

    func testSameNamedPlacesFarApartAreNotDuplicates() {
        let missouri = AppPlace(
            name: "Springfield", detail: "Missouri, US", latitude: 37.209, longitude: -93.292,
            timeZone: "America/Chicago")
        let illinois = AppPlace(
            name: "Springfield", detail: "Illinois, US", latitude: 39.799, longitude: -89.644,
            timeZone: "America/Chicago")
        XCTAssertFalse(AppPlace.isDuplicate(missouri, of: illinois))
        // A few hundred metres away, under another name: the same place.
        var nearby = missouri
        nearby.name = "Downtown Springfield"
        nearby.latitude += 0.004
        XCTAssertTrue(AppPlace.isDuplicate(nearby, of: missouri))
    }

    // MARK: Alert sources

    func testAlertsArriveFromTheNWSOrOneCall() {
        let new_york = AppPlace(
            name: "New York", detail: "New York, US", latitude: 40.7143, longitude: -74.00597,
            timeZone: "America/New_York")
        let london = AppPlace(
            name: "London", detail: "England, GB", latitude: 51.5, longitude: -0.13,
            timeZone: "Europe/London")
        XCTAssertTrue(AlertSources.alertsPossible(places: [london, new_york], oneCall: false, appleAlerts: false))
        XCTAssertFalse(AlertSources.alertsPossible(places: [london], oneCall: false, appleAlerts: false))
        XCTAssertTrue(AlertSources.alertsPossible(places: [london], oneCall: true, appleAlerts: false))
        XCTAssertFalse(AlertSources.alertsPossible(places: [], oneCall: false, appleAlerts: false))
        XCTAssertTrue(AlertSources.alertsPossible(places: [], oneCall: true, appleAlerts: false))
    }

    func testALegacyCacheIsFoundThroughTheFallback() throws {
        let dir = FileManager.default.temporaryDirectory.appending(
            path: UUID().uuidString, directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let place = AppPlace(
            id: "name:Queens", name: "Queens", detail: nil, latitude: 40.73,
            longitude: -73.8, timeZone: "America/New_York")
        XCTAssertNil(place.existingCacheURL(in: dir))
        try Data("{}".utf8).write(to: dir.appending(path: "Queens.json"))
        XCTAssertEqual(place.existingCacheURL(in: dir)?.lastPathComponent, "Queens.json")
        try Data("{}".utf8).write(to: place.cacheURL(in: dir))
        XCTAssertEqual(place.existingCacheURL(in: dir)?.lastPathComponent, "name_Queens.json")
    }

    // MARK: The widget's place

    private func places(_ ids: String...) -> [AppPlace] {
        ids.map { AppPlace(id: $0, name: $0, detail: nil, latitude: 0, longitude: 0, timeZone: "UTC") }
    }

    func testTheWidgetFollowsItsPlaceThroughAReorder() {
        let before = places("a", "b", "c")
        XCTAssertEqual(WidgetData.chosenIndex(places: before, chosenID: "b", legacyIndex: nil, active: 0), 1)
        let after = places("b", "c", "a")
        XCTAssertEqual(WidgetData.chosenIndex(places: after, chosenID: "b", legacyIndex: nil, active: 2), 0)
    }

    func testADeletedPlaceFallsBackToTheAppsActivePlace() {
        let saved = places("a", "c")
        XCTAssertEqual(WidgetData.chosenIndex(places: saved, chosenID: "b", legacyIndex: 1, active: 1), 1)
        XCTAssertEqual(WidgetData.chosenIndex(places: saved, chosenID: "b", legacyIndex: nil, active: 7), 1)
    }

    func testTheLegacyIndexIsUsedOnlyWithoutAnId() {
        let saved = places("a", "b", "c")
        XCTAssertEqual(WidgetData.chosenIndex(places: saved, chosenID: nil, legacyIndex: 2, active: 0), 2)
        XCTAssertEqual(WidgetData.chosenIndex(places: saved, chosenID: nil, legacyIndex: 9, active: 0), 2)
        XCTAssertEqual(WidgetData.chosenIndex(places: saved, chosenID: nil, legacyIndex: nil, active: 1), 1)
    }

    func testNoPlacesNoChoice() {
        XCTAssertNil(WidgetData.chosenIndex(places: [], chosenID: "a", legacyIndex: 0, active: 0))
        XCTAssertNil(WidgetData.nextID(places: [], after: nil))
    }

    func testTheButtonCyclesThroughTheIdsInOrder() {
        let saved = places("a", "b", "c")
        XCTAssertEqual(WidgetData.nextID(places: saved, after: 0), "b")
        XCTAssertEqual(WidgetData.nextID(places: saved, after: 2), "a")
    }

    func testTheLegacyIndexIsConvertedOnceAndRemoved() throws {
        let defaults = try XCTUnwrap(UserDefaults(suiteName: "widget-choice-test"))
        defer { defaults.removePersistentDomain(forName: "widget-choice-test") }
        defaults.set(1, forKey: WidgetData.legacyPlaceKey)
        let saved = places("a", "b")
        XCTAssertEqual(WidgetData.chosenIndex(places: saved, defaults: defaults, favouriteID: nil), 1)
        XCTAssertEqual(defaults.string(forKey: WidgetData.placeKey), "b")
        XCTAssertNil(defaults.object(forKey: WidgetData.legacyPlaceKey))
        // The id now wins, whatever the order.
        XCTAssertEqual(WidgetData.chosenIndex(places: places("b", "a"), defaults: defaults, favouriteID: nil), 0)
    }

    // MARK: Plans

    private func date(_ y: Int, _ m: Int, _ d: Int) -> Date {
        Calendar.current.date(from: DateComponents(year: y, month: m, day: d, hour: 12))!
    }

    private var somewhere: AppPlace {
        AppPlace(name: "Somewhere", detail: nil, latitude: 1, longitude: 2, timeZone: "UTC")
    }

    func testAPlanAcrossNewYearBuildsTheRightTrip() {
        var plan = SavedPlan.new(at: somewhere, number: 1)
        plan.from = date(2026, 12, 28)
        plan.to = date(2027, 1, 3)
        plan.years = 10
        let trip = plan.trip(lastYear: 2025)
        XCTAssertEqual(
            [trip.fromYear, Int32(trip.fromMonth), Int32(trip.fromDay)], [2026, 12, 28])
        XCTAssertEqual([trip.toYear, Int32(trip.toMonth), Int32(trip.toDay)], [2027, 1, 3])
        XCTAssertEqual([trip.firstYear, trip.lastYear], [2016, 2025])
    }

    private func calendar(_ zone: String) -> Calendar {
        var c = Calendar(identifier: .gregorian)
        // INVARIANT: the zones named in the tests exist.
        c.timeZone = TimeZone(identifier: zone)!
        return c
    }

    func testAPlanAcrossNewYearReadsItsDaysDirectly() {
        var plan = SavedPlan.new(at: somewhere, number: 1)
        plan.fromDay = PlanDay(year: 2026, month: 12, day: 28)
        plan.toDay = PlanDay(year: 2027, month: 1, day: 3)
        plan.years = 30
        let trip = plan.trip(lastYear: 2025)
        XCTAssertEqual(
            [trip.fromYear, Int32(trip.fromMonth), Int32(trip.fromDay)], [2026, 12, 28])
        XCTAssertEqual([trip.toYear, Int32(trip.toMonth), Int32(trip.toDay)], [2027, 1, 3])
        XCTAssertEqual([trip.firstYear, trip.lastYear], [1996, 2025])
    }

    func testAPlanSavedWithInstantsBecomesDaysOnceInTheDevicesZone() throws {
        let ny = calendar("America/New_York")
        let from = try XCTUnwrap(ny.date(from: DateComponents(year: 2027, month: 7, day: 12, hour: 22)))
        let to = try XCTUnwrap(ny.date(from: DateComponents(year: 2027, month: 7, day: 19, hour: 9)))
        // As the earlier version wrote it: dates as seconds since 2001.
        let old = """
            {"id":"E621E1F8-C36C-495A-93FC-0C247A3E6E5F","name":"Summer",
             "place":{"name":"Somewhere","latitude":1,"longitude":2,"timeZone":"UTC"},
             "from":\(from.timeIntervalSinceReferenceDate),"to":\(to.timeIntervalSinceReferenceDate),
             "years":12,"looked":true}
            """
        let decoder = JSONDecoder()
        decoder.userInfo[SavedPlan.calendarKey] = ny
        let plan = try decoder.decode(SavedPlan.self, from: Data(old.utf8))
        XCTAssertEqual(plan.fromDay, PlanDay(year: 2027, month: 7, day: 12))
        XCTAssertEqual(plan.toDay, PlanDay(year: 2027, month: 7, day: 19))
        XCTAssertEqual(plan.name, "Summer")
        XCTAssertEqual(plan.id.uuidString, "E621E1F8-C36C-495A-93FC-0C247A3E6E5F")
        XCTAssertEqual(plan.years, 12)
        XCTAssertTrue(plan.looked)
        // Saved again, it is days: the instants are not written.
        let saved = try JSONSerialization.jsonObject(with: JSONEncoder().encode(plan)) as? [String: Any]
        XCTAssertEqual(saved?["fromDay"] as? String, "2027-07-12")
        XCTAssertNil(saved?["from"])
    }

    func testAnOldPlanWithFewFieldsKeepsTodaysDefaults() throws {
        let old = """
            {"name":"Summer","place":{"name":"Somewhere","latitude":1,"longitude":2,"timeZone":"UTC"},
             "from":0,"to":86400}
            """
        let plan = try JSONDecoder().decode(SavedPlan.self, from: Data(old.utf8))
        XCTAssertEqual(plan.years, 30)
        XCTAssertFalse(plan.looked)
        XCTAssertEqual(plan.fromDay, PlanDay(Date(timeIntervalSinceReferenceDate: 0)))
    }

    func testAPlanMadeInNewYorkKeepsItsDaysWhenReadInParis() throws {
        let ny = calendar("America/New_York")
        let paris = calendar("Europe/Paris")
        // 10 pm on July 12 in New York is already July 13 in Paris.
        let evening = try XCTUnwrap(
            ny.date(from: DateComponents(year: 2027, month: 7, day: 12, hour: 22)))
        XCTAssertEqual(PlanDay(evening, in: paris).day, 13, "an instant moves with the zone")
        var plan = SavedPlan.new(at: somewhere, number: 1)
        plan.fromDay = PlanDay(evening, in: ny)
        plan.toDay = PlanDay(year: 2027, month: 7, day: 19)
        let sent = try JSONEncoder().encode(plan)
        let read = try JSONDecoder().decode(SavedPlan.self, from: sent)
        XCTAssertEqual(read.fromDay, PlanDay(year: 2027, month: 7, day: 12))
        // Shown in Paris, the start of the day is still the 12th and the 19th.
        let start = read.fromDay.date(in: paris)
        XCTAssertEqual(paris.dateComponents([.day, .hour], from: start).day, 12)
        XCTAssertEqual(paris.dateComponents([.day, .hour], from: start).hour, 0)
        XCTAssertEqual(PlanDay(read.toDay.date(in: paris), in: paris), read.toDay)
    }

    func testANewSaveRoundTripsAsDays() throws {
        var plan = SavedPlan.new(at: somewhere, number: 3)
        plan.fromDay = PlanDay(year: 2027, month: 7, day: 12)
        plan.toDay = PlanDay(year: 2027, month: 7, day: 19)
        plan.years = 7
        plan.looked = true
        let data = try JSONEncoder().encode(plan)
        let saved = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        XCTAssertEqual(saved?["fromDay"] as? String, "2027-07-12")
        XCTAssertEqual(saved?["toDay"] as? String, "2027-07-19")
        XCTAssertEqual(try JSONDecoder().decode(SavedPlan.self, from: data), plan)
    }

    func testADayThatIsNotOneIsRefused() {
        for text in ["2027-13-01", "2027-07-32", "2027-07", "July 12", "0000-01-01"] {
            XCTAssertThrowsError(
                try JSONDecoder().decode(PlanDay.self, from: Data("\"\(text)\"".utf8)), text)
        }
        XCTAssertLessThan(PlanDay(year: 2026, month: 12, day: 31), PlanDay(year: 2027, month: 1, day: 1))
    }

    func testADayThatDoesNotExistIsRefused() throws {
        for text in ["2027-02-31", "2027-02-29", "2027-04-31", "1900-02-29", "2027-06-00"] {
            XCTAssertThrowsError(
                try JSONDecoder().decode(PlanDay.self, from: Data("\"\(text)\"".utf8)), text)
        }
        for text in ["2028-02-29", "2000-02-29", "2027-02-28", "2027-12-31", "2027-04-30"] {
            let day = try JSONDecoder().decode(PlanDay.self, from: Data("\"\(text)\"".utf8))
            XCTAssertTrue(day.exists, text)
        }
        XCTAssertFalse(PlanDay(year: 2027, month: 2, day: 29).exists)
        XCTAssertTrue(PlanDay(year: 2028, month: 2, day: 29).exists)
    }

    func testDaysBetweenTwoDaysAreCountedOnTheCalendar() {
        let july12 = PlanDay(year: 2027, month: 7, day: 12)
        XCTAssertEqual(july12.days(to: july12), 0)
        XCTAssertEqual(july12.days(to: PlanDay(year: 2027, month: 7, day: 19)), 7)
        XCTAssertEqual(PlanDay(year: 2027, month: 7, day: 19).days(to: july12), -7)
        XCTAssertEqual(
            PlanDay(year: 2027, month: 12, day: 30).days(to: PlanDay(year: 2028, month: 1, day: 2)), 3)
        XCTAssertEqual(
            PlanDay(year: 2028, month: 2, day: 28).days(to: PlanDay(year: 2028, month: 3, day: 1)), 2,
            "a leap year")
        XCTAssertEqual(
            PlanDay(year: 2027, month: 2, day: 28).days(to: PlanDay(year: 2027, month: 3, day: 1)), 1)
        // Across the clocks going forward in New York: still whole days.
        XCTAssertEqual(
            PlanDay(year: 2027, month: 3, day: 13).days(to: PlanDay(year: 2027, month: 3, day: 15)), 2)
    }

    func testNewPlansAreNamedByNumberAndLastAWeek() {
        XCTAssertEqual(SavedPlan.new(at: somewhere, number: 1).name, "New plan")
        let second = SavedPlan.new(at: somewhere, number: 2)
        XCTAssertEqual(second.name, "New plan 2")
        let days = Calendar.current.dateComponents([.day], from: second.from, to: second.to).day
        XCTAssertEqual(days, 6)
        XCTAssertFalse(second.looked)
        XCTAssertEqual(second.years, 30)
    }

    // MARK: The hours' window

    /// 24 hours of 46 pt in a 322 pt window: the strip scrolls 782 pt, and the
    /// needle travels 138 pt to the middle (161) over 276 pt of scrolling at each end.
    private let track = HourTrack(step: 46, viewport: 322, count: 24)

    func testTheNeedleStartsLeftStaysInTheMiddleAndEndsRight() {
        XCTAssertEqual(track.maxOffset, 782)
        XCTAssertEqual(track.lead, 138)
        XCTAssertEqual(track.ramp, 276)
        XCTAssertEqual(track.needle(atOffset: 0), 23, "on the first hour at the left edge")
        XCTAssertEqual(track.needle(atOffset: 138), 92, accuracy: 0.001, "halfway there, halfway scrolled")
        XCTAssertEqual(track.needle(atOffset: 276), 161, "in the middle")
        XCTAssertEqual(track.needle(atOffset: 400), 161, "stays there")
        XCTAssertEqual(track.needle(atOffset: 782), 299, "on the last hour at the right edge")
        XCTAssertEqual(track.needle(atOffset: -50), 23, "a bounce past the start")
    }

    func testTheChosenHourNeverRacesAheadOfTheStrip() {
        // The hour under the needle moves at most about 1.75 times as fast as the
        // strip, easing in and out: no sudden change of speed anywhere.
        var previous = track.cursor(atOffset: 0)
        var fastest: CGFloat = 0
        for o in stride(from: CGFloat(1), through: 782, by: 1) {
            let now = track.cursor(atOffset: o)
            XCTAssertGreaterThan(now, previous, "always moving forward")
            fastest = max(fastest, now - previous)
            previous = now
        }
        XCTAssertLessThanOrEqual(fastest, 1.76)
        XCTAssertEqual(track.cursor(atOffset: 1) - track.cursor(atOffset: 0), 1, accuracy: 0.01, "starts at the strip's speed")
    }

    func testTheHourUnderTheNeedleFollowsTheScroll() {
        XCTAssertEqual(track.hour(atOffset: 0), 0)
        XCTAssertEqual(track.hour(atOffset: 46), 1)
        XCTAssertEqual(track.hour(atOffset: 138), 5, "the needle's travel adds three hours")
        XCTAssertEqual(track.hour(atOffset: 322), 10, "one hour per hour in the middle")
        XCTAssertEqual(track.hour(atOffset: 782), 23)
        XCTAssertEqual(track.hour(atOffset: 9999), 23)
    }

    func testEveryHourHasAnOffsetThatPutsItUnderTheNeedle() {
        XCTAssertEqual(track.offset(of: 0), 0, accuracy: 0.01)
        XCTAssertEqual(track.offset(of: 10), 322)
        XCTAssertEqual(track.offset(of: 23), 782, accuracy: 0.01)
        for hour in 0..<24 {
            let o = track.offset(of: hour)
            XCTAssertEqual(track.cursor(atOffset: o), (CGFloat(hour) + 0.5) * 46, accuracy: 0.01, "hour \(hour)")
        }
    }

    func testAScrollSettlesWithTheNearestHourUnderTheNeedle() {
        XCTAssertEqual(track.cursor(atOffset: track.settle(30)), 69, accuracy: 0.01, "hour 1")
        XCTAssertEqual(track.settle(330), 322, "hour 10")
        XCTAssertEqual(track.settle(-40), 0, accuracy: 0.01, "never before the first")
        XCTAssertEqual(track.settle(5000), 782, accuracy: 0.01, "never past the last")
    }

    func testATapBringsTheNeedleToTheTappedHour() {
        XCTAssertEqual(track.hour(atTap: 0, offset: 0), 0)
        XCTAssertEqual(track.hour(atTap: 300, offset: 0), 6)
        XCTAssertEqual(track.hour(atTap: 161, offset: 322), 10)
        XCTAssertEqual(track.hour(atTap: 9999, offset: 782), 23)
    }

    func testThePinchStopsWhereTheNeedleCouldNoLongerReachTheMiddle() {
        XCTAssertEqual(track.narrowest, 2 * 322 / 25, accuracy: 0.001)
        let narrow = HourTrack(step: track.narrowest, viewport: 322, count: 24)
        XCTAssertGreaterThanOrEqual(narrow.maxOffset + 0.001, 2 * narrow.lead)
        XCTAssertEqual(narrow.needle(atOffset: narrow.maxOffset), 322 - narrow.step / 2, accuracy: 0.001)
    }

    func testAPinchZoomsWithinItsRange() {
        XCTAssertEqual(HourWindow.zoomed(54, by: 0.5, within: 7...108), 27)
        XCTAssertEqual(HourWindow.zoomed(54, by: 0.01, within: 7...108), 7, "48 hours across, no fewer pixels")
        XCTAssertEqual(HourWindow.zoomed(54, by: 5, within: 7...108), 108)
    }

    func testLabelsThinOutOnRoundHoursAsTheHoursNarrow() {
        XCTAssertEqual(HourWindow.labelStride(step: 54, spacing: 40), 1, "every hour")
        XCTAssertEqual(HourWindow.labelStride(step: 30, spacing: 40), 2)
        XCTAssertEqual(HourWindow.labelStride(step: 12, spacing: 40), 4)
        XCTAssertEqual(HourWindow.labelStride(step: 8, spacing: 40), 6, "not 5: round hours")
        XCTAssertEqual(HourWindow.labelStride(step: 1, spacing: 40), 24)
    }

    // MARK: Apple's minutes (WeatherKit economy)

    func testAppleIsAskedOnlyForTheShownPlaceWithRainNearAndTenMinutesApart() {
        let now: Int64 = 100_000
        func ask(shown: Bool = true, only: Bool = false, wanted: Bool = true, last: Int64? = nil) -> Bool {
            MinutesPolicy.shouldAsk(isShown: shown, onlyPlace: only, wanted: wanted, lastAskedAt: last, now: now)
        }
        XCTAssertTrue(ask(), "never asked: ask")
        XCTAssertFalse(ask(shown: false), "another page")
        XCTAssertTrue(ask(shown: false, only: true), "the only place counts as shown")
        XCTAssertFalse(ask(wanted: false), "a dry two hours")
        XCTAssertFalse(ask(last: now - 599), "asked 9 minutes 59 seconds ago")
        XCTAssertTrue(ask(last: now - 600), "asked ten minutes ago")
    }

    func testMinutesAreKeptForLessThanAnHour() {
        let now: Int64 = 100_000
        XCTAssertTrue(MinutesPolicy.keeps(askedAt: now - 3_599, now: now))
        XCTAssertFalse(MinutesPolicy.keeps(askedAt: now - 3_600, now: now))
        XCTAssertFalse(MinutesPolicy.keeps(askedAt: nil, now: now), "caches from older builds")
    }

    func testKeptMinutesRoundTrip() throws {
        let kept = [KeptMinute(time: 60, mmPerHour: 1.5), KeptMinute(time: 120, mmPerHour: 0)]
        let back = try JSONDecoder().decode([KeptMinute].self, from: JSONEncoder().encode(kept))
        XCTAssertEqual(back, kept)
    }
}
