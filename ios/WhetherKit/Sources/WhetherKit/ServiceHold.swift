import Foundation

/// MET Norway's terms ask apps to wait for a response's `Expires` before asking for the same
/// thing again, and to back off at once after a 403 or 429. This is that memory for `api.met.no`,
/// kept in a small file between launches. A hold is for one URL (each resource has its own
/// `Expires`: the forecast and the sunrise differ), except after a refusal, which holds the host.
struct ServiceHold: Codable, Equatable {
    /// The hosts whose terms ask for it.
    static let hosts: Set<String> = ["api.met.no"]
    /// How long a 403 or 429 holds every request to the host, in seconds.
    static let afterRefusal: Int64 = 3600
    /// How long a success that named no `Expires` holds its URL, in seconds.
    static let withoutExpires: Int64 = 600
    /// The longest any hold lasts, so a wrong `Expires` can't lock a service out for days.
    static let longest: Int64 = 3 * 3600
    /// The most URLs remembered.
    static let capacity = 128

    /// Until when (Unix seconds) each URL waits.
    var urls: [String: Int64] = [:]
    /// Until when every request to the host waits.
    var host: Int64 = 0

    /// Whether `url` is to a host that asks for holds.
    static func applies(to url: URL) -> Bool {
        url.host.map { hosts.contains($0.lowercased()) } ?? false
    }

    /// The seconds `url` must still wait at `now` (0 when it may go).
    func wait(for url: String, now: Int64) -> Int64 {
        max(0, max(host, urls[url] ?? 0) - now)
    }

    /// Notes a response to `url`: its `Expires` holds that URL, a 403 or 429 holds the host.
    mutating func record(url: String, status: UInt16, expires: Int64?, now: Int64) {
        urls = urls.filter { $0.value > now }
        switch status {
        case 403, 429:
            host = min(max(host, now + Self.afterRefusal), now + Self.longest)
        case 200, 304:
            let until = expires ?? now + Self.withoutExpires
            if until > now { urls[url] = min(until, now + Self.longest) }
        default:
            break
        }
        while urls.count > Self.capacity, let oldest = urls.min(by: { $0.value < $1.value })?.key {
            urls[oldest] = nil
        }
    }

    /// The saved holds, or none when the file is missing or unreadable.
    static func load(from file: URL) -> ServiceHold {
        (try? Data(contentsOf: file)).flatMap { try? JSONDecoder().decode(ServiceHold.self, from: $0) }
            ?? ServiceHold()
    }

    func save(to file: URL) {
        if let data = try? JSONEncoder().encode(self) { try? data.write(to: file, options: .atomic) }
    }
}
