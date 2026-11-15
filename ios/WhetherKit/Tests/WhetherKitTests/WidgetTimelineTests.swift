import XCTest

@testable import WhetherKit

/// The widget's forecast choice, its own kept forecast, its timeline and its view.
final class WidgetTimelineTests: XCTestCase {
    private let place = AppPlace(
        id: "a", name: "A", detail: nil, latitude: 40.75, longitude: -73.99, timeZone: "America/New_York")
    private let t0: Int64 = 1_800_000_000

    private func own(at: Int64, latitude: Double = 40.75) -> WidgetForecast {
        WidgetForecast(snapshotJson: "own", fetchedAt: at, latitude: latitude, longitude: -73.99)
    }

    private func date(_ seconds: Int64) -> Date { Date(timeIntervalSince1970: TimeInterval(seconds)) }

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

    // MARK: The widget's view

    func testAWidgetsViewHoldsOnlyWhatAWidgetShows() {
        var app = defaultSettings()
        app.feelsLike = true
        app.nowcastHours = 1
        let overview = WidgetData.viewSettings(app, content: .overview)
        XCTAssertTrue(overview.feelsLike, "the app's temperature")
        XCTAssertEqual(overview.hourlyHours, 13, "now and the large widget's 12 hours")
        XCTAssertEqual(overview.dailyDays, 5)
        XCTAssertFalse(overview.dayHours)
        XCTAssertFalse(overview.history)
        XCTAssertEqual(overview.nowcastHours, 1, "the app's rain graph")
        XCTAssertEqual(WidgetData.viewSettings(app, content: .today).dailyDays, 6)
        XCTAssertEqual(WidgetData.viewSettings(app, content: .nextHours).dailyDays, 1, "today's high and low")
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
