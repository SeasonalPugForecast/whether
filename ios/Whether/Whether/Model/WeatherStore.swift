import BackgroundTasks
import CloudKit
import CoreLocation
import Foundation
import Observation
import SwiftUI
import UserNotifications
import WhetherKit
import WidgetKit

/// What the app keeps per place between launches.
nonisolated struct PlaceCache: Codable, Sendable, Equatable {
    /// The core's snapshot JSON.
    var snapshotJson: String?
    /// `Last-Modified` for the next conditional request.
    var lastModified: String?
    /// Unix seconds of the last successful fetch.
    var lastSuccess: Int64?
    /// The last fetch's error, in words for the person using the app.
    var error: String?
    /// The raw text behind `error` (provider attempts), for a "Details" disclosure.
    var errorDetail: String?
    /// Whether the snapshot holds Apple's minute nowcast (credited in the footer).
    var appleMinutes: Bool?
    /// Unix seconds of the last WeatherKit call for this place (`nil` in caches
    /// from older builds): calls are at least ten minutes apart.
    var appleMinutesAt: Int64?
    /// The minutes that call returned, put back into a snapshot refreshed without
    /// another call, while less than an hour old.
    var appleMinuteSteps: [KeptMinute]?
    /// Whether the snapshot holds Apple's alerts (credited in the footer).
    var appleAlerts: Bool?
    /// Unix seconds of the last answer from Apple's alerts for this place (`nil`
    /// in caches from older builds): calls are at least 30 minutes apart.
    var appleAlertsAt: Int64?
    /// The alerts that call returned, put back into a snapshot refreshed without
    /// another call, while less than an hour old.
    var appleAlertList: [KeptAlert]?
    /// The kept history (`fetchHistory`'s `historyJson`), fetched about once a day.
    var historyJson: String?
    /// Normals computed from 30 years of ERA5, kept for good (abroad, where no
    /// station has them).
    var normalsJson: String?
    /// When the last history fetch failed: it waits an hour.
    var historyFailedAt: Int64?
    /// "OpenWeather didn't answer: …" while another provider serves in its place;
    /// cleared when OpenWeather serves again (`nil` in caches from older builds).
    var keyProblem: String?
    /// The OpenWeather attempts behind `keyProblem`, for a "Details" disclosure.
    var keyProblemDetail: String?
    /// The "Storm ahead" card (`StormAhead` JSON) from the last check that answered;
    /// `nil` when nothing significant is coming, or never checked (older caches).
    var stormAheadJson: String?
}

/// The forecast state: the saved places, their cached snapshots, the settings,
/// and the views the core builds from them.
@Observable
final class WeatherStore {
    /// Saved places, in the user's order (empty until the first is added).
    private(set) var places: [AppPlace]
    /// The place shown.
    var active: Int {
        didSet {
            SharedContainer.defaults.set(active, forKey: "active")
            // Shown now, a page left behind by the minute's rebuild catches up.
            if let id = shownPlaceID, behind.contains(id) { rebuild(only: [id]) }
        }
    }
    /// One view per place, in the places' order, rebuilt after every fetch, every
    /// minute and every change.
    private(set) var views: [ViewModel] = []
    /// The place each of `views` is for.
    @ObservationIgnored private var viewIDs: [String] = []
    /// The views to build next, asked for while a build runs.
    @ObservationIgnored private var wantedViews: ViewRequest?
    /// The build running, which also builds what is asked for meanwhile.
    @ObservationIgnored private var building: Task<Void, Never>?
    /// Places off screen whose views the minute's rebuild left out.
    @ObservationIgnored private var behind: Set<String> = []
    private(set) var refreshing = false
    /// Why the views could not be built, in words, and the raw text behind it.
    private(set) var failure: String?
    private(set) var failureDetail: String?
    /// The app's settings.
    var preferences: Preferences {
        didSet {
            guard preferences != oldValue else { return }
            // Saved, and the widgets told, once the changes stop: a slider or a color
            // picker being dragged changes this many times a second.
            savePreferencesSoon()
            if preferences.refreshMinutes != oldValue.refreshMinutes { scheduleRefresh() }
            // Layout-only choices (cards, tiles, the graph's measures) leave the
            // views as they are.
            let settings = preferences.settings(withKey: false)
            if settings != viewSettings {
                viewSettings = settings
                rebuild()
                widgetsWaiting = true
            } else if preferences.colorTheme != oldValue.colorTheme
                || preferences.colorVision != oldValue.colorVision
                || preferences.patterns != oldValue.patterns
                || preferences.customThemes != oldValue.customThemes
            {
                // The widgets paint with the same colors (`WidgetData.entry`).
                widgetsWaiting = true
            }
        }
    }
    /// The settings the views were last built with (keyless).
    @ObservationIgnored private var viewSettings: Settings?
    /// A settings change waiting to be saved.
    @ObservationIgnored private var pendingPreferences: Task<Void, Never>?
    /// The widgets read the settings again once the change waiting is saved.
    @ObservationIgnored private var widgetsWaiting = false
    private var caches: [String: PlaceCache] = [:]
    /// The other places' caches being read at launch (`nil` once they are in):
    /// fetches wait for it.
    @ObservationIgnored private var cachesRead: Task<Void, Never>?
    private let directory: URL
    /// What has been notified about which alert.
    private var notifier = AlertNotifier()
    /// What the running refresh still has to fetch (`nil` while none runs); a
    /// refresh asked for meanwhile joins it.
    private var queue: RefreshQueue?
    /// Saved trip plans, in the user's order.
    private(set) var plans: [SavedPlan] = []
    /// A plan someone sent, waiting for Add or Decline.
    var incoming: SharedPlan?
    /// An invitation to a live shared plan the system handed to the app (a link opened from
    /// Messages or Mail), waiting for Add or Decline: nothing is accepted before Add.
    var incomingShare: IncomingShare?
    /// Colors someone sent (a `.whethertheme` file), waiting for Add or Decline.
    var incomingTheme: IncomingTheme?
    /// Why a file that was opened isn't a plan (or a theme), for an alert.
    var incomingProblem: String?
    /// The plan the Plan tab opens next: the one just added.
    var planToOpen: UUID?
    /// A plan change waiting to be saved.
    @ObservationIgnored private var pendingSave: Task<Void, Never>?
    /// iCloud for shared plans: `nil` until a plan is shared or found shared, and never made
    /// on a normal launch (`planCloud()`, `PlanCloud`'s note: creating it without the iCloud
    /// entitlement crashes).
    @ObservationIgnored private(set) var cloud: PlanCloud?
    /// Edits of shared plans waiting for a quiet moment before they go to iCloud, by plan id.
    @ObservationIgnored private var pendingCloud: [UUID: Task<Void, Never>] = [:]
    /// Shared plans with an edit that isn't in iCloud yet (offline, or it failed): kept across
    /// launches and sent with the next `syncSharedPlans`.
    private(set) var unsentPlans = Set(
        (SharedContainer.defaults.stringArray(forKey: "unsentSharedPlans") ?? [])
            .compactMap(UUID.init(uuidString:)))
    /// Why the last iCloud step for shared plans failed, as a headline; `nil` after one worked.
    var cloudProblem: String?
    /// Plans someone else changed, to say so at the top of the app, the first showing now.
    /// The view removes each after a few seconds (`dismissPlanBanner`).
    private(set) var planBanners: [PlanBanner] = []
    /// Set by `openPlan`: the app shows the Plan tab (the view clears it).
    var planTabRequested = false
    /// The alert (`alertKey`) whose notification was tapped: the Now tab opens its sheet and
    /// clears it.
    var alertToOpen: String?
    /// Syncs wait for the one before them: a launch, becoming active and a push can all ask at
    /// once, and two fetches from one change token would find each change twice.
    @ObservationIgnored private var syncChain: Task<Void, Never>?
    /// Subscriptions and push registration were made sure of (this launch).
    @ObservationIgnored private var subscribed = false
    /// iCloud was asked for shared plans although none is on this device (this launch): the
    /// check that finds an owner's plans on a second device.
    @ObservationIgnored private var checkedICloud = false
    /// Why the last sync failed (`nil` after one that worked).
    @ObservationIgnored private var lastSyncError: Error?
    /// The headline of what went wrong with a shared plan in the sync running now (an edit
    /// refused, a plan no longer shared), which the sync's end must not clear.
    @ObservationIgnored private var syncProblem: String?
    /// Shared plans deleted on this device whose stop or leave didn't reach iCloud yet (a
    /// tombstone each, kept across launches): retried with every sync, and until it works
    /// iCloud's copy is not applied (or the plan would come back).
    @ObservationIgnored private var endedSharing: [PlanSharing] = {
        SharedContainer.defaults.data(forKey: "endedSharedPlans")
            .flatMap { try? JSONDecoder().decode([PlanSharing].self, from: $0) } ?? []
    }()
    /// Alerts the user has read (opened or marked), by `alertKey`: the Now
    /// tab's badge counts only the others.
    private(set) var readAlerts = Set(SharedContainer.defaults.stringArray(forKey: "readAlerts") ?? [])
    /// Plan places' forecasts, by rounded coordinates, with when they were fetched.
    @ObservationIgnored private var planForecasts: [String: (at: Date, view: ViewModel)] = [:]
    /// Sources' forecasts being compared, by source and place, with when they came.
    @ObservationIgnored private var comparisons: [String: (at: Date, view: ViewModel)] = [:]

    /// Location access is off while a place follows the phone: it keeps its last coordinates.
    private(set) var locationOff = false
    /// A location read is under way (one at a time).
    @ObservationIgnored private var followingNow = false

    #if DEBUG
        /// The card `-stormahead` shows, and the time it is shown at.
        private var sampleStormAhead: String?
        @ObservationIgnored private var sampleStormAheadNow: Int64 = 0
    #endif

    /// The shown place's view.
    var view: ViewModel? { views.indices.contains(active) ? views[active] : views.first }

    /// The id of the place `view` is for.
    private var shownPlaceID: String? {
        (places.indices.contains(active) ? places[active] : places.first)?.id
    }

