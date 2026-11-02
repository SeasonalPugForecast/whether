import SwiftUI
import WhetherKit

/// The app's tabs: the overview (alerts at its top), the hours in detail, the
/// days with their history, the radar (a globe when zoomed out) and the trip
/// planner. In each, one page per saved place: swipe between them. The bar above opens the place list, sharing and the
/// settings.
struct ForecastView: View {
    @Bindable var store: WeatherStore
    @Environment(\.scenePhase) private var phase
    @Environment(\.colorTheme) private var colors
    @State private var tab = AppTab.now
    /// The Now tab as tiles to reorder, hide and add back.
    @State private var editingSections = false

    enum AppTab: Hashable {
        case now, hourly, days, radar, plan

        init(_ item: AppTabItem) {
            self =
                switch item {
                case .hourly: .hourly
                case .days: .days
                case .radar: .radar
                case .plan: .plan
                }
        }

        /// The choosable tab this is; `nil` for Now, which always shows.
        var item: AppTabItem? {
            switch self {
            case .now: nil
            case .hourly: .hourly
            case .days: .days
            case .radar: .radar
            case .plan: .plan
            }
        }
    }
    @State private var showPlaces = false
    @State private var showSettings = false
    @State private var showWidgets = false
    /// The Customize page, as a sheet (a card's "Customize…" with no editor of its own).
    @State private var showCustomize = false
    /// What fetching depends on, when the settings opened.
    @State private var fetchState: FetchState?
    /// The plan someone sent, as the sheet shows it: `store.incoming`, once no other
    /// sheet is up (a sheet can't show over another).
    @State private var receivedPlan: SharedPlan?
    /// The invitation to a live shared plan, as its sheet shows it: `store.incomingShare`,
    /// likewise (the sheet reads the store's, which gains the plan's details once fetched).
    @State private var receivedShare: IncomingShare?
    /// Colors someone sent (a `.whethertheme` file), as their sheet shows them: likewise.
    @State private var receivedTheme: IncomingTheme?
    /// The problem with a file that was opened, as the alert shows it: likewise.
    @State private var receivedProblem: String?

    /// Settings that change what is fetched.
    private struct FetchState: Equatable {
        /// Each keyed provider's key (compared, never shown).
        var keys: [String: String]
        /// The order and switches of the sources.
        var sources: [Preferences.SourceRow]
        var oneCall: Bool
        var fillGaps: Bool
        var days: Int
        var dailyCalls: Int
        var models: [String]
        var appleMinutes: Bool
        var appleAlerts: Bool
        var feelsLike: Bool
        var history: Bool
    }

    private var currentFetchState: FetchState {
        let p = store.preferences
        return FetchState(
            keys: Keychain.keys(for: Sources.keyedProviders), sources: p.sources, oneCall: p.oneCall, fillGaps: p.fillGaps, days: p.days,
            dailyCalls: p.dailyCalls, models: p.models, appleMinutes: p.appleMinutes,
            appleAlerts: p.appleAlerts,
            feelsLike: p.feelsLike, history: p.history)
    }

