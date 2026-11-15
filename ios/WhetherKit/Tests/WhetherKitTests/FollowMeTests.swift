import CoreLocation
import XCTest

@testable import WhetherKit

final class FollowMeTests: XCTestCase {
    private func place(_ id: String, follows: Bool = false) -> AppPlace {
        AppPlace(
            id: id, name: id, detail: nil, latitude: 40, longitude: -75, timeZone: "UTC", follows: follows)
    }

    private func at(_ meters: Double) -> CLLocationCoordinate2D {
        // 1 degree of latitude is about 111.2 km.
        CLLocationCoordinate2D(latitude: 40 + meters / 111_195, longitude: -75)
    }

    func testAPlaceMovesAtOneKilometerOrMore() {
        let here = CLLocationCoordinate2D(latitude: 40, longitude: -75)
        XCTAssertFalse(AppPlace.shouldMove(old: here, new: here))
        XCTAssertFalse(AppPlace.shouldMove(old: here, new: at(900)))
        XCTAssertTrue(AppPlace.shouldMove(old: here, new: at(1_100)))
        XCTAssertTrue(AppPlace.shouldMove(old: here, new: at(50_000)))
    }

    func testACoarseFixMovesThePlaceOnlyWhenItIsSurelyAKilometerAway() {
        let here = CLLocationCoordinate2D(latitude: 40, longitude: -75)
        // A Wi-Fi fix, good to 65 m.
        XCTAssertTrue(AppPlace.shouldMove(old: here, new: at(1_100), accuracy: 65))
        XCTAssertFalse(AppPlace.shouldMove(old: here, new: at(1_050), accuracy: 65))
        // Cell towers only, good to 3 km: a still phone's fix lands 1.5 km off.
        XCTAssertFalse(AppPlace.shouldMove(old: here, new: at(1_500), accuracy: 3_000))
        XCTAssertTrue(AppPlace.shouldMove(old: here, new: at(50_000), accuracy: 3_000))
        // No accuracy: the coordinates mean nothing.
        XCTAssertFalse(AppPlace.shouldMove(old: here, new: at(50_000), accuracy: -1))
    }

    func testAFixAnswersForTenMinutes() {
        let now = Date(timeIntervalSince1970: 1_000_000)
        XCTAssertTrue(AppPlace.fixIsRecent(now, now: now))
        XCTAssertTrue(AppPlace.fixIsRecent(now.addingTimeInterval(-599), now: now))
        XCTAssertFalse(AppPlace.fixIsRecent(now.addingTimeInterval(-600), now: now))
        XCTAssertFalse(AppPlace.fixIsRecent(now.addingTimeInterval(60), now: now), "the clock went back")
    }

    func testOpeningReusesARecentFixOnlyWhileAccessIsOn() {
        let now = Date(timeIntervalSince1970: 1_000_000)
        XCTAssertTrue(AppPlace.reusesFix(taken: now.addingTimeInterval(-120), now: now, allowed: true))
        XCTAssertFalse(
            AppPlace.reusesFix(taken: now.addingTimeInterval(-120), now: now, allowed: false),
            "access turned off since: the new read says so")
        XCTAssertFalse(AppPlace.reusesFix(taken: now.addingTimeInterval(-900), now: now, allowed: true))
    }

    func testAtMostOneFollowingPlaceAndItIsFirst() {
        let list = AppPlace.arranged([
            place("a"), place("b", follows: true), place("c"), place("d", follows: true),
        ])
        XCTAssertEqual(list.map(\.id), ["b", "a", "c", "d"])
        XCTAssertEqual(list.filter(\.follows).map(\.id), ["b"])
    }

    func testAListWithoutAFollowingPlaceIsUntouched() {
        let list = [place("a"), place("b")]
        XCTAssertEqual(AppPlace.arranged(list), list)
        XCTAssertEqual(AppPlace.arranged([]), [])
    }

    func testAFollowingPlaceAlreadyFirstStaysAndOthersKeepTheirOrder() {
        let list = [place("f", follows: true), place("z"), place("y")]
        XCTAssertEqual(AppPlace.arranged(list).map(\.id), ["f", "z", "y"])
    }

    func testOldPlacesDecodeAsFixed() throws {
        let json = #"[{"name":"Brooklyn","latitude":40.650,"longitude":-73.9496,"timeZone":"America/New_York"}]"#
        let places = try JSONDecoder().decode([AppPlace].self, from: Data(json.utf8))
        XCTAssertFalse(places[0].follows)
    }

    func testTheFollowFlagRoundTripsAndOldReadersCanIgnoreIt() throws {
        let data = try JSONEncoder().encode(place("f", follows: true))
        XCTAssertTrue(try JSONDecoder().decode(AppPlace.self, from: data).follows)
        // The widget of an older build decodes only the keys it knows.
        struct Old: Decodable { var name: String; var latitude: Double }
        XCTAssertEqual(try JSONDecoder().decode(Old.self, from: data).name, "f")
    }
}
