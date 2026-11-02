import XCTest
@testable import WhetherKit

final class WhetherKitTests: XCTestCase {
    func testTheCoreAnswers() throws {
        XCTAssertFalse(coreVersion().isEmpty)
        let json = try viewModel(locations: [], active: 0, settings: defaultSettings(), now: 0)
        XCTAssertTrue(json.contains("\"tabs\":[]"))
        XCTAssertEqual(radarFrames(latest: 1_791_663_300).count, 6)
    }

    /// The sample the Rust test `sample.rs` writes: every field decodes.
    func testTheSampleViewDecodes() throws {
        let url = try XCTUnwrap(
            Bundle.module.url(forResource: "sample-view", withExtension: "json", subdirectory: "Resources"))
        let view = try ViewModel.decode(String(contentsOf: url, encoding: .utf8))
        XCTAssertEqual(view.location.name, "New York")
        XCTAssertEqual(view.current?.temperature.text, "60°")
        XCTAssertEqual(view.current?.temperature.unit, "°F")
        XCTAssertEqual(view.tabs.count, 2)
        XCTAssertGreaterThanOrEqual(view.hourly?.hours.count ?? 0, 24)
        XCTAssertEqual(view.daily?.days.first?.label, "Today")
        XCTAssertEqual(view.hourly?.hours.first?.day, "Today")
        XCTAssertEqual(view.hourly?.hours.first?.dateShort, view.daily?.days.first?.dateShort)
        XCTAssertEqual(view.daily?.days.first?.range.count, 2)
        XCTAssertEqual(view.alerts.first?.title, "Flood Warning")
        XCTAssertEqual(view.airQuality?.pollutants.first?.first, "PM2.5")
        XCTAssertNotNil(view.nowcast)
        XCTAssertEqual(view.precipitation?.hours.count, 12)
        XCTAssertEqual(view.precipitation?.hours.first?.label, "Now")
        XCTAssertTrue(view.attribution.contains("Open-Meteo"))
        XCTAssertEqual(WeatherSymbols.symbol(view.current?.icon ?? ""), "cloud.sun.fill")
        XCTAssertEqual(WeatherSymbols.symbol(view.moon?.icon ?? ""), "moonphase.waning.crescent")
        XCTAssertEqual(chanceText(percent: 40), "40%")
        // History.
        let history = try XCTUnwrap(view.history)
        XCTAssertEqual(history.days.count, 7)
        XCTAssertEqual(history.days.last?.label, "Yesterday")
        XCTAssertEqual(history.today?.normalText, "Normal 69° / 55°")
        XCTAssertEqual(history.today?.recordHigh, "94° in 1941")
        XCTAssertTrue(history.credit.hasPrefix("NY City Central Park"))
        XCTAssertEqual(view.daily?.days.first?.normalHigh?.text, "69°")
        XCTAssertNotNil(view.daily?.days.first?.outlook)
        XCTAssertTrue(view.details.contains { $0.kind == "records" })
    }

    /// A trip planned on the phone: the core checks it before anything is asked.
    func testTripsAreChecked() {
        let july = Trip(
            fromYear: 2027, fromMonth: 7, fromDay: 12, toYear: 2027, toMonth: 7, toDay: 19,
            firstYear: 1995, lastYear: 2024)
        XCTAssertNil(tripProblem(trip: july))
        var long = july
        long.toMonth = 8
        long.toDay = 20
        XCTAssertEqual(tripProblem(trip: long), "at most 31 days")
    }

