import XCTest

@testable import WhetherKit

/// the radar's map style, loop speed, opacity and starting zoom.
final class RadarOptionsTests: XCTestCase {
    private func decode(_ json: String) throws -> Preferences {
        try JSONDecoder().decode(Preferences.self, from: Data(json.utf8))
    }

    func testTheDefaultsAreTheRadarAsItWas() {
        let o = RadarOptions()
        XCTAssertEqual(o.style, .muted)
        XCTAssertEqual(o.speed.frameMilliseconds, 600)
        XCTAssertEqual(o.opacity, 0.75)
        XCTAssertEqual(o.startZoom.span, 1.4)
        XCTAssertEqual(o.startZoom.cardSpan, 2.5)
        XCTAssertEqual(Preferences().radar, o)
    }

    func testPreferencesSavedBeforeTheRadarOptionsKeepTheDefaults() throws {
        let old = try decode(#"{"units":"metric","days":5}"#)
        XCTAssertEqual(old.radar, RadarOptions())
        XCTAssertEqual(old.days, 5)
    }

    func testSavedOptionsRoundTrip() throws {
        var p = Preferences()
        p.radar = RadarOptions()
        p.radar.style = .hybrid
        p.radar.speed = .fast
        p.radar.opacity = 0.4
        p.radar.startZoom = .state
        let back = try JSONDecoder().decode(Preferences.self, from: JSONEncoder().encode(p))
        XCTAssertEqual(back.radar, p.radar)
    }

    func testAFieldMissingOrUnknownKeepsItsDefaultAndTheRest() throws {
        let p = try decode(#"{"radar":{"style":"satellite","speed":"warp","startZoom":"galaxy"}}"#)
        XCTAssertEqual(p.radar.style, .satellite)
        XCTAssertEqual(p.radar.speed, .normal)
        XCTAssertEqual(p.radar.startZoom, .region)
        XCTAssertEqual(p.radar.opacity, 0.75)
        // A wrong type for the whole value falls back to the defaults.
        XCTAssertEqual(try decode(#"{"radar":"hybrid","days":4}"#).radar, RadarOptions())
    }

    func testOpacityStaysWithinItsRange() throws {
        XCTAssertEqual(try decode(#"{"radar":{"opacity":0}}"#).radar.opacity, 0.2)
        XCTAssertEqual(try decode(#"{"radar":{"opacity":7}}"#).radar.opacity, 1.0)
        XCTAssertEqual(try decode(#"{"radar":{"opacity":0.5}}"#).radar.opacity, 0.5)
        XCTAssertEqual(RadarOptions.clamped(.nan), 0.75)
        XCTAssertEqual(RadarOptions.clamped(-.infinity), 0.75)
    }

    func testSpeedsAreOrderedAndZoomsWiden() {
        let ms = RadarOptions.LoopSpeed.allCases.map(\.frameMilliseconds)
        XCTAssertEqual(ms, ms.sorted(by: >), "slow to fast")
        let spans = RadarOptions.StartZoom.allCases.map(\.span)
        XCTAssertEqual(spans, spans.sorted())
        let card = RadarOptions.StartZoom.allCases.map(\.cardSpan)
        XCTAssertEqual(card, card.sorted())
        XCTAssertTrue(zip(spans, card).allSatisfy { $0 < $1 }, "the card shows a wider area")
    }

    func testLaunchArgumentsOverrideWithoutTouchingTheRest() {
        let base = RadarOptions()
        let o = base.overridden(by: ["-seed", "-radarstyle", "hybrid", "-radaropacity", "0.4", "-radarzoom", "state"])
        XCTAssertEqual(o.style, .hybrid)
        XCTAssertEqual(o.opacity, 0.4)
        XCTAssertEqual(o.startZoom, .state)
        XCTAssertEqual(o.speed, base.speed)
        XCTAssertEqual(base.overridden(by: ["-radarstyle", "nonsense", "-radaropacity"]), base)
        XCTAssertEqual(base.overridden(by: ["-radaropacity", "9"]).opacity, 1.0)
    }
}
