import CoreLocation
import Foundation
import WhetherKit

/// A place the app forecasts.
struct AppPlace: Codable, Hashable, Identifiable {
    /// Stable across renames: a UUID for new places, `name:<name>` for places
    /// saved before ids existed (the same in the app and the widget).
    var id: String
    var name: String
    /// "New York, US", shown in the place list.
    var detail: String?
    var latitude: Double
    var longitude: Double
    /// IANA zone, for the offset and its DST changes (Foundation computes them;
    /// the core has no time zone database).
    var timeZone: String
    /// Follows the phone: its coordinates are the phone's location, read each time
    /// the app opens (`WeatherStore.followMe`). At most one place does, and it is
    /// first in the list (`arranged`). `false` in places saved before.
    var follows = false
    /// Pinned: listed after the following place and before the others (`arranged`).
    /// `false` in places saved before.
    var pinned = false

    #if DEBUG
        /// The test place, for screenshots (`-seed`) and tests only.
        static let new_york = AppPlace(
            id: "name:New York", name: "New York", detail: "New York, US", latitude: 40.7143,
            longitude: -74.00597, timeZone: "America/New_York")
    #endif

    /// A search result as a place.
    init(_ found: FoundPlace) {
        self.init(
            name: found.name,
            detail: [found.region, found.countryCode].compactMap { $0 }.joined(separator: ", "),
            latitude: found.latitude, longitude: found.longitude,
            timeZone: found.timezone ?? TimeZone.current.identifier)
    }

    init(
        id: String = UUID().uuidString, name: String, detail: String?, latitude: Double,
        longitude: Double, timeZone: String, follows: Bool = false, pinned: Bool = false
    ) {
        self.follows = follows
        self.pinned = pinned
        self.id = id
        self.name = name
        self.detail = detail
        self.latitude = latitude
        self.longitude = longitude
        self.timeZone = timeZone
    }

    private enum CodingKeys: String, CodingKey {
        case id, name, detail, latitude, longitude, timeZone, follows, pinned
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        name = try c.decode(String.self, forKey: .name)
        detail = try c.decodeIfPresent(String.self, forKey: .detail)
        latitude = try c.decode(Double.self, forKey: .latitude)
        longitude = try c.decode(Double.self, forKey: .longitude)
        timeZone = try c.decode(String.self, forKey: .timeZone)
        // Saved by an older version: deterministic, so the app and the widget
        // agree before either rewrites `places.json`.
        id = try c.decodeIfPresent(String.self, forKey: .id) ?? "name:" + name
        follows = try c.decodeIfPresent(Bool.self, forKey: .follows) ?? false
        pinned = try c.decodeIfPresent(Bool.self, forKey: .pinned) ?? false
    }

    // MARK: Following

    /// Whether a place at `old` moves to `new`: the phone is 1 km or more away.
    static func shouldMove(old: CLLocationCoordinate2D, new: CLLocationCoordinate2D) -> Bool {
        CLLocation(latitude: old.latitude, longitude: old.longitude)
            .distance(from: CLLocation(latitude: new.latitude, longitude: new.longitude)) >= 1_000
    }

    var coordinate: CLLocationCoordinate2D {
        CLLocationCoordinate2D(latitude: latitude, longitude: longitude)
    }

    // MARK: What leaves the phone

    /// Decimals of every coordinate that leaves the phone: 2, about 1.1 km (the core's
    /// `COORDINATE_DECIMALS` too). Apple counts 3 or more as Precise Location, and the
    /// privacy label says Coarse. The saved place keeps its full coordinates.
    static let outgoingDecimals = 2

    /// The place's coordinate to `decimals` places: what WeatherKit and the geocoder get.
    func rounded(decimals: Int = AppPlace.outgoingDecimals) -> CLLocationCoordinate2D {
        coordinate.rounded(decimals: decimals)
    }

    /// The place as a location to hand to a system service that sends it to Apple
    /// (WeatherKit), rounded to `outgoingDecimals`.
    var outgoingLocation: CLLocation {
        CLLocation(latitude: rounded().latitude, longitude: rounded().longitude)
    }

    /// `places` with at most one following place (the first that follows; the
    /// others become fixed), then the pinned places, then the rest. That one is
    /// first; each group keeps its order.
    static func arranged(_ places: [AppPlace]) -> [AppPlace] {
        var seen = false
        let places = places.map { place in
            var place = place
            if place.follows { place.follows = !seen; seen = true }
            return place
        }
        return places.filter(\.follows) + places.filter { !$0.follows && $0.pinned }
            + places.filter { !$0.follows && !$0.pinned }
    }

    /// `places` after the ones at `source` go before the element at `destination`
    /// (as `List.onMove` says it), then `arranged`. A place dropped above a pinned
    /// one becomes pinned; one that is pinned and dropped below an unpinned one
    /// becomes unpinned (a drop between the groups changes nothing). The following
    /// place is not counted: it is always first.
    static func moving(_ places: [AppPlace], from source: IndexSet, to destination: Int) -> [AppPlace] {
        let source = IndexSet(source.filter { places.indices.contains($0) })
        guard !source.isEmpty else { return arranged(places) }
        let ids = Set(source.map { places[$0].id })
        var list = places
        list.relocate(fromOffsets: source, toOffset: max(0, min(destination, places.count)))
        for id in ids {
            guard let i = list.firstIndex(where: { $0.id == id }), !list[i].follows else { continue }
            let counted: (AppPlace) -> Bool = { !$0.follows && !ids.contains($0.id) }
            if list[(i + 1)...].filter(counted).contains(where: \.pinned) {
                list[i].pinned = true
            } else if list[..<i].filter(counted).contains(where: { !$0.pinned }) {
                list[i].pinned = false
            }
        }
        return arranged(list)
    }

