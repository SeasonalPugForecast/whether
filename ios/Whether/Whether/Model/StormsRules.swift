import Foundation
import WhetherKit

/// What the Storms tab reads from the core's outlook JSON besides the view: when it
/// was fetched and each model's run, to ask for the runs before it.
struct StormOutlookRuns: Decodable, Sendable, Hashable {
    /// Unix seconds.
    var fetchedAt: Int64
    var points: [Point]

    struct Point: Decodable, Sendable, Hashable {
        /// The Open-Meteo id, `ecmwf_ifs025`.
        var model: String
        var run: Run?
    }

    struct Run: Decodable, Sendable, Hashable {
        /// The run's initialisation, Unix seconds (`init` in the JSON).
        var start: Int64

        private enum CodingKeys: String, CodingKey {
            case start = "init"
        }
    }

    static func decode(_ json: String) throws -> StormOutlookRuns {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return try decoder.decode(StormOutlookRuns.self, from: Data(json.utf8))
    }

    /// The start of `model`'s run in the outlook, when known.
    func latestInit(of model: String) -> Int64? {
        points.first { $0.model == model }?.run?.start
    }
}

/// The earlier runs of one model over one event's window, for the run-to-run bars.
struct PastRunsRequest: Hashable, Sendable {
    var model: String
    /// The run the outlook shows; the earlier ones come before it.
    var latestInit: Int64
    var windowStart: Int64
    var windowEnd: Int64

    /// The key the set is kept under: the four fix its runs and totals for good.
    var key: String { "\(model)@\(latestInit):\(windowStart)-\(windowEnd)" }
}

/// When the Storms tab fetches, and what it keeps.
enum StormsRules {
    /// An outlook older than this says so.
    static let staleAfter: Int64 = 12 * 3_600
    /// The models' run times are read at most this often per place; the outlook itself
    /// is fetched again only when a model has a newer run (the core decides).
    static let checkEvery: Int64 = 15 * 60
    /// The models whose earlier runs are compared.
    static let pastRunRoles: Set<String> = ["ecmwf", "gfs"]
    /// How many earlier runs each comparison asks for.
    static let pastRuns: UInt32 = 4

    /// Whether an outlook fetched at `fetchedAt` is too old to show without a note.
    static func isStale(fetchedAt: Int64, now: Int64) -> Bool {
        now - fetchedAt > staleAfter
    }

    /// Whether the run times are due again: never read, read `checkEvery` ago or more,
    /// or read "in the future" (the clock moved back).
    static func isDue(lastChecked: Int64?, now: Int64) -> Bool {
        guard let lastChecked else { return true }
        return now - lastChecked >= checkEvery || now < lastChecked
    }

    /// The earlier runs to show for `event`: ECMWF's and GFS's, before the runs the
    /// outlook holds, over the event's window. A model without a known run is skipped.
    static func pastRunsRequests(
        for event: StormsView.Event, models: [StormsView.ModelChip], runs: StormOutlookRuns
    ) -> [PastRunsRequest] {
        models.filter { pastRunRoles.contains($0.role) }.compactMap { chip in
            runs.latestInit(of: chip.model).map {
                PastRunsRequest(
                    model: chip.model, latestInit: $0, windowStart: event.start, windowEnd: event.end)
            }
        }
    }

    /// The requests still to send: not kept, not failed (until the outlook changes,
    /// which changes their keys), and not already under way.
    static func toFetch(
        _ requests: [PastRunsRequest], kept: Set<String>, failed: Set<String>, running: Set<String>
    ) -> [PastRunsRequest] {
        requests.filter { !kept.contains($0.key) && !failed.contains($0.key) && !running.contains($0.key) }
    }

    /// The JSON lists `sets` (each as `fetchPastRuns` returned it) as one list, or `nil`
    /// when none holds a run.
    static func joined(_ sets: [String]) -> String? {
        let bodies = sets.compactMap { set -> String? in
            let list = set.trimmingCharacters(in: .whitespacesAndNewlines)
            guard list.hasPrefix("["), list.hasSuffix("]") else { return nil }
            let body = list.dropFirst().dropLast().trimmingCharacters(in: .whitespacesAndNewlines)
            return body.isEmpty ? nil : body
        }
        return bodies.isEmpty ? nil : "[" + bodies.joined(separator: ",") + "]"
    }

    /// Where the outlooks and earlier runs are kept: the shared caches directory.
    static var directory: URL {
        SharedContainer.cachesDirectory.appending(path: "storms", directoryHint: .isDirectory)
    }

    /// The file `placeID`'s outlook is kept in, under `directory`. The Storms tab and the
    /// check for a storm ahead share it: whichever fetched last, the other starts from it.
    static func outlookURL(_ placeID: String, in directory: URL = StormsRules.directory) -> URL {
        directory.appending(path: fileName(placeID, prefix: "outlook"))
    }

    /// A file name for `key`: letters, digits, `_` and `-`, everything else `_`.
    static func fileName(_ key: String, prefix: String) -> String {
        let safe = String(
            String.UnicodeScalarView(
                key.unicodeScalars.map { s in
                    s.isASCII && (CharacterSet.alphanumerics.contains(s) || s == "_" || s == "-") ? s : "_"
                }))
        return "\(prefix)-\(safe).json"
    }
}
