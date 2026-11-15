import XCTest

@testable import WhetherKit

/// The widget's forecast choice, its own kept forecast, its timeline and its options.
final class WidgetTimelineTests: XCTestCase {
    private let place = AppPlace(
        id: "a", name: "A", detail: nil, latitude: 40.75, longitude: -73.99, timeZone: "America/New_York")
    private let t0: Int64 = 1_800_000_000

    private func own(at: Int64, latitude: Double = 40.75) -> WidgetForecast {
        WidgetForecast(snapshotJson: "own", fetchedAt: at, latitude: latitude, longitude: -73.99)
    }

    private func date(_ seconds: Int64) -> Date { Date(timeIntervalSince1970: TimeInterval(seconds)) }

    /// The core's sample (`testdata::SAMPLE_NOW`): when `sample-snapshot.json` was observed.
    private let sampleNow: Int64 = 1_791_212_400

    private func resource(_ name: String) throws -> String {
        let url = try XCTUnwrap(Bundle.module.url(forResource: name, withExtension: "json", subdirectory: "Resources"))
        return try String(contentsOf: url, encoding: .utf8)
    }

    // MARK: Which forecast

    func testAForecastIsFreshForTwoHours() {
        XCTAssertTrue(WidgetData.isFresh(t0, now: date(t0 + 2 * 3600 - 1)))
        XCTAssertFalse(WidgetData.isFresh(t0, now: date(t0 + 2 * 3600)))
        XCTAssertFalse(WidgetData.isFresh(nil, now: date(t0)), "a forecast without a time is old")
    }

    func testTheNewestOfTheAppsAndTheWidgetsOwnIsShown() {
        let newer = WidgetData.newest(appSnapshot: "app", appAt: t0, widget: own(at: t0 + 60), place: place)
        XCTAssertEqual(newer?.json, "own")
        XCTAssertEqual(newer?.at, t0 + 60)
        let older = WidgetData.newest(appSnapshot: "app", appAt: t0, widget: own(at: t0 - 60), place: place)
        XCTAssertEqual(older?.json, "app")
        XCTAssertEqual(
            WidgetData.newest(appSnapshot: "app", appAt: nil, widget: own(at: t0), place: place)?.json, "own",
            "the app's cache without a time loses")
        XCTAssertEqual(WidgetData.newest(appSnapshot: nil, appAt: nil, widget: own(at: t0), place: place)?.json, "own")
        XCTAssertEqual(WidgetData.newest(appSnapshot: "app", appAt: t0, widget: nil, place: place)?.json, "app")
        XCTAssertNil(WidgetData.newest(appSnapshot: nil, appAt: nil, widget: nil, place: place))
    }

    func testTheWidgetsOwnForecastIsDroppedOnceThePlaceMoves() {
        // 0.05° of latitude is about 5.5 km.
        let moved = own(at: t0 + 60, latitude: 40.80)
        XCTAssertFalse(moved.belongs(to: place))
        XCTAssertTrue(own(at: t0).belongs(to: place))
        XCTAssertEqual(WidgetData.newest(appSnapshot: "app", appAt: t0, widget: moved, place: place)?.json, "app")
        XCTAssertNil(WidgetData.newest(appSnapshot: nil, appAt: nil, widget: moved, place: place))
    }

    func testTheWidgetsOwnForecastsLiveApartFromTheAppsCaches() throws {
        let dir = URL(filePath: "/tmp/whether", directoryHint: .isDirectory)
        let url = WidgetData.forecastURL(for: place, in: dir)
        XCTAssertEqual(url.deletingLastPathComponent().lastPathComponent, "widget")
        XCTAssertNotEqual(url, place.cacheURL(in: dir))
        let kept = own(at: t0)
        XCTAssertEqual(try JSONDecoder().decode(WidgetForecast.self, from: JSONEncoder().encode(kept)), kept)
    }

    // MARK: The timeline

