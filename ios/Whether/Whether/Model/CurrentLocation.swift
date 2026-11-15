import CoreLocation
import Foundation

/// The phone's location as a place (when-in-use permission, asked once). Never
/// Always: a place that follows you is read when the app opens.
enum CurrentLocation {
    enum Failure: LocalizedError {
        case denied, unavailable
        var errorDescription: String? {
            switch self {
            case .denied: "Location access is off for Pug Forecast (Settings → Privacy)."
            case .unavailable: "The location could not be found."
            }
        }
    }

    /// The last fix: with `reuseRecent`, one under ten minutes old answers for a
    /// new one (`AppPlace.reusesFix`).
    private static var last: CLLocation?

    /// One location fix, at about a hundred metres: well inside the kilometre the
    /// following place moves at, which a fix also has to be sure of
    /// (`AppPlace.shouldMove`). With `reuseRecent` (coming back to the app) the
    /// last one answers while it is recent and access is still on; asked for
    /// (a pull to refresh, "where am I") a new one is read.
    /// Gives up after `timeout` seconds (no signal, or a permission prompt left alone).
    static func fix(timeout: Double = 12, reuseRecent: Bool = false) async throws -> CLLocation {
        #if DEBUG
            if let fake = debugFix() { return try fake.get() }
        #endif
        if let last, reuseRecent {
            let status = CLLocationManager().authorizationStatus
            if status == .denied || status == .restricted {
                self.last = nil
                throw Failure.denied
            }
            let allowed = status == .authorizedWhenInUse || status == .authorizedAlways
            if AppPlace.reusesFix(taken: last.timestamp, now: .now, allowed: allowed) { return last }
        }
        let location = try await withThrowingTaskGroup(of: CLLocation.self) { group in
            group.addTask { try await liveFix() }
            group.addTask {
                try await Task.sleep(for: .seconds(timeout))
                throw Failure.unavailable
            }
            defer { group.cancelAll() }
            // INVARIANT: the group has two tasks and one of them always finishes.
            return try await group.next()!
        }
        last = location
        return location
    }

    private static func liveFix() async throws -> CLLocation {
        let session = CLServiceSession(authorization: .whenInUse)
        defer { withExtendedLifetime(session) {} }
        switch CLLocationManager().authorizationStatus {
        case .authorizedWhenInUse, .authorizedAlways:
            // Wi-Fi where it answers, rather than the best accuracy (which keeps
            // GPS on).
            return try await CoarseFix().location()
        case .denied, .restricted:
            throw Failure.denied
        default:
            // Not asked yet: the live updates ask, and say what was answered.
            break
        }
        for try await update in CLLocationUpdate.liveUpdates() {
            if update.authorizationDenied || update.authorizationDeniedGlobally {
                throw Failure.denied
            }
            if let location = update.location { return location }
        }
        throw Failure.unavailable
    }

    /// The place `location` is: its locality, region and time zone.
    static func place(at location: CLLocation) async -> AppPlace {
        #if DEBUG
            if let named = debugName() {
                return AppPlace(
                    name: named.name, detail: named.detail, latitude: location.coordinate.latitude,
                    longitude: location.coordinate.longitude, timeZone: TimeZone.current.identifier)
            }
        #endif
        // Apple's geocoder gets the fix at about 1 km; the place keeps the exact one.
        let mark = try? await CLGeocoder().reverseGeocodeLocation(location.outgoing).first
        return AppPlace(
            name: mark?.locality ?? mark?.name ?? "Current location",
            detail: [mark?.administrativeArea, mark?.isoCountryCode].compactMap { $0 }
                .joined(separator: ", "),
            latitude: location.coordinate.latitude, longitude: location.coordinate.longitude,
            timeZone: (mark?.timeZone ?? .current).identifier)
    }

    static func place() async throws -> AppPlace {
        await place(at: try await fix())
    }

    #if DEBUG
        /// The simulator has no location. `-fakeLocation "lat:lon:Name:Detail"` gives the
        /// fix (and its name, without geocoding); `-fakeLocation off` refuses access.
        private static var fakeArgument: [Substring]? {
            let args = ProcessInfo.processInfo.arguments
            guard let i = args.firstIndex(of: "-fakeLocation"), i + 1 < args.count else { return nil }
            return args[i + 1].split(separator: ":", omittingEmptySubsequences: false)
        }

        private static func debugFix() -> Result<CLLocation, Failure>? {
            guard let parts = fakeArgument else { return nil }
            if parts.first == "off" { return .failure(.denied) }
            guard parts.count >= 2, let lat = Double(parts[0]), let lon = Double(parts[1]) else {
                return .failure(.unavailable)
            }
            return .success(CLLocation(latitude: lat, longitude: lon))
        }

        private static func debugName() -> (name: String, detail: String)? {
            guard let parts = fakeArgument, parts.count >= 3 else { return nil }
            let words = { (part: Substring) in part.replacingOccurrences(of: "_", with: " ") }
            return (words(parts[2]), parts.count > 3 ? words(parts[3]) : "")
        }
    #endif
}

/// One fix at about a hundred metres (`kCLLocationAccuracyHundredMeters`), once
/// location access is given.
private final class CoarseFix: NSObject, CLLocationManagerDelegate {
    private let manager = CLLocationManager()
    private var continuation: CheckedContinuation<CLLocation, any Error>?

    func location() async throws -> CLLocation {
        try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                guard !Task.isCancelled else {
                    continuation.resume(throwing: CancellationError())
                    return
                }
                self.continuation = continuation
                manager.delegate = self
                manager.desiredAccuracy = kCLLocationAccuracyHundredMeters
                manager.requestLocation()
            }
        } onCancel: {
            Task { @MainActor in self.finish(.failure(CancellationError())) }
        }
    }

    private func finish(_ result: Result<CLLocation, any Error>) {
        guard let continuation else { return }
        self.continuation = nil
        manager.stopUpdatingLocation()
        continuation.resume(with: result)
    }

    nonisolated func locationManager(_ manager: CLLocationManager, didUpdateLocations locations: [CLLocation]) {
        guard let location = locations.last else { return }
        Task { @MainActor in self.finish(.success(location)) }
    }

    nonisolated func locationManager(_ manager: CLLocationManager, didFailWithError error: any Error) {
        let denied = (error as? CLError)?.code == .denied
        Task { @MainActor in
            self.finish(.failure(denied ? CurrentLocation.Failure.denied : CurrentLocation.Failure.unavailable))
        }
    }
}
