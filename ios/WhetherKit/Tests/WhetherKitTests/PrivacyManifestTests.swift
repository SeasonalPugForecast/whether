import Foundation
import XCTest

/// The privacy manifests (app and widget) declare what the App Store label says: coarse
/// location and search history, for app functionality, not linked, no tracking.
final class PrivacyManifestTests: XCTestCase {
    private func manifest(_ target: String) throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .appending(path: "../../../Whether/\(target)/PrivacyInfo.xcprivacy")
        let data = try Data(contentsOf: url)
        return try XCTUnwrap(PropertyListSerialization.propertyList(from: data, format: nil) as? [String: Any])
    }

    func testTheManifestsDeclareCoarseLocationAndSearchHistory() throws {
        let app = try manifest("Whether")
        XCTAssertEqual(app["NSPrivacyTracking"] as? Bool, false)
        let collected = try XCTUnwrap(app["NSPrivacyCollectedDataTypes"] as? [[String: Any]])
        XCTAssertEqual(
            collected.compactMap { $0["NSPrivacyCollectedDataType"] as? String },
            ["NSPrivacyCollectedDataTypeCoarseLocation", "NSPrivacyCollectedDataTypeSearchHistory"])
        for type in collected {
            XCTAssertEqual(type["NSPrivacyCollectedDataTypeLinked"] as? Bool, false)
            XCTAssertEqual(type["NSPrivacyCollectedDataTypeTracking"] as? Bool, false)
            XCTAssertEqual(
                type["NSPrivacyCollectedDataTypePurposes"] as? [String],
                ["NSPrivacyCollectedDataTypePurposeAppFunctionality"])
        }
    }

    func testTheWidgetDeclaresTheSameAsTheApp() throws {
        let app = try manifest("Whether"), widget = try manifest("WhetherWidget")
        XCTAssertEqual(NSDictionary(dictionary: app), NSDictionary(dictionary: widget))
    }
}
