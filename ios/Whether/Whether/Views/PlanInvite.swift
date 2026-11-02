import CloudKit
import SwiftUI
import UIKit
import WhetherKit

/// What the invitation controller needs: the plan's share (saved in iCloud already) and the
/// container it lives in.
struct InviteRequest: Identifiable {
    let id = UUID()
    let planID: UUID
    let share: CKShare
    let container: CKContainer
    /// The plan's name, for the invitation message.
    let title: String
}

/// iOS's own invitation page (`UICloudSharingController`) for a plan that is shared: who it is
/// shared with, a link to send, and per person "View only" or "Can make changes". People are
/// invited read-only; the owner's "Can edit" switches in the editor do the same thing later,
/// without inviting again.
///
/// It is presented from UIKit, from the top view controller, **not** from a SwiftUI `.sheet`
/// with a `UIViewControllerRepresentable`: the controller wants to be presented by a view
/// controller (it presents its own pages over itself), and inside a SwiftUI sheet it is a known
/// source of blank pages. `ShareLink` with `CKShareTransferRepresentation` was the other way;
/// it can't be tried in the simulator (no iCloud), and it would give up the per-person
/// permission choices this page has, so the tried-and-documented controller stays.
@MainActor
enum CloudSharingPresenter {
    /// The delegate of the page showing (the controller only holds it weakly).
    private static var current: Coordinator?

    /// Shows the page for `request`, over whatever is on screen; `onFinish` runs once it has
    /// gone. Does nothing if no view controller can present it.
    static func present(_ request: InviteRequest, store: WeatherStore, onFinish: @escaping () -> Void) {
        guard let top = topViewController() else { return }
        let coordinator = Coordinator(request: request, store: store)
        let controller = InviteController(share: request.share, container: request.container)
        // Invitation only, with a permission per person.
        controller.availablePermissions = [.allowPrivate, .allowReadOnly, .allowReadWrite]
        controller.delegate = coordinator
        controller.modalPresentationStyle = .formSheet
        controller.onGone = {
            if current === coordinator { current = nil }
            onFinish()
        }
        current = coordinator
        top.present(controller, animated: true)
    }

    /// The view controller on top of the key window's: what a new page is presented from.
    private static func topViewController() -> UIViewController? {
        let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
        let scene = scenes.first { $0.activationState == .foregroundActive } ?? scenes.first
        var top = scene?.keyWindow?.rootViewController ?? scene?.windows.first?.rootViewController
        while let next = top?.presentedViewController, !next.isBeingDismissed { top = next }
        return top
    }

    /// Tells when the page has been dismissed (its Done button, or a swipe down).
    private final class InviteController: UICloudSharingController {
        var onGone: (() -> Void)?
        private var told = false

        override func viewDidDisappear(_ animated: Bool) {
            super.viewDidDisappear(animated)
            // Not when it only gave way to a page of its own.
            guard !told, isBeingDismissed || presentingViewController == nil else { return }
            told = true
            onGone?()
        }
    }

    private final class Coordinator: NSObject, UICloudSharingControllerDelegate {
        let request: InviteRequest
        let store: WeatherStore

        init(request: InviteRequest, store: WeatherStore) {
            self.request = request
            self.store = store
        }

        func itemTitle(for csc: UICloudSharingController) -> String? { request.title }

        func cloudSharingController(
            _ csc: UICloudSharingController, failedToSaveShareWithError error: Error
        ) {
            Task { @MainActor in store.cloudProblem = userMessage(PlanCloud.friendly(error)).title }
        }

        func cloudSharingControllerDidSaveShare(_ csc: UICloudSharingController) {
            Task { @MainActor in store.cloudProblem = nil }
        }

        /// The owner (or an invited person) stopped sharing inside the controller: the share
        /// is gone, so make the plan private here too (this also removes the owner's record).
        func cloudSharingControllerDidStopSharing(_ csc: UICloudSharingController) {
            let id = request.planID
            Task { @MainActor in try? await store.stopSharing(id) }
        }
    }
}

/// The plan editor's Share button: **Invite to this plan** (live, through iCloud) and
/// **Send a copy** (a `.whetherplan` file, which works anywhere). Without iCloud
/// (`offersLive` false) or for someone who may not invite, it is only the copy's share link.
struct PlanShareMenu: View {
    let plan: SavedPlan
    /// Whether live sharing is offered (`WeatherStore.offersLiveSharing`).
    let offersLive: Bool
    /// Starts the invitation (owner, or a plan not shared yet).
    let invite: () -> Void

    private var mayInvite: Bool { offersLive && (plan.sharing?.isOwner ?? true) }

    var body: some View {
        if mayInvite {
            Menu {
                Button("Invite to this plan", systemImage: "person.badge.plus", action: invite)
                PlanShareButton(plan: plan, title: "Send a copy")
            } label: {
                Image(systemName: "square.and.arrow.up")
            }
            .accessibilityLabel("Share")
        } else {
            PlanShareButton(plan: plan)
        }
    }
}
