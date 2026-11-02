import Charts
import SwiftUI
import WhetherKit

/// The trip planner: saved plans, each with its own name, place,
/// dates (up to a month, any year ahead) and how many years to look back over
/// (1 to 50). "New plan" adds one; swipe to delete, drag to reorder.
struct PlanTab: View {
    @Bindable var store: WeatherStore
    @State private var path: [UUID] = []
    /// With no saved place, a new plan starts by choosing where.
    @State private var choosingPlace = false
    /// The tabs' editor, from a card's menu.
    @State private var editingTabs = false
    @Environment(\.colorTheme) private var colors

    private func startPlan() {
        if let id = store.newPlan() {
            path.append(id)
        } else {
            choosingPlace = true
        }
    }

    var body: some View {
        NavigationStack(path: $path) {
            Group {
                if store.plans.isEmpty {
                    ContentUnavailableView {
                        Label("No plans yet", systemImage: "suitcase")
                    } description: {
                        Text("See how a trip's dates were over the years: averages, the hottest and coldest, and how often it rained.")
                    } actions: {
                        Button("New plan", systemImage: "plus") { startPlan() }
                            .buttonStyle(.borderedProminent)
                            .tint(colors.color(.accent))
                    }
                } else {
                    List {
                        ForEach(store.plans) { plan in
                            NavigationLink(value: plan.id) { PlanRow(plan: plan) }
                                .listRowBackground(Color.white.opacity(0.08))
                                .contextMenu {
                                    Button("Delete", systemImage: "trash", role: .destructive) {
                                        if let i = store.plans.firstIndex(where: { $0.id == plan.id }) {
                                            withAnimation { store.removePlans(at: IndexSet(integer: i)) }
                                        }
                                    }
                                    Divider()
                                    CardMenuButtons()
                                }
                        }
                        .onDelete { store.removePlans(at: $0) }
                        .onMove { store.movePlans(from: $0, to: $1) }
                    }
                    .scrollContentBackground(.hidden)
                }
            }
            .navigationTitle("Plans")
            .toolbar {
                ToolbarItem(placement: .primaryAction) {
                    Button("New plan", systemImage: "plus") { startPlan() }
                }
                if !store.plans.isEmpty {
                    ToolbarItem(placement: .topBarLeading) { EditButton() }
                }
            }
            .sheet(isPresented: $choosingPlace) {
                PlaceSearch(store: store) { found in
                    if let id = store.newPlan(at: found) { path.append(id) }
                }
            }
            .navigationDestination(for: UUID.self) { id in
                PlanEditor(store: store, id: id)
            }
            .background {
                if store.preferences.background == .sky, let view = store.view {
                    SkyBackground(theme: view.theme)
                }
            }
        }
        .environment(
            \.skyCard,
            store.preferences.background == .sky ? store.view.map { colors.card($0.theme) } : nil)
        // Touch and hold on the plans and their cards: Customize…, Edit the tabs, and
        // Hide the Plan tab (the plan's own pieces are not optional).
        .environment(
            \.cardMenu,
            CardMenu(
                editTitle: "Edit the tabs", edit: { editingTabs = true },
                hideTitle: "Hide the Plan tab",
                hide: { withAnimation(.snappy) { store.preferences.tabs.set(.plan, shown: false) } })
        )
        .sheet(isPresented: $editingTabs) {
            EditorSheet { TabsEditor(tabs: $store.preferences.tabs) }
        }
        // A plan just added from a shared file opens.
        .onChange(of: store.planToOpen, initial: true) {
            if let id = store.planToOpen {
                path = [id]
                store.planToOpen = nil
            }
        }
        .task {
            #if DEBUG
                // Screenshots: `-trip 2027-07-12:2027-07-19:30` saves that trip
                // ("Summer at the shore") and opens it; with `-planlist`, stays on
                // the list.
                let args = ProcessInfo.processInfo.arguments
                if let i = args.firstIndex(of: "-trip"), i + 1 < args.count {
                    let parts = args[i + 1].split(separator: ":").map(String.init)
                    let f = DateFormatter()
                    f.dateFormat = "yyyy-MM-dd"
                    // `+3` is three days from today (for a trip the forecast reaches),
                    let date = { (text: String) -> Date? in
                        // `-2` two days ago (a trip under way).
                        if text.hasPrefix("+") || text.hasPrefix("-"), let n = Int(text) {
                            return Calendar.current.date(byAdding: .day, value: n, to: .now)
                        }
                        return f.date(from: text)
                    }
                    if parts.count == 3, let a = date(parts[0]), let b = date(parts[1]),
                        let n = Int(parts[2])
                    {
                        let id: UUID? =
                            store.plans.first { $0.name == "Summer at the shore" }?.id ?? store.newPlan()
                        guard let id, var plan = store.plans.first(where: { $0.id == id }) else { return }
                        plan.name = "Summer at the shore"
                        plan.from = a
                        plan.to = b
                        plan.years = n
                        plan.looked = true
                        store.update(plan)
                        if !args.contains("-planlist") { path = [plan.id] }
                    }
                }
                // `-sharedPlan owner|reader|editor|list`: that plan as shared (made-up people;
                // nothing is saved and CloudKit is never touched).
                if let i = args.firstIndex(of: "-sharedPlan"), i + 1 < args.count {
                    store.seedSharedPlans(args[i + 1])
                }
            #endif
        }
    }
}