    /// Rain now: the core's condition is an object, which must not stop the view
    /// from decoding (it did, and the app showed "No forecast").
    func testAViewWithRainNowDecodes() throws {
        let url = try XCTUnwrap(
            Bundle.module.url(forResource: "sample-view", withExtension: "json", subdirectory: "Resources"))
        var json = try XCTUnwrap(
            JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any])
        var current = try XCTUnwrap(json["current"] as? [String: Any])
        current["condition"] = ["rain_showers": "light"]
        json["current"] = current
        let data = try JSONSerialization.data(withJSONObject: json)
        let view = try ViewModel.decode(String(decoding: data, as: UTF8.self))
        XCTAssertNotNil(view.current)
    }

    func testAlertsNotifyOnlyOnSignificantChanges() {
        func alert(_ level: String, end: Int64, id: String? = "a1", body: String = "Flooding.")
            -> ViewModel.Alert
        {
            ViewModel.Alert(
                id: id, title: "Flood Warning", body: body, level: level, period: "Mon", end: end)
        }
        var n = AlertNotifier()
        XCTAssertEqual(n.notices(for: [alert("poor", end: 10_000)], place: "D", now: 0).count, 1)
        XCTAssertEqual(
            n.notices(for: [alert("poor", end: 10_000, body: "Reworded.")], place: "D", now: 60)
                .count, 0, "rewording is quiet")
        XCTAssertEqual(
            n.notices(for: [alert("poor", end: 12_000)], place: "D", now: 60).count, 0,
            "33 minutes later is quiet")
        XCTAssertEqual(
            n.notices(for: [alert("poor", end: 13_600)], place: "D", now: 60).count, 1,
            "an hour later notifies")
        XCTAssertEqual(
            n.notices(for: [alert("very_poor", end: 13_600)], place: "D", now: 60).count, 1,
            "a stronger level notifies")
        XCTAssertEqual(
            n.notices(for: [alert("poor", end: 13_600)], place: "E", now: 60).count, 1,
            "another place is another alert")
        XCTAssertEqual(
            n.notices(for: [alert("poor", end: 50, id: nil)], place: "D", now: 60).count, 0,
            "ended alerts are not notified")
        _ = n.notices(for: [], place: "D", now: 20_000)
        XCTAssertTrue(n.notified.isEmpty, "ended alerts are forgotten")
    }

    func testHTTPDates() {
        XCTAssertEqual(URLSessionHost.unixTime("Sun, 11 Oct 2026 12:00:00 GMT"), 1_791_720_000)
        XCTAssertNil(URLSessionHost.unixTime("yesterday"))
    }

    /// One live fetch for New York from Open-Meteo (no key needed).
    func testLiveFetch() async throws {
        let ledger = FileManager.default.temporaryDirectory.appendingPathComponent("ledger.json")
        let place = Place(
            name: "New York", latitude: 40.7143, longitude: -74.00597, units: nil,
            utcOffsetSeconds: Int32(TimeZone(identifier: "America/New_York")!.secondsFromGMT()),
            offsetChanges: []
        )
        let out = try await fetch(
            place: place, settings: defaultSettings(), cached: Cached(snapshotJson: nil, lastModified: nil, usageJson: nil),
            now: Int64(Date().timeIntervalSince1970), host: URLSessionHost(ledgerURL: ledger)
        )
        XCTAssertEqual(out.provider, "open-meteo")
    }

    // MARK: Friendly errors

    /// A fetch with every request failing as when offline, end to end through the core.
    func testSimulatedOfflineFetchReadsAsNoConnection() async {
        URLSessionHost.simulateOffline = true
        defer { URLSessionHost.simulateOffline = false }
        let ledger = FileManager.default.temporaryDirectory.appendingPathComponent("offline-ledger.json")
        let place = Place(
            name: "Nowhere", latitude: 40, longitude: -75, units: nil, utcOffsetSeconds: 0,
            offsetChanges: [])
        do {
            _ = try await fetch(
                place: place, settings: defaultSettings(),
                cached: Cached(snapshotJson: nil, lastModified: nil, usageJson: nil),
                now: 1_791_720_000, host: URLSessionHost(ledgerURL: ledger))
            XCTFail("offline should fail")
        } catch {
            XCTAssertEqual(userMessage(error).title, "No connection", "\(error)")
        }
    }

    func testOfflineForecastFailureIsNoConnection() {
        let summary = "open-meteo forecast: network: The Internet connection appears to be offline."
        let message = userMessage(WhetherError.NoForecast(summary: summary))
        XCTAssertEqual(message.title, "No connection")
        XCTAssertEqual(message.detail, summary)
    }

    func testOfflineMessagesThatCarryOnlyTheCode() {
        let summary = "open-meteo forecast: network: The operation couldn’t be completed. (NSURLErrorDomain error -1009.)"
        XCTAssertEqual(userMessage(WhetherError.NoForecast(summary: summary)).title, "No connection")
        XCTAssertEqual(
            userMessage(WhetherError.NoHistory(summary: "x: (NSURLErrorDomain error -1005.)")).title,
            "No connection")
    }

    func testOtherForecastFailuresSayTheServicesDidNotAnswer() {
        let message = userMessage(WhetherError.NoForecast(summary: "open-meteo forecast: http 503"))
        XCTAssertEqual(message.title, "The weather services didn't answer")
        XCTAssertEqual(message.detail, "open-meteo forecast: http 503")
    }

    func testSearchAndTripAndUrlErrors() {
        XCTAssertEqual(
            userMessage(WhetherError.Search(message: "bad gateway")).title, "Couldn't find that place")
        XCTAssertEqual(
            userMessage(WhetherError.Trip(message: "Pick up to a month.")).title, "Pick up to a month.")
        XCTAssertNil(userMessage(WhetherError.Trip(message: "x")).detail)
        XCTAssertEqual(userMessage(URLError(.notConnectedToInternet)).title, "No connection")
        XCTAssertEqual(
            userMessage(URLError(.timedOut)).title, "The weather services didn't answer")
        XCTAssertEqual(userMessage(CocoaError(.fileNoSuchFile)).title, "Something went wrong")
    }

    func testAnOpenWeatherFallbackIsNamed() {
        let rejected = AttemptInfo(
            provider: "openweather", endpoint: "current",
            error: "unauthorized: OpenWeather rejected the API key.")
        let served = AttemptInfo(provider: "open-meteo", endpoint: "forecast", error: nil)
        let problem = openWeatherProblem(attempts: [rejected, served], served: "open-meteo")
        XCTAssertEqual(problem?.title, "OpenWeather didn't answer: the key was rejected")
        XCTAssertEqual(problem?.detail, "openweather current: unauthorized: OpenWeather rejected the API key.")
        let offline = AttemptInfo(
            provider: "openweather", endpoint: "current",
            error: "network: The Internet connection appears to be offline.")
        XCTAssertEqual(
            openWeatherProblem(attempts: [offline, served], served: "open-meteo")?.title,
            "OpenWeather didn't answer: No connection")
    }

    func testOpenWeatherServingOrNotAskedClearsTheProblem() {
        let ok = AttemptInfo(provider: "openweather", endpoint: "current", error: nil)
        let air = AttemptInfo(provider: "openweather", endpoint: "airquality", error: "HTTP 502")
        XCTAssertNil(openWeatherProblem(attempts: [ok, air], served: "openweather"))
        let served = AttemptInfo(provider: "open-meteo", endpoint: "forecast", error: nil)
        XCTAssertNil(openWeatherProblem(attempts: [served], served: "open-meteo"))
    }
}