    init(directory: URL? = nil) {
        let directory = directory ?? SharedContainer.directory
        self.directory = directory
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        // Shown in Files (On My iPhone › Whether), where a desktop config.toml can go.
        try? FileManager.default.createDirectory(at: .documentsDirectory, withIntermediateDirectories: true)
        Self.moveIntoGroup(directory)
        #if DEBUG
            // Screenshots: `-offline` fails every request, as with no connection.
            if ProcessInfo.processInfo.arguments.contains("-offline") {
                URLSessionHost.simulateOffline = true
            }
            // `-slow`: every request waits 4 s (to see the loading states).
            if ProcessInfo.processInfo.arguments.contains("-slow") {
                URLSessionHost.simulatedDelay = .seconds(4)
            }
            // `-reset` forgets the saved places (the welcome screen),
            // `-seed` adds New York when there are none.
            let args = ProcessInfo.processInfo.arguments
            if args.contains("-reset") {
                try? FileManager.default.removeItem(at: directory.appending(path: "places.json"))
            }
        #endif
        var places = (try? Data(contentsOf: directory.appending(path: "places.json")))
            .flatMap { try? JSONDecoder().decode([AppPlace].self, from: $0) } ?? []
        #if DEBUG
            if args.contains("-seed"), places.isEmpty {
                places = [.new_york]
                // `-threePlaces` adds two more (to count WeatherKit calls in a refresh).
                if args.contains("-threePlaces") {
                    places += [
                        AppPlace(
                            name: "Brooklyn", detail: "New York, US", latitude: 40.6500,
                            longitude: -73.9496, timeZone: "America/New_York"),
                        AppPlace(
                            name: "Philadelphia", detail: "Pennsylvania, US", latitude: 39.9526,
                            longitude: -75.1652, timeZone: "America/New_York"),
                    ]
                }
                try? JSONEncoder().encode(places)
                    .write(to: directory.appending(path: "places.json"), options: .atomic)
            }
            // `-seedFollowing`: the first seeded place follows the phone
            // (with `-fakeLocation`, which gives the simulator a location).
            if args.contains("-seedFollowing"), !places.isEmpty { places[0].follows = true }
            // `-seedLondon`: London instead, in memory only (a place outside the US).
            if args.contains("-seedLondon") { places = [.london] }
        #endif
        var loaded = Preferences.load()
        #if DEBUG
            // `-seedPins` (three places): the last is pinned and the second is the
            // favourite, in memory only. `-openOn favourite|following` opens on one.
            if args.contains("-seedPins"), places.count >= 3 {
                places[2].pinned = true
                loaded.favouritePlaceID = places[1].id
            }
            if let i = args.firstIndex(of: "-openOn"), i + 1 < args.count,
                let choice = Preferences.OpenOn(rawValue: args[i + 1])
            {
                loaded.openOn = choice
            }
        #endif
        let arranged = AppPlace.arranged(places)
        self.places = arranged
        // The app opens on the place the preference says; the widget reads the same index.
        let opening = PlaceChoice.openIndex(
            loaded.openOn, places: arranged, favouriteID: loaded.favouritePlaceID,
            lastShown: SharedContainer.defaults.integer(forKey: "active"))
        active = opening
        SharedContainer.defaults.set(opening, forKey: "active")
        preferences = loaded
        #if DEBUG
            // Screenshots: `-units uk` shows °C with mph for this launch (not saved).
            if let i = args.firstIndex(of: "-units"), i + 1 < args.count, args[i + 1] == "uk" {
                preferences.unitOverrides = Preferences.UnitPicks(
                    UnitOverrides(
                        temperature: .celsius, wind: .mph, pressure: nil, precipitation: nil,
                        distance: nil))
            }
            // `-colors aurora` paints with a preset for this launch (not saved).
            if let i = args.firstIndex(of: "-colors"), i + 1 < args.count {
                preferences.colorTheme = args[i + 1]
            }
            // `-vision red_green` (or `blue_yellow`, `no_color`) and `-patterns on` (or
            // `off`, `automatic`) for this launch (not saved).
            if let i = args.firstIndex(of: "-vision"), i + 1 < args.count,
                let vision = ColorVision(rawValue: args[i + 1])
            {
                preferences.colorVision = vision
            }
            if let i = args.firstIndex(of: "-patterns"), i + 1 < args.count,
                let patterns = Preferences.PatternsChoice(rawValue: args[i + 1])
            {
                preferences.patterns = patterns
            }
            // `-dates compact` (any `DateChoice`) for this launch (not saved).
            if let i = args.firstIndex(of: "-dates"), i + 1 < args.count,
                let dates = Preferences.DateChoice(rawValue: args[i + 1])
            {
                preferences.dates = dates
            }
        #endif
        // Before the first frame, only the place shown: its cache and its view. The
        // others' caches are read off the main actor, and then every view is built.
        let shown = self.places.indices.contains(opening) ? self.places[opening] : self.places.first
        if let shown { caches[shown.id] = withDebugAlert(load(shown), for: shown) }
        plans = (try? Data(contentsOf: directory.appending(path: "plans.json")))
            .flatMap { try? JSONDecoder().decode([SavedPlan].self, from: $0) } ?? []
        if let data = try? Data(contentsOf: directory.appending(path: "notified.json")),
            let notified = try? JSONDecoder().decode([String: AlertNotifier.Notified].self, from: data)
        {
            notifier = AlertNotifier(notified: notified)
        }
        // The other places' views, without their data yet, are off screen.
        alignViews()
        viewSettings = preferences.settings(withKey: false)
        let others = self.places.filter { $0.id != shown?.id }
        if !others.isEmpty {
            cachesRead = Task { await self.readCaches(of: others) }
        }
        #if DEBUG
            if args.contains("-stormahead") { Task { await seedStormAhead() } }
        #endif
    }

    /// Reads the caches of `places` off the main actor, keeps those still wanted (a
    /// place removed, or reset by a move, meanwhile is not given its old cache), and
    /// rebuilds every view.
    private func readCaches(of places: [AppPlace]) async {
        let files = places.map { (id: $0.id, url: $0.cacheURL(in: directory), legacy: $0.legacyCacheURL(in: directory)) }
        let read = await Task.detached(priority: .userInitiated) {
            files.map { ($0.id, Self.readCache(at: $0.url, legacy: $0.legacy)) }
        }.value
        for (id, cache) in read where caches[id] == nil {
            guard let place = self.places.first(where: { $0.id == id }) else { continue }
            caches[id] = withDebugAlert(cache, for: place)
        }
        cachesRead = nil
        rebuild()
    }

    /// `cache` with `-fakeAppleAlert`'s alert in DEBUG builds, in memory only.
    private func withDebugAlert(_ cache: PlaceCache, for place: AppPlace) -> PlaceCache {
        #if DEBUG
            let now = Int64(Date.now.timeIntervalSince1970)
            if let fake = Self.fakeAppleAlert(at: now) {
                var cache = cache
                applyAlerts([fake], asked: true, at: now, for: place, to: &cache)
                return cache
            }
        #endif
        return cache
    }

    /// Moves files from Application Support into the App Group the first time it
    /// is available, so places and forecasts survive the switch.
    private static func moveIntoGroup(_ directory: URL) {
        let old = URL.applicationSupportDirectory.appending(path: "whether", directoryHint: .isDirectory)
        guard SharedContainer.isShared, old.standardizedFileURL != directory.standardizedFileURL,
            let files = try? FileManager.default.contentsOfDirectory(at: old, includingPropertiesForKeys: nil)
        else { return }
        for file in files {
            let target = directory.appending(path: file.lastPathComponent)
            if !FileManager.default.fileExists(atPath: target.path()) {
                try? FileManager.default.moveItem(at: file, to: target)
            }
        }
    }

    // MARK: Places

    private func savePlaces() {
        if let data = try? JSONEncoder().encode(places) {
            try? data.write(to: directory.appending(path: "places.json"), options: .atomic)
        }
    }

    /// Asks the widgets to read the places and caches again. Once per change or
    /// refresh, never per place: WidgetKit throttles reloads from the background.
    private func reloadWidgets() {
        // They read the settings too: a change waiting is saved first.
        flushPreferences()
        widgetsWaiting = false
        WidgetCenter.shared.reloadAllTimelines()
    }