    /// `places` with the place `id` first after the following one (`toTop`) or
    /// last: moved as a drop there would.
    static func moving(_ places: [AppPlace], id: String, toTop: Bool) -> [AppPlace] {
        guard let i = places.firstIndex(where: { $0.id == id }) else { return places }
        return moving(places, from: IndexSet(integer: i), to: toTop ? 0 : places.count)
    }

    /// `places` without the fixed places within 1 km of the place `id`, which
    /// follows the phone and keeps its own id and name: it is the same spot, and
    /// two places would show the same forecast twice. The ones removed come second
    /// (their caches go with them).
    static func removingDuplicates(of id: String, in places: [AppPlace]) -> (kept: [AppPlace], removed: [AppPlace]) {
        guard let followed = places.first(where: { $0.id == id && $0.follows }) else { return (places, []) }
        let removed = places.filter { $0.id != id && !$0.follows && isDuplicate(followed, of: $0) }
        let ids = Set(removed.map(\.id))
        return (places.filter { !ids.contains($0.id) }, removed)
    }

    /// Whether a forecast fetched for `fetched` still belongs to the place with
    /// its id in `places`: the place is still saved and has not moved since (a
    /// following place moves while a fetch for its old coordinates runs).
    static func isCurrent(_ fetched: AppPlace, in places: [AppPlace]) -> Bool {
        places.contains {
            $0.id == fetched.id && $0.latitude == fetched.latitude && $0.longitude == fetched.longitude
        }
    }

    // MARK: Identity

    /// Whether `place` is `saved` again: within 1 km, whatever the names.
    static func isDuplicate(_ place: AppPlace, of saved: AppPlace) -> Bool {
        CLLocation(latitude: place.latitude, longitude: place.longitude)
            .distance(from: CLLocation(latitude: saved.latitude, longitude: saved.longitude)) <= 1_000
    }

    /// The cache file: the id with everything outside `[A-Za-z0-9_-]` as `_`.
    func cacheURL(in directory: URL) -> URL {
        let safe = String(
            String.UnicodeScalarView(
                id.unicodeScalars.map { s in
                    s.isASCII && (CharacterSet.alphanumerics.contains(s) || s == "_" || s == "-") ? s : "_"
                }))
        return directory.appending(path: "\(safe).json")
    }

    /// The cache file before ids: named after the place (a `/` made it a sub-path).
    func legacyCacheURL(in directory: URL) -> URL {
        directory.appending(path: "\(name).json")
    }

    /// The cache file that exists: the id's, else the legacy one; `nil` with neither.
    func existingCacheURL(in directory: URL) -> URL? {
        [cacheURL(in: directory), legacyCacheURL(in: directory)].first {
            FileManager.default.fileExists(atPath: $0.path(percentEncoded: false))
        }
    }

    /// The core's place: the zone's offset now and its changes over the forecast.
    func corePlace(now: Date = .now) -> Place {
        let zone = TimeZone(identifier: timeZone) ?? .current
        var changes: [OffsetChange] = []
        var cursor = now
        let end = now.addingTimeInterval(17 * 86_400)
        while let next = zone.nextDaylightSavingTimeTransition(after: cursor), next <= end {
            changes.append(
                OffsetChange(
                    at: Int64(next.timeIntervalSince1970),
                    offsetSeconds: Int32(zone.secondsFromGMT(for: next))))
            cursor = next
        }
        return Place(
            name: name, latitude: latitude, longitude: longitude, units: nil,
            utcOffsetSeconds: Int32(zone.secondsFromGMT(for: now)), offsetChanges: changes)
    }
}

extension Settings {
    /// The core's settings from the phone's region: units from its measurement
    /// system, the clock from its time format.
    static func phone(locale: Locale = .current) -> Settings {
        var settings = defaultSettings()
        settings.units = locale.measurementSystem == .us ? .imperial : .metric
        let clock = DateFormatter.dateFormat(fromTemplate: "j", options: 0, locale: locale) ?? ""
        settings.clock24h = !clock.contains("a")
        settings.language = locale.language.languageCode?.identifier ?? "en"
        if let language = locale.language.languageCode?.identifier,
            let region = locale.region?.identifier
        {
            settings.locale = "\(language)_\(region)"
        }
        return settings
    }
}

extension CLLocationCoordinate2D {
    /// Rounded to `decimals` places, half away from zero.
    func rounded(decimals: Int = AppPlace.outgoingDecimals) -> CLLocationCoordinate2D {
        let scale = pow(10.0, Double(decimals))
        return CLLocationCoordinate2D(
            latitude: (latitude * scale).rounded() / scale,
            longitude: (longitude * scale).rounded() / scale)
    }
}

extension CLLocation {
    /// The fix as a location to hand to a system service that sends it to Apple
    /// (the geocoder), rounded to `AppPlace.outgoingDecimals`.
    var outgoing: CLLocation {
        let c = coordinate.rounded()
        return CLLocation(latitude: c.latitude, longitude: c.longitude)
    }
}
