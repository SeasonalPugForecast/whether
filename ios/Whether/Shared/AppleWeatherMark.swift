import Foundation

/// Apple's Weather mark for the widgets and the Live Activity, which can't fetch it: the app
/// saves the mark (light and dark) and the address of Apple's page of data sources into the
/// shared container after it has used WeatherKit, and the widget draws them from there.
/// WeatherKit's terms ask for the mark and that link wherever its data shows.
enum AppleWeatherMark {
    /// The saved mark's file in `directory`, for a dark or a light background.
    static func file(dark: Bool, in directory: URL = SharedContainer.directory) -> URL {
        directory.appending(path: dark ? "apple-weather-mark-dark.png" : "apple-weather-mark-light.png")
    }

    /// Where the address of Apple's page of data sources is kept (`SharedContainer.defaults`).
    static let legalPageKey = "appleWeatherLegalPage"

    /// Apple's page of data sources, once the app has saved it.
    static var legalPage: URL? {
        SharedContainer.defaults.string(forKey: legalPageKey).flatMap(URL.init(string:))
    }

    /// Apple may change its mark: a saved one is fetched again after 30 days.
    static let keepFor: TimeInterval = 30 * 86_400

    /// Whether the mark saved at `savedAt` (`nil`: none is saved) should be fetched again at `now`.
    static func isDue(savedAt: Date?, now: Date) -> Bool {
        guard let savedAt else { return true }
        return now.timeIntervalSince(savedAt) >= keepFor || savedAt > now
    }

    /// When the light mark in `directory` was saved, if it was.
    static func savedAt(in directory: URL = SharedContainer.directory) -> Date? {
        let path = file(dark: false, in: directory).path(percentEncoded: false)
        return (try? FileManager.default.attributesOfItem(atPath: path))?[.modificationDate] as? Date
    }
}