    /// Saves the settings half a second after the last change, as `update(_:)` saves plans.
    private func savePreferencesSoon() {
        pendingPreferences?.cancel()
        pendingPreferences = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(500))
            guard !Task.isCancelled else { return }
            self?.savePendingPreferences()
        }
    }

    /// Saves a settings change still waiting (also when the app goes to the
    /// background), and has the widgets read it when it changes what they show.
    func savePendingPreferences() {
        flushPreferences()
        if widgetsWaiting { reloadWidgets() }
    }

    private func flushPreferences() {
        guard let pending = pendingPreferences else { return }
        pending.cancel()
        pendingPreferences = nil
        preferences.save()
    }

    /// Adds `place` (or selects the saved one within 1 km of it) and fetches it.
    /// Same-named places farther apart are both kept.
    func add(_ place: AppPlace) async {
        if let existing = places.firstIndex(where: { AppPlace.isDuplicate(place, of: $0) }) {
            active = existing
            rebuild()
            return
        }
        places.append(place)
        savePlaces()
        active = places.count - 1
        rebuild()
        reloadWidgets()
        // Only the new place: the others are not made to wait for (or delay) it.
        await refresh(only: [place.id])
        // In context: a place whose alerts can arrive is the moment to ask.
        await requestNotificationsIfUseful()
    }

    /// Removes the places at `offsets`; removing the last returns to the welcome screen.
    func remove(at offsets: IndexSet) {
        for i in offsets {
            caches[places[i].id] = nil
            try? FileManager.default.removeItem(at: places[i].cacheURL(in: directory))
            try? FileManager.default.removeItem(at: places[i].legacyCacheURL(in: directory))
        }
        let removed = Set(offsets.map { places[$0].id })
        places.remove(atOffsets: offsets)
        if let favourite = preferences.favouritePlaceID, removed.contains(favourite) {
            preferences.favouritePlaceID = nil
        }
        active = max(0, min(active, places.count - 1))
        savePlaces()
        rebuild()
        reloadWidgets()
        if places.isEmpty {
            // No place left to be rained on: end a running Live Activity.
            Task { await RainActivityController.update(view: nil, placeID: nil, enabled: false) }
        }
    }

    /// Reorders the places (a drag in the list): the one shown stays shown, and a
    /// place dropped above a pinned one is pinned, or unpinned below an unpinned one
    /// (`AppPlace.moving`).
    func move(from source: IndexSet, to destination: Int) {
        arrange(AppPlace.moving(places, from: source, to: destination))
    }

    /// Moves the place `id` to the top of the list (below the following place),
    /// or to the bottom.
    func move(id: String, toTop: Bool) {
        guard places.first(where: { $0.id == id })?.follows == false else { return }
        arrange(AppPlace.moving(places, id: id, toTop: toTop))
    }

    /// Pins the place `id`, or unpins it. The following place is always first.
    func togglePin(id: String) {
        guard let i = places.firstIndex(where: { $0.id == id }), !places[i].follows else { return }
        var places = places
        places[i].pinned.toggle()
        arrange(places)
    }

    /// Makes the place `id` the favourite, or (when it is) none.
    func toggleFavourite(id: String) {
        preferences.favouritePlaceID = preferences.favouritePlaceID == id ? nil : id
        // The widget shows the favourite until its button picks another place: told
        // once the change is saved.
        widgetsWaiting = true
    }

    /// Shows the place `id`.
    func show(id: String) {
        if let i = places.firstIndex(where: { $0.id == id }) { active = i }
    }

    /// Keeps `new` (the same places in another order or state) as the list, with
    /// the place shown still shown.
    private func arrange(_ new: [AppPlace]) {
        let shown = places.indices.contains(active) ? places[active] : nil
        places = AppPlace.arranged(new)
        active = shown.flatMap { shown in places.firstIndex { $0.id == shown.id } } ?? 0
        savePlaces()
        rebuild()
        reloadWidgets()
    }

    /// Renames the place `id` (its cache stays); an empty name is refused.
    @discardableResult
    func rename(id: String, to name: String) -> Bool {
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty, let i = places.firstIndex(where: { $0.id == id }) else { return false }
        places[i].name = name
        savePlaces()
        rebuild()
        reloadWidgets()
        return true
    }

    // MARK: Following

    /// Makes `found` the place that follows the phone: the one that already does
    /// (it keeps its id), else a saved place within 1 km of it, else a new one.
    /// It is first in the list and shown.
    func follow(_ found: AppPlace) async {
        var place = found
        place.follows = true
        let i = places.firstIndex(where: \.follows)
            ?? places.firstIndex { AppPlace.isDuplicate(found, of: $0) }
        if let i {
            let old = places[i]
            place.id = old.id
            places[i] = place
            // The cache is for the old coordinates (and its validators too).
            if AppPlace.shouldMove(old: old.coordinate, new: place.coordinate) {
                caches[place.id] = PlaceCache()
                save(place)
            }
        } else {
            places.append(place)
            caches[place.id] = PlaceCache()
        }
        places = AppPlace.arranged(places)
        dropDuplicates(of: place.id)
        active = places.firstIndex { $0.id == place.id } ?? 0
        savePlaces()
        rebuild()
        reloadWidgets()
        await refresh(only: [place.id])
        await requestNotificationsIfUseful()
    }

    /// Removes the saved fixed places within 1 km of the following place `id`
    /// (and their caches): after a move there it would be listed twice.
    private func dropDuplicates(of id: String) {
        let (kept, removed) = AppPlace.removingDuplicates(of: id, in: places)
        for place in removed {
            caches[place.id] = nil
            try? FileManager.default.removeItem(at: place.cacheURL(in: directory))
            try? FileManager.default.removeItem(at: place.legacyCacheURL(in: directory))
        }
        places = kept
    }

    /// Moves the following place to where the phone is, when that is 1 km or more
    /// from its coordinates (updating its name and time zone, keeping its id, and
    /// fetching it again unless `refetch` is `false`). With access turned off the
    /// place keeps its last coordinates and `locationOff` says why. Gives up on
    /// the location after `timeout` seconds. Returns whether it moved.
    @discardableResult
    func followMe(refetch: Bool = true, timeout: Double = 12) async -> Bool {
        guard !followingNow, let followed = places.first(where: \.follows) else {
            if !places.contains(where: \.follows) { locationOff = false }
            return false
        }
        followingNow = true
        defer { followingNow = false }
        let location: CLLocation
        do {
            location = try await CurrentLocation.fix(timeout: timeout)
        } catch CurrentLocation.Failure.denied {
            locationOff = true
            return false
        } catch {
            return false
        }
        locationOff = false
        guard AppPlace.shouldMove(old: followed.coordinate, new: location.coordinate) else { return false }
        let named = await CurrentLocation.place(at: location)
        guard let i = places.firstIndex(where: { $0.id == followed.id }) else { return false }
        places[i].name = named.name
        places[i].detail = named.detail
        places[i].latitude = named.latitude
        places[i].longitude = named.longitude
        places[i].timeZone = named.timeZone
        caches[followed.id] = PlaceCache()
        save(places[i])
        let shown = places.indices.contains(active) ? places[active].id : nil
        dropDuplicates(of: followed.id)
        if let shown, let now = places.firstIndex(where: { $0.id == shown }) { active = now }
        active = max(0, min(active, places.count - 1))
        savePlaces()
        rebuild()
        reloadWidgets()
        if refetch { await refresh(only: [followed.id]) }
        return true
    }

    // MARK: Cache

    /// The One Call ledger (the phone's share of the day's calls).
    private var ledgerURL: URL { directory.appending(path: "ledger.json") }
    /// The metered fetches, one after another (`counted`).
    @ObservationIgnored private let meteredFetches = TaskChain()

    /// `place`'s cache, moving a legacy one (named after the place) to the id's name.
    private func load(_ place: AppPlace) -> PlaceCache {
        Self.readCache(at: place.cacheURL(in: directory), legacy: place.legacyCacheURL(in: directory))
    }

    /// The cache at `url`, moving one at `legacy` there first; on any thread.
    nonisolated private static func readCache(at url: URL, legacy: URL) -> PlaceCache {
        let files = FileManager.default
        if !files.fileExists(atPath: url.path(percentEncoded: false)),
            files.fileExists(atPath: legacy.path(percentEncoded: false))
        {
            try? files.moveItem(at: legacy, to: url)
        }
        guard let data = try? Data(contentsOf: url) else { return PlaceCache() }
        return (try? JSONDecoder().decode(PlaceCache.self, from: data)) ?? PlaceCache()
    }

    private func save(_ place: AppPlace) {
        guard let data = try? JSONEncoder().encode(caches[place.id]) else { return }
        try? data.write(to: place.cacheURL(in: directory), options: .atomic)
    }

    // MARK: Forecast

    /// The raw text behind `place`'s last fetch error, if it had one.
    func errorDetail(for place: AppPlace) -> String? {
        caches[place.id]?.errorDetail
    }

    /// Why OpenWeather did not serve `place` last time another provider did, and
    /// the raw attempts behind it.
    func keyProblem(for place: AppPlace) -> (title: String, detail: String?)? {
        guard let cache = caches[place.id], let title = cache.keyProblem else { return nil }
        return (title, cache.keyProblemDetail)
    }

    /// One Call calls this phone has made today, by the saved ledger.
    func oneCallCallsToday() -> UInt32 {
        callsToday(
            usageJson: try? String(contentsOf: ledgerURL, encoding: .utf8), provider: "openweather",
            now: Int64(Date.now.timeIntervalSince1970))
    }

    /// Asks OpenWeather whether `key` works, with a free (unmetered) call.
    func checkKey(_ key: String) async -> KeyCheck {
        await checkOpenweatherKey(key: key, host: URLSessionHost(ledgerURL: ledgerURL))
    }

    /// Asks Pirate Weather whether `key` works with one small forecast request, which
    /// counts as one of the key's calls for the month.
    func checkPirateKey(_ key: String) async -> KeyCheck {
        await checkPirateWeatherKey(key: key, host: URLSessionHost(ledgerURL: ledgerURL))
    }

    /// This phone's Pirate Weather calls this month against its limit, in the core's
    /// words, by the saved ledger.
    func pirateWeatherUsage() -> String {
        monthUsageText(
            usageJson: try? String(contentsOf: ledgerURL, encoding: .utf8), provider: Sources.pirateWeather,
            limit: UInt32(clamping: preferences.pirateWeatherCalls), now: Int64(Date.now.timeIntervalSince1970))
    }

    /// The places whose data is old enough to fetch again.
    private var duePlaces: Set<String> {
        Freshness.due(
            places.map { ($0.id, caches[$0.id]?.lastSuccess) }, now: Date.now.timeIntervalSince1970,
            after: preferences.fetchDueAfter)
    }

    /// A launch's or a return's refresh is under way (`refreshOnOpen`).
    @ObservationIgnored private var opening = false

    /// On launch and on coming back to the app: fetches the places whose data is
    /// due, and moves the following place to where the phone is. That place waits
    /// for the location (briefly), so it is fetched once, where the phone is,
    /// rather than where it was and then again. A refresh already running is
    /// fresh enough.
    func refreshOnOpen() async {
        guard !opening, !refreshing else { return }
        opening = true
        defer { opening = false }
        await cachesRead?.value
        var due = duePlaces
        #if DEBUG
            // Screenshots of a fetch (failing, or slow) fetch on every launch.
            let args = ProcessInfo.processInfo.arguments
            if args.contains("-offline") || args.contains("-slow") { due = Set(places.map(\.id)) }
        #endif
        let following = places.first(where: \.follows)?.id
        let others = due.subtracting([following].compactMap { $0 })
        async let fetched: Void = refresh(only: others)
        let moved = await followMe(refetch: false, timeout: 5)
        if let following, moved || due.contains(following) {
            // Joins the refresh of the others when it still runs.
            await refresh(only: [following])
        }
        await fetched
    }

    /// Rebuilds the views of every place (or of the places `only`) off the main
    /// actor; each shows when built. Asked for again meanwhile, the newer request
    /// wins. The task ends once the views are current.
    @discardableResult
    func rebuild(only ids: Set<String>? = nil) -> Task<Void, Never> {
        alignViews()
        wantedViews = (ids.map(ViewRequest.places) ?? .all).merged(with: wantedViews)
        if let building { return building }
        let task = Task { await buildWantedViews() }
        building = task
        return task
    }

    /// Every minute, so relative times ("Updated 3 min ago") move on: the place
    /// shown is rebuilt, and the others when they are shown.
    func tick() {
        guard let id = shownPlaceID else { return }
        behind.formUnion(viewIDs)
        rebuild(only: [id])
    }

    /// Builds what is asked for until nothing is.
    private func buildWantedViews() async {
        while let request = wantedViews {
            wantedViews = nil
            let ids = places.map(\.id)
            let indices = request.indices(in: ids)
            guard !indices.isEmpty else { continue }
            let entries = entries()
            // Views need no key (this runs often: no Keychain reads).
            let settings = preferences.settings(withKey: false)
            let built = await Task.detached(priority: .userInitiated) {
                Result { try ViewModel.buildAll(locations: entries, indices: indices, settings: settings) }
            }.value
            publish(built, for: indices.map { ids[$0] })
        }
        building = nil
    }

    /// Shows the views built for the places `ids`, but none asked for again
    /// meanwhile (the next build has newer data), and none that did not change.
    private func publish(_ built: Result<[ViewModel], any Error>, for ids: [String]) {
        switch built {
        case .success(let built):
            var views = views
            var changed = false
            for (id, view) in zip(ids, built) where wantedViews?.covers(id) != true {
                guard let i = viewIDs.firstIndex(of: id), views.indices.contains(i) else { continue }
                behind.remove(id)
                if views[i] != view {
                    views[i] = view
                    changed = true
                }
            }
            if changed { self.views = views }
            if failure != nil {
                failure = nil
                failureDetail = nil
            }
        case .failure(let error):
            show(error)
        }
    }

    /// Keeps `views` in step with `places` at once: each view goes with its place,
    /// and a place without one (just added) gets one now, from its own data only
    /// (cheap: no other snapshot is read). Its tabs then lack the other places'
    /// conditions, until the build that follows.
    private func alignViews() {
        let ids = places.map(\.id)
        guard ids != viewIDs else { return }
        var (aligned, missing) = ViewRequest.aligned(views, ids: viewIDs, to: ids)
        if !missing.isEmpty {
            let own = Set(missing.map { ids[$0] })
            do {
                let built = try ViewModel.buildAll(
                    locations: entries(dataOf: own), indices: missing,
                    settings: preferences.settings(withKey: false))
                for (i, view) in zip(missing, built) { aligned[i] = view }
            } catch {
                show(error)
                return
            }
            if own.count < ids.count {
                wantedViews = ViewRequest.places(own).merged(with: wantedViews)
            }
        }
        views = aligned.compactMap { $0 }
        viewIDs = ids
    }

    /// What the core builds the views from: every place, with the cached data of
    /// all of them (or of those in `dataOf` only).
    private func entries(dataOf ids: Set<String>? = nil) -> [LocationEntry] {
        places.map { place in
            let cache = ids?.contains(place.id) == false ? PlaceCache() : caches[place.id] ?? PlaceCache()
            return LocationEntry(
                place: place.corePlace(), snapshotJson: cache.snapshotJson,
                lastSuccess: cache.lastSuccess, error: cache.error, refreshing: refreshing,
                historyJson: cache.historyJson)
        }
    }

    private func show(_ error: any Error) {
        let message = userMessage(error)
        failure = message.title
        failureDetail = message.detail ?? String(describing: error)
    }

    /// Fetches every place (or those whose ids are in `only`) side by side, keeping
    /// the cache when nothing changed or a fetch fails; each place shows as
    /// soon as its forecast arrives. In the background, new or worse alerts
    /// become notifications; in the foreground they are only remembered (they
    /// are on screen).
    func refresh(inBackground: Bool = false, only: Set<String>? = nil) async {
        // A fetch starts from the place's cache (its validators, minutes and history).
        await cachesRead?.value
        if only?.isEmpty == true { return }
        guard !refreshing else {
            // Joins the running refresh, which fetches these before it ends.
            queue?.request(only)
            return
        }
        // The pill says so at once (`refreshing`); the views say it with the first
        // forecast that comes, as no screen reads it from them.
        refreshing = true
        // In the background some sources are left out (`Sources.foregroundOnly`).
        let settings = inBackground ? preferences.settings().forBackground() : preferences.settings()
        let host = URLSessionHost(ledgerURL: ledgerURL)
        queue = RefreshQueue(only: only)
        while let ids = queue?.next(among: places.map(\.id)), !ids.isEmpty {
            let due = places.filter { ids.contains($0.id) }
            await withTaskGroup(of: Void.self) { group in
                for place in due {
                    group.addTask { await self.fetchOne(place, settings: settings, host: host) }
                }
            }
            // Places added or moved meanwhile are fetched now.
        }
        queue = nil
        refreshing = false
        // The alerts to notify about are in the views.
        await rebuild().value
        reloadWidgets()
        await notify(post: inBackground)
        await RainActivityController.update(
            view: view, placeID: shownPlaceID, enabled: preferences.rainActivity)
    }

    /// Fetches `place`, keeping its cache when nothing changed or the fetch fails.
    private func fetchOne(_ place: AppPlace, settings: Settings, host: URLSessionHost) async {
        var cache = caches[place.id] ?? PlaceCache()
        let now = Date.now
        do {
            let outcome = try await counted(
                place.corePlace(now: now), settings: settings, snapshotJson: cache.snapshotJson,
                lastModified: cache.lastModified, now: Int64(now.timeIntervalSince1970), host: host)
            cache.snapshotJson = outcome.snapshotJson
            cache.lastModified = outcome.lastModified
            cache.lastSuccess = Int64(now.timeIntervalSince1970)
            cache.error = nil
            cache.errorDetail = nil
            let problem = openWeatherProblem(attempts: outcome.attempts, served: outcome.provider)
            cache.keyProblem = problem?.title
            cache.keyProblemDetail = problem?.detail
        } catch WhetherError.NotModified {
            cache.lastSuccess = Int64(now.timeIntervalSince1970)
            cache.error = nil
            cache.errorDetail = nil
        } catch {
            let message = userMessage(error)
            cache.error = message.title
            cache.errorDetail = message.detail
        }
        // The forecast shows at once, with the Apple alerts last held; Apple's
        // minutes and alerts and the history (the slow part: several requests)
        // follow side by side.
        let nowSeconds = Int64(now.timeIntervalSince1970)
        applyAlerts(nil, asked: false, at: nowSeconds, for: place, to: &cache)
        // Written once, below, with the rest.
        commit(cache, for: place, persist: false)
        #if DEBUG
            print("fetch \(place.name): forecast shown after \(Date.now.timeIntervalSince(now))s")
        #endif
        let fetched = cache
        // Apple is asked only when the forecast hints at rain soon and for the
        // place shown (see `MinutesPolicy`); history waits for neither.
        let ask = shouldAskApple(place, cache: fetched, now: nowSeconds)
        async let minutes: [MinuteStep]? = ask ? appleMinutes(for: place) : nil
        // Apple's alerts: outside the National Weather Service's coverage, every
        // 30 minutes (`AppleAlertsPolicy`).
        let askAlerts = shouldAskAppleAlerts(place, cache: fetched, now: nowSeconds)
        async let alerts: [ExternalAlert]? = askAlerts ? appleAlerts(for: place, now: nowSeconds) : nil
        async let history = historyUpdate(fetched, at: place, settings: settings, host: host, now: now)
        // A storm ahead, for the place shown only: one Open-Meteo call at most every
        // 3 hours (none while the outlook kept is younger), and only while its card shows.
        let checkStorm = StormAheadRules.checks(
            place.id, shown: shownPlaceID, visible: preferences.sections.filter(\.shown).map(\.section))
        async let storm: StormAheadCheck =
            checkStorm ? stormAheadCheck(place, settings: settings, host: host, now: now) : .unanswered
        let (found, foundAlerts, kept, stormCheck) = await (minutes, alerts, history, storm)
        applyMinutes(found, asked: ask, at: nowSeconds, to: &cache)
        applyAlerts(foundAlerts, asked: askAlerts, at: nowSeconds, for: place, to: &cache)
        cache.stormAheadJson = StormAheadRules.kept(cache.stormAheadJson, after: stormCheck)
        if let kept {
            cache.historyJson = kept.historyJson
            cache.normalsJson = kept.normalsJson
            cache.historyFailedAt = kept.failedAt
        }
        // Shown again only when the minutes, alerts or history changed something.
        commit(cache, for: place, show: cache != fetched)
        #if DEBUG
            print(
                "fetch \(place.name): minutes and history after \(Date.now.timeIntervalSince(now))s"
                    + " (WeatherKit calls this launch: \(Self.weatherKitCalls))")
        #endif
    }

    /// `fetch` with the call ledger as last saved. Metered fetches (One Call,
    /// Pirate Weather) run one at a time: side by side, each would count from the
    /// same ledger and the last to save it would win, undercounting the day's calls.
    private func counted(
        _ place: Place, settings: Settings, snapshotJson: String?, lastModified: String?, now: Int64,
        host: URLSessionHost
    ) async throws -> FetchOutcome {
        let attempt = { @MainActor @Sendable [ledgerURL] () async -> Result<FetchOutcome, any Error> in
            do {
                return .success(
                    try await fetch(
                        place: place, settings: settings,
                        cached: Cached(
                            snapshotJson: snapshotJson, lastModified: lastModified,
                            usageJson: try? String(contentsOf: ledgerURL, encoding: .utf8)),
                        now: now, host: host))
            } catch {
                return .failure(error)
            }
        }
        let result =
            fetchMetered(place: place, settings: settings) ? await meteredFetches.run(attempt) : await attempt()
        return try result.get()
    }

    /// Keeps `cache` for `place` (unless the place was removed or moved meanwhile:
    /// a forecast for the old coordinates would replace the reset cache), writes
    /// it unless `persist` is `false`, and shows it unless `show` is.
    private func commit(_ cache: PlaceCache, for place: AppPlace, persist: Bool = true, show: Bool = true) {
        guard AppPlace.isCurrent(place, in: places) else { return }
        caches[place.id] = cache
        if persist { save(place) }
        // The other places' views change only in their tabs, which the refresh's
        // last rebuild brings up to date.
        if show { rebuild(only: [place.id]) }
    }

    /// A place's history after a fetch: what to keep.
    private struct HistoryUpdate: Sendable {
        var historyJson: String?
        var normalsJson: String?
        var failedAt: Int64?
    }

    /// Fetches `place`'s history when it is due (about once a day there): `nil`
    /// when not due; a failure keeps the old one and waits an hour.
    private func historyUpdate(
        _ cache: PlaceCache, at place: AppPlace, settings: Settings, host: URLSessionHost,
        now: Date
    ) async -> HistoryUpdate? {
        let t = Int64(now.timeIntervalSince1970)
        if let failed = cache.historyFailedAt, t - failed < 3600 { return nil }
        let corePlace = place.corePlace(now: now)
        guard historyDue(place: corePlace, settings: settings, keptJson: cache.historyJson, now: t)
        else { return nil }
        do {
            let outcome = try await fetchHistory(
                place: corePlace, settings: settings, normalsJson: cache.normalsJson, now: t, host: host)
            return HistoryUpdate(
                historyJson: outcome.historyJson, normalsJson: outcome.normalsJson ?? cache.normalsJson,
                failedAt: nil)
        } catch {
            return HistoryUpdate(historyJson: cache.historyJson, normalsJson: cache.normalsJson, failedAt: t)
        }
    }

    // MARK: Storm ahead

    /// Whether a significant storm is coming to `place`: the core builds the card from
    /// the newer of the Storms tab's outlook and the check's own while it is younger than
    /// 3 hours, else fetches the models' hours there once. What it fetched is kept in the
    /// check's own file, never over the tab's full outlook. A failure answers nothing, so
    /// the last card stays.
    private func stormAheadCheck(
        _ place: AppPlace, settings: Settings, host: URLSessionHost, now: Date
    ) async -> StormAheadCheck {
        #if DEBUG
            // `-stormahead` shows the built-in card instead (`seedStormAhead`).
            if ProcessInfo.processInfo.arguments.contains("-stormahead") { return .unanswered }
        #endif
        let full = try? String(contentsOf: StormsRules.outlookURL(place.id), encoding: .utf8)
        let url = StormsRules.aheadURL(place.id)
        let own = try? String(contentsOf: url, encoding: .utf8)
        do {
            let outcome = try await stormAhead(
                place: place.corePlace(now: now), settings: settings,
                cachedJson: StormAheadRules.cachedOutlook(full: full, ahead: own),
                now: Int64(now.timeIntervalSince1970), host: host)
            if outcome.refreshed {
                try? FileManager.default.createDirectory(
                    at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
                try? Data(outcome.outlookJson.utf8).write(to: url, options: .atomic)
            }
            return .answered(outcome.viewJson)
        } catch {
            return .unanswered
        }
    }

    /// The "Storm ahead" card for `place`, until its storm has ended.
    func stormAheadCard(for place: AppPlace) -> StormAhead? {
        #if DEBUG
            if let sampleStormAhead, place.id == shownPlaceID {
                return StormAheadRules.shown(json: sampleStormAhead, now: sampleStormAheadNow)
            }
        #endif
        return StormAheadRules.shown(
            json: caches[place.id]?.stormAheadJson, now: Int64(Date.now.timeIntervalSince1970))
    }

    #if DEBUG
        /// Screenshots: `-stormahead` builds the card from the built-in nor'easter through
        /// the core, at the time the sample was made for (its outlook is fresh then, so no
        /// request goes out); `-stormahead hurricane` shows Josephine's card. In memory only.
        func seedStormAhead() async {
            let args = ProcessInfo.processInfo.arguments
            if let i = args.firstIndex(of: "-stormahead"), i + 1 < args.count, args[i + 1] == "hurricane" {
                // Josephine's card, as the core gave it for a place on her track (the core's
                // checks need the models' hours there, which the sample has none of).
                let read = { (name: String) in
                    Bundle.main.url(forResource: name, withExtension: "json")
                        .flatMap { try? String(contentsOf: $0, encoding: .utf8) }
                }
                guard let card = read("storms-ahead-josephine"),
                    let runs = read("storms-hurricane-josephine").flatMap({ try? StormOutlookRuns.decode($0) })
                else { return }
                sampleStormAheadNow = runs.fetchedAt
                sampleStormAhead = card
                return
            }
            guard places.indices.contains(active),
                let url = Bundle.main.url(forResource: "storms-noreaster", withExtension: "json"),
                let outlook = try? String(contentsOf: url, encoding: .utf8),
                let runs = try? StormOutlookRuns.decode(outlook)
            else { return }
            let place = places[active]
            let now = Date(timeIntervalSince1970: TimeInterval(runs.fetchedAt))
            let outcome = try? await stormAhead(
                place: place.corePlace(now: now), settings: preferences.settings(withKey: false),
                cachedJson: outlook, now: runs.fetchedAt, host: URLSessionHost(ledgerURL: ledgerURL))
            sampleStormAheadNow = runs.fetchedAt
            sampleStormAhead = outcome?.viewJson
        }
    #endif

    // MARK: Trips

    private func savePlans() {
        #if DEBUG
            // Screenshots with made-up shared plans: none of it is kept.
            if Self.fakeSharing { return }
        #endif
        if let data = try? JSONEncoder().encode(plans) {
            try? data.write(to: directory.appending(path: "plans.json"), options: .atomic)
        }
    }

    // MARK: Alerts read

    /// An alert's identity across fetches: the provider's id, else its title and end.
    static func alertKey(_ alert: ViewModel.Alert) -> String {
        alert.id ?? "\(alert.title)|\(alert.end)"
    }

    /// The shown place's alerts not read yet.
    var unreadAlerts: [ViewModel.Alert] {
        (view?.alerts ?? []).filter { !readAlerts.contains(Self.alertKey($0)) }
    }

    /// Marks `alerts` read, forgetting alerts no place has any more.
    func markRead(_ alerts: [ViewModel.Alert]) {
        let live = Set(views.flatMap(\.alerts).map(Self.alertKey))
        readAlerts = readAlerts.union(alerts.map(Self.alertKey)).intersection(live)
        SharedContainer.defaults.set(Array(readAlerts).sorted(), forKey: "readAlerts")
    }

    /// The forecast at `place` from `provider` alone (first in the order, with
    /// Open-Meteo only as the fallback when it has no data there, and no gap
    /// filling, so each source shows what it brings), for comparing sources.
    /// OpenWeather uses One Call when it is on, counted against its daily calls
    /// like any refresh; kept 30 minutes.
    func compareForecast(at place: AppPlace, provider: String) async throws -> ViewModel {
        let key = String(format: "%@-%.3f_%.3f", provider, place.latitude, place.longitude)
        if let kept = comparisons[key], Date.now.timeIntervalSince(kept.at) < 1800 { return kept.view }
        var settings = preferences.settings()
        settings.fillGaps = false
        settings.history = false
        settings.dailyDays = 8
        settings.sources =
            [SourceChoice(provider: provider, enabled: true)]
            + Sources.catalog.map(\.id).filter { $0 != provider }.map {
                SourceChoice(provider: $0, enabled: $0 == Sources.openMeteo)
            }
        let now = Date.now
        let t = Int64(now.timeIntervalSince1970)
        let outcome = try await counted(
            place.corePlace(now: now), settings: settings, snapshotJson: nil, lastModified: nil,
            now: t, host: URLSessionHost(ledgerURL: ledgerURL))
        let entry = LocationEntry(
            place: place.corePlace(now: now), snapshotJson: outcome.snapshotJson, lastSuccess: t,
            error: nil, refreshing: false)
        let view = try ViewModel.build(locations: [entry], active: 0, settings: settings, now: now)
        comparisons[key] = (now, view)
        return view
    }

    /// The forecast at `place` from the forecast models `models` (a preset; empty is
    /// Open-Meteo's best match), as they serve when chosen: best match fills what they
    /// lack. For comparing presets; One Call stays off; kept 30 minutes.
    func compareModels(at place: AppPlace, models: [String]) async throws -> ViewModel {
        let key = String(
            format: "models:%@-%.3f_%.3f", models.joined(separator: ","), place.latitude, place.longitude)
        if let kept = comparisons[key], Date.now.timeIntervalSince(kept.at) < 1800 { return kept.view }
        // Keyless: only Open-Meteo answers.
        var settings = preferences.settings(withKey: false)
        settings.oneCall = false
        settings.history = false
        settings.dailyDays = 8
        settings.models = models
        settings.sources = Sources.catalog.map(\.id).map {
            SourceChoice(
                provider: $0,
                enabled: $0 == Sources.openMeteo || ($0 == Sources.models && !models.isEmpty))
        }
        let now = Date.now
        let t = Int64(now.timeIntervalSince1970)
        let outcome = try await fetch(
            place: place.corePlace(now: now), settings: settings,
            cached: Cached(snapshotJson: nil, lastModified: nil, usageJson: nil),
            now: t, host: URLSessionHost(ledgerURL: ledgerURL))
        let entry = LocationEntry(
            place: place.corePlace(now: now), snapshotJson: outcome.snapshotJson, lastSuccess: t,
            error: nil, refreshing: false)
        let view = try ViewModel.build(locations: [entry], active: 0, settings: settings, now: now)
        comparisons[key] = (now, view)
        return view
    }

    /// The 16-day forecast at a plan's place, kept an hour per place; `nil` when
    /// it cannot be had. Never metered: One Call is left out, as in the widget.
    /// `daysBack` (up to 31) adds the days already past as observed, in
    /// `history.days`, for a plan under way.
    func planForecast(at place: AppPlace, daysBack: Int = 0) async -> ViewModel? {
        let back = min(max(daysBack, 0), 31)
        let key = String(format: "%.3f_%.3f-%d", place.latitude, place.longitude, back)
        if let kept = planForecasts[key], Date.now.timeIntervalSince(kept.at) < 3600 { return kept.view }
        // Never metered and keyless: the keys are not even read.
        var settings = preferences.settings(withKey: false)
        settings.oneCall = false
        settings.dailyDays = 16
        if back > 0 {
            settings.history = true
            settings.historyDays = UInt8(back)
        }
        let now = Date.now
        let t = Int64(now.timeIntervalSince1970)
        let corePlace = place.corePlace(now: now)
        let ledger = FileManager.default.temporaryDirectory.appending(path: "plan-ledger.json")
        let host = URLSessionHost(ledgerURL: ledger)
        // The forecast and the observed days side by side.
        async let forecast = try? fetch(
            place: corePlace, settings: settings,
            cached: Cached(snapshotJson: nil, lastModified: nil, usageJson: nil), now: t, host: host)
        async let observed =
            back > 0
            ? (try? await fetchHistory(place: corePlace, settings: settings, normalsJson: nil, now: t, host: host))?
                .historyJson : nil
        guard let outcome = await forecast else { return nil }
        let entry = LocationEntry(
            place: corePlace, snapshotJson: outcome.snapshotJson, lastSuccess: t, error: nil,
            refreshing: false, historyJson: await observed)
        guard let view = try? ViewModel.build(locations: [entry], active: 0, settings: settings, now: now)
        else { return nil }
        planForecasts[key] = (now, view)
        return view
    }

    /// A new plan at `place` (else the place shown); its id, to open it. `nil`
    /// with no place at all: the caller asks for one first.
    func newPlan(at place: AppPlace? = nil) -> UUID? {
        guard let place = place ?? (places.indices.contains(active) ? places[active] : places.first)
        else { return nil }
        let plan = SavedPlan.new(at: place, number: plans.count + 1)
        plans.append(plan)
        savePlans()
        return plan.id
    }

    /// Keeps a changed plan: saved half a second after the last change, so
    /// typing a name or sliding the years writes once.
    func update(_ plan: SavedPlan) {
        guard let i = plans.firstIndex(where: { $0.id == plan.id }) else { return }
        // The editor never changes how a plan is shared, and a participant who may only
        // look changes nothing but `looked`.
        let plan = plans[i].editing(plan)
        guard plans[i] != plan else { return }
        let shared = plans[i].sharing != nil && plan.sharedFieldsDiffer(from: plans[i])
        plans[i] = plan
        if shared { queueCloudSave(plan.id) }
        pendingSave?.cancel()
        pendingSave = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(500))
            guard !Task.isCancelled else { return }
            self?.savePlans()
        }
    }

    /// A file that was opened (from Messages, Mail, AirDrop or Files): a theme by its
    /// `.whethertheme` extension, anything else as a plan.
    func receiveFile(from url: URL) {
        if SharedTheme.isThemeFile(url) {
            receiveTheme(from: url)
        } else {
            receivePlan(from: url)
        }
    }

    /// Reads a `.whethertheme` that was opened, as `receivePlan` reads a plan: the sheet
    /// asks Add or Decline, or an alert says why it can't be read; the inbox copy is deleted.
    func receiveTheme(from url: URL) {
        guard url.isFileURL else { return }
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        defer { removeIfInInbox(url) }
        let data: Data
        do {
            data = try Data(contentsOf: url)
        } catch {
            incomingProblem = userMessage(SharedThemeError.unopenable).title
            return
        }
        do {
            incomingTheme = IncomingTheme(theme: try SharedTheme(data: data))
        } catch {
            incomingProblem = userMessage(error).title
        }
    }

    /// Adds colors someone sent as a custom theme and paints with them; when the same
    /// colors are already here, uses those instead.
    func receive(_ shared: SharedTheme) {
        let month = Calendar.current.component(.month, from: .now)
        var prefs = preferences
        if let same = shared.match(among: prefs.customThemes, month: month) {
            prefs.colorTheme = same.id
        } else {
            let theme = shared.theme(among: prefs.customThemes, month: month)
            prefs.customThemes.append(theme)
            prefs.colorTheme = theme.id
        }
        incomingTheme = nil
        preferences = prefs
    }

    /// Reads a `.whetherplan` that was opened (from Messages, Mail, AirDrop or Files):
    /// the sheet asks Add or Decline, or an alert says why it can't be read. A file the
    /// system copied into this app's inbox is deleted once read, so it doesn't pile up
    /// in Documents (which the Files app shows).
    func receivePlan(from url: URL) {
        guard url.isFileURL else { return }
        // A file opened in place, from another app's folder, needs this; the system's
        // own copy in the inbox doesn't (and says `false`).
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        defer { removeIfInInbox(url) }
        let data: Data
        do {
            data = try Data(contentsOf: url)
        } catch {
            incomingProblem = userMessage(SharedPlanError.unopenable).title
            return
        }
        do {
            incoming = try SharedPlan(data: data)
        } catch {
            incomingProblem = userMessage(error).title
        }
    }

    /// Deletes `url` only if it lies in this app's own `Documents/Inbox`; a file opened in
    /// place from somewhere else is not ours to delete.
    private func removeIfInInbox(_ url: URL) {
        let inbox = URL.documentsDirectory.appending(path: "Inbox", directoryHint: .isDirectory)
        guard SharedPlan.isInside(url, inbox: inbox) else { return }
        try? FileManager.default.removeItem(at: url)
    }

    /// Adds a plan someone sent, as `choice` says (`SharedPlan.accepted`), and has the
    /// Plan tab open it.
    func receive(_ shared: SharedPlan, _ choice: ReceiveChoice = .add) {
        // Replacing a plan that is shared is an edit like any other: it keeps `sharing` and
        // goes to iCloud.
        if let replaced = shared.replacementForSharedPlan(choice, among: plans) {
            update(replaced)
            incoming = nil
            planToOpen = shared.id
            return
        }
        let result = shared.accepted(choice, into: plans)
        if result.plans != plans {
            plans = result.plans
            savePlans()
        }
        incoming = nil
        planToOpen = result.open
    }

    // MARK: Shared plans

    /// Whether any plan is shared: the only reason to touch iCloud without being asked.
    var hasSharedPlans: Bool { plans.contains { $0.sharing != nil } }

    /// Whether the screens offer live sharing: only where iCloud can work
    /// (`PlanCloud.isAvailable`). Copies as `.whetherplan` files need nothing. Screenshots
    /// (`-sharedPlan`) show it with made-up people and never reach CloudKit.
    var offersLiveSharing: Bool {
        #if DEBUG
            if Self.fakeSharing { return true }
        #endif
        return PlanCloud.isAvailable
    }

    #if DEBUG
        /// `-sharedPlan owner|reader|editor|list` in the launch arguments.
        static var fakeSharing: Bool { ProcessInfo.processInfo.arguments.contains("-sharedPlan") }

        /// The people the owner's sharing section lists in screenshots.
        static let fakeParticipants = [
            PlanParticipant(id: "a", name: "Robin", canEdit: true, pending: false),
            PlanParticipant(id: "b", name: "Alex", canEdit: false, pending: false),
            PlanParticipant(id: "c", name: nil, canEdit: false, pending: true),
        ]

        /// `-sharedPlan owner|reader|editor|list`: marks "Summer at the shore" (made if
        /// needed) as shared, in memory only (`savePlans` saves nothing in these launches), with
        /// made-up people: `owner` shared it, `reader` was shared it and may only look,
        /// `editor` was shared it and may edit. `list` also shows a plan shared with this person
        /// (read-only) and a private one.
        func seedSharedPlans(_ kind: String) {
            guard let id = plans.first(where: { $0.name == "Summer at the shore" })?.id ?? newPlan(),
                let i = plans.firstIndex(where: { $0.id == id })
            else { return }
            plans[i].name = "Summer at the shore"
            func shared(_ id: UUID, canEdit: Bool) -> PlanSharing {
                PlanSharing(
                    role: .participant, ownerName: "Robin", canEdit: canEdit,
                    record: PlanRecordRef(recordName: id.uuidString, zoneOwner: "_robin"))
            }
            switch kind {
            case "reader": plans[i].sharing = shared(id, canEdit: false)
            case "editor": plans[i].sharing = shared(id, canEdit: true)
            default: plans[i].sharing = .owned(id)
            }
            // `-unsentPlan`: the plan's last edit didn't reach iCloud (offline), in memory.
            if ProcessInfo.processInfo.arguments.contains("-unsentPlan") {
                unsentPlans.insert(id)
                cloudProblem = "No connection"
            }
            if kind == "list" {
                let stratton = AppPlace(
                    name: "Stratton", detail: "Vermont, US", latitude: 43.1134, longitude: -72.9081,
                    timeZone: "America/New_York")
                var ski = SavedPlan.new(at: stratton, number: 2)
                ski.name = "Ski week"
                ski.sharing = shared(ski.id, canEdit: false)
                var weekend = SavedPlan.new(at: places.first ?? stratton, number: 3)
                weekend.name = "Weekend away"
                plans += [ski, weekend]
            }
        }
    #endif

    /// The people plan `id` is shared with (the owner's sharing section). Failures are in
    /// `cloudProblem`, and thrown for the caller to show.
    func participants(of id: UUID) async throws -> [PlanParticipant] {
        guard let plan = plans.first(where: { $0.id == id }) else { throw PlanCloudError.notShared }
        #if DEBUG
            if Self.fakeSharing { return Self.fakeParticipants }
        #endif
        do {
            return try await planCloud().participants(of: plan)
        } catch {
            cloudProblem = userMessage(error).title
            throw error
        }
    }

    /// Lets `participant` edit plan `id`, or only look at it (the owner's "Can edit" switch).
    func setCanEdit(_ canEdit: Bool, for participant: String, in id: UUID) async throws {
        guard let plan = plans.first(where: { $0.id == id }) else { throw PlanCloudError.notShared }
        #if DEBUG
            if Self.fakeSharing { return }
        #endif
        do {
            try await planCloud().setCanEdit(canEdit, for: participant, in: plan)
            cloudProblem = nil
        } catch {
            cloudProblem = userMessage(error).title
            throw error
        }
    }

    /// Saves a private copy of a shared plan (`SavedPlan.privateCopy`: a new id, not shared)
    /// and has the Plan tab open it. The shared plan stays as it is.
    @discardableResult
    func makeOwnCopy(of id: UUID) -> UUID? {
        guard let plan = plans.first(where: { $0.id == id }) else { return nil }
        let copy = plan.privateCopy()
        plans.append(copy)
        savePlans()
        planToOpen = copy.id
        return copy.id
    }

    // MARK: Invitations

    /// An invitation the system handed over (`windowScene(_:userDidAcceptCloudKitShareWith:)`):
    /// the Add / Decline sheet shows at once, with the plan's details once they are fetched.
    /// Nothing joins before Add (`acceptShare`).
    func receiveShare(_ metadata: CKShare.Metadata) {
        guard let cloud = try? planCloud() else { return }
        let invitation = IncomingShare(metadata: metadata, ownerName: PlanCloud.ownerName(of: metadata))
        incomingShare = invitation
        Task {
            let plan = await cloud.preview(metadata)
            // Declined (or replaced by another invitation) meanwhile.
            guard incomingShare?.id == invitation.id else { return }
            incomingShare?.plan = plan
        }
    }

    /// Add: accepts the invitation in iCloud, fetches the plan, and has the Plan tab open it.
    /// A plan this person owns already (they opened their own link) is only opened. Throws
    /// what went wrong, and leaves the invitation up to try again.
    func acceptShare(_ invitation: IncomingShare) async throws {
        if let id = invitation.planID, plans.first(where: { $0.id == id })?.sharing?.isOwner == true {
            incomingShare = nil
            planToOpen = id
            return
        }
        if let metadata = invitation.metadata {
            do {
                try await planCloud().accept(metadata)
                cloudProblem = nil
            } catch {
                cloudProblem = userMessage(error).title
                throw error
            }
            // Until a sync works the plan isn't here: asked again on the next activation.
            setSyncPending(true)
            await subscribeToChanges()
            await syncSharedPlans(force: true)
        }
        incomingShare = nil
        if let id = invitation.planID, plans.contains(where: { $0.id == id }) { planToOpen = id }
    }

    /// Decline: nothing was accepted, so nothing joins; the invitation just goes away.
    func declineShare() { incomingShare = nil }

    /// iCloud for shared plans, created now. Call it only where sharing was asked for or a
    /// plan is already shared (`hasSharedPlans`): see `cloud`.
    ///
    /// Throws `.notInThisBuild` where there is no iCloud (`PlanCloud.isAvailable`): the one
    /// gate every way into iCloud passes, since creating the container there crashes.
    func planCloud() throws -> PlanCloud {
        if let cloud { return cloud }
        guard PlanCloud.isAvailable else { throw PlanCloudError.notInThisBuild }
        let made = PlanCloud()
        cloud = made
        return made
    }

    /// Sets how a plan is shared (after `PlanCloud.share` or an accepted invitation), or
    /// `nil` to make it private again.
    func setSharing(_ sharing: PlanSharing?, for id: UUID) {
        guard let i = plans.firstIndex(where: { $0.id == id }), plans[i].sharing != sharing else {
            return
        }
        plans[i].sharing = sharing
        savePlans()
    }

    /// Starts sharing a plan: puts it in iCloud and returns its share, for the invitation
    /// controller. The plan is marked as shared by this person.
    func startSharing(_ id: UUID) async throws -> CKShare {
        guard let plan = plans.first(where: { $0.id == id }) else { throw PlanCloudError.notShared }
        do {
            let share = try await planCloud().share(plan)
            setSharing(.owned(id), for: id)
            cloudProblem = nil
            Task { await subscribeToChanges() }
            return share
        } catch {
            cloudProblem = userMessage(error).title
            throw error
        }
    }

    /// Ends sharing a plan, as its owner (stops it for everyone) or as a participant (leaves
    /// it): the plan stays here as a private plan.
    func stopSharing(_ id: UUID) async throws {
        guard let plan = plans.first(where: { $0.id == id }), let sharing = plan.sharing else { return }
        do {
            if sharing.isOwner {
                try await planCloud().stopSharing(sharing)
            } else {
                try await planCloud().leave(sharing)
            }
            cloudProblem = nil
        } catch {
            cloudProblem = userMessage(error).title
            throw error
        }
        pendingCloud.removeValue(forKey: id)?.cancel()
        markSent(id)
        setSharing(nil, for: id)
    }

    /// A deleted plan that was shared: stops or leaves in iCloud, quietly. A tombstone is kept
    /// first (`endedSharing`): if the stop doesn't reach iCloud (offline), it is retried with
    /// every sync, and what iCloud still has of the plan is not applied (it would come back).
    private func endSharing(_ plan: SavedPlan) {
        pendingCloud.removeValue(forKey: plan.id)?.cancel()
        markSent(plan.id)
        guard let sharing = plan.sharing else { return }
        endedSharing.removeAll { $0.planID == plan.id }
        endedSharing.append(sharing)
        persistEnded()
        guard PlanCloud.isAvailable else { return }
        Task { await inChain { await self.retryEndedSharing() } }
    }

    private func persistEnded() {
        SharedContainer.defaults.set(try? JSONEncoder().encode(endedSharing), forKey: "endedSharedPlans")
    }

    /// Tries again each stop or leave that didn't reach iCloud; one that worked (or that
    /// iCloud says is already done) loses its tombstone. Stops at the first failure that
    /// isn't about that plan (no account, offline): the next sync tries again.
    private func retryEndedSharing() async {
        for sharing in endedSharing {
            do {
                let cloud = try planCloud()
                if sharing.isOwner { try await cloud.stopSharing(sharing) } else { try await cloud.leave(sharing) }
            } catch let error as PlanCloudError where error == .notShared {
                // Nothing there to stop: done.
            } catch {
                return
            }
            endedSharing.removeAll { $0 == sharing }
            persistEnded()
        }
    }

    /// Merges what changed in iCloud into `plans` (`PlanSync.apply`): a plan's name, place,
    /// dates and years become the sender's (its kept answer stays only if the trip is the
    /// same), its sharing is kept (permission and owner's name refreshed), a plan no longer
    /// shared stays as a private plan, a plan with an edit still to be sent is left to that
    /// save's merge, and a plan deleted here stays deleted. Returns what changed, for the
    /// banner.
    @discardableResult
    func applyCloudChanges(_ changes: [CloudChange]) -> AppliedChanges {
        let holding = unsentPlans.union(pendingCloud.keys)
        let ended = Set(endedSharing.compactMap(\.planID))
        let result = PlanSync.apply(changes, to: plans, holding: holding, ended: ended)
        if result.plans != plans {
            plans = result.plans
            savePlans()
        }
        return result.applied
    }

    /// Runs `work` after every sync, send and stop before it, and before any after it: the
    /// sends and the fetches of shared plans never overlap, so a fetch can't read a record
    /// while an edit is merging into it (a launch, becoming active, a push and an edit can all
    /// ask at once, and two fetches from one change token would find each change twice).
    private func inChain<T: Sendable>(_ work: @escaping @MainActor () async -> T) async -> T {
        let previous = syncChain
        let run = Task { () -> T in
            await previous?.value
            return await work()
        }
        syncChain = Task { _ = await run.value }
        return await run.value
    }

    /// Fetches what changed in iCloud and applies it, first sending edits that didn't go (so
    /// this is also what resends offline edits: on launch, on becoming active, on a push). Says
    /// what changed (`announce`). Does nothing (and doesn't create `PlanCloud`) while no plan
    /// is shared, unless `force` (right after accepting an invitation, when the plan isn't
    /// here yet; or when iCloud pushed). Failures are in `cloudProblem`, except for a `quiet`
    /// check (looking in iCloud where this person may never have shared anything).
    @discardableResult
    func syncSharedPlans(force: Bool = false, quiet: Bool = false) async -> AppliedChanges {
        guard PlanCloud.isAvailable, force || hasSharedPlans || !endedSharing.isEmpty else {
            return AppliedChanges()
        }
        return await inChain { await self.syncNow(quiet: quiet) }
    }

    private func syncNow(quiet: Bool) async -> AppliedChanges {
        lastSyncError = nil
        syncProblem = nil
        await retryEndedSharing()
        for id in unsentPlans { await sendToCloud(id) }
        do {
            let cloud = try planCloud()
            let fetched = try await cloud.fetchChanges()
            let applied = applyCloudChanges(fetched.changes)
            // Kept only now: a plan whose update was left out keeps its old base.
            cloud.commit(skipping: applied.skipped)
            await announce(applied)
            if let failure = fetched.failure { throw failure }
            cloudProblem = syncProblem
            setSyncPending(false)
            if hasSharedPlans { await subscribeToChanges() }
            return applied
        } catch {
            lastSyncError = error
            if !quiet { cloudProblem = userMessage(error).title }
            return AppliedChanges()
        }
    }

    /// An invitation was accepted and the plan hasn't been fetched yet: the next activation
    /// looks in iCloud again, whether or not a plan is shared here.
    private var syncPending: Bool { SharedContainer.defaults.bool(forKey: "planSyncPending") }

    private func setSyncPending(_ pending: Bool) {
        guard syncPending != pending else { return }
        SharedContainer.defaults.set(pending, forKey: "planSyncPending")
    }

    /// The sync the app runs on launch and each time it becomes active: whenever a
    /// plan is shared (or an invitation was accepted without a sync working yet, or a deleted
    /// plan's stop is waiting); and, once per launch where iCloud answers, a quiet look for
    /// plans this person shared or accepted on another device (their second phone). Never
    /// where there is no iCloud (`PlanCloud.isAvailable`: simulator, tests, screenshots).
    func followSharedPlans() async {
        guard PlanCloud.isAvailable else { return }
        if hasSharedPlans || syncPending || !endedSharing.isEmpty {
            await syncSharedPlans(force: true)
        } else if !checkedICloud {
            await syncSharedPlans(force: true, quiet: true)
            // iCloud's answer, "no account" included, is final for this launch; being offline
            // is asked again at the next activation.
            if lastSyncError == nil || (lastSyncError as? PlanCloudError) == .noAccount {
                checkedICloud = true
            }
        }
    }

    /// Makes sure iCloud will push when a shared plan changes (database subscriptions, once;
    /// verified once per launch) and the app is registered for the pushes. A failure (offline)
    /// is tried again with the next sync.
    private func subscribeToChanges() async {
        guard !subscribed, PlanCloud.isAvailable, let cloud = try? planCloud() else { return }
        do {
            try await cloud.ensureSubscriptions()
            subscribed = true
            UIApplication.shared.registerForRemoteNotifications()
        } catch {
            // Not a reason to show a problem: the fetches on launch and activation still work.
        }
    }

    // MARK: Telling people

    /// Whether the app is in front (a banner shows) rather than in the background or about to
    /// be (a notification does).
    private var appIsActive: Bool { UIApplication.shared.applicationState == .active }

    /// Says what others changed in `applied` (`PlanChange.notices`, never what this person
    /// changed): a banner when the app is in front, else a notification, when Settings >
    /// Alerts has "Shared plan changes" on.
    private func announce(_ applied: AppliedChanges) async {
        guard preferences.planChangeNotices else { return }
        let notices = PlanChange.notices(for: applied.changed, ignoring: cloud?.myName)
        guard !notices.isEmpty else { return }
        if appIsActive {
            planBanners += PlanBanner.banners(for: notices)
            askForPlanNoticesSoon()
        } else {
            await postPlanNotices(notices)
        }
    }

    #if DEBUG
        /// `-planChangedBanner sample`: shows Robin's change (nothing reaches iCloud).
        func showSamplePlanBanner() { planBanners.append(.sample) }
    #endif

    /// The banner shown goes (it timed out, or was tapped).
    func dismissPlanBanner(_ id: UUID) {
        planBanners.removeAll { $0.id == id }
    }

    /// Opens a plan in the Plan tab (a banner or a notification was tapped).
    func openPlan(_ id: UUID) {
        guard plans.contains(where: { $0.id == id }) else { return }
        planToOpen = id
        planTabRequested = true
    }

    /// Shows the place of an alert whose notification was tapped and opens the alert.
    func openAlert(_ key: String, place id: String) {
        guard let index = places.firstIndex(where: { $0.id == id }) else { return }
        active = index
        alertToOpen = key
    }

    /// Whether the run is a screenshot run (no permission prompts).
    private var isScreenshotRun: Bool {
        #if DEBUG
            let args = ProcessInfo.processInfo.arguments
            return args.contains("-seed") || args.contains("-reset")
        #else
            return false
        #endif
    }

    /// Asks for permission to notify, from a task of its own: the alert waits for the person,
    /// and a sync (which is what announces) must not wait with it. Only called with the app in
    /// front, after a banner was shown; in the background nothing is asked.
    private func askForPlanNoticesSoon() {
        Task { await askForPlanNoticesIfUseful() }
    }

    /// Asks for permission to notify, once, when the first change to a shared plan arrives
    /// and the switch is on: never again after iOS has an answer.
    private func askForPlanNoticesIfUseful() async {
        await updateNotificationStatus()
        guard !isScreenshotRun, notificationStatus == .notDetermined, preferences.planChangeNotices
        else { return }
        _ = try? await UNUserNotificationCenter.current()
            .requestAuthorization(options: [.alert, .sound])
        await updateNotificationStatus()
    }

    /// One local notification per changed plan, grouped in the `plans` thread, which opens the
    /// plan when tapped (a newer change of a plan replaces its older notification).
    private func postPlanNotices(_ notices: [PlanChangeNotice]) async {
        // Never asks here (in the background nobody can answer): without permission, no post.
        await updateNotificationStatus()
        guard !isScreenshotRun, [.authorized, .provisional].contains(notificationStatus) else {
            return
        }
        for notice in notices {
            let content = UNMutableNotificationContent()
            content.title = "Shared plan changed"
            content.body = notice.sentence
            content.sound = .default
            content.threadIdentifier = NotificationTaps.planThread
            content.userInfo = [NotificationTaps.planKey: notice.planID.uuidString]
            try? await UNUserNotificationCenter.current().add(
                UNNotificationRequest(
                    identifier: "plan-change-\(notice.planID.uuidString)", content: content,
                    trigger: nil))
        }
    }

    /// Queues an owner's or editor's change to go to iCloud, half a second after the last one
    /// (like `pendingSave`), and remembers it as unsent until it has.
    private func queueCloudSave(_ id: UUID) {
        unsentPlans.insert(id)
        persistUnsent()
        // Without iCloud (a build that has none) it waits for a build that has.
        guard PlanCloud.isAvailable else { return }
        pendingCloud[id]?.cancel()
        pendingCloud[id] = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(500))
            guard !Task.isCancelled else { return }
            // After a sync or send that is running, never alongside it.
            await self?.inChain { await self?.sendToCloud(id) }
        }
    }

    /// Sends the plan as it is now, and applies what iCloud holds afterwards (theirs where they
    /// changed something this edit didn't). An edit made **while** it was going is merged into
    /// that the same way (`SavedPlan.afterSend`), so the next save doesn't undo their change,
    /// and is sent next. What others changed is announced. A failure leaves it unsent, except
    /// two that say the plan is no longer this person's to send: their permission was taken
    /// away (the server's plan replaces the local one, which would stay a fork), or it isn't
    /// shared with them any more (it becomes private).
    private func sendToCloud(_ id: UUID) async {
        guard unsentPlans.contains(id) else { return }
        guard let plan = plans.first(where: { $0.id == id }), let sharing = plan.sharing,
            sharing.mayEdit
        else { return markSent(id) }
        guard PlanCloud.isAvailable else { return }
        do {
            let saved = try await planCloud().save(plan)
            guard let i = plans.firstIndex(where: { $0.id == id }) else { return markSent(id) }
            let outcome = plans[i].afterSend(
                of: plan, stored: saved.stored, mergedWith: saved.mergedWith)
            if outcome.plan != plans[i] {
                plans[i] = outcome.plan
                savePlans()
            }
            syncProblem = nil
            cloudProblem = nil
            if outcome.resend {
                // Edited meanwhile: the rest goes out in a moment.
                queueCloudSave(id)
            } else {
                pendingCloud.removeValue(forKey: id)?.cancel()
                markSent(id)
            }
            await announce(AppliedChanges(changed: outcome.change.map { [$0] } ?? []))
        } catch {
            switch error as? PlanCloudError {
            case .readOnly? where !sharing.isOwner: await adoptServerPlan(id, sharing)
            case .notShared?: makePrivate(id, sharing)
            default:
                syncProblem = userMessage(error).title
                cloudProblem = syncProblem
            }
        }
    }

    /// iCloud refused an edit: the owner took this person's permission to change the plan away.
    /// What was edited here is dropped for what iCloud holds (the plan as the owner has it, and
    /// "can't edit" for this person), or the copy on this device would go its own way.
    private func adoptServerPlan(_ id: UUID, _ sharing: PlanSharing) async {
        pendingCloud.removeValue(forKey: id)?.cancel()
        markSent(id)
        if let i = plans.firstIndex(where: { $0.id == id }) {
            plans[i].sharing?.canEdit = false
            savePlans()
        }
        syncProblem = userMessage(PlanCloudError.readOnly).title
        cloudProblem = syncProblem
        do {
            guard let change = try await planCloud().current(sharing) else {
                return makePrivate(id, sharing)
            }
            var update = change
            if case .updated(let shared, var now) = change {
                now.canEdit = false
                update = .updated(shared, now)
            }
            applyCloudChanges([update])
        } catch {
            // Offline or so: the plan is read-only here now, and the next sync refreshes it.
        }
    }

    /// The plan isn't shared with this person any more (its record or their share is gone):
    /// it stays here as a private plan, nothing about it is remembered for iCloud, and the
    /// person is told.
    private func makePrivate(_ id: UUID, _ sharing: PlanSharing) {
        pendingCloud.removeValue(forKey: id)?.cancel()
        markSent(id)
        cloud?.forget(sharing.record)
        let name = plans.first(where: { $0.id == id })?.name ?? "This plan"
        setSharing(nil, for: id)
        let text = sharing.lostText(planName: name)
        syncProblem = text
        cloudProblem = text
        planBanners.append(PlanBanner(planID: id, text: text))
    }

    private func markSent(_ id: UUID) {
        guard unsentPlans.remove(id) != nil else { return }
        persistUnsent()
    }

    private func persistUnsent() {
        SharedContainer.defaults.set(unsentPlans.map(\.uuidString).sorted(), forKey: "unsentSharedPlans")
    }

    /// Saves a plan change still waiting (the app is going to the background).
    func savePendingPlans() {
        guard let pending = pendingSave else { return }
        pending.cancel()
        pendingSave = nil
        savePlans()
    }

    func removePlans(at offsets: IndexSet) {
        let removed = offsets.map { plans[$0] }
        plans.remove(atOffsets: offsets)
        savePlans()
        for plan in removed where plan.sharing != nil { endSharing(plan) }
    }

    func movePlans(from source: IndexSet, to destination: Int) {
        plans.move(fromOffsets: source, toOffset: destination)
        savePlans()
    }

    /// How `trip`'s dates were at `place` over its years: kept on disk, as past
    /// years never change.
    func plan(_ trip: Trip, at place: AppPlace) async throws -> PlanView {
        // The climate comes from keyless sources: no key is read.
        let settings = preferences.settings(withKey: false)
        let corePlace = place.corePlace()
        let name = String(
            format: "%.3f_%.3f-%04d%02d%02d-%04d%02d%02d-%d-%d", place.latitude, place.longitude,
            trip.fromYear, trip.fromMonth, trip.fromDay, trip.toYear, trip.toMonth, trip.toDay,
            trip.firstYear, trip.lastYear)
        let folder = directory.appending(path: "climate", directoryHint: .isDirectory)
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let file = folder.appending(path: "\(name).json")
        let climate: String
        if let kept = try? String(contentsOf: file, encoding: .utf8) {
            climate = kept
        } else {
            climate = try await fetchClimate(
                place: corePlace, settings: settings, trip: trip,
                now: Int64(Date.now.timeIntervalSince1970), host: URLSessionHost(ledgerURL: ledgerURL))
            try? climate.write(to: file, atomically: true, encoding: .utf8)
        }
        return try PlanView.decode(
            planView(place: corePlace, settings: settings, trip: trip, climateJson: climate))
    }

    #if DEBUG
        /// WeatherKit calls made since the app started (printed with the fetch timings).
        private static var weatherKitCalls = 0
    #endif

    /// Apple's minute nowcast at `place`, when it is switched on and Apple has one.
    /// Every call here counts against the account's WeatherKit allowance.
    private func appleMinutes(for place: AppPlace) async -> [MinuteStep]? {
        guard preferences.appleMinutes else { return nil }
        #if DEBUG
            Self.weatherKitCalls += 1
        #endif
        let minutes = await MinuteForecast.minutes(for: place)
        // The widgets and the Live Activity show Apple's minutes with its mark, which they
        // can't fetch themselves.
        if minutes != nil { await MinuteForecast.saveMarkForWidgets() }
        return minutes
    }

    /// Whether a WeatherKit call is due for `place` now: the switch is on, the
    /// place is the one shown (or the only one), `cache`'s forecast has rain
    /// hinted within two hours, and the last call is ten minutes old.
    private func shouldAskApple(_ place: AppPlace, cache: PlaceCache, now: Int64) -> Bool {
        guard preferences.appleMinutes, let json = cache.snapshotJson else { return false }
        let shown = places.indices.contains(active) && places[active].id == place.id
        return MinutesPolicy.shouldAsk(
            isShown: shown, onlyPlace: places.count == 1,
            wanted: (try? minutesWanted(snapshotJson: json, now: now)) ?? false,
            lastAskedAt: cache.appleMinutesAt, now: now)
    }

    /// Puts minutes into `cache`'s snapshot: the ones just fetched when Apple was
    /// asked, else the last ones while under an hour old, else none (which clears
    /// them from a fresh snapshot).
    private func applyMinutes(
        _ found: [MinuteStep]?, asked: Bool, at now: Int64, to cache: inout PlaceCache
    ) {
        if asked {
            cache.appleMinutesAt = now
            cache.appleMinuteSteps = found?.map { KeptMinute(time: $0.time, mmPerHour: $0.mmPerHour) }
            addMinutes(found, to: &cache)
        } else if preferences.appleMinutes, MinutesPolicy.keeps(askedAt: cache.appleMinutesAt, now: now),
            let steps = cache.appleMinuteSteps, !steps.isEmpty
        {
            addMinutes(steps.map { MinuteStep(time: $0.time, mmPerHour: $0.mmPerHour) }, to: &cache)
        } else {
            cache.appleMinuteSteps = nil
            // Only a snapshot holding Apple's minutes (kept when nothing changed) has
            // any to clear: a fresh one never does.
            if cache.appleMinutes == true { addMinutes(nil, to: &cache) }
        }
    }

    /// Apple's alerts at `place`, when they are switched on and `place` is outside
    /// the National Weather Service's coverage. Every call here counts against
    /// the account's WeatherKit allowance.
    private func appleAlerts(for place: AppPlace, now: Int64) async -> [ExternalAlert]? {
        #if DEBUG
            Self.weatherKitCalls += 1
            if let fake = Self.fakeAppleAlert(at: now) { return [fake] }
        #endif
        return await AppleAlerts.alerts(for: place, at: now)
    }

    /// Whether a WeatherKit alerts call is due for `place` now (`AppleAlertsPolicy`).
    private func shouldAskAppleAlerts(_ place: AppPlace, cache: PlaceCache, now: Int64) -> Bool {
        AppleAlertsPolicy.shouldAsk(
            enabled: preferences.appleAlerts,
            nwsCovers: nwsCovers(latitude: place.latitude, longitude: place.longitude),
            lastAskedAt: cache.appleAlertsAt, now: now)
    }

    /// Puts Apple's alerts into `cache`'s snapshot: the ones just fetched when
    /// Apple was asked and answered, else the last ones while under an hour old
    /// (also when a call failed), else none (which clears them from a fresh
    /// snapshot). A failed call (`found` nil) is not counted as an answer.
    private func applyAlerts(
        _ found: [ExternalAlert]?, asked: Bool, at now: Int64, for place: AppPlace,
        to cache: inout PlaceCache
    ) {
        let usable = preferences.appleAlerts
            && !nwsCovers(latitude: place.latitude, longitude: place.longitude)
        if asked, let found {
            cache.appleAlertsAt = now
            cache.appleAlertList = found.map(KeptAlert.init)
        }
        let shown: [KeptAlert]
        if usable, AppleAlertsPolicy.keeps(askedAt: cache.appleAlertsAt, now: now),
            let list = cache.appleAlertList
        {
            shown = list.filter { $0.end > now }
        } else {
            cache.appleAlertList = nil
            shown = []
        }
        guard let json = cache.snapshotJson else { return }
        // A snapshot with no Apple alerts and none to add stays as it is.
        if shown.isEmpty, cache.appleAlerts != true, !json.contains(AppleAlertsPolicy.idPrefix) { return }
        if let updated = try? withAlerts(snapshotJson: json, alerts: shown.map(\.external)) {
            cache.snapshotJson = updated
            cache.appleAlerts = !shown.isEmpty
        }
    }

    #if DEBUG
        /// `-fakeAppleAlert`: one alert instead of WeatherKit's answer (the
        /// simulator's build is unsigned), for the `alerts` screenshot.
        private static func fakeAppleAlert(at now: Int64) -> ExternalAlert? {
            guard ProcessInfo.processInfo.arguments.contains("-fakeAppleAlert") else { return nil }
            return AppleAlertsPolicy.alert(
                summary: "Flood Warning", region: "River Thames and tributaries",
                severity: .severe, source: "Environment Agency",
                url: URL(string: "https://example.invalid/flood-warning"), answeredAt: now)
        }
    #endif

    /// Places whose minutes are being fetched on their own (see `minutesIfWanted`).
    private var minutesFetching: Set<String> = []

    /// A minutes-only fetch for `place` when its page comes on screen: the
    /// forecast is cached, so only Apple is asked, and only if the same rules
    /// as in a refresh allow it. Does nothing during a refresh, which handles
    /// the place shown itself.
    func minutesIfWanted(for place: AppPlace) async {
        let now = Int64(Date.now.timeIntervalSince1970)
        guard !refreshing, !minutesFetching.contains(place.id),
            let cache = caches[place.id], shouldAskApple(place, cache: cache, now: now)
        else { return }
        minutesFetching.insert(place.id)
        defer { minutesFetching.remove(place.id) }
        let found = await appleMinutes(for: place)
        // The cache may have changed meanwhile: apply to what is there now.
        guard var current = caches[place.id] else { return }
        applyMinutes(found, asked: true, at: now, to: &current)
        commit(current, for: place)
        reloadWidgets()
        #if DEBUG
            print("minutes for \(place.name) (WeatherKit calls this launch: \(Self.weatherKitCalls))")
        #endif
    }

    /// Adds Apple's minute nowcast to the snapshot, or clears an old one where
    /// Apple has none now.
    private func addMinutes(_ minutes: [MinuteStep]?, to cache: inout PlaceCache) {
        guard let json = cache.snapshotJson else { return }
        if let updated = try? withMinutes(snapshotJson: json, minutes: minutes ?? []) {
            cache.snapshotJson = updated
            cache.appleMinutes = !(minutes ?? []).isEmpty
        }
    }

    /// What Apple's credit says for `place` while its mark loads.
    func appleCredit(_ place: AppPlace) -> LocalizedStringKey {
        let minutes = caches[place.id]?.appleMinutes == true
        let alerts = caches[place.id]?.appleAlerts == true
        if minutes, alerts { return "Next hour and alerts: Apple Weather" }
        return alerts ? "Alerts: Apple Weather" : "Next hour: Apple Weather"
    }

    /// Whether `place`'s forecast holds Apple's minutes or alerts.
    func usesAppleWeather(_ place: AppPlace) -> Bool {
        caches[place.id]?.appleMinutes == true || caches[place.id]?.appleAlerts == true
    }

    // MARK: Alerts and background refresh

    /// The background refresh's identifier (also in Whether-Info.plist).
    static let refreshTask = "dev.savery.whether.refresh"

    /// Asks iOS for a background refresh after the refresh interval (iOS decides).
    func scheduleRefresh() {
        let request = BGAppRefreshTaskRequest(identifier: Self.refreshTask)
        request.earliestBeginDate = .now.addingTimeInterval(preferences.backgroundInterval)
        try? BGTaskScheduler.shared.submit(request)
    }

    /// A background refresh: the next one is asked for first.
    func backgroundRefresh() async {
        scheduleRefresh()
        await refresh(inBackground: true)
    }

    /// Whether alerts can arrive for the saved places (from the NWS, or anywhere
    /// with One Call).
    var alertsPossible: Bool {
        AlertSources.alertsPossible(
            places: places, oneCall: preferences.oneCall, appleAlerts: preferences.appleAlerts)
    }

    /// What iOS answered about notifications (`.notDetermined` until asked).
    private(set) var notificationStatus: UNAuthorizationStatus = .notDetermined

    /// Reads iOS's answer again (it can change in the Settings app).
    func updateNotificationStatus() async {
        notificationStatus = await UNUserNotificationCenter.current().notificationSettings()
            .authorizationStatus
    }

    /// Asks for permission to notify when alerts can arrive, once: never again
    /// after iOS has an answer.
    func requestNotificationsIfUseful() async {
        await updateNotificationStatus()
        #if DEBUG
            // Screenshot runs: iOS's permission alert would cover them.
            let args = ProcessInfo.processInfo.arguments
            if args.contains("-seed") || args.contains("-reset") { return }
        #endif
        guard notificationStatus == .notDetermined, preferences.alertNotifications, alertsPossible
        else { return }
        _ = try? await UNUserNotificationCenter.current()
            .requestAuthorization(options: [.alert, .sound])
        await updateNotificationStatus()
    }

    /// Remembers the alerts shown, and posts the new or worse ones when `post`
    /// and iOS allows it.
    private func notify(post: Bool) async {
        if post { await updateNotificationStatus() }
        let allowed = [.authorized, .provisional].contains(notificationStatus)
        let now = Int64(Date.now.timeIntervalSince1970)
        for (place, view) in zip(places, views) {
            let notices = notifier.notices(for: view.alerts, place: place.id, now: now)
            guard post, allowed, preferences.alertNotifications else { continue }
            for alert in notices {
                let content = UNMutableNotificationContent()
                let message = AlertNotifier.message(for: alert, placeName: place.name)
                content.title = message.title
                content.subtitle = message.subtitle
                content.body = message.body
                content.userInfo = [
                    NotificationTaps.alertPlaceKey: place.id, NotificationTaps.alertKey: Self.alertKey(alert),
                ]
                content.sound = .default
                if ["very_poor", "extreme"].contains(alert.level) {
                    content.interruptionLevel = .timeSensitive
                }
                try? await UNUserNotificationCenter.current().add(
                    UNNotificationRequest(
                        identifier: AlertNotifier.key(alert, place: place.id), content: content,
                        trigger: nil))
            }
        }
        if let data = try? JSONEncoder().encode(notifier.notified) {
            try? data.write(to: directory.appending(path: "notified.json"), options: .atomic)
        }
    }

    /// After settings that change what is fetched (key, providers, days): fetch
    /// again without the conditional request.
    func refetch() async {
        await cachesRead?.value
        for place in places {
            caches[place.id]?.lastModified = nil
        }
        await refresh()
    }

    // MARK: Desktop config

    /// The day and hour counts the settings offer.
    static let dayChoices = [5, 7, 8, 10, 14, 16]
    static let precipHourChoices = [6, 12, 24, 48]

    /// Imports a desktop `config.toml`: the settings it sets and its places
    /// (added after these, then fetched). Returns what happened, line by line.
    func importDesktopConfig(_ text: String) async throws -> [String] {
        let imported = try importConfig(text: text)
        var p = preferences
        if let units = imported.units {
            p.units = switch units {
            case .auto: .automatic
            case .metric: .metric
            case .imperial: .imperial
            }
        }
        if let overrides = imported.unitOverrides { p.unitOverrides = Preferences.UnitPicks(overrides) }
        if let clock = imported.clock24h { p.clock = clock ? .twentyFour : .twelve }
        if let style = imported.dateStyle { p.dates = Preferences.DateChoice(style) }
        if let days = imported.dailyDays { p.days = Self.nearest(Int(days), in: Self.dayChoices) }
        if let hours = imported.precipHours {
            p.precipHours = Self.nearest(Int(hours), in: Self.precipHourChoices)
        }
        if let daylight = imported.daylightIcons { p.daylightIcons = daylight }
        if let feelsLike = imported.feelsLike { p.feelsLike = feelsLike }
        if let fill = imported.fillGaps { p.fillGaps = fill }
        // Models and One Call also switch their source rows, or the core would
        // ignore them (`nws` is its row already).
        if let models = imported.models { p.setModels(models) }
        if let nws = imported.nws { p.nws = nws }
        if let oneCall = imported.oneCall {
            // One Call needs the key, which stays out of the file.
            p.setOneCall(oneCall, hasKey: Keychain.key(for: Sources.openWeather) != nil)
        }
        preferences = p

        var added: [String] = []
        var lines: [String] = []
        for found in imported.places where !places.contains(where: { $0.name == found.name }) {
            if let place = await resolve(found) {
                places.append(place)
                added.append(place.name)
            } else {
                lines.append("\(found.name) was not found.")
            }
        }
        if !added.isEmpty {
            savePlaces()
            rebuild()
        }
        lines.insert(
            added.isEmpty
                ? "Settings imported; no new places."
                : "Settings imported; added \(added.joined(separator: ", ")).", at: 0)
        await refresh()
        return lines + imported.notes
    }

    /// The choice closest to `value`.
    private static func nearest(_ value: Int, in choices: [Int]) -> Int {
        choices.min { abs($0 - value) < abs($1 - value) } ?? value
    }

    /// An imported place with a time zone: coordinates are matched to the
    /// search result of the same name nearby (for its zone and region); postal
    /// codes and queries take the first result.
    private func resolve(_ imported: ImportedPlace) async -> AppPlace? {
        if let latitude = imported.latitude, let longitude = imported.longitude {
            let near = (try? await search(imported.name))?.first {
                abs($0.latitude - latitude) + abs($0.longitude - longitude) < 0.5
            }
            return AppPlace(
                name: imported.name,
                detail: near.map { AppPlace($0).detail } ?? nil,
                latitude: latitude, longitude: longitude,
                timeZone: imported.timezone ?? near?.timezone ?? TimeZone.current.identifier)
        }
        guard let query = imported.search,
            let first = try? await searchPlaces(
                query: query, country: imported.country ?? Locale.current.region?.identifier,
                language: Locale.current.language.languageCode?.identifier ?? "en",
                host: URLSessionHost(ledgerURL: ledgerURL)
            ).first
        else { return nil }
        var place = AppPlace(first)
        place.name = imported.name
        if let zone = imported.timezone { place.timeZone = zone }
        return place
    }

    /// Places matching `query` (a name, or a postal code in the phone's region).
    func search(_ query: String) async throws -> [FoundPlace] {
        try await searchPlaces(
            query: query, country: Locale.current.region?.identifier,
            language: Locale.current.language.languageCode?.identifier ?? "en",
            host: URLSessionHost(ledgerURL: ledgerURL))
    }
}
