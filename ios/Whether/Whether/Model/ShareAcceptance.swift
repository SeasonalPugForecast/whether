import CloudKit
import SwiftUI
import UIKit
import UserNotifications

/// An invitation to a live shared plan, waiting for Add or Decline.
struct IncomingShare: Identifiable {
    let id = UUID()
    /// What iCloud handed the app; `nil` only for the debug sample (`-receiveShare sample`).
    let metadata: CKShare.Metadata?
    /// Who shared it ("Robin"), when iCloud told.
    var ownerName: String?
    /// The plan itself, once it is fetched (`PlanCloud.preview`); `nil` before, or when it
    /// can't be read (the sheet then says only that a plan was shared).
    var plan: SharedPlan?

    /// The plan's id: the name of the share's root record (`hierarchicalRootRecordID`).
    var planID: UUID? {
        metadata.flatMap { UUID(uuidString: $0.hierarchicalRootRecordID?.recordName ?? "") } ?? plan?.id
    }

    #if DEBUG
        /// For screenshots: Robin's sample plan.
        static var sample: IncomingShare {
            IncomingShare(metadata: nil, ownerName: "Robin", plan: .sample)
        }
    #endif
}

/// Where an accepted invitation waits until the app is ready for it: the system may hand it
/// over (at launch, from the app delegate's connection options) before the store exists.
@MainActor
enum IncomingShares {
    private static var handler: ((CKShare.Metadata) -> Void)?
    private static var waiting: [CKShare.Metadata] = []

    /// An invitation arrived.
    static func deliver(_ metadata: CKShare.Metadata) {
        if let handler { handler(metadata) } else { waiting.append(metadata) }
    }

    /// The app is ready: `handler` gets this and every later invitation.
    static func attach(_ handler: @escaping (CKShare.Metadata) -> Void) {
        self.handler = handler
        let early = waiting
        waiting = []
        for metadata in early { handler(metadata) }
    }
}

/// Gives the window scene a delegate that hears CloudKit invitations. SwiftUI still makes
/// and owns the window: only the scene's delegate class is ours, because
/// `windowScene(_:userDidAcceptCloudKitShareWith:)` is a scene delegate method.
final class AppDelegate: NSObject, UIApplicationDelegate {
    /// Hears taps on notifications (a shared plan that changed opens).
    private let taps = NotificationTaps()

    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil
    ) -> Bool {
        UNUserNotificationCenter.current().delegate = taps
        return true
    }

    /// iCloud's silent push: a shared plan changed in the private or the shared database
    ///. Fetches, applies and announces it (`WeatherStore.syncSharedPlans`). Pushes
    /// only come once subscriptions exist, which only a build with iCloud makes
    /// (`PlanCloud.isAvailable`); anything else is ignored without touching CloudKit. `force`:
    /// a push means something is in iCloud, even if no plan is shared on this device yet.
    func application(
        _ application: UIApplication, didReceiveRemoteNotification userInfo: [AnyHashable: Any]
    ) async -> UIBackgroundFetchResult {
        guard PlanCloud.isAvailable, let store = PlanUpdates.store,
            CKNotification(fromRemoteNotificationDictionary: userInfo)?.notificationType == .database
        else { return .noData }
        let applied = await store.syncSharedPlans(force: true)
        if store.cloudProblem != nil { return .failed }
        return applied.isEmpty ? .noData : .newData
    }

    func application(
        _ application: UIApplication, configurationForConnecting connectingSceneSession: UISceneSession,
        options: UIScene.ConnectionOptions
    ) -> UISceneConfiguration {
        // The app was launched by tapping an invitation: it is in the connection options,
        // and the scene delegate isn't asked.
        if let metadata = options.cloudKitShareMetadata { IncomingShares.deliver(metadata) }
        let configuration = UISceneConfiguration(name: nil, sessionRole: connectingSceneSession.role)
        configuration.delegateClass = ShareSceneDelegate.self
        return configuration
    }
}

/// The app was running (or in the background) when an invitation was tapped.
final class ShareSceneDelegate: NSObject, UIWindowSceneDelegate {
    func windowScene(
        _ windowScene: UIWindowScene, userDidAcceptCloudKitShareWith cloudKitShareMetadata: CKShare.Metadata
    ) {
        IncomingShares.deliver(cloudKitShareMetadata)
    }
}
