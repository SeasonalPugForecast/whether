import Foundation
import Observation
import WhetherKit

/// The Storms tab's state: the outlook for the place shown, kept per place in memory
/// and in the caches directory, and the view the core builds from it.
///
/// The tab loads only while it shows (`load`, from `.task(id:)` on the place). The
/// models' run times are read at most every `StormsRules.checkEvery`; the core fetches
/// the data again only when a model has a newer run. The view is rebuilt by the core
/// when the clock or the settings change; nothing is formatted here.
@Observable
final class StormsStore {
    /// The view for the place shown.
    private(set) var view: StormsView?
    /// A fetch is under way.
    private(set) var loading = false
    /// Why the outlook couldn't be had (or updated), in words, and the text behind it.
    private(set) var failure: String?
    private(set) var failureDetail: String?
    /// The outlook shown is over `StormsRules.staleAfter` old.
    private(set) var stale = false

    /// The outlook shown and what it is for.
    private struct Shown {
        var place: AppPlace
        var outlookJson: String
        var runs: StormOutlookRuns
        /// The earlier runs' sets that go with it, by key, in the order they came.
        var pastRunKeys: [String] = []
    }

    @ObservationIgnored private var shown: Shown?
    @ObservationIgnored private var settings: Settings?
    /// Outlook JSON by place id, as on disk.
    @ObservationIgnored private var outlooks: [String: String] = [:]
    /// When each place's run times were last read, Unix seconds.
    @ObservationIgnored private var checked: [String: Int64] = [:]
    /// Earlier runs' sets (JSON lists) by `PastRunsRequest.key`, as on disk.
    @ObservationIgnored private var pastRuns: [String: String] = [:]
    @ObservationIgnored private var failedPastRuns: Set<String> = []
    @ObservationIgnored private var runningPastRuns: Set<String> = []
    private let directory: URL
    private let ledgerURL: URL

    init(
        directory: URL = SharedContainer.cachesDirectory.appending(path: "storms", directoryHint: .isDirectory),
        ledgerURL: URL = SharedContainer.directory.appending(path: "ledger.json")
    ) {
        self.directory = directory
        self.ledgerURL = ledgerURL
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    }

    /// The time the view is built for.
    private var now: Date { .now }

    // MARK: Loading

    /// Shows `place`'s outlook: the kept one at once, then a fresh one when the models'
    /// run times are due.
    func load(place: AppPlace, settings: Settings) async {
        self.settings = settings
        if shown?.place.id != place.id {
            shown = nil
            view = nil
            failure = nil
            failureDetail = nil
            stale = false
        }
        let kept = outlooks[place.id] ?? readOutlook(place)
        if shown == nil, let kept {
            show(kept, for: place)
            rebuild()
        }
        let t = Int64(now.timeIntervalSince1970)
        guard view == nil || StormsRules.isDue(lastChecked: checked[place.id], now: t) else { return }
        loading = true
        defer { loading = false }
        do {
            let outcome = try await fetchStorms(
                place: place.corePlace(now: now), settings: settings, cachedJson: kept, now: t,
                host: URLSessionHost(ledgerURL: ledgerURL))
            checked[place.id] = t
            failure = nil
            failureDetail = nil
            if outcome.outlookJson != kept { keep(outcome.outlookJson, for: place) }
            // The place may have changed while the request ran.
            if shown == nil || shown?.place.id == place.id {
                if shown?.outlookJson != outcome.outlookJson { show(outcome.outlookJson, for: place) }
                rebuild()
            }
        } catch {
            guard shown == nil || shown?.place.id == place.id else { return }
            let message = userMessage(error)
            failure = message.title
            failureDetail = message.detail
        }
    }

    /// Builds the view again (the clock moved, or the settings changed).
    func rebuild(settings: Settings? = nil) {
        if let settings { self.settings = settings }
        guard let shown, let settings = self.settings else { return }
        let t = Int64(now.timeIntervalSince1970)
        do {
            let json = try stormsView(
                place: shown.place.corePlace(now: now), outlookJson: shown.outlookJson,
                pastRunsJson: StormsRules.joined(shown.pastRunKeys.compactMap { pastRuns[$0] }),
                settings: settings, now: t)
            view = try StormsView.decode(json)
            stale = StormsRules.isStale(fetchedAt: shown.runs.fetchedAt, now: t)
        } catch {
            let message = userMessage(error)
            failure = message.title
            failureDetail = message.detail
        }
    }

    /// Fetches ECMWF's and GFS's earlier runs over `event`'s window (each set once, then
    /// kept for good) and builds the view again with them.
    func loadPastRuns(for event: StormsView.Event) async {
        guard let current = shown, let view else { return }
        let requests = StormsRules.pastRunsRequests(for: event, models: view.models, runs: current.runs)
        var added = false
        for request in requests where pastRuns[request.key] == nil {
            if let kept = try? String(contentsOf: pastRunsURL(request.key), encoding: .utf8) {
                pastRuns[request.key] = kept
            }
        }
        for request in requests where pastRuns[request.key] != nil && !current.pastRunKeys.contains(request.key) {
            shown?.pastRunKeys.append(request.key)
            added = true
        }
        let missing = StormsRules.toFetch(
            requests, kept: Set(pastRuns.keys), failed: failedPastRuns, running: runningPastRuns)
        let place = current.place
        for request in missing {
            runningPastRuns.insert(request.key)
            defer { runningPastRuns.remove(request.key) }
            do {
                let json = try await fetchPastRuns(
                    place: place.corePlace(now: now), model: request.model,
                    windowStart: request.windowStart, windowEnd: request.windowEnd,
                    latestInit: request.latestInit, runs: StormsRules.pastRuns,
                    host: URLSessionHost(ledgerURL: ledgerURL))
                pastRuns[request.key] = json
                write(json, to: pastRunsURL(request.key))
                // Still the same outlook: its bars gain the runs.
                if shown?.place.id == place.id, shown?.runs == current.runs {
                    shown?.pastRunKeys.append(request.key)
                    added = true
                }
            } catch {
                failedPastRuns.insert(request.key)
            }
        }
        if added { rebuild() }
    }

    // MARK: Keeping

    private func show(_ outlookJson: String, for place: AppPlace) {
        guard let runs = try? StormOutlookRuns.decode(outlookJson) else { return }
        shown = Shown(place: place, outlookJson: outlookJson, runs: runs)
    }

    private func outlookURL(_ place: AppPlace) -> URL {
        directory.appending(path: StormsRules.fileName(place.id, prefix: "outlook"))
    }

    private func pastRunsURL(_ key: String) -> URL {
        directory.appending(path: StormsRules.fileName(key, prefix: "runs"))
    }

    private func readOutlook(_ place: AppPlace) -> String? {
        let json = try? String(contentsOf: outlookURL(place), encoding: .utf8)
        if let json { outlooks[place.id] = json }
        return json
    }

    private func keep(_ outlookJson: String, for place: AppPlace) {
        outlooks[place.id] = outlookJson
        write(outlookJson, to: outlookURL(place))
    }

    private func write(_ json: String, to url: URL) {
        try? Data(json.utf8).write(to: url, options: .atomic)
    }
}