    var body: some View {
        Group {
            if store.places.isEmpty {
                WelcomeView(store: store)
            } else {
                tabs
            }
        }
        .environment(\.feelsLikeLeads, store.view?.feelsLikeLeads ?? false)
        .environment(\.sunItems, $store.preferences.sunItems)
        .environment(\.dayRowItems, $store.preferences.dayRowItems)
        .environment(\.dayDetailItems, $store.preferences.dayDetailItems)
        .environment(\.radarOptions, $store.preferences.radar)
        .environment(\.hourlyCards, $store.preferences.hourlyCards)
        .environment(\.daysCards, $store.preferences.daysCards)
        .environment(\.openCustomize, CustomizeOpener(open: { showCustomize = true }))
        .task {
            await store.refresh()
            await store.followMe()
        }
        // Shared plans follow iCloud: on launch (here), on becoming active, and on a push
        // (`AppDelegate`). Does nothing without iCloud (`PlanCloud.isAvailable`).
        .task { await store.followSharedPlans() }
        .task {
            #if DEBUG
                // Screenshots: `-show places` or `-show settings` opens a sheet;
                // `-tab hourly` picks a tab.
                let args = ProcessInfo.processInfo.arguments
                if let i = args.firstIndex(of: "-tab"), i + 1 < args.count {
                    tab =
                        switch args[i + 1] {
                        case "hourly": .hourly
                        case "days": .days
                        case "radar": .radar
                        case "plan": .plan
                        default: .now
                        }
                }
                if args.contains("-centered") { store.preferences.layout = .centered }
                if args.contains("-editsections") { editingSections = true }
                if args.contains("-darksky") { store.preferences.layout = .darkSky }
                if args.contains("-sky") { store.preferences.background = .sky }
                if let i = args.firstIndex(of: "-page"), i + 1 < args.count, let page = Int(args[i + 1]),
                    store.places.indices.contains(page)
                {
                    store.active = page
                }
                if args.contains("-plain") { store.preferences.background = .plain }
                // `-appearance light` (or `dark`, `system`, `sun`) for the plain background.
                if let i = args.firstIndex(of: "-appearance"), i + 1 < args.count,
                    let choice = Preferences.AppearanceChoice(rawValue: args[i + 1])
                {
                    store.preferences.appearance = choice
                }
                // `-showradar`: the radar section, hidden by default.
                if args.contains("-showradar") { store.preferences.show(.radar) }
                if let i = args.firstIndex(of: "-show"), i + 1 < args.count {
                    try? await Task.sleep(for: .milliseconds(800))
                    showPlaces = args[i + 1] == "places"
                    showSettings = ["settings", "sections", "units", "tabs", "customize", "colors", "sources", "source-openweather", "source-pirate-weather", "compare", "compare-models"].contains(args[i + 1])
                    showWidgets = args[i + 1] == "widgets"
                    showCustomize = args[i + 1] == "customize-sheet"
                }
                // `-receivePlan sample` (or a `.whetherplan` file's path) opens the sheet for a
                // plan that was sent to this phone.
                if let i = args.firstIndex(of: "-receivePlan"), i + 1 < args.count {
                    try? await Task.sleep(for: .milliseconds(800))
                    if args[i + 1] == "sample" {
                        store.incoming = SharedPlan.sample
                    } else {
                        store.receivePlan(from: URL(fileURLWithPath: args[i + 1]))
                    }
                }
                // `-receiveTheme sample` (or a `.whethertheme` file's path) opens the sheet
                // for colors that were sent to this phone.
                if let i = args.firstIndex(of: "-receiveTheme"), i + 1 < args.count {
                    try? await Task.sleep(for: .milliseconds(800))
                    if args[i + 1] == "sample" {
                        store.incomingTheme = IncomingTheme(theme: SharedTheme.sample)
                    } else {
                        store.receiveTheme(from: URL(fileURLWithPath: args[i + 1]))
                    }
                }
                // `-receiveShare sample` opens the invitation sheet for a live shared plan
                // (a made-up invitation: nothing reaches iCloud).
                if let i = args.firstIndex(of: "-receiveShare"), i + 1 < args.count,
                    args[i + 1] == "sample"
                {
                    try? await Task.sleep(for: .milliseconds(800))
                    store.incomingShare = IncomingShare.sample
                }
                // `-planChangedBanner sample` shows the banner for a shared plan Robin changed
                // (it stays up in these runs: `PlanChangedBanner`).
                if let i = args.firstIndex(of: "-planChangedBanner"), i + 1 < args.count,
                    args[i + 1] == "sample"
                {
                    try? await Task.sleep(for: .milliseconds(1500))
                    store.showSamplePlanBanner()
                }
                // `-import config.toml` imports that file from Documents and
                // writes what happened to Documents/import-result.txt.
                if let i = args.firstIndex(of: "-import"), i + 1 < args.count {
                    let file = URL.documentsDirectory.appending(path: args[i + 1])
                    let result: String
                    do {
                        let text = try String(contentsOf: file, encoding: .utf8)
                        result = try await store.importDesktopConfig(text).joined(separator: "\n")
                    } catch {
                        result = "failed: \(error)"
                    }
                    try? result.write(
                        to: .documentsDirectory.appending(path: "import-result.txt"), atomically: true,
                        encoding: .utf8)
                    store.active = store.places.count - 1
                }
                // `-deletePlace 1`: adds a second place if there is one only,
                // shows it, then deletes place 1 (reproduces deleting a place).
                if let i = args.firstIndex(of: "-deletePlace"), i + 1 < args.count, let n = Int(args[i + 1]) {
                    if store.places.count < 2 {
                        await store.add(
                            AppPlace(
                                name: "Brooklyn", detail: "New York, US", latitude: 40.6500,
                                longitude: -73.9496, timeZone: "America/New_York"))
                    }
                    store.active = store.places.count - 1
                    try? await Task.sleep(for: .seconds(2))
                    showPlaces = true
                    try? await Task.sleep(for: .seconds(2))
                    // As a swipe in the list does: inside an animation.
                    withAnimation { store.remove(at: IndexSet(integer: n)) }
                }
                // `-nowcastHours 8` sets the rain graph's hours.
                if let i = args.firstIndex(of: "-nowcastHours"), i + 1 < args.count,
                    let hours = Int(args[i + 1])
                {
                    store.preferences.nowcastHours = hours
                }
            #endif
        }
        .task {
            // Relative times ("Updated 3 min ago") move on.
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(60))
                store.rebuild()
            }
        }
        .onChange(of: store.active) {
            // Swiping to a page that may need Apple's minutes (not fetched while it was off screen).
            if store.places.indices.contains(store.active) {
                let place = store.places[store.active]
                Task { await store.minutesIfWanted(for: place) }
            }
        }
        .onChange(of: phase) {
            if phase == .active {
                Task {
                    if store.due { await store.refresh() }
                    await store.followMe()
                }
                Task { await store.followSharedPlans() }
            }
        }
        // A banner or a notification about a shared plan was tapped: the Plan tab shows it.
        .onChange(of: store.planTabRequested) {
            if store.planTabRequested {
                store.planTabRequested = false
                showPlanTab()
            }
        }
        .sheet(isPresented: $showPlaces, onDismiss: presentReceived) { PlacesView(store: store) }
        #if DEBUG
            .sheet(isPresented: $showWidgets, onDismiss: presentReceived) {
                WidgetGallery(view: store.view)
            }
        #endif
        .sheet(isPresented: $showCustomize, onDismiss: presentReceived) {
            EditorSheet { CustomizeView(store: store) }
        }
        // A plan someone sent (a `.whetherplan` file opened): Add or Decline. Swiping the
        // sheet away declines.
        .sheet(
            item: $receivedPlan,
            onDismiss: {
                store.incoming = nil
                presentReceived()
            }
        ) { shared in
            ReceivedPlanSheet(store: store, shared: shared) { showPlanTab() }
        }
        // An invitation to a live shared plan (iCloud): Add or Decline likewise.
        .sheet(
            item: $receivedShare,
            onDismiss: {
                store.declineShare()
                presentReceived()
            }
        ) { _ in
            ReceivedShareSheet(store: store) { showPlanTab() }
        }
        // Colors someone sent (a `.whethertheme` file opened): Add or Decline. Swiping the
        // sheet away declines.
        .sheet(
            item: $receivedTheme,
            onDismiss: {
                store.incomingTheme = nil
                presentReceived()
            }
        ) { incoming in
            ReceivedThemeSheet(store: store, shared: incoming.theme)
        }
        .alert(
            receivedProblem ?? "",
            isPresented: Binding(
                get: { receivedProblem != nil },
                set: {
                    if !$0 {
                        receivedProblem = nil
                        store.incomingProblem = nil
                    }
                })
        ) {
            Button("OK") {}
        }
        .sheet(
            isPresented: $showSettings,
            onDismiss: {
                if fetchState != currentFetchState {
                    Task { await store.refetch() }
                }
                presentReceived()
            }
        ) {
            SettingsView(store: store)
        }
        // `initial`: a file that opened the app is already waiting when this appears.
        .onChange(of: store.incoming, initial: true) { presentReceived() }
        .onChange(of: store.incomingShare?.id, initial: true) { presentReceived() }
        .onChange(of: store.incomingTheme?.id, initial: true) { presentReceived() }
        .onChange(of: store.incomingProblem, initial: true) { presentReceived() }
    }

    /// Shows the plan, the colors or the problem of a file that was opened. When one of
    /// the sheets is up, closes it and returns: each sheet's `onDismiss` calls this again
    /// once it is gone, since the received sheets and the alert can't show over it. One
    /// received sheet shows at a time (a plan, then an invitation, then colors), and the
    /// alert waits for them too.
    private func presentReceived() {
        if showPlaces || showSettings || showWidgets || showCustomize {
            showPlaces = false
            showSettings = false
            showWidgets = false
            showCustomize = false
            return
        }
        let planWasUp = receivedPlan != nil || receivedShare != nil || receivedTheme != nil
        receivedPlan = store.incoming
        // An invitation waits while a copy's sheet is up (its `onDismiss` comes back).
        receivedShare = receivedPlan == nil ? store.incomingShare : nil
        // Colors wait for both.
        receivedTheme = receivedPlan == nil && receivedShare == nil ? store.incomingTheme : nil
        // Not while a received sheet is up or still going away: its `onDismiss` comes back.
        if !planWasUp && receivedPlan == nil && receivedShare == nil && receivedTheme == nil {
            receivedProblem = store.incomingProblem
        }
    }

    /// The tabs after Now that show, in order. Screenshots may choose them for
    /// one launch, without saving: `-tabs plan,hourly` shows those two, in that order.
    private var visibleTabs: [AppTabItem] {
        #if DEBUG
            if let names = ProcessInfo.processInfo.launchList("-tabs") {
                return TabChoice.arranged(by: names).shownItems()
            }
        #endif
        return TabLayout.visible(store.preferences.tabs)
    }

    /// The Plan tab, to show a plan just added; it comes back if Settings had hidden it.
    private func showPlanTab() {
        if !visibleTabs.contains(.plan) { store.preferences.tabs.set(.plan, shown: true) }
        tab = .plan
    }

    /// Back to Now when the tab shown is hidden (in Settings, or by a launch argument).
    private func keepTabVisible() {
        if tab.item != nil, TabLayout.keeping(tab.item, among: visibleTabs) == nil {
            tab = .now
        }
    }

    private var tabs: some View {
        TabView(selection: $tab) {
            Tab("Now", systemImage: "thermometer.sun", value: AppTab.now) {
                pager { view, place, shown in overviewPage(view, place: place, shown: shown) }
            }
            // Alerts lead the Now tab; the badge counts the ones not read yet.
            .badge(store.unreadAlerts.count)
            // The others, as Settings → Tabs orders and shows them.
            ForEach(visibleTabs) { item in
                Tab(item.title, systemImage: item.symbol, value: AppTab(item)) {
                    tabContent(item)
                }
            }
        }
        .onChange(of: visibleTabs) { keepTabVisible() }
        .onChange(of: tab) { keepTabVisible() }
        // White controls on the skies (the accent blue vanishes on them).
        .tint(store.preferences.background == .sky ? .white : nil)
        // Someone changed a shared plan: a line at the top for a few seconds.
        .overlay(alignment: .top) {
            // The animation is the banner's own (not the whole tab view's): a glass shape
            // that animates with its content keeps the one-line size it had at first.
            ZStack {
                if let banner = store.planBanners.first {
                    PlanChangedBanner(banner: banner) {
                        store.dismissPlanBanner(banner.id)
                        if let id = banner.planID { store.openPlan(id) } else { showPlanTab() }
                    }
                    .id(banner.id)
                    .task { await timeOut(banner) }
                    .padding(.horizontal, 16)
                    .transition(.move(edge: .top).combined(with: .opacity))
                }
            }
            .animation(.snappy, value: store.planBanners.first?.id)
        }
        .overlay {
            if store.views.isEmpty, let failure = store.failure {
                ContentUnavailableView {
                    Label("No forecast", systemImage: "exclamationmark.triangle")
                } description: {
                    Text(failure)
                } actions: {
                    if let detail = store.failureDetail {
                        DisclosureGroup("Details") { Text(detail).font(.caption) }
                            .padding(.horizontal, 32)
                    }
                }
            }
        }
    }

    /// The banner goes after five seconds (the next one, if any, shows then). Screenshot runs
    /// of the sample keep it up.
    private func timeOut(_ banner: PlanBanner) async {
        #if DEBUG
            if ProcessInfo.processInfo.arguments.contains("-planChangedBanner") { return }
        #endif
        try? await Task.sleep(for: .seconds(5))
        guard !Task.isCancelled else { return }
        store.dismissPlanBanner(banner.id)
    }

    @ViewBuilder private func tabContent(_ item: AppTabItem) -> some View {
        switch item {
        case .hourly:
            pager { view, _, _ in
                HourlyDetailPage(
                    view: view, cards: cards, tiles: $store.preferences.hourTiles,
                    measures: $store.preferences.graphMeasures,
                    options: $store.preferences.graphOptions)
            }
        case .days:
            pager { view, _, _ in DaysPage(view: view, cards: cards) }
        case .radar:
            RadarTab(store: store)
                .modifier(TopBar { header })
        case .plan:
            PlanTab(store: store)
        }
    }

    /// The place name (opens the list), its time, and the settings button,
    /// floating over the forecast.
    private var header: some View {
        HStack(alignment: .center) {
            Button {
                showPlaces = true
            } label: {
                VStack(alignment: .leading, spacing: 1) {
                    HStack(spacing: 4) {
                        Text(store.view?.location.name ?? "Seasonal Pug Forecast").font(.headline)
                        Image(systemName: "chevron.down").font(.caption.weight(.semibold))
                    }
                    if let location = store.view?.location {
                        Text([location.region, location.localTime].compactMap { $0 }
                            .joined(separator: " · "))
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
                .padding(.horizontal, 6)
                .padding(.vertical, 2)
            }
            .glassButton()
            .accessibilityLabel("Places")
            // Touch and hold: jump to any place without swiping through the pages.
            .contextMenu { placesMenu }
            Spacer()
            if let view = store.view, view.current != nil {
                ShareLink(
                    item: ForecastImage(view: view, colors: colors),
                    preview: SharePreview(
                        view.location.name, image: Image(systemName: "cloud.sun.fill"))
                ) {
                    Image(systemName: "square.and.arrow.up")
                        .font(.title3)
                        .padding(4)
                }
                .glassButton()
                .buttonBorderShape(.circle)
                .accessibilityLabel("Share the forecast")
            }
            Button {
                fetchState = currentFetchState
                showSettings = true
            } label: {
                Image(systemName: "gearshape")
                    .font(.title3)
                    .padding(4)
            }
            .glassButton()
            .buttonBorderShape(.circle)
            .accessibilityLabel("Settings")
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 6)
        // While fetching, a pill under the bar says so (over the forecast,
        // so nothing moves when it comes and goes).
        .overlay(alignment: .bottom) {
            if store.refreshing {
                UpdatingPill()
                    .offset(y: 34)
                    .transition(.opacity.combined(with: .move(edge: .top)))
            }
        }
        .animation(.snappy, value: store.refreshing)
    }

    /// The place pill's menu: every place (the shown one checked), then pin or
    /// unpin the shown place, and the place list.
    @ViewBuilder private var placesMenu: some View {
        Picker("Place", selection: $store.active) {
            ForEach(Array(store.places.enumerated()), id: \.element.id) { index, place in
                Label(place.name, systemImage: marker(of: place)).tag(index)
            }
        }
        if store.places.indices.contains(store.active), !store.places[store.active].follows {
            let place = store.places[store.active]
            Button(
                place.pinned ? "Unpin \(place.name)" : "Pin \(place.name)",
                systemImage: place.pinned ? "pin.slash" : "pin"
            ) {
                withAnimation { store.togglePin(id: place.id) }
            }
        }
        Button("Edit places…", systemImage: "list.bullet") { showPlaces = true }
    }

    /// The symbol that says what a place is in the menu: following, the favourite, pinned, or none of those.
    private func marker(of place: AppPlace) -> String {
        if place.follows { return "location.fill" }
        if store.preferences.favouritePlaceID == place.id { return "star.fill" }
        return place.pinned ? "pin.fill" : "mappin"
    }

    /// Whether sections sit on glass cards: in the centered layout, and on the
    /// skies (where bare sections would float on the gradient).
    private var cards: Bool {
        store.preferences.layout == .centered || store.preferences.background == .sky
    }

    /// One page per place, swiped with the selection shared by every tab, each
    /// on its own sky.
    private func pager<Page: View>(
        @ViewBuilder _ page: @escaping (ViewModel, AppPlace, Bool) -> Page
    ) -> some View {
        // Each page carries its own place and view: indexing both arrays from
        // a page could run past one of them while a deleted place animates
        // away, or when the views lag the places (a crash on deleting places).
        let pages = Array(zip(store.places, store.views).enumerated())
        return TabView(selection: $store.active) {
            ForEach(pages, id: \.element.0.id) { index, pair in
                page(pair.1, pair.0, index == store.active)
                    .tag(index)
            }
        }
        .tabViewStyle(.page(indexDisplayMode: store.places.count > 1 ? .always : .never))
        .indexViewStyle(.page(backgroundDisplayMode: .always))
        .modifier(TopBar { header })
        // Behind the bars too; it follows the place shown.
        .background {
            if store.preferences.background == .sky, let view = store.view {
                SkyBackground(theme: view.theme)
            }
        }
        .environment(
            \.skyCard,
            store.preferences.background == .sky ? store.view.map { colors.card($0.theme) } : nil)
    }

    @ViewBuilder
    private func overviewPage(_ view: ViewModel, place: AppPlace, shown: Bool) -> some View {
        if editingSections {
            SectionTiles(store: store) {
                withAnimation(.snappy) { editingSections = false }
            }
            .transition(.opacity)
        } else {
            overviewScroll(view, place: place, shown: shown)
        }
    }

    private func overviewScroll(_ view: ViewModel, place: AppPlace, shown: Bool) -> some View {
        ScrollViewReader { proxy in
            ScrollView {
                content(view, place: place)
                    .padding(.horizontal, cards ? 16 : 20)
                    .padding(.top, 12)
                    .padding(.bottom, 40)
            }
            .refreshable {
                // The following place first, so the refresh fetches where you are.
                await store.followMe(refetch: false)
                await store.refresh()
            }
            .task(id: shown && view.current != nil) {
                #if DEBUG
                    // Screenshots (`cargo xtask ios remote shots`): `-scrollTo days`
                    // scrolls once the forecast is laid out.
                    let args = ProcessInfo.processInfo.arguments
                    guard shown, view.current != nil,
                        let i = args.firstIndex(of: "-scrollTo"), i + 1 < args.count
                    else { return }
                    try? await Task.sleep(for: .milliseconds(600))
                    proxy.scrollTo(args[i + 1], anchor: .top)
                #endif
            }
        }
    }

    /// What touching and holding a section of the Now tab adds to its menu:
    /// Edit the Now tab, and Hide (not for the current conditions or alerts).
    private func menu(for section: Preferences.OverviewSection) -> CardMenu {
        CardMenu(
            editTitle: "Edit the Now tab",
            edit: { withAnimation(.snappy) { editingSections = true } },
            hideTitle: "Hide \(section.title)",
            hide: Preferences.hideable(section)
                ? { withAnimation(.snappy) { store.preferences.hide(section) } } : nil)
    }

    /// One section of the Now tab, when there is something to show.
    @ViewBuilder private func section(
        _ section: Preferences.OverviewSection, view: ViewModel, current: ViewModel.Current,
        place: AppPlace
    ) -> some View {
        switch section {
        case .now:
            Group {
                // The layout picks the top (cards also show on the sky).
                if store.preferences.layout == .centered {
                    CenteredNowSection(view: view, current: current)
                } else {
                    NowSection(view: view, current: current)
                }
            }
            // The sun and moon behind the summary, on their way across (a switch in Settings).
            .background {
                if store.preferences.skyBodies { SkyBodies(sun: view.sun, moon: view.moon) }
            }
            .contentShape(.rect)
            .cardContextMenu()
            .id("now")
        case .precipitation:
            if view.nowcast != nil || view.precipitation != nil {
                RainSection(
                    nowcast: view.nowcast, precipitation: view.precipitation,
                    span: $store.preferences.rainSpan,
                    nowcastHours: $store.preferences.nowcastHours,
                    precipHours: $store.preferences.precipHours
                )
                .glassCard(cards)
                .cardContextMenu()
                .id("nexthours")
            }
        case .alerts:
            if !view.alerts.isEmpty {
                AlertsSection(
                    alerts: view.alerts, read: store.readAlerts, markRead: { store.markRead($0) })
            }
        case .hours:
            if let hourly = view.hourly {
                HourlySection(hourly: hourly, rows: $store.preferences.hourRows).glassCard(cards).id("hours")
            }
        case .days:
            if let daily = view.daily {
                DailySection(daily: daily).glassCard(cards).id("days")
            }
        case .radar:
            let settings = store.preferences.settings()
            if settings.radarSource(for: place) != .none {
                RadarSection(place: place, settings: settings)
                    .glassCard(cards)
                    .id("radar")
            }
        case .details:
            if !view.details.isEmpty {
                DetailsSection(details: view.details, choices: $store.preferences.detailCards)
                    .id("details")
            }
        case .sun:
            // On the Hourly tab now; older saved lists drop it (Preferences.complete).
            EmptyView()
        case .history:
            if let history = view.history {
                HistorySection(history: history).glassCard(cards).cardContextMenu().id("history")
            }
        }
    }

    @ViewBuilder private func content(_ view: ViewModel, place: AppPlace) -> some View {
        if let current = view.current {
            VStack(alignment: .leading, spacing: cards ? 14 : 28) {
                // In the order Settings → Sections gives, without the hidden.
                ForEach(store.preferences.sections.filter(\.shown)) { choice in
                    section(choice.section, view: view, current: current, place: place)
                        // Touch and hold: Customize…, Edit the Now tab, Hide … (the
                        // system's menu: no clash with scrolling or the cards' taps).
                        // Each card puts them in its own menu or, without one, takes the
                        // card-wide menu below.
                        .environment(\.cardMenu, menu(for: choice.section))
                }
                FooterSection(
                    view: view, appleWeather: store.usesAppleWeather(place),
                    appleCredit: store.appleCredit(place),
                    errorDetail: store.errorDetail(for: place), keyProblem: store.keyProblem(for: place),
                    locationOff: place.follows && store.locationOff
                ).id("footer")
                Button {
                    withAnimation(.snappy) { editingSections = true }
                } label: {
                    Label("Edit the Now tab", systemImage: "square.grid.2x2")
                        .font(.callout)
                }
                .buttonStyle(.bordered)
                .frame(maxWidth: .infinity)
            }
        } else {
            VStack(spacing: 12) {
                ProgressView()
                Text(view.status.error ?? "Fetching the forecast…")
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                if view.status.error != nil, let detail = store.errorDetail(for: place) {
                    DisclosureGroup("Details") { Text(detail).font(.caption) }
                        .padding(.horizontal, 32)
                }
                Text(view.attribution).font(.caption2).foregroundStyle(.tertiary)
            }
            .frame(maxWidth: .infinity, minHeight: 400)
        }
    }
}

/// The bar above the forecast: Liquid Glass over the scrolling content on
/// iOS 26, a plain bar before.
private struct TopBar<Bar: View>: ViewModifier {
    @ViewBuilder var bar: () -> Bar

    func body(content: Content) -> some View {
        if #available(iOS 26.0, *) {
            content.safeAreaBar(edge: .top) { bar() }
        } else {
            content.safeAreaInset(edge: .top, spacing: 0) { bar().background(.bar) }
        }
    }
}

