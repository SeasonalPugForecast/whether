import MapKit
import WhetherKit

/// A place offered while typing: Apple Maps' completion, or Open-Meteo's
/// search result when Apple has none.
enum PlaceSuggestion: Hashable, Identifiable {
    case apple(MKLocalSearchCompletion)
    case found(FoundPlace)

    var id: Self { self }

    var title: String {
        switch self {
        case .apple(let completion): completion.title
        case .found(let place): place.name
        }
    }

    var subtitle: String? {
        switch self {
        case .apple(let completion): completion.subtitle.isEmpty ? nil : completion.subtitle
        case .found(let place): AppPlace(place).detail
        }
    }
}

/// Places suggested as you type, nearest first: Apple Maps' completer
/// (towns and postal codes, from the first letters, ranked near the place
/// shown), then Open-Meteo's search, sorted by distance, when Apple has
/// nothing or cannot be reached.
@Observable
final class PlaceSuggestions: NSObject, MKLocalSearchCompleterDelegate {
    private(set) var items: [PlaceSuggestion] = []
    /// Why there are none ("No places found", or a friendly error).
    private(set) var status: String?
    /// The raw text behind `status`.
    private(set) var statusDetail: String?

    @ObservationIgnored private let completer = MKLocalSearchCompleter()
    @ObservationIgnored private let store: WeatherStore
    @ObservationIgnored private let near: CLLocation?
    @ObservationIgnored private var query = ""
    @ObservationIgnored private var fallback: Task<Void, Never>?

    /// Suggestions ranked near `near` (the place shown), when there is one.
    init(store: WeatherStore, near: AppPlace?) {
        self.store = store
        self.near = near.map { CLLocation(latitude: $0.latitude, longitude: $0.longitude) }
        super.init()
        completer.resultTypes = .address
        completer.addressFilter = MKAddressFilter(including: [.locality, .postalCode])
        if let near = self.near {
            completer.region = MKCoordinateRegion(
                center: near.coordinate.rounded(), latitudinalMeters: 800_000, longitudinalMeters: 800_000)
        }
        completer.delegate = self
    }

    /// Suggests places for `text` (from one letter).
    func update(_ text: String) {
        query = text.trimmingCharacters(in: .whitespaces)
        fallback?.cancel()
        statusDetail = nil
        guard !query.isEmpty else {
            completer.cancel()
            items = []
            status = nil
            return
        }
        completer.queryFragment = query
    }

    nonisolated func completerDidUpdateResults(_ completer: MKLocalSearchCompleter) {
        // Called on the main thread; this completer is the only one.
        MainActor.assumeIsolated {
            let results = self.completer.results
            if results.isEmpty {
                searchOpenMeteo()
            } else {
                fallback?.cancel()
                items = results.map(PlaceSuggestion.apple)
                status = nil
            }
        }
    }

    nonisolated func completer(_ completer: MKLocalSearchCompleter, didFailWithError error: Error) {
        MainActor.assumeIsolated { searchOpenMeteo() }
    }

    /// Open-Meteo's search for the query, nearest first.
    private func searchOpenMeteo() {
        let text = query
        guard text.count >= 2 else {
            items = []
            status = text.isEmpty ? nil : "Keep typing"
            return
        }
        fallback?.cancel()
        fallback = Task {
            do {
                let found = try await store.search(text)
                guard !Task.isCancelled, text == query else { return }
                items = nearestFirst(found).map(PlaceSuggestion.found)
                status = items.isEmpty ? "No places found" : nil
            } catch {
                guard !Task.isCancelled, text == query else { return }
                items = []
                let message = userMessage(error)
                status = message.title
                statusDetail = message.detail
            }
        }
    }

    private func nearestFirst(_ found: [FoundPlace]) -> [FoundPlace] {
        guard let near else { return found }
        let distance = { (p: FoundPlace) in near.distance(from: CLLocation(latitude: p.latitude, longitude: p.longitude)) }
        return found.sorted { distance($0) < distance($1) }
    }

    /// The suggestion as a place to save: Apple's is looked up for its
    /// coordinates and time zone.
    func place(for suggestion: PlaceSuggestion) async throws -> AppPlace {
        switch suggestion {
        case .found(let found):
            return AppPlace(found)
        case .apple(let completion):
            let response = try await MKLocalSearch(request: MKLocalSearch.Request(completion: completion)).start()
            guard let item = response.mapItems.first else { throw CurrentLocation.Failure.unavailable }
            let mark = item.placemark
            return AppPlace(
                name: mark.locality ?? item.name ?? completion.title,
                detail: [mark.administrativeArea, mark.isoCountryCode].compactMap { $0 }.joined(separator: ", "),
                latitude: mark.coordinate.latitude, longitude: mark.coordinate.longitude,
                timeZone: (item.timeZone ?? .current).identifier)
        }
    }
}