/// A plan in the list: its name, place and dates; a shared plan has a person badge, and one
/// someone shared with this person says who.
private struct PlanRow: View {
    let plan: SavedPlan

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 6) {
                Text(plan.name).font(.headline)
                if plan.sharing != nil {
                    Image(systemName: "person.2.fill")
                        .font(.caption)
                        .foregroundStyle(.primary.opacity(0.8))
                        .accessibilityLabel("Shared")
                }
            }
            Text("\(plan.place.name) · \(planDates(from: plan.fromDay, to: plan.toDay))")
                .font(.caption)
                .foregroundStyle(.secondary)
            if let sharing = plan.sharing, !sharing.isOwner {
                Text(sharing.sharedBy).font(.caption).foregroundStyle(.secondary)
            }
        }
        .padding(.vertical, 2)
    }
}

/// One plan: its name, place, dates and years, and how those dates were.
/// Every change is kept.
struct PlanEditor: View {
    @Bindable var store: WeatherStore
    @Environment(\.colorTheme) private var colors
    let id: UUID
    @State private var answer: PlanView?
    @State private var loading = false
    @State private var problem: String?
    /// The raw text behind `problem`, behind a "Details" disclosure.
    @State private var problemDetail: String?
    @State private var searching = false
    /// The opened day, by its date.
    @State private var open: String?
    /// The forecast for the plan's dates still within the next 16 days, by
    /// date ("Jul 12"), shown beside the typical weather; empty otherwise.
    @State private var forecast: [String: ViewModel.Day] = [:]
    /// The plan's dates already past, as observed, by date ("Jul 12").
    @State private var observed: [String: ViewModel.PastDay] = [:]
    /// The forecast's provider and attribution.
    @State private var forecastCredit: (provider: String, attribution: String)?
    /// Where the observed days come from: "NY City Central Park, 7.8 km …".
    @State private var observedCredit: String?
    /// The owner's sharing section: who the plan is shared with (`nil` until loaded).
    @State private var people: [PlanParticipant]?
    /// Why the people couldn't be loaded.
    @State private var peopleProblem: String?
    @State private var preparingInvite = false
    @State private var confirmingEnd = false
    @State private var ending = false
    /// A failed sharing step, as an alert.
    @State private var sharingProblem: String?

    /// Stands in while the plan is gone (deleted under the editor).
    private static let blank = SavedPlan.new(
        at: AppPlace(name: "", detail: nil, latitude: 0, longitude: 0, timeZone: "UTC"), number: 1)

    /// The last whole year: past years never change, so answers are kept.
    private var lastYear: Int { Calendar.current.component(.year, from: .now) - 1 }

    private var plan: SavedPlan? { store.plans.first { $0.id == id } }

    /// A binding to one of the plan's fields; setting it keeps the plan and
    /// clears an answer that no longer fits.
    private func field<T: Equatable>(_ key: WritableKeyPath<SavedPlan, T>, clears: Bool = true)
        -> Binding<T>
    {
        Binding(
            get: { plan.map { $0[keyPath: key] } ?? Self.blank[keyPath: key] },
            set: { value in
                guard var p = plan, p[keyPath: key] != value else { return }
                p[keyPath: key] = value
                if clears {
                    p.looked = false
                    answer = nil
                    problem = nil
                }
                store.update(p)
            })
    }

