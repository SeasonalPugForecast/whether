import CloudKit
import SwiftUI
import UIKit
import UserNotifications

// Telling people that a shared plan changed: the sentence, the banner, the tap on
// the notification, and the way the app delegate (which iCloud's pushes reach) gets to the
// store. Which sentence, and its parts, is decided in `PlanSync.swift`
// (`PlanChange.notices`, tested); here the words are composed.

extension PlanChangeNotice {
    /// "Robin moved Summer at the shore to Jul 12–Jul 19, 2027", "… to Cape May", "Robin
    /// renamed Summer to Beach week", "Robin changed Summer at the shore". "Someone" when
    /// iCloud gave no name. Dates are written as the plan list writes them (`planDates`).
    var sentence: String {
        let who = who ?? SharedPlan.someone
        switch kind {
        case .movedDates(let from, let to):
            return "\(who) moved \(planName) to \(planDates(from: from, to: to))"
        case .movedPlace(let place):
            return "\(who) moved \(planName) to \(place)"
        case .renamed(let old):
            return "\(who) renamed \(old) to \(planName)"
        case .changed:
            return "\(who) changed \(planName)"
        }
    }
}

/// A line shown at the top of the app for a few seconds: a plan someone changed.
struct PlanBanner: Identifiable, Equatable {
    let id = UUID()
    /// The plan that opens when it is tapped; `nil` for the summary of several plans (the
    /// Plan tab opens).
    var planID: UUID?
    var text: String

    /// The banners for `notices`: the plan's own sentence for one; one line for several
    /// plans ("3 plans changed").
    static func banners(for notices: [PlanChangeNotice]) -> [PlanBanner] {
        switch notices.count {
        case 0: []
        case 1: [PlanBanner(planID: notices[0].planID, text: notices[0].sentence)]
        default: [PlanBanner(planID: nil, text: "\(notices.count) plans changed")]
        }
    }

    #if DEBUG
        /// `-planChangedBanner sample`: Robin moved the shore trip.
        static var sample: PlanBanner {
            let notice = PlanChangeNotice(
                planID: UUID(), who: "Robin", planName: "Summer at the shore",
                kind: .movedDates(
                    from: PlanDay(Date.now.addingTimeInterval(30 * 86_400), in: .current),
                    to: PlanDay(Date.now.addingTimeInterval(36 * 86_400), in: .current)))
            return PlanBanner(planID: nil, text: notice.sentence)
        }
    #endif
}

/// The way from the app delegate, which iCloud's silent pushes and a tap on a notification
/// reach before (or without) any view, to the store. `WhetherApp` attaches the store when the
/// app is made, which is also when the system launches it in the background for a push.
@MainActor
enum PlanUpdates {
    private(set) static weak var store: WeatherStore?
    /// A tapped notification's plan, when it arrives before the store does.
    private static var waiting: UUID?

    static func attach(_ store: WeatherStore) {
        self.store = store
        if let id = waiting {
            waiting = nil
            store.openPlan(id)
        }
    }

    /// A notification about a plan was tapped.
    static func open(_ id: UUID) {
        if let store { store.openPlan(id) } else { waiting = id }
    }
}

/// Hears taps on notifications. Only plan-change notifications carry a plan id; the weather
/// alerts' taps just open the app.
nonisolated final class NotificationTaps: NSObject, UNUserNotificationCenterDelegate {
    static let planKey = "planID"
    /// The thread the plan-change notifications group in.
    static let planThread = "plans"

    nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse
    ) async {
        guard
            let text = response.notification.request.content.userInfo[Self.planKey] as? String,
            let id = UUID(uuidString: text)
        else { return }
        await PlanUpdates.open(id)
    }
}