extension View {
    /// Liquid Glass on iOS 26, plain before.
    @ViewBuilder func glassButton() -> some View {
        if #available(iOS 26.0, *) {
            buttonStyle(.glass)
        } else {
            buttonStyle(.plain)
        }
    }
}

/// "Robin moved Summer at the shore to Jul 12–Jul 19, 2027": a Liquid Glass card at the top of
/// the app, as wide as the screen allows and as tall as its text (one line or two), tapped to
/// open the plan.
private struct PlanChangedBanner: View {
    let banner: PlanBanner
    let open: () -> Void

    var body: some View {
        // The glass is on the button itself, around the whole label (not on the label, inside
        // the button), so it takes the size the text gives.
        Button(action: open) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Image(systemName: "person.2.fill").font(.caption)
                Text(banner.text)
                    .font(.footnote.weight(.semibold))
                    .multilineTextAlignment(.leading)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 12)
            .contentShape(.rect(cornerRadius: 20))
        }
        .buttonStyle(.plain)
        .bannerGlass()
        .accessibilityLabel(banner.text)
        .accessibilityHint("Opens the plan")
    }
}

extension View {
    /// A rounded card of Liquid Glass (iOS 26), else of material. Not a capsule: two lines of
    /// text in a capsule look odd.
    @ViewBuilder fileprivate func bannerGlass() -> some View {
        if #available(iOS 26.0, *) {
            GlassEffectContainer { glassEffect(.regular, in: .rect(cornerRadius: 20)) }
        } else {
            background(.regularMaterial, in: .rect(cornerRadius: 20))
                .shadow(color: .black.opacity(0.15), radius: 6, y: 2)
        }
    }
}

/// "Updating the forecast" with a spinner, while a fetch runs.
private struct UpdatingPill: View {
    var body: some View {
        HStack(spacing: 8) {
            ProgressView().controlSize(.small)
            Text("Updating the forecast").font(.caption.weight(.semibold))
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 7)
        .background(.regularMaterial, in: .capsule)
        .shadow(color: .black.opacity(0.15), radius: 6, y: 2)
        .accessibilityElement(children: .combine)
        .accessibilityLabel("Updating the forecast")
    }
}
