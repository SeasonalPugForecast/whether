import XCTest

@testable import WhetherKit

/// The sun and moon crossing the Now tab's top: where they are on the arc, when
/// they are not drawn, and the switch that hides them.
final class SkyArcTests: XCTestCase {
    private func decode(_ json: String) throws -> Preferences {
        try JSONDecoder().decode(Preferences.self, from: Data(json.utf8))
    }

    private let rise: Int64 = 1_000_000
    private let set: Int64 = 1_000_000 + 12 * 3_600

    // MARK: The sun

    func testTheSunCrossesFromTheLeftOnTheHorizonToTheTopInTheMiddleAndDown() throws {
        let sunrise = try XCTUnwrap(SkyArc.sun(now: rise, rise: rise, set: set))
        XCTAssertEqual(sunrise.along, 0, accuracy: 1e-9)
        XCTAssertEqual(sunrise.height, 0, accuracy: 1e-9)
        XCTAssertEqual(sunrise.fade, 0, accuracy: 1e-9, "it fades in, not pops")
        let noon = try XCTUnwrap(SkyArc.sun(now: rise + 6 * 3_600, rise: rise, set: set))
        XCTAssertEqual(noon.along, 0.5, accuracy: 1e-9)
        XCTAssertEqual(noon.height, 1, accuracy: 1e-9)
        XCTAssertEqual(noon.fade, 1, accuracy: 1e-9)
        let quarter = try XCTUnwrap(SkyArc.sun(now: rise + 3 * 3_600, rise: rise, set: set))
        XCTAssertEqual(quarter.along, 0.25, accuracy: 1e-9)
        XCTAssertEqual(quarter.height, sin(Double.pi / 4), accuracy: 1e-9, "a sine of the way across")
        let afternoon = try XCTUnwrap(SkyArc.sun(now: rise + 9 * 3_600, rise: rise, set: set))
        XCTAssertEqual(afternoon.height, quarter.height, accuracy: 1e-9, "symmetric")
    }

    func testTheSunIsNotDrawnBeforeItRisesAfterItSetsOrWithoutTheTimes() {
        XCTAssertNil(SkyArc.sun(now: rise - 1, rise: rise, set: set))
        XCTAssertNil(SkyArc.sun(now: set + 1, rise: rise, set: set))
        XCTAssertNil(SkyArc.sun(now: rise + 100, rise: nil, set: set), "a polar night")
        XCTAssertNil(SkyArc.sun(now: rise + 100, rise: rise, set: nil))
        XCTAssertNil(SkyArc.sun(now: rise + 100, rise: set, set: rise), "set before rise")
    }

    func testTheArcMapsToTheAreaFromTheHorizonUpToThePeak() throws {
        let noon = try XCTUnwrap(SkyArc.sun(now: rise + 6 * 3_600, rise: rise, set: set))
        let top = SkyArc.point(noon, width: 400, horizon: 200, peak: 30, inset: 30)
        XCTAssertEqual(top.x, 200, accuracy: 1e-9)
        XCTAssertEqual(top.y, 30, accuracy: 1e-9)
        let start = SkyArc.point(SkyArc.place(progress: 0), width: 400, horizon: 200, peak: 30, inset: 30)
        XCTAssertEqual(start.x, 30, accuracy: 1e-9)
        XCTAssertEqual(start.y, 200, accuracy: 1e-9)
        let end = SkyArc.point(SkyArc.place(progress: 1), width: 400, horizon: 200, peak: 30, inset: 30)
        XCTAssertEqual(end.x, 370, accuracy: 1e-9)
        XCTAssertEqual(end.y, 200, accuracy: 1e-6)
    }

    func testAScreenshotClockIsAShareOfTheDay() {
        XCTAssertEqual(SkyArc.now(fraction: 0, rise: rise, set: set), rise)
        XCTAssertEqual(SkyArc.now(fraction: 0.5, rise: rise, set: set), rise + 6 * 3_600)
        XCTAssertNil(SkyArc.sun(now: SkyArc.now(fraction: 1.2, rise: rise, set: set), rise: rise, set: set))
        XCTAssertNil(SkyArc.sun(now: SkyArc.now(fraction: -0.2, rise: rise, set: set), rise: rise, set: set))
    }

    // MARK: The moon

    func testTheMoonCrossesBetweenItsRiseAndSet() throws {
        let night: Int64 = 3_600
        let moonrise = rise + 11 * night, moonset = moonrise + 10 * night  // up 10 hours
        let mid = try XCTUnwrap(SkyArc.moon(now: moonrise + 5 * night, rise: moonrise, set: moonset))
        XCTAssertEqual(mid.along, 0.5, accuracy: 1e-9)
        XCTAssertEqual(mid.height, 1, accuracy: 1e-9)
        XCTAssertNil(SkyArc.moon(now: moonrise - 1, rise: moonrise, set: moonset))
        XCTAssertNil(SkyArc.moon(now: moonset + 1, rise: moonrise, set: moonset))
    }

    func testAMoonThatSetsBeforeItRisesTodayIsUpUntilSetAndFromRise() throws {
        let hour: Int64 = 3_600
        // Sets at 03:00 and rises at 17:00 (by a clock where today starts at 0).
        let moonset = 3 * hour, moonrise = 17 * hour
        let early = try XCTUnwrap(SkyArc.moon(now: hour, rise: moonrise, set: moonset), "up before it sets")
        XCTAssertGreaterThan(early.along, 0.5, "past the middle of last night's crossing")
        XCTAssertNil(SkyArc.moon(now: 10 * hour, rise: moonrise, set: moonset), "down in the day")
        let evening = try XCTUnwrap(
            SkyArc.moon(now: 18 * hour, rise: moonrise, set: moonset), "up after it rises")
        XCTAssertLessThan(evening.along, 0.2)
        // The windows meet the lunar day: yesterday's rise is 24 h 50 min before today's.
        XCTAssertEqual(
            SkyArc.moonWindows(rise: moonrise, set: moonset),
            [(moonrise - 89_400)...moonset, moonrise...(moonset + 89_400)])
    }

    func testAMoonWithOneTimeIsUpHalfALunarDayFromIt() {
        XCTAssertEqual(SkyArc.moonWindows(rise: 100_000, set: nil), [100_000...(100_000 + 44_700)])
        XCTAssertEqual(SkyArc.moonWindows(rise: nil, set: 100_000), [(100_000 - 44_700)...100_000])
        XCTAssertEqual(SkyArc.moonWindows(rise: nil, set: nil), [])
        XCTAssertNil(SkyArc.moon(now: 5, rise: nil, set: nil), "no times: not drawn")
    }

    // MARK: The switch

    func testTheSkyBodiesAreOnByDefaultAndASavedChoiceStays() throws {
        XCTAssertTrue(Preferences().skyBodies)
        XCTAssertTrue(try decode(#"{"units":"metric"}"#).skyBodies, "saved before the switch: on")
        XCTAssertTrue(try decode("{}").skyBodies)
        XCTAssertFalse(try decode(#"{"skyBodies":false}"#).skyBodies, "a saved off stays off")
        XCTAssertTrue(try decode(#"{"skyBodies":"nonsense"}"#).skyBodies, "garbage: the default")
    }
}
