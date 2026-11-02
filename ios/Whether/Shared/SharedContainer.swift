import Foundation

/// Where the app keeps places, forecasts and settings: the App Group the widget
/// shares in signed builds, else the app's own Application Support (the widget
/// then fetches for itself).
enum SharedContainer {
    /// `group.<bundle id>`, from Info.plist.
    static var groupID: String? {
        Bundle.main.object(forInfoDictionaryKey: "WhetherAppGroup") as? String
    }

    /// The directory for places.json, the forecasts and the call ledger.
    static var directory: URL {
        let group = groupID.flatMap {
            FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: $0)
        }
        let base = group ?? URL.applicationSupportDirectory
        let dir = base.appending(path: "whether", directoryHint: .isDirectory)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    /// Whether the App Group is available (signed builds with the entitlement).
    static var isShared: Bool {
        groupID.flatMap {
            FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: $0)
        } != nil
    }

    /// Settings shared with the widget.
    static var defaults: UserDefaults {
        groupID.flatMap { UserDefaults(suiteName: $0) } ?? .standard
    }
}
