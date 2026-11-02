import XCTest

@testable import WhetherKit

/// Where a place's radar comes from: NEXRAD in the US, OpenWeather's
/// layer abroad with a key, else none. The rule lives in the core; this checks the
/// app's use of it and its handling of the key.
final class RadarSourceTests: XCTestCase {
    private let new_york = AppPlace(
        name: "New York", detail: "New York, US", latitude: 40.7143, longitude: -74.00597,
        timeZone: "America/New_York")
    private let london = AppPlace(
        name: "London", detail: "England, GB", latitude: 51.5072, longitude: -0.1276,
        timeZone: "Europe/London")

    private func settings(key: String?) -> Settings {
        var s = defaultSettings()
        s.openweatherKey = key
        return s
    }

    func testTheCompositeComesFirstThenTheKey() {
        XCTAssertEqual(settings(key: nil).radarSource(for: new_york), .nexrad)
        XCTAssertEqual(settings(key: "abc").radarSource(for: new_york), .nexrad)
        XCTAssertEqual(settings(key: "abc").radarSource(for: london), .openWeather)
        XCTAssertEqual(settings(key: nil).radarSource(for: london), .none)
    }

    func testABlankKeyIsNoKey() {
        XCTAssertNil(settings(key: nil).radarKey)
        XCTAssertNil(settings(key: "").radarKey)
        XCTAssertNil(settings(key: " \n ").radarKey)
        XCTAssertEqual(settings(key: " abc ").radarKey, "abc")
        XCTAssertEqual(settings(key: "  ").radarSource(for: london), .none)
    }

    func testOpenWeatherIsOneCurrentFrameWithItsCredit() {
        let frames = radarOpenweatherFrames(key: "abc")
        XCTAssertEqual(frames.count, 1, "no loop")
        XCTAssertEqual(
            frames[0].tileTemplate,
            "https://tile.openweathermap.org/map/precipitation_new/{z}/{x}/{y}.png?appid=abc")
        XCTAssertEqual(radarOpenweatherCredit(), "Radar: OpenWeather")
        XCTAssertTrue(radarOpenweatherFrames(key: " ").isEmpty)
        XCTAssertTrue(radarCoversWith(latitude: 51.5, longitude: -0.12, hasKey: true))
        XCTAssertFalse(radarCoversWith(latitude: 51.5, longitude: -0.12, hasKey: false))
    }
}
