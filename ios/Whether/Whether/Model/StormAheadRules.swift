import Foundation
import WhetherKit

/// What a check for a storm ahead came back with.
enum StormAheadCheck: Equatable, Sendable {
    /// The check answered: the card's JSON, or `nil` when nothing coming is significant.
    case answered(String?)
    /// Not checked (another place than the one shown), or the check failed.
    case unanswered
}

/// When the Now tab's "Storm ahead" card shows, and what it keeps between checks.
enum StormAheadRules {
    /// The card JSON to keep after `check`: what an answer says (even that there is no
    /// storm), else the last one, so a failed check changes nothing.
    static func kept(_ previous: String?, after check: StormAheadCheck) -> String? {
        switch check {
        case .answered(let json): json
        case .unanswered: previous
        }
    }

    /// The card to show at `now`: none once its storm has ended.
    static func shown(_ card: StormAhead?, now: Int64) -> StormAhead? {
        guard let card, card.end > now else { return nil }
        return card
    }

    /// The card kept as `json`, shown at `now`: `nil` for none, a storm that has ended,
    /// or JSON that doesn't read.
    static func shown(json: String?, now: Int64) -> StormAhead? {
        shown(json.flatMap { try? StormAhead.decode($0) }, now: now)
    }

    /// Whether a refresh of `placeID` checks for a storm ahead: only the place shown, and
    /// only while the Now tab shows the card (`visible`: its shown sections). A hidden
    /// card keeps its last answer, which nothing draws.
    static func checks(
        _ placeID: String, shown: String?, visible: [Preferences.OverviewSection]
    ) -> Bool {
        placeID == shown && visible.contains(.storm)
    }

    /// The outlook a check starts from: whichever of the Storms tab's full outlook and
    /// the check's own was fetched later (the full one on a tie), so a fresh full outlook
    /// saves the check its request. `nil` when neither reads.
    static func cachedOutlook(full: String?, ahead: String?) -> String? {
        func fetchedAt(_ json: String?) -> Int64? {
            json.flatMap { try? StormOutlookRuns.decode($0).fetchedAt }
        }
        switch (fetchedAt(full), fetchedAt(ahead)) {
        case let (f?, a?): return a > f ? ahead : full
        case (.some, nil): return full
        case (nil, .some): return ahead
        case (nil, nil): return nil
        }
    }

    /// The SF Symbol beside the card's level color, so the level never rests on color
    /// alone (the core's `Level` names; another level gets the storm itself).
    static func symbol(_ level: String) -> String {
        switch level {
        case "good": "checkmark.circle.fill"
        case "fair": "info.circle.fill"
        case "moderate": "exclamationmark.circle.fill"
        case "poor": "exclamationmark.triangle.fill"
        case "very_poor", "extreme": "exclamationmark.octagon.fill"
        default: "hurricane"
        }
    }

    /// Where tapping the card goes: the Storms tab when the tab bar has it, else the
    /// Storms screen full screen.
    static func opensTab(visible: [AppTabItem]) -> Bool {
        visible.contains(.storms)
    }
}
