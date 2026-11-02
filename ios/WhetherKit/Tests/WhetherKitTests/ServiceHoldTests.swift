import XCTest

@testable import WhetherKit

/// MET Norway's waits: until `Expires` for one URL, an hour for the host after a refusal.
final class ServiceHoldTests: XCTestCase {
    private let now: Int64 = 1_800_000_000
    private let forecast = "https://api.met.no/weatherapi/locationforecast/2.0/complete?lat=40.7143&lon=-74.006"
    private let sun = "https://api.met.no/weatherapi/sunrise/3.0/sun?lat=40.7143&lon=-74.006&date=2027-01-15"

    func testAnExpiresHoldsOnlyThatURLUntilThen() {
        var holds = ServiceHold()
        holds.record(url: forecast, status: 200, expires: now + 1200, now: now)
        XCTAssertEqual(holds.wait(for: forecast, now: now), 1200)
        XCTAssertEqual(holds.wait(for: forecast, now: now + 1199), 1)
        XCTAssertEqual(holds.wait(for: forecast, now: now + 1200), 0)
        XCTAssertEqual(holds.wait(for: sun, now: now), 0, "the sunrise has its own Expires")
    }

    func testANotModifiedAnswerHoldsToo() {
        var holds = ServiceHold()
        holds.record(url: forecast, status: 304, expires: now + 300, now: now)
        XCTAssertEqual(holds.wait(for: forecast, now: now), 300)
    }

    func testASuccessWithoutExpiresWaitsTenMinutes() {
        var holds = ServiceHold()
        holds.record(url: forecast, status: 200, expires: nil, now: now)
        XCTAssertEqual(holds.wait(for: forecast, now: now), 600)
    }

    func testAnExpiresInThePastHoldsNothing() {
        var holds = ServiceHold()
        holds.record(url: forecast, status: 200, expires: now - 5, now: now)
        XCTAssertEqual(holds.wait(for: forecast, now: now), 0)
        XCTAssertTrue(holds.urls.isEmpty)
    }

    func testARefusalHoldsTheWholeHostForAnHour() {
        for status: UInt16 in [403, 429] {
            var holds = ServiceHold()
            holds.record(url: forecast, status: status, expires: nil, now: now)
            XCTAssertEqual(holds.wait(for: forecast, now: now), 3600, "\(status)")
            XCTAssertEqual(holds.wait(for: sun, now: now), 3600, "\(status): every URL of the host")
            XCTAssertEqual(holds.wait(for: sun, now: now + 3600), 0)
        }
    }

    func testOtherAnswersChangeNothing() {
        var holds = ServiceHold()
        for status: UInt16 in [0, 203, 404, 500, 503] {
            holds.record(url: forecast, status: status, expires: now + 900, now: now)
        }
        XCTAssertEqual(holds, ServiceHold())
    }

    func testAWrongExpiresCannotLockTheServiceOutForDays() {
        var holds = ServiceHold()
        holds.record(url: forecast, status: 200, expires: now + 7 * 86400, now: now)
        XCTAssertEqual(holds.wait(for: forecast, now: now), ServiceHold.longest)
    }

    func testHoldsThatHavePassedAreForgottenOnTheNextRecord() {
        var holds = ServiceHold()
        holds.record(url: forecast, status: 200, expires: now + 100, now: now)
        holds.record(url: sun, status: 200, expires: now + 100 + 500, now: now + 200)
        XCTAssertEqual(Set(holds.urls.keys), [sun])
    }

    func testTheMemoryIsBoundedAndKeepsTheLatestWaits() {
        var holds = ServiceHold()
        for i in 0..<(ServiceHold.capacity + 10) {
            holds.record(url: "https://api.met.no/x?\(i)", status: 200, expires: now + 100 + Int64(i), now: now)
        }
        XCTAssertEqual(holds.urls.count, ServiceHold.capacity)
        XCTAssertNil(holds.urls["https://api.met.no/x?0"], "the earliest to pass goes first")
        XCTAssertNotNil(holds.urls["https://api.met.no/x?\(ServiceHold.capacity + 9)"])
    }

    func testOnlyMetNorwayAsksForHolds() throws {
        XCTAssertTrue(ServiceHold.applies(to: try XCTUnwrap(URL(string: forecast))))
        XCTAssertTrue(ServiceHold.applies(to: try XCTUnwrap(URL(string: "https://API.MET.NO/x"))))
        for other in [
            "https://api.open-meteo.com/v1/forecast", "https://api.weather.gov/alerts", "https://met.no/x",
            "https://api.met.no.example.com/x",
        ] {
            XCTAssertFalse(ServiceHold.applies(to: try XCTUnwrap(URL(string: other))), other)
        }
    }

    func testHoldsSurviveALaunchAndAMissingOrBrokenFileIsNoHold() throws {
        let file = FileManager.default.temporaryDirectory.appending(path: "holds-\(UUID().uuidString).json")
        defer { try? FileManager.default.removeItem(at: file) }
        XCTAssertEqual(ServiceHold.load(from: file), ServiceHold(), "no file")
        var holds = ServiceHold()
        holds.record(url: forecast, status: 429, expires: nil, now: now)
        holds.record(url: sun, status: 200, expires: now + 90, now: now)
        holds.save(to: file)
        XCTAssertEqual(ServiceHold.load(from: file), holds)
        try Data("not json".utf8).write(to: file)
        XCTAssertEqual(ServiceHold.load(from: file), ServiceHold(), "a broken file")
    }
}
