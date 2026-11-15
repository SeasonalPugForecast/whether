import Foundation

/// Performs the core's requests (GET, or POST with a JSON body) with `URLSession` and keeps the One Call ledger in
/// a file. The URL may carry the OpenWeather key: never log it.
public final class URLSessionHost: FetchHost, @unchecked Sendable {
    let session: URLSession
    private let ledgerURL: URL
    /// Where the waits some services ask for are kept: beside the ledger.
    private let holdURL: URL
    /// One at a time: the places of a refresh send side by side and share the file.
    private static let holdLock = NSLock()

    /// `ledgerURL`: where the call ledger lives (pass its contents back as
    /// `Cached.usageJson` on the next fetch).
    public init(session: URLSession = .noStore, ledgerURL: URL) {
        self.session = session
        self.ledgerURL = ledgerURL
        holdURL = ledgerURL.deletingLastPathComponent().appending(path: "service-holds.json")
    }

    #if DEBUG
        /// Screenshots and tests: every request fails as when the phone is offline.
        nonisolated(unsafe) public static var simulateOffline = false
        /// Screenshots: each request waits this long first (a slow network).
        nonisolated(unsafe) public static var simulatedDelay: Duration = .zero
    #endif

    public func send(request: HostRequest) async throws -> HostResponse {
        #if DEBUG
            if Self.simulateOffline {
                throw HostError.Failed(message: URLError(.notConnectedToInternet).localizedDescription)
            }
            if Self.simulatedDelay > .zero { try? await Task.sleep(for: Self.simulatedDelay) }
        #endif
        guard let url = URL(string: request.url) else {
            throw HostError.Failed(message: "invalid URL")
        }
        var urlRequest = URLRequest(url: url, timeoutInterval: 30)
        switch request.method {
        case .get:
            urlRequest.httpMethod = "GET"
        case .post:
            urlRequest.httpMethod = "POST"
            urlRequest.setValue("application/json", forHTTPHeaderField: "Content-Type")
            urlRequest.httpBody = request.body.map { Data($0.utf8) }
        }
        for header in request.headers {
            urlRequest.setValue(header.value, forHTTPHeaderField: header.name)
        }
        // A service that asked for a wait (MET Norway: until `Expires`, an hour after a refusal)
        // is not asked before then.
        let waits = ServiceHold.applies(to: url)
        if waits {
            let seconds = Self.holdLock.withLock {
                ServiceHold.load(from: holdURL).wait(
                    for: request.url, now: Int64(Date().timeIntervalSince1970))
            }
            if seconds > 0 {
                throw HostError.Failed(
                    message: "\(url.host ?? "the service") asked apps to wait \((seconds + 59) / 60) more min")
            }
        }
        do {
            let (data, response) = try await session.data(for: urlRequest)
            let http = response as? HTTPURLResponse
            let status = UInt16(clamping: http?.statusCode ?? 0)
            let expires = Self.unixTime(http?.value(forHTTPHeaderField: "Expires"))
            if waits {
                Self.holdLock.withLock {
                    var holds = ServiceHold.load(from: holdURL)
                    holds.record(
                        url: request.url, status: status, expires: expires,
                        now: Int64(Date().timeIntervalSince1970))
                    holds.save(to: holdURL)
                }
            }
            return HostResponse(
                status: status,
                body: data,
                date: Self.unixTime(http?.value(forHTTPHeaderField: "Date")),
                expires: expires,
                lastModified: http?.value(forHTTPHeaderField: "Last-Modified")
            )
        } catch {
            throw HostError.Failed(message: error.localizedDescription)
        }
    }

    public func saveUsage(ledgerJson: String) throws {
        do {
            try Data(ledgerJson.utf8).write(to: ledgerURL, options: .atomic)
        } catch {
            throw HostError.Failed(message: error.localizedDescription)
        }
    }

    /// An HTTP date (`Sun, 11 Oct 2026 12:00:00 GMT`) as Unix seconds.
    static func unixTime(_ header: String?) -> Int64? {
        guard let header else { return nil }
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = TimeZone(identifier: "GMT")
        formatter.dateFormat = "EEE, dd MMM yyyy HH:mm:ss zzz"
        return formatter.date(from: header).map { Int64($0.timeIntervalSince1970) }
    }
}

extension URLSession {
    /// The session for every request the app makes itself. An OpenWeather URL carries the user's
    /// key (`appid=`), so responses are cached in memory only and nothing (cache, cookies,
    /// credentials) is written to disk, where the shared session's cache would keep the URL.
    public static let noStore: URLSession = {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.urlCache = URLCache(memoryCapacity: 8 << 20, diskCapacity: 0)
        return URLSession(configuration: configuration)
    }()

    /// The host of NOAA's radar tiles (the Iowa Environmental Mesonet).
    static let radarTileHost = "mesonet.agron.iastate.edu"

    /// The session for radar tiles that carry no key (NOAA's composite from IEM): a disk cache
    /// in the app's Caches folder keeps them across launches for as long as IEM allows (14 days
    /// for a past composite's tiles), and more of them load side by side than the default
    /// allows (IEM speaks HTTP/1.1, one tile per connection at a time). No cookies or
    /// credentials are kept.
    public static let radarTiles: URLSession = {
        let configuration = URLSessionConfiguration.default
        let folder = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask).first?
            .appending(path: "radar-tiles", directoryHint: .isDirectory)
        configuration.urlCache = URLCache(memoryCapacity: 16 << 20, diskCapacity: 100 << 20, directory: folder)
        configuration.requestCachePolicy = .useProtocolCachePolicy
        configuration.httpMaximumConnectionsPerHost = 8
        configuration.httpCookieStorage = nil
        configuration.httpShouldSetCookies = false
        configuration.urlCredentialStorage = nil
        return URLSession(configuration: configuration)
    }()

    /// The session to fetch a map tile with: `radarTiles` for IEM's tiles, which carry no
    /// secret, and `noStore` for anything else (OpenWeather's carry the user's key).
    public static func tiles(for url: URL) -> URLSession {
        url.host() == radarTileHost && url.query() == nil ? radarTiles : noStore
    }
}