    /// "30 years (1996–2025)", "1 year (2025)".
    private var yearsLabel: String {
        let years = plan?.years ?? 30
        return years == 1
            ? "1 year (\(String(lastYear)))"
            : "\(years) years (\(String(lastYear - years + 1))–\(String(lastYear)))"
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                form.glassCard(true).cardContextMenu()
                sharingCard
                if let answer {
                    results(answer).glassCard(true).cardContextMenu()
                } else if let problem {
                    VStack(alignment: .leading, spacing: 6) {
                        Label(problem, systemImage: "exclamationmark.triangle")
                            .font(.callout)
                        if let problemDetail {
                            DisclosureGroup("Details") { Text(problemDetail).font(.caption) }
                        }
                    }
                    .glassCard(true)
                    .cardContextMenu()
                }
            }
            .padding(.horizontal, 16)
            .padding(.top, 12)
            .padding(.bottom, 40)
        }
        .navigationTitle(plan?.name ?? "Plan")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            if let plan {
                ToolbarItem(placement: .primaryAction) {
                    PlanShareMenu(plan: plan, offersLive: store.offersLiveSharing, invite: invite)
                }
            }
        }
        .background {
            if store.preferences.background == .sky, let view = store.view {
                SkyBackground(theme: view.theme)
            }
        }
        .sheet(isPresented: $searching) {
            PlaceSearch(store: store) { found in field(\.place).wrappedValue = found }
        }
        .sensoryFeedback(.selection, trigger: open)
        .alert(
            sharingProblem ?? "",
            isPresented: Binding(
                get: { sharingProblem != nil }, set: { if !$0 { sharingProblem = nil } })
        ) {
            Button("OK") {}
        }
        .task(id: plan?.sharing?.isOwner == true && store.offersLiveSharing) { await loadPeople() }
        // A plan looked up before shows its kept answer at once.
        .task { if plan?.looked == true { await look() } }
        .task(id: forecastKey) { await loadForecast() }
    }

    /// What the forecast depends on: the place and the dates.
    private var forecastKey: String {
        guard let p = plan else { return "" }
        return "\(p.place.latitude),\(p.place.longitude),\(p.from.timeIntervalSince1970),\(p.to.timeIntervalSince1970)"
    }

    /// Fetches the forecast when some of the plan's dates are within the next
    /// 16 days (Open-Meteo's longest forecast), and the dates already past as
    /// observed (up to a month back), so a trip under way shows how it went.
    private func loadForecast() async {
        guard let p = plan else { return }
        let c = Calendar.current
        let today = c.startOfDay(for: .now)
        guard let horizon = c.date(byAdding: .day, value: 16, to: today),
            let monthAgo = c.date(byAdding: .day, value: -31, to: today),
            p.from < horizon, c.startOfDay(for: p.to) >= monthAgo
        else {
            forecast = [:]
            observed = [:]
            forecastCredit = nil
            observedCredit = nil
            return
        }
        let daysBack = max(c.dateComponents([.day], from: c.startOfDay(for: p.from), to: today).day ?? 0, 0)
        guard let view = await store.planForecast(at: p.place, daysBack: daysBack) else { return }
        forecast = Dictionary(
            (view.daily?.days ?? []).map { ($0.dateShort, $0) }, uniquingKeysWith: { a, _ in a })
        // Observed days, but not today (its forecast is the better guess).
        let past = (view.history?.days ?? []).filter { forecast[$0.dateShort] == nil && $0.high.value != nil }
        observed = Dictionary(past.map { ($0.dateShort, $0) }, uniquingKeysWith: { a, _ in a })
        forecastCredit = (view.status.provider, view.attribution)
        observedCredit = past.isEmpty ? nil : view.history?.credit
    }

    /// Whether any of the plan's dates has a forecast or an observation.
    private func comparing(_ plan: PlanView) -> Bool {
        plan.days.contains { forecast[$0.dateShort] != nil || observed[$0.dateShort] != nil }
    }

    /// Name, where, when, and over which years; as text for someone who may only look.
    @ViewBuilder
    private var form: some View {
        if plan?.isReadOnlyForMe == true {
            readOnlyForm
        } else {
            editableForm
        }
    }

    /// "Shared by Robin", for a plan someone else shared.
    @ViewBuilder
    private var sharedByLine: some View {
        if let sharing = plan?.sharing, !sharing.isOwner {
            Label(sharing.sharedBy, systemImage: "person.2.fill")
                .font(.callout)
                .foregroundStyle(.secondary)
        }
    }

    /// "Not yet shared — will send when online": an edit of this shared plan that iCloud
    /// didn't take (it is queued, `unsentPlans`, and goes with the next sync). Shown once a
    /// send has failed, not during the moment an edit waits to go.
    @ViewBuilder
    private var unsentLine: some View {
        if plan?.sharing?.mayEdit == true, store.unsentPlans.contains(id), store.cloudProblem != nil {
            Label("Not yet shared — will send when online", systemImage: "icloud.slash")
                .font(.callout)
                .foregroundStyle(.secondary)
        }
    }

    /// A shared plan its owner hasn't let this person change: the same fields as text, and
    /// still "Show how it was" (what this phone looks up is its own).
    private var readOnlyForm: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(plan?.name ?? "").font(.title3.weight(.semibold))
            sharedByLine
            Label(plan?.place.name ?? "", systemImage: "mappin.and.ellipse")
            Label(
                plan.map { planDates(from: $0.fromDay, to: $0.toDay) } ?? "",
                systemImage: "calendar")
            HStack {
                Text("Look back over")
                Spacer()
                Text(verbatim: yearsLabel).monospacedDigit().foregroundStyle(.secondary)
            }
            Text("\(plan?.sharing?.ownerName ?? "The person who shared it") hasn't let you change this plan.")
                .font(.caption)
                .foregroundStyle(.secondary)
            showButton
        }
    }

    private var editableForm: some View {
        VStack(alignment: .leading, spacing: 12) {
            TextField("Name", text: field(\.name, clears: false))
                .font(.title3.weight(.semibold))
                .submitLabel(.done)
            sharedByLine
            unsentLine
            Menu {
                ForEach(store.places) { place in
                    Button(place.name) { field(\.place).wrappedValue = place }
                }
                Divider()
                Button("Another place…", systemImage: "magnifyingglass") { searching = true }
            } label: {
                HStack {
                    Label(plan?.place.name ?? "Choose a place", systemImage: "mappin.and.ellipse")
                    Spacer()
                    Image(systemName: "chevron.up.chevron.down").font(.caption)
                }
                .contentShape(.rect)
            }
            .buttonStyle(.plain)
            DatePicker("From", selection: fromBinding, displayedComponents: .date)
            DatePicker(
                "To", selection: field(\.to),
                in: (plan?.from ?? .now)...(Calendar.current.date(byAdding: .day, value: 30, to: plan?.from ?? .now) ?? .now),
                displayedComponents: .date)
            VStack(alignment: .leading, spacing: 4) {
                HStack {
                    Text("Look back over")
                    Spacer()
                    // Years as written, not as numbers ("1,996").
                    Text(verbatim: yearsLabel).monospacedDigit().foregroundStyle(.secondary)
                }
                Slider(
                    value: Binding(
                        get: { Double(plan?.years ?? 30) },
                        set: { field(\.years).wrappedValue = Int($0.rounded()) }),
                    in: 1...50
                ) {
                    Text("Years")
                } minimumValueLabel: {
                    Text("1").font(.caption)
                } maximumValueLabel: {
                    Text("50").font(.caption)
                }
            }
            showButton
        }
    }

    private var showButton: some View {
        Button {
            Task { await look() }
        } label: {
            HStack {
                if loading { ProgressView().padding(.trailing, 4) }
                Text(loading ? "Looking back…" : "Show how it was")
            }
            .frame(maxWidth: .infinity)
        }
        .buttonStyle(.borderedProminent)
        .tint(colors.color(.accent))
        .disabled(loading || plan == nil)
    }

    // MARK: Sharing

    /// The sharing card: for the owner who they share with (and Stop sharing); for someone
    /// it was shared with, Make my own copy (read-only) and Leave.
    @ViewBuilder
    private var sharingCard: some View {
        if let sharing = plan?.sharing {
            if sharing.isOwner {
                if store.offersLiveSharing { ownerSharing.glassCard(true).cardContextMenu() }
            } else {
                participantActions(sharing).glassCard(true).cardContextMenu()
            }
        }
    }

    /// The owner's "Shared with": each person with a Can edit switch (read-only by default),
    /// and Stop sharing.
    private var ownerSharing: some View {
        VStack(alignment: .leading, spacing: 12) {
            Label("Shared with", systemImage: "person.2.fill").font(.headline)
            if let people {
                if people.isEmpty {
                    Text("No one yet. Use the share button to invite people.")
                        .font(.callout).foregroundStyle(.secondary)
                }
                ForEach(people) { person in
                    VStack(alignment: .leading, spacing: 2) {
                        Text(person.name ?? "Invited").font(.body.weight(.medium))
                        if person.pending {
                            Text("Hasn't answered yet").font(.caption).foregroundStyle(.secondary)
                        }
                        // The app tints white on the skies, which makes a switch's "on" white
                        // on white: the switch takes the editor's blue (the card's buttons
                        // keep the app's tint, which shows on the sky).
                        Toggle("Can edit", isOn: canEditBinding(for: person))
                            .font(.callout)
                            .tint(colors.color(.accent))
                    }
                }
            } else if let peopleProblem {
                Label(peopleProblem, systemImage: "exclamationmark.triangle").font(.callout)
            } else {
                ProgressView()
            }
            Divider()
            Button("Invite more people", systemImage: "person.badge.plus", action: invite)
                .disabled(preparingInvite)
            Button("Stop sharing", systemImage: "person.slash", role: .destructive) {
                confirmingEnd = true
            }
            .disabled(ending)
            .confirmationDialog(
                "Stop sharing \"\(plan?.name ?? "this plan")\"?", isPresented: $confirmingEnd,
                titleVisibility: .visible
            ) {
                Button("Stop sharing", role: .destructive) { endSharing() }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("Nobody else will see it. It stays in your plans as a private plan.")
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// For someone the plan was shared with: a copy of their own (when they may only look),
    /// and Leave.
    private func participantActions(_ sharing: PlanSharing) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Label(sharing.sharedBy, systemImage: "person.2.fill").font(.headline)
            if !sharing.mayEdit {
                Button("Make my own copy", systemImage: "doc.on.doc") { store.makeOwnCopy(of: id) }
            }
            if store.offersLiveSharing {
                Button("Leave", systemImage: "rectangle.portrait.and.arrow.right", role: .destructive) {
                    confirmingEnd = true
                }
                .disabled(ending)
                .confirmationDialog(
                    "Leave \"\(plan?.name ?? "this plan")\"?", isPresented: $confirmingEnd,
                    titleVisibility: .visible
                ) {
                    Button("Leave", role: .destructive) { endSharing() }
                    Button("Cancel", role: .cancel) {}
                } message: {
                    Text("It stays in your plans as a private plan, but you won't get its changes.")
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// The switch for one person; flipping it saves their permission, and puts it back if
    /// iCloud refuses.
    private func canEditBinding(for person: PlanParticipant) -> Binding<Bool> {
        Binding(
            get: { people?.first { $0.id == person.id }?.canEdit ?? person.canEdit },
            set: { value in
                setPerson(person.id, canEdit: value)
                Task {
                    do {
                        try await store.setCanEdit(value, for: person.id, in: id)
                    } catch {
                        setPerson(person.id, canEdit: !value)
                        sharingProblem = userMessage(error).title
                    }
                }
            })
    }

    private func setPerson(_ personID: String, canEdit: Bool) {
        guard let i = people?.firstIndex(where: { $0.id == personID }) else { return }
        people?[i].canEdit = canEdit
    }

    private func loadPeople() async {
        guard plan?.sharing?.isOwner == true, store.offersLiveSharing else { return }
        do {
            people = try await store.participants(of: id)
            peopleProblem = nil
        } catch {
            peopleProblem = userMessage(error).title
        }
    }

    /// Puts the plan in iCloud (if it isn't) and shows iOS's invitation page (presented from
    /// UIKit: `CloudSharingPresenter`).
    private func invite() {
        guard let plan, !preparingInvite else { return }
        preparingInvite = true
        Task {
            defer { preparingInvite = false }
            do {
                let share = try await store.startSharing(id)
                let request = InviteRequest(
                    planID: id, share: share, container: try store.planCloud().container,
                    title: plan.name)
                CloudSharingPresenter.present(request, store: store) {
                    Task {
                        await store.syncSharedPlans()
                        await loadPeople()
                    }
                }
            } catch {
                sharingProblem = userMessage(error).title
            }
        }
    }

    /// Stop sharing (owner) or Leave (participant): the plan stays as a private plan.
    private func endSharing() {
        ending = true
        Task {
            defer { ending = false }
            do {
                try await store.stopSharing(id)
                people = nil
            } catch {
                sharingProblem = userMessage(error).title
            }
        }
    }

    /// The first date; the last moves with it to keep the trip within a month.
    private var fromBinding: Binding<Date> {
        Binding(
            get: { plan?.from ?? .now },
            set: { from in
                guard var p = plan, p.fromDay != PlanDay(from) else { return }
                // In calendar days: a span of seconds would lose a day across a clock change.
                let c = Calendar.current
                let length = c.dateComponents([.day], from: p.from, to: p.to).day ?? 6
                p.from = from
                p.to = c.date(byAdding: .day, value: min(max(length, 0), 30), to: p.from) ?? p.from
                p.looked = false
                answer = nil
                problem = nil
                store.update(p)
            })
    }

    private func look() async {
        guard var p = plan else { return }
        let trip = p.trip(lastYear: lastYear)
        if let why = tripProblem(trip: trip) {
            problem = why
            problemDetail = nil
            answer = nil
            return
        }
        loading = true
        defer { loading = false }
        do {
            answer = try await store.plan(trip, at: p.place)
            problem = nil
            p.looked = true
            store.update(p)
        } catch {
            answer = nil
            let message = userMessage(error)
            problem = message.title
            problemDetail = message.detail
        }
    }

    private func results(_ plan: PlanView) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(plan.title).font(.headline)
            if let average = plan.average {
                Label(average, systemImage: "thermometer.medium").font(.callout)
            }
            if let wet = plan.wetDays {
                Label(wet, systemImage: "umbrella").font(.callout).foregroundStyle(colors.rain)
            }
            PlanChart(days: plan.days)
            let compare = comparing(plan)
            VStack(spacing: 0) {
                if compare { compareHeader }
                ForEach(plan.days, id: \.dateShort) { day in
                    if compare {
                        compareRow(day, forecast[day.dateShort])
                    } else {
                        row(day)
                    }
                    if open == day.dateShort {
                        details(day)
                    }
                    Divider()
                }
            }
            Text(plan.credit).font(.caption2).foregroundStyle(.secondary)
            Text(plan.attribution).font(.caption2).foregroundStyle(.tertiary)
            if compare, let credit = forecastCredit {
                Text("Forecast: \(credit.provider)").font(.caption2).foregroundStyle(.secondary)
                if let observedCredit {
                    Text("Observed: \(observedCredit)").font(.caption2).foregroundStyle(.secondary)
                }
                Text(credit.attribution).font(.caption2).foregroundStyle(.tertiary)
            }
        }
    }

    /// One date: its weekday this trip, how often it was wet, the average low
    /// and high on the trip's scale with the extremes as ticks.
    private func row(_ day: PlanView.Day) -> some View {
        Button {
            withAnimation(.snappy) { open = open == day.dateShort ? nil : day.dateShort }
        } label: {
            HStack(spacing: 10) {
                VStack(alignment: .leading, spacing: 0) {
                    Text(day.label)
                    Text(day.dateShort).font(.caption2).foregroundStyle(.secondary)
                }
                .frame(width: 50, alignment: .leading)
                ConditionIcon(icon: day.icon, size: 20).frame(width: 26)
                Text(day.precipChance.text)
                    .font(.caption)
                    .foregroundStyle(colors.rain)
                    .frame(width: 34, alignment: .leading)
                Text(day.low.text).foregroundStyle(.secondary).frame(width: 36, alignment: .trailing)
                if (day.range.first ?? -1) >= 0 {
                    RangeBar(
                        low: day.range.first ?? 0, high: day.range.last ?? 1,
                        normalLow: (day.extremes.first ?? -1) >= 0 ? day.extremes.first : nil,
                        normalHigh: (day.extremes.last ?? -1) >= 0 ? day.extremes.last : nil
                    )
                    .frame(height: 6)
                } else {
                    Spacer()
                }
                Text(day.high.text).frame(width: 36, alignment: .leading)
            }
            .monospacedDigit()
            .padding(.vertical, 8)
            .contentShape(.rect)
            .accessibilityElement(children: .combine)
            .accessibilityHint("Shows the hottest and coldest")
        }
        .buttonStyle(.plain)
    }

    /// Over the two columns: the typical weather and the forecast.
    private var compareHeader: some View {
        HStack(spacing: 8) {
            Color.clear.frame(width: 46, height: 1)
            Text("Typical").frame(maxWidth: .infinity, alignment: .leading)
            Text(observed.isEmpty ? "Forecast" : "Actual · forecast")
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .font(.caption.weight(.semibold))
        .foregroundStyle(.secondary)
        .padding(.bottom, 4)
    }

    /// One date with its typical weather beside its forecast.
    private func compareRow(_ day: PlanView.Day, _ f: ViewModel.Day?) -> some View {
        Button {
            withAnimation(.snappy) { open = open == day.dateShort ? nil : day.dateShort }
        } label: {
            HStack(spacing: 8) {
                VStack(alignment: .leading, spacing: 0) {
                    Text(day.label)
                    Text(day.dateShort).font(.caption2).foregroundStyle(.secondary)
                }
                .frame(width: 46, alignment: .leading)
                cell(icon: day.icon, high: day.high.text, low: day.low.text, chance: day.precipChance.text)
                if let f {
                    // The air temperature, as the typical column's is.
                    cell(
                        icon: f.icon, high: f.actualHigh.text, low: f.actualLow.text,
                        chance: f.precipProbability.map { chanceText(percent: $0) })
                } else if let o = observed[day.dateShort] {
                    // A date already past: what it was, with its rain or snow.
                    cell(icon: nil, high: o.high.text, low: o.low.text, chance: o.precipitation.text)
                } else {
                    Text("–").foregroundStyle(.secondary).frame(maxWidth: .infinity, alignment: .leading)
                }
            }
            .monospacedDigit()
            .padding(.vertical, 8)
            .contentShape(.rect)
            .accessibilityElement(children: .combine)
            .accessibilityHint("Shows the hottest and coldest")
        }
        .buttonStyle(.plain)
    }

    /// An icon, the high and low, and the chance of rain or snow under them.
    /// Without an icon, the day is observed (no condition is kept for it).
    private func cell(icon: String?, high: String, low: String, chance: String?) -> some View {
        HStack(spacing: 6) {
            if let icon {
                ConditionIcon(icon: icon, size: 20)
            } else {
                Image(systemName: "checkmark.circle")
                    .font(.system(size: 17))
                    .foregroundStyle(.secondary)
                    .frame(width: 20)
                    .accessibilityLabel("Observed")
            }
            VStack(alignment: .leading, spacing: 0) {
                HStack(spacing: 4) {
                    Text(high)
                    Text(low).foregroundStyle(.secondary)
                }
                if let chance {
                    Text(chance).font(.caption2).foregroundStyle(colors.rain)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func details(_ day: PlanView.Day) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            if let f = forecast[day.dateShort] {
                Label(f.outlook ?? f.conditionLabel, systemImage: "binoculars")
                    .foregroundStyle(.primary)
            } else if let o = observed[day.dateShort] {
                Label(o.summary ?? "Observed \(o.high.text) / \(o.low.text)", systemImage: "checkmark.circle")
                    .foregroundStyle(.primary)
            }
            if let hottest = day.hottest { Label("Hottest \(hottest)", systemImage: "sun.max") }
            if let coldest = day.coldest { Label("Coldest \(coldest)", systemImage: "snowflake") }
            Label(
                "Rain or snow in \(day.precipChance.text) of years · \(day.precipitation.text) on average",
                systemImage: "umbrella")
        }
        .font(.caption)
        .foregroundStyle(.secondary)
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.bottom, 10)
        .transition(.opacity.combined(with: .move(edge: .top)))
    }
}

/// Finds a place by name or postal code, for a trip somewhere not saved.
private struct PlaceSearch: View {
    let store: WeatherStore
    let pick: (AppPlace) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var query = ""
    /// Places suggested as you type, nearest the place shown first.
    @State private var suggestions: PlaceSuggestions
    @State private var failure: String?

    init(store: WeatherStore, pick: @escaping (AppPlace) -> Void) {
        self.store = store
        self.pick = pick
        let shown = store.places.indices.contains(store.active) ? store.places[store.active] : nil
        _suggestions = State(initialValue: PlaceSuggestions(store: store, near: shown))
    }

    var body: some View {
        NavigationStack {
            List {
                ForEach(suggestions.items) { suggestion in
                    Button {
                        Task {
                            do {
                                pick(try await suggestions.place(for: suggestion))
                                dismiss()
                            } catch {
                                failure = userMessage(error).title
                            }
                        }
                    } label: {
                        SuggestionRow(suggestion: suggestion)
                    }
                }
                if let message = failure ?? suggestions.status {
                    Text(message).foregroundStyle(.secondary)
                }
            }
            .searchable(text: $query, placement: .navigationBarDrawer(displayMode: .always))
            // Suggestions as you type, from the first letters.
            .onChange(of: query) { suggestions.update(query) }
            .navigationTitle("Trip to…")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
            }
        }
    }
}

/// The trip's temperatures over the years looked back at: for each date, a thin
/// bar from the coldest low to the hottest high, a thick one from the average
/// low to the average high, and lines through the average highs and lows.
private struct PlanChart: View {
    let days: [PlanView.Day]
    /// The date under the finger (its `dateShort`) while sliding across.
    @State private var picked: String?
    @Environment(\.colorTheme) private var colors

    private var warm: Color { colors.color(.tempChartWarm) }
    private var cool: Color { colors.color(.tempChartCool) }

    /// The axis labels: every date for a week, fewer for longer trips.
    private var labelEvery: Int { max(1, Int((Double(days.count) / 8).rounded(.up))) }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Chart {
                if let picked {
                    RuleMark(x: .value("Date", picked))
                        .foregroundStyle(.white.opacity(0.5))
                        .lineStyle(StrokeStyle(lineWidth: 1.5))
                }
                ForEach(days, id: \.dateShort) { day in
                    if let lo = day.lowest.value, let hi = day.highest.value {
                        BarMark(
                            x: .value("Date", day.dateShort), yStart: .value("Coldest", lo),
                            yEnd: .value("Hottest", hi), width: .ratio(0.18)
                        )
                        .foregroundStyle(.white.opacity(0.28))
                        .clipShape(.capsule)
                    }
                    if let lo = day.low.value, let hi = day.high.value {
                        BarMark(
                            x: .value("Date", day.dateShort), yStart: .value("Average low", lo),
                            yEnd: .value("Average high", hi), width: .ratio(0.55)
                        )
                        .foregroundStyle(
                            .linearGradient(
                                colors: [cool, warm], startPoint: .bottom, endPoint: .top)
                        )
                        .clipShape(.rect(cornerRadius: 4))
                    }
                    if let hi = day.high.value {
                        LineMark(x: .value("Date", day.dateShort), y: .value("Temperature", hi), series: .value("Line", "high"))
                            .foregroundStyle(warm)
                            .interpolationMethod(.catmullRom)
                            .lineStyle(StrokeStyle(lineWidth: 1.5, dash: [3, 3]))
                    }
                    if let lo = day.low.value {
                        LineMark(x: .value("Date", day.dateShort), y: .value("Temperature", lo), series: .value("Line", "low"))
                            .foregroundStyle(cool)
                            .interpolationMethod(.catmullRom)
                            .lineStyle(StrokeStyle(lineWidth: 1.5, dash: [3, 3]))
                    }
                }
            }
            .chartYScale(domain: .automatic(includesZero: false))
            // The trip's order (else the picked date's rule, drawn first, would
            // pull its date to the front while sliding).
            .chartXScale(domain: days.map(\.dateShort))
            .chartXAxis {
                AxisMarks(
                    values: stride(from: 0, to: days.count, by: labelEvery).map { days[$0].dateShort }
                        .filter { $0 != picked }
                ) { _ in
                    AxisValueLabel().font(.caption2)
                }
                // The date under the finger, on the axis itself.
                AxisMarks(values: picked.map { [$0] } ?? []) { _ in
                    AxisValueLabel().font(.caption2.weight(.bold)).foregroundStyle(.white)
                }
            }
            .chartYAxis {
                AxisMarks(position: .leading) { value in
                    AxisGridLine().foregroundStyle(.white.opacity(0.12))
                    AxisValueLabel {
                        if let t = value.as(Double.self) {
                            Text(verbatim: "\(Int(t.rounded()))°").font(.caption2)
                        }
                    }
                }
            }
            // Slide across for each date's figures.
            .chartXSelection(value: $picked)
            // The date under the finger on a pill at the top, clear of the hand
            // (an overlay: annotations go missing on glass).
            .chartOverlay { proxy in
                GeometryReader { geo in
                    if let picked, let day = days.first(where: { $0.dateShort == picked }),
                        let anchor = proxy.plotFrame, let x = proxy.position(forX: picked)
                    {
                        let frame = geo[anchor]
                        Text("\(day.label), \(day.dateShort)")
                            .font(.caption.weight(.bold))
                            .foregroundStyle(.black)
                            .padding(.horizontal, 8)
                            .padding(.vertical, 3)
                            .background(.white, in: .capsule)
                            .fixedSize()
                            .position(
                                x: min(max(frame.minX + x, frame.minX + 50), frame.maxX - 50),
                                y: frame.minY + 10)
                    }
                }
                .allowsHitTesting(false)
            }
            .frame(height: 190)
            .accessibilityLabel("Temperatures over the trip's dates in the years looked back at")
            .sensoryFeedback(.selection, trigger: picked)
            if let day = days.first(where: { $0.dateShort == picked }) {
                VStack(alignment: .leading, spacing: 2) {
                    Text("\(day.label) \(day.dateShort): \(day.high.text) / \(day.low.text) on average")
                        .font(.caption.weight(.semibold))
                    Text(
                        [
                            "wet in \(day.precipChance.text) of years", day.hottest.map { "hottest \($0)" },
                            day.coldest.map { "coldest \($0)" },
                        ]
                        .compactMap { $0 }.joined(separator: " · ")
                    )
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                }
                .transition(.opacity)
            }
            HStack(spacing: 14) {
                legend(
                    AnyShapeStyle(
                        LinearGradient(colors: [cool, warm], startPoint: .bottom, endPoint: .top)),
                    "Average low–high", width: 10)
                legend(AnyShapeStyle(.white.opacity(0.35)), "Coldest–hottest", width: 4)
            }
            .font(.caption2)
            .foregroundStyle(.secondary)
        }
        .padding(.vertical, 4)
        .onAppear {
            #if DEBUG
                // Screenshots: `-pickDate 2` slides to the third date.
                let args = ProcessInfo.processInfo.arguments
                if let i = args.firstIndex(of: "-pickDate"), i + 1 < args.count, let n = Int(args[i + 1]),
                    days.indices.contains(n)
                {
                    picked = days[n].dateShort
                }
            #endif
        }
    }

    private func legend(_ style: AnyShapeStyle, _ text: String, width: CGFloat) -> some View {
        HStack(spacing: 5) {
            RoundedRectangle(cornerRadius: 2).fill(style).frame(width: width, height: 12)
            Text(text)
        }
    }
}
