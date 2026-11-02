import Foundation

/// Performs the core's requests (GET, or POST with a JSON body) with `URLSession` and keeps the One Call ledger in
/// a file. The URL may carry the OpenWeather key: never log it.
public final class URLSessionHost: FetchHost, @unchecked Sendable {
    private let session: URLSession
    private let ledgerURL: URL

    /// `ledgerURL`: where the call ledger lives (pass its contents back as
    /// `Cached.usageJson` on the next fetch).
    public init(session: URLSession = .shared, ledgerURL: URL) {
        self.session = session
        self.ledgerURL = ledgerURL
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
        do {
            let (data, response) = try await session.data(for: urlRequest)
            let http = response as? HTTPURLResponse
            return HostResponse(
                status: UInt16(clamping: http?.statusCode ?? 0),
                body: data,
                date: Self.unixTime(http?.value(forHTTPHeaderField: "Date")),
                expires: Self.unixTime(http?.value(forHTTPHeaderField: "Expires")),
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
