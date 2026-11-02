import SwiftUI
import UniformTypeIdentifiers
import WhetherKit

extension UTType {
    /// A trip plan sent between people: JSON in a `.whetherplan` file. Declared in
    /// `Whether-Info.plist` (exported, and openable by the app).
    nonisolated static let whetherPlan = UTType(exportedAs: "dev.savery.whether.plan")
}

/// A plan as the file that is shared: its name and JSON, plain values, so the system
/// can ask for the file from any thread. It is not isolated to the main actor (like
/// `PlanDay`), so its `Transferable` conformance isn't either, which the sharing
/// framework needs.
nonisolated struct PlanFile: Transferable, Sendable {
    /// "Summer at the shore.whetherplan".
    let fileName: String
    /// The `SharedPlan` JSON.
    let json: Data

    static var transferRepresentation: some TransferRepresentation {
        FileRepresentation(exportedContentType: .whetherPlan) { file in
            // In a folder of its own, so two plans with one name don't meet.
            let folder = URL.temporaryDirectory.appending(
                path: UUID().uuidString, directoryHint: .isDirectory)
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
            let url = folder.appending(path: file.fileName, directoryHint: .notDirectory)
            try file.json.write(to: url, options: .atomic)
            return SentTransferredFile(url)
        }
    }
}

extension SharedPlan {
    /// The file to send; `nil` if it can't be written as JSON.
    var file: PlanFile? {
        guard let json = try? data() else { return nil }
        return PlanFile(fileName: fileName, json: json)
    }

    /// One line for the share sheet: "Summer at the shore · Jul 12–Jul 19, 2027 · New York".
    var preview: String {
        "\(name) · \(planDates(from: from, to: to)) · \(place.name)"
    }

    #if DEBUG
        /// For screenshots (`-receivePlan sample`): Robin's plan for a week a month out.
        static var sample: SharedPlan {
            let c = Calendar.current
            let from = c.date(byAdding: .day, value: 30, to: .now) ?? .now
            let to = c.date(byAdding: .day, value: 6, to: from) ?? from
            return SharedPlan(
                id: UUID(), name: "Summer at the shore", place: SharedPlace(.new_york),
                from: PlanDay(from, in: c), to: PlanDay(to, in: c), years: 30, modifiedBy: "Robin",
                modifiedAt: .now)
        }
    #endif
}

/// The dates of a plan as the plan list writes them: "Jul 12–Jul 19, 2027".
func planDates(from: PlanDay, to: PlanDay) -> String {
    "\(from.date().formatted(.dateTime.month(.abbreviated).day()))–\(to.date().formatted(.dateTime.month(.abbreviated).day().year()))"
}

/// Share a copy of a plan: the system's share sheet with a `.whetherplan` file.
struct PlanShareButton: View {
    let plan: SavedPlan
    /// The button's text (in a menu); `nil` is the system's share icon.
    var title: String?

    var body: some View {
        // No name is known: it goes as "Someone".
        let shared = SharedPlan(plan, by: SharedPlan.someone)
        if let file = shared.file {
            if let title {
                ShareLink(item: file, preview: SharePreview(shared.preview)) {
                    Label(title, systemImage: "doc.on.doc")
                }
            } else {
                ShareLink(item: file, preview: SharePreview(shared.preview))
            }
        }
    }
}

/// A plan someone sent: what it is, and Add or Decline. When a plan with its id is
/// already here, it says so and offers a copy or to replace that one.
struct ReceivedPlanSheet: View {
    @Bindable var store: WeatherStore
    @Environment(\.colorTheme) private var colors
    let shared: SharedPlan
    /// Called once the plan is saved (`store.planToOpen` names it), to show the Plan tab.
    let onAdded: () -> Void

    private var receipt: PlanReceipt { shared.receipt(among: store.plans) }
    /// A plan shared with this person to look at isn't theirs to replace: only a copy.
    private var canReplace: Bool { shared.canReplace(among: store.plans) }

