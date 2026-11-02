import XCTest

@testable import WhetherKit

/// Apple Weather's alerts outside the US: when to ask, and
/// what the app keeps. The sources in `AppSources/` are the app's own files.
final class AppleAlertsTests: XCTestCase {
    private let now: Int64 = 1_000_000

    func testAppleIsAskedOnlyOutsideNWSCoverageWhenSwitchedOnAndHalfAnHourOld() {
        func ask(
            enabled: Bool = true, nws: Bool = false, last: Int64? = nil
        ) -> Bool {
            AppleAlertsPolicy.shouldAsk(enabled: enabled, nwsCovers: nws, lastAskedAt: last, now: now)
        }
        XCTAssertTrue(ask(), "never asked before")
        XCTAssertFalse(ask(enabled: false), "switched off")
        XCTAssertFalse(ask(nws: true), "the NWS has alerts there")
        XCTAssertFalse(ask(last: now - 1_799), "asked 29 minutes ago")
        XCTAssertTrue(ask(last: now - 1_800), "asked 30 minutes ago")
        XCTAssertTrue(ask(last: now - 86_400))
    }

    func testAlertsAreKeptForLessThanAnHour() {
        XCTAssertTrue(AppleAlertsPolicy.keeps(askedAt: now - 3_599, now: now))
        XCTAssertFalse(AppleAlertsPolicy.keeps(askedAt: now - 3_600, now: now))
        XCTAssertFalse(AppleAlertsPolicy.keeps(askedAt: nil, now: now), "caches from older builds")
    }

    func testAnAppleAlertBecomesTheCoresAlert() throws {
        let alert = AppleAlertsPolicy.alert(
            summary: "Flood Warning", region: " Kent \n", severity: .severe,
            source: "Environment Agency", url: URL(string: "https://example.invalid/w"),
            answeredAt: now)
        XCTAssertEqual(alert.id, "apple:Environment Agency|Flood Warning|Kent")
        XCTAssertEqual(alert.title, "Flood Warning")
        XCTAssertEqual(alert.summary, "Kent")
        XCTAssertEqual(alert.start, now)
        XCTAssertEqual(alert.end, now + 3_600)
        XCTAssertEqual(alert.url, "https://example.invalid/w")
        let bare = AppleAlertsPolicy.alert(
            summary: "Gale", region: nil, severity: .unknown, source: "Met Office", url: nil,
            answeredAt: now)
        XCTAssertEqual(bare.id, "apple:Met Office|Gale|", "the same alert keeps its id")
        XCTAssertEqual(bare.summary, "")
        XCTAssertNil(bare.url)
    }

    func testKeptAlertsRoundTripThroughJSONAndBack() throws {
        let alert = AppleAlertsPolicy.alert(
            summary: "Flood Warning", region: "Kent", severity: .extreme, source: "EA",
            url: URL(string: "https://example.invalid/w"), answeredAt: now)
        let kept = [KeptAlert(alert)]
        let back = try JSONDecoder().decode([KeptAlert].self, from: JSONEncoder().encode(kept))
        XCTAssertEqual(back, kept)
        XCTAssertEqual(back[0].external, alert)
        for severity in [AlertSeverity.minor, .moderate, .severe, .extreme, .unknown] {
            XCTAssertEqual(KeptAlert.severity(KeptAlert.name(severity)), severity)
        }
        XCTAssertEqual(KeptAlert.severity("surprising"), .unknown)
    }

    func testPreferencesWithoutTheKeyTurnAppleAlertsOn() throws {
        func decode(_ json: String) throws -> Preferences {
            try JSONDecoder().decode(Preferences.self, from: Data(json.utf8))
        }
        XCTAssertTrue(Preferences().appleAlerts)
        XCTAssertTrue(try decode(#"{"units":"metric"}"#).appleAlerts, "saved before the field existed")
        XCTAssertFalse(try decode(#"{"appleAlerts":false}"#).appleAlerts, "a saved off stays off")
    }

    func testAlertsPossibleCountsApple() {
        XCTAssertTrue(AlertSources.alertsPossible(places: [], oneCall: false, appleAlerts: true))
        XCTAssertFalse(AlertSources.alertsPossible(places: [], oneCall: false, appleAlerts: false))
    }

    func testTheViewModelAlertReadsItsLink() throws {
        let json = #"""
            {"id":"apple:x","title":"Gale","body":"","level":"moderate","period":"As of Mon 10:00","end":5,
             "url":"https://example.invalid/w"}
            """#
        let alert = try JSONDecoder().decode(ViewModel.Alert.self, from: Data(json.utf8))
        XCTAssertEqual(alert.url, "https://example.invalid/w")
        let old = try JSONDecoder().decode(
            ViewModel.Alert.self,
            from: Data(#"{"title":"Gale","body":"","level":"moderate","period":"p","end":5}"#.utf8))
        XCTAssertNil(old.url, "alerts without a link")
        XCTAssertTrue(alert.isApple)
        XCTAssertFalse(old.isApple)
    }

    /// Apple's alerts have no end of their own: ours moves with each call, and
    /// that must not repeat the notification every hour.
    func testAnAppleAlertNotifiesOnceUntilItGetsWorse() {
        func alert(_ level: String, end: Int64) -> ViewModel.Alert {
            ViewModel.Alert(
                id: "apple:EA|Flood Warning|Kent", title: "Flood Warning", body: "Kent", level: level,
                period: "As of Mon", end: end)
        }
        var n = AlertNotifier()
        var t: Int64 = 0
        XCTAssertEqual(n.notices(for: [alert("poor", end: t + 3_600)], place: "L", now: t).count, 1)
        // Asked every 30 minutes for five hours: the end moves, the alert is the same.
        for _ in 1...10 {
            t += 1_800
            XCTAssertEqual(
                n.notices(for: [alert("poor", end: t + 3_600)], place: "L", now: t).count, 0,
                "quiet at \(t)")
        }
        XCTAssertEqual(
            n.notices(for: [alert("extreme", end: t + 3_600)], place: "L", now: t).count, 1,
            "a worse level notifies")
    }
}
