import CoreLocation
import XCTest

@testable import WhetherKit

/// What leaves the phone is rounded to 2 decimals (about 1.1 km); the saved place is not.
final class OutgoingCoordinatesTests: XCTestCase {
    private let new_york = AppPlace(
        name: "New York", detail: nil, latitude: 40.71427, longitude: -74.00597,
        timeZone: "America/New_York")

    func testThePlaceIsRoundedToTwoDecimals() {
        XCTAssertEqual(AppPlace.outgoingDecimals, 2)
        let c = new_york.rounded()
        XCTAssertEqual(c.latitude, 40.71, accuracy: 1e-9)
        XCTAssertEqual(c.longitude, -74.01, accuracy: 1e-9)
        let d = new_york.rounded(decimals: 3)
        XCTAssertEqual(d.latitude, 40.714, accuracy: 1e-9)
        XCTAssertEqual(d.longitude, -74.006, accuracy: 1e-9)
    }

    func testTheSavedPlaceKeepsItsFullCoordinates() {
        _ = new_york.rounded()
        _ = new_york.outgoingLocation
        XCTAssertEqual(new_york.latitude, 40.71427)
        XCTAssertEqual(new_york.longitude, -74.00597)
    }

    func testWeatherKitGetsTheRoundedLocation() {
        let location = new_york.outgoingLocation
        XCTAssertEqual(location.coordinate.latitude, 40.71, accuracy: 1e-9)
        XCTAssertEqual(location.coordinate.longitude, -74.01, accuracy: 1e-9)
        XCTAssertLessThan(location.distance(from: CLLocation(latitude: 40.71427, longitude: -74.00597)), 1_200)
    }

    func testTheGeocoderGetsTheRoundedFix() {
        let fix = CLLocation(latitude: -33.868_820_1, longitude: 151.209_296_4)
        XCTAssertEqual(fix.outgoing.coordinate.latitude, -33.87, accuracy: 1e-9)
        XCTAssertEqual(fix.outgoing.coordinate.longitude, 151.21, accuracy: 1e-9)
        XCTAssertEqual(fix.coordinate.latitude, -33.868_820_1, "the fix itself is untouched")
    }

    /// No more than 2 decimals survive, for every sign and for the half-way cases.
    func testNoCoordinateKeepsMoreThanTwoDecimals() {
        for (lat, lon) in [(40.715, -74.005), (-0.004_9, 0.005_1), (89.999_9, -179.999_9), (51.5074, -0.1278)] {
            let c = CLLocationCoordinate2D(latitude: lat, longitude: lon).rounded()
            for value in [c.latitude, c.longitude] {
                XCTAssertEqual(value * 100, (value * 100).rounded(), accuracy: 1e-6, "\(lat), \(lon)")
            }
        }
        let zero = CLLocationCoordinate2D(latitude: 0.004, longitude: -0.004).rounded()
        XCTAssertEqual(zero.latitude, 0)
        XCTAssertEqual(zero.longitude, 0)
    }
}