    func testEntriesAreNowThenTheStartOfEachNextHour() throws {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = try XCTUnwrap(TimeZone(identifier: "America/New_York"))
        // Not on the hour.
        let now = date(1_799_916_440)
        let dates = WidgetData.entryDates(now: now, count: 4, calendar: calendar)
        XCTAssertEqual(dates.count, 4)
        XCTAssertEqual(dates[0], now)
        let hour = calendar.dateInterval(of: .hour, for: now)?.start
        XCTAssertEqual(dates[1], hour.map { $0.addingTimeInterval(3600) })
        XCTAssertEqual(dates[3].timeIntervalSince(dates[1]), 2 * 3600)
        XCTAssertEqual(calendar.component(.minute, from: dates[2]), 0)
        XCTAssertEqual(WidgetData.entryDates(now: now, count: 1, calendar: calendar), [now])
        XCTAssertEqual(WidgetData.entryDates(now: now, count: 0, calendar: calendar), [])
        XCTAssertEqual(WidgetData.entryDates(now: now, calendar: calendar).count, WidgetData.entryCount)
    }

    func testHalfHourZonesStartTheirHoursOnTheHalfHour() throws {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = try XCTUnwrap(TimeZone(identifier: "Asia/Kolkata"))
        let now = date(1_799_916_440)
        let next = WidgetData.entryDates(now: now, count: 2, calendar: calendar)[1]
        XCTAssertEqual(Int(next.timeIntervalSince1970) % 3600, 1800)
    }

    func testTheTimelineReloadsWhenItsForecastTurnsOld() {
        let now = date(t0)
        func reload(_ dataAt: Int64?, _ content: WidgetContent = .overview) -> TimeInterval {
            WidgetData.reloadDate(now: now, dataAt: dataAt, content: content).timeIntervalSince(now)
        }
        XCTAssertEqual(reload(t0), 2 * 3600, "just fetched: two hours")
        XCTAssertEqual(reload(t0 - 3600), 3600, "an hour old: in an hour")
        XCTAssertEqual(reload(t0 - 2 * 3600 + 60), 30 * 60, "never sooner than half an hour")
        XCTAssertEqual(reload(t0 - 5 * 3600), 30 * 60, "old (a fetch failed): half an hour")
        XCTAssertEqual(reload(nil), 30 * 60)
        XCTAssertEqual(reload(t0 + 600), 2 * 3600, "never later than two hours")
        XCTAssertEqual(reload(t0, .nextHour), 30 * 60, "the next hour's rain goes stale by the minute")
        for content in [WidgetContent.nextHours, .today] {
            XCTAssertEqual(reload(t0, content), 2 * 3600)
        }
    }

    func testTheHoursForecastLeadsOnceTheObservationIsAnHourOld() {
        XCTAssertFalse(WidgetData.leadsWithHour(at: date(t0 + 3599), observedAt: t0))
        XCTAssertTrue(WidgetData.leadsWithHour(at: date(t0 + 3600), observedAt: t0))
        XCTAssertFalse(WidgetData.leadsWithHour(at: date(t0), observedAt: nil))
    }

