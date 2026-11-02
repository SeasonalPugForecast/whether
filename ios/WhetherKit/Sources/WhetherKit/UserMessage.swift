import Foundation

/// An error that already says what happened in words for the person using the app
/// (a shared plan from a newer version): `userMessage` uses it as the headline.
public protocol FriendlyError: Error {
    var friendlyMessage: String { get }
}

/// A failure in words for the person using the app: a short `title` for the
/// headline and the raw text (provider attempts, system messages) as `detail`,
/// to sit behind a "Details" disclosure.
public func userMessage(_ error: Error) -> (title: String, detail: String?) {
    if let error = error as? WhetherError {
        switch error {
        case .NoForecast(let summary):
            return (networkTitle(summary), summary)
        case .NoHistory(let summary):
            return (
                isOffline(summary) ? "No connection" : "History isn't available right now", summary
            )
        case .Search(let message):
            return (
                isOffline(message) ? "No connection" : "Couldn't find that place", message
            )
        case .Trip(let message):
            return (message, nil)
        case .Config(let message):
            return ("That file couldn't be read", message)
        case .InvalidJson(let what, let message):
            return ("Saved \(what) couldn't be read", message)
        case .NotModified:
            return ("Nothing has changed", nil)
        }
    }
    if let error = error as? URLError {
        switch error.code {
        case .notConnectedToInternet, .networkConnectionLost, .dataNotAllowed,
            .internationalRoamingOff:
            return ("No connection", error.localizedDescription)
        default:
            return ("The weather services didn't answer", error.localizedDescription)
        }
    }
    if let error = error as? FriendlyError {
        return (error.friendlyMessage, nil)
    }
    return ("Something went wrong", error.localizedDescription)
}

/// "No connection" when the text says the phone is offline, else the services
/// not answering.
private func networkTitle(_ text: String) -> String {
    isOffline(text) ? "No connection" : "The weather services didn't answer"
}

/// Whether a system or provider message says the phone has no connection.
func isOffline(_ text: String) -> Bool {
    let text = text.lowercased()
    return [
        "offline", "not connected to the internet", "internet connection",
        "network connection was lost", "cannot find host", "could not connect",
        // Messages that carry only the code: not connected, connection lost,
        // roaming off, data not allowed.
        "nsurlerrordomain error -1009", "nsurlerrordomain error -1005",
        "nsurlerrordomain error -1018", "nsurlerrordomain error -1020",
    ].contains { text.contains($0) }
}

/// What a fetch says about OpenWeather: when one of its requests failed and
/// another provider served, a line for the footer ("OpenWeather didn't
/// answer: …") and the raw attempts behind it; `nil` when OpenWeather served
/// or was not asked, which clears an earlier problem.
public func openWeatherProblem(attempts: [AttemptInfo], served: String) -> (title: String, detail: String)? {
    let failed = attempts.filter { $0.provider == "openweather" && $0.error != nil }
    guard served != "openweather", let first = failed.first?.error else { return nil }
    let text = first.lowercased()
    let reason =
        if text.contains("unauthorized") {
            "the key was rejected"
        } else if text.contains("call budget") || text.contains("rate limited") {
            "today's calls are used up"
        } else {
            userMessage(WhetherError.NoForecast(summary: first)).title
        }
    return (
        "OpenWeather didn't answer: \(reason)",
        failed.map { "openweather \($0.endpoint): \($0.error ?? "")" }.joined(separator: "\n")
    )
}