    private func accept(_ choice: ReceiveChoice) {
        store.receive(shared, choice)
        onAdded()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text(shared.headline).font(.title2.weight(.semibold))
            VStack(alignment: .leading, spacing: 6) {
                Text(shared.name).font(.headline)
                Label(shared.place.name, systemImage: "mappin.and.ellipse")
                Label(planDates(from: shared.from, to: shared.to), systemImage: "calendar")
            }
            .font(.callout)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(14)
            .background(.quaternary, in: .rect(cornerRadius: 14))
            switch receipt {
            case .new:
                EmptyView()
            case .same:
                Text("This plan is already in your list.").font(.callout).foregroundStyle(.secondary)
            case .differs:
                Text(
                    canReplace
                        ? "You already have a plan like this with other details. Add this one as a copy, or replace yours."
                        : "You already have a plan like this with other details. Add this one as a copy."
                )
                .font(.callout).foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
            VStack(spacing: 10) {
                switch receipt {
                case .new:
                    button("Add", prominent: true) { accept(.add) }
                case .same:
                    button("Open", prominent: true) { accept(.add) }
                case .differs:
                    button("Add as a copy", prominent: true) { accept(.copy) }
                    if canReplace { button("Replace mine", prominent: false) { accept(.replace) } }
                }
                Button("Decline", role: .cancel) { store.incoming = nil }
                    .controlSize(.large)
            }
        }
        .padding(20)
        .tint(colors.color(.accent))
        .presentationDetents([.medium, .large])
    }

    @ViewBuilder
    private func button(_ title: String, prominent: Bool, action: @escaping () -> Void) -> some View {
        let label = Button(action: action) { Text(title).frame(maxWidth: .infinity) }
        if prominent {
            label.buttonStyle(.borderedProminent).controlSize(.large)
        } else {
            label.buttonStyle(.bordered).controlSize(.large)
        }
    }
}

/// An invitation to a live shared plan: who sent it, the plan (once fetched), and Add or
/// Decline. Nothing is accepted before Add; swiping the sheet away declines, as for a copy.
struct ReceivedShareSheet: View {
    @Bindable var store: WeatherStore
    @Environment(\.colorTheme) private var colors
    /// Called once the plan is added (`store.planToOpen` names it), to show the Plan tab.
    let onAdded: () -> Void
    @State private var adding = false
    @State private var problem: String?

    var body: some View {
        if let share = store.incomingShare {
            VStack(alignment: .leading, spacing: 16) {
                Text(SharedPlan.headline(from: share.ownerName)).font(.title2.weight(.semibold))
                if let plan = share.plan {
                    VStack(alignment: .leading, spacing: 6) {
                        Text(plan.name).font(.headline)
                        Label(plan.place.name, systemImage: "mappin.and.ellipse")
                        Label(planDates(from: plan.from, to: plan.to), systemImage: "calendar")
                    }
                    .font(.callout)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(14)
                    .background(.quaternary, in: .rect(cornerRadius: 14))
                }
                Text(
                    "Add it to see it here, and when it changes. Decline and nothing is added."
                )
                .font(.callout).foregroundStyle(.secondary)
                if let problem {
                    Label(problem, systemImage: "exclamationmark.triangle").font(.callout)
                }
                Spacer(minLength: 0)
                VStack(spacing: 10) {
                    Button(action: { add(share) }) {
                        HStack {
                            if adding { ProgressView().padding(.trailing, 4) }
                            Text(adding ? "Adding…" : "Add").frame(maxWidth: .infinity)
                        }
                    }
                    .buttonStyle(.borderedProminent).controlSize(.large)
                    .disabled(adding)
                    Button("Decline", role: .cancel) { store.declineShare() }
                        .controlSize(.large)
                        .disabled(adding)
                }
            }
            .padding(20)
            .tint(colors.color(.accent))
            .interactiveDismissDisabled(adding)
            .presentationDetents([.medium, .large])
        }
    }

    private func add(_ share: IncomingShare) {
        adding = true
        problem = nil
        Task {
            defer { adding = false }
            do {
                try await store.acceptShare(share)
                onAdded()
            } catch {
                problem = userMessage(error).title
            }
        }
    }
}
