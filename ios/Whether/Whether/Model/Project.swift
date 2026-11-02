import Foundation

/// Where the project lives, for links in the app.
enum Project {
    static let url = URL(string: "https://github.com/SeasonalPugForecast/whether")!
    /// The privacy policy, rendered from `PRIVACY.md` by the project's GitHub Pages site.
    static let privacyURL = URL(string: "https://seasonalpugforecast.github.io/whether/privacy/")!
    /// Where to ask for help.
    static let supportURL = URL(string: "https://seasonalpugforecast.github.io/whether/support/")!
}
