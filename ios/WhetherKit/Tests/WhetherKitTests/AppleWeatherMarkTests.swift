import XCTest

/// The Apple Weather mark the app saves for the widgets (`AppleWeatherMark`).
final class AppleWeatherMarkTests: XCTestCase {
    func testAMissingMarkIsFetched() {
        XCTAssertTrue(AppleWeatherMark.isDue(savedAt: nil, now: .now))
    }

    func testASavedMarkIsKeptForThirtyDays() {
        let saved = Date(timeIntervalSince1970: 1_791_720_000)
        XCTAssertFalse(AppleWeatherMark.isDue(savedAt: saved, now: saved.addingTimeInterval(29 * 86_400)))
        XCTAssertTrue(AppleWeatherMark.isDue(savedAt: saved, now: saved.addingTimeInterval(30 * 86_400)))
    }

    func testAMarkFromTheFutureIsFetchedAgain() {
        let now = Date(timeIntervalSince1970: 1_791_720_000)
        XCTAssertTrue(AppleWeatherMark.isDue(savedAt: now.addingTimeInterval(3_600), now: now))
    }

    func testTheSavedDateIsTheLightFiles() throws {
        let dir = FileManager.default.temporaryDirectory.appending(path: UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        XCTAssertNil(AppleWeatherMark.savedAt(in: dir))
        try Data([1]).write(to: AppleWeatherMark.file(dark: true, in: dir))
        XCTAssertNil(AppleWeatherMark.savedAt(in: dir), "only the light mark counts")
        try Data([1]).write(to: AppleWeatherMark.file(dark: false, in: dir))
        XCTAssertNotNil(AppleWeatherMark.savedAt(in: dir))
    }
}