    func testTheObservationTimeComesFromTheSnapshot() throws {
        XCTAssertEqual(WidgetData.observedAt(snapshotJson: try resource("sample-snapshot")), sampleNow)
        XCTAssertNil(WidgetData.observedAt(snapshotJson: #"{"current": null}"#), "no current conditions")
        XCTAssertNil(WidgetData.observedAt(snapshotJson: "not json"))
    }

    func testAnHoursSkyFollowsItsWeatherAndTheSun() {
        XCTAssertEqual(WidgetData.sky(icon: "clear_day", isDay: true), "clear_day")
        XCTAssertEqual(WidgetData.sky(icon: "partly_cloudy_night", isDay: false), "clear_night")
        XCTAssertEqual(WidgetData.sky(icon: "overcast_day", isDay: true), "cloudy_day")
        XCTAssertEqual(WidgetData.sky(icon: "cloudy", isDay: false), "cloudy_night")
        XCTAssertEqual(WidgetData.sky(icon: "showers_night", isDay: false), "rain")
        XCTAssertEqual(WidgetData.sky(icon: "snow_showers_day", isDay: true), "snow")
        XCTAssertEqual(WidgetData.sky(icon: "thunderstorms_rain", isDay: true), "storm")
        XCTAssertEqual(WidgetData.sky(icon: "fog_day", isDay: true), "fog")
        XCTAssertEqual(WidgetData.sky(icon: "something_new", isDay: false), "clear_night")
        // Every sky has the theme's colors.
        for icon in ["clear_day", "cloudy", "rain", "snow", "fog_day", "thunderstorms_day"] {
            for isDay in [true, false] {
                let sky = WidgetData.sky(icon: icon, isDay: isDay)
                XCTAssertNotNil(ColorRole(rawValue: "sky.\(sky).top"), sky)
            }
        }
    }

    func testTheGaugeRunsFromTodaysLowToHigh() {
        XCTAssertEqual(WidgetData.gaugeRange(value: 60, low: 50, high: 70), 50...70)
        XCTAssertEqual(WidgetData.gaugeRange(value: 75, low: 50, high: 70), 50...75, "warmer than the high")
        XCTAssertEqual(WidgetData.gaugeRange(value: 40, low: 50, high: 70), 40...70)
        XCTAssertEqual(WidgetData.gaugeRange(value: 50, low: 50, high: 50), 50...51, "never empty")
    }

    // MARK: What leads

    func testTheObservationLeadsUntilTheHoursForecastTakesOver() throws {
        let url = try XCTUnwrap(
            Bundle.module.url(forResource: "sample-view", withExtension: "json", subdirectory: "Resources"))
        let view = try ViewModel.decode(String(contentsOf: url, encoding: .utf8))
        let current = try XCTUnwrap(view.current)
        let hour = try XCTUnwrap(view.hourly?.hours.first)
        let observed = try XCTUnwrap(WidgetNow(view, fromHour: false))
        XCTAssertEqual(observed.temperature, current.temperature)
        XCTAssertEqual(observed.icon, current.icon)
        XCTAssertEqual(observed.sky, view.theme)
        let forecast = try XCTUnwrap(WidgetNow(view, fromHour: true))
        XCTAssertEqual(forecast.temperature, hour.temperature)
        XCTAssertEqual(forecast.conditionLabel, hour.conditionLabel)
        XCTAssertEqual(forecast.sky, WidgetData.sky(icon: hour.icon, isDay: hour.isDay))
        XCTAssertEqual(forecast.high, current.high, "today's high and low either way")
        var empty = view
        empty.current = nil
        XCTAssertNil(WidgetNow(empty, fromHour: true))
    }

    // MARK: Options

    func testTheTemperatureOptionFollowsTheAppByDefault() {
        XCTAssertTrue(WidgetTemperature.app.feelsLike(app: true))
        XCTAssertFalse(WidgetTemperature.app.feelsLike(app: false))
        XCTAssertFalse(WidgetTemperature.actual.feelsLike(app: true))
        XCTAssertTrue(WidgetTemperature.feelsLike.feelsLike(app: false))
    }

    func testTheSavedNamesOfTheOptionsStayStable() {
        XCTAssertEqual(WidgetBackdrop.allCases.map(\.rawValue), ["sky", "plain"])
        XCTAssertEqual(WidgetTemperature.allCases.map(\.rawValue), ["app", "actual", "feelsLike"])
    }

    func testAWidgetsViewHoldsOnlyWhatAWidgetShows() {
        var app = defaultSettings()
        app.feelsLike = true
        app.nowcastHours = 1
        let overview = WidgetData.viewSettings(app, content: .overview, temperature: .app)
        XCTAssertTrue(overview.feelsLike)
        XCTAssertEqual(overview.hourlyHours, 13, "now and the large widget's 12 hours")
        XCTAssertEqual(overview.dailyDays, 5)
        XCTAssertFalse(overview.dayHours)
        XCTAssertFalse(overview.history)
        XCTAssertEqual(overview.nowcastHours, 1, "the app's rain graph")
        XCTAssertEqual(WidgetData.viewSettings(app, content: .today, temperature: .app).dailyDays, 6)
        XCTAssertEqual(WidgetData.viewSettings(app, content: .nextHours, temperature: .app).dailyDays, 1, "today's high and low")
        XCTAssertFalse(WidgetData.viewSettings(app, content: .overview, temperature: .actual).feelsLike)
    }

    func testTheSlimViewStillListsEveryHourAndDayTheWidgetsShow() {
        for content in WidgetContent.allCases {
            XCTAssertLessThanOrEqual(content.hours(large: true) + 1, 13, "\(content)")
            XCTAssertLessThanOrEqual(content.hours(large: false) + 1, 13, "\(content)")
        }
        XCTAssertEqual(WidgetContent.overview.days, 5)
        XCTAssertEqual(WidgetContent.today.days - 1, 5, "five days after today")
    }
}
