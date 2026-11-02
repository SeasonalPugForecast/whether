import Foundation
import WhetherKit

/// The phone's weather sources as an ordered list: which providers there
/// are, what each covers and adds, and the rules for the saved order. Pure logic,
/// shared by the Sources screen, Settings and the tests; the core builds the chain.
enum Sources {
    /// Sources whose terms forbid fetching while the app is not in use (MET Norway: "must not
    /// retrieve new data" then), so a widget or a background refresh leaves them out.
    static let foregroundOnly: Set<String> = [metNorway]

    static let nws = "nws"
    static let models = "open-meteo-models"
    static let openWeather = "openweather"
    static let pirateWeather = "pirate-weather"
    static let openMeteo = "open-meteo"
    static let metNorway = "met-norway"

    /// What the screen says about one provider.
    struct Info: Equatable, Identifiable {
        var id: String
        /// Short name, for the order summary.
        var name: String
        /// Where it has data.
        var covers: String
        /// What it brings.
        var adds: String
        /// Whether it needs the user's own key.
        var keyed = false
        /// Where to get a key.
        var signUp: URL?
    }

    /// Every provider this build knows. Adding a provider is one more entry here,
    /// one more id in the core, and its detail page.
    static let catalog: [Info] = [
        Info(
            id: nws, name: "NWS", covers: "US only",
            adds: "official alerts, the hourly grid, station conditions"),
        Info(
            id: models, name: "Forecast models", covers: "worldwide",
            adds: "the weather models you choose, each value from the first that has it"),
        Info(
            id: openWeather, name: "OpenWeather", covers: "worldwide",
            adds: "forecasts anywhere; with One Call, official alerts",
            keyed: true, signUp: URL(string: "https://home.openweathermap.org/users/sign_up")),
        Info(
            id: pirateWeather, name: "Pirate Weather", covers: "worldwide",
            adds: "a second forecast with the next hour of rain and alerts; 10,000 free calls a month",
            keyed: true, signUp: URL(string: "https://pirate-weather.apiable.io/")),
        Info(
            id: openMeteo, name: "Open-Meteo", covers: "worldwide",
            adds: "fills the gaps: UV, moon, air quality, the next hour"),
        Info(
            id: metNorway, name: "MET Norway", covers: "worldwide",
            adds: "a last-resort forecast, without UV or air quality"),
    ]

    static func info(_ id: String) -> Info? { catalog.first { $0.id == id } }

    /// The rows the user can move: the others sit last, in this order.
    static let movable = [nws, models, openWeather, pirateWeather]
    /// The providers that need a key of their own.
    static var keyedProviders: [String] { catalog.filter(\.keyed).map(\.id) }

    /// `rows` as the screen and the core want them: each provider once (the first
    /// row counts), the movable ones first in their order (a movable provider this
    /// build gained is added at the end, as the default has it), then ids from a
    /// newer build (kept, so they survive a downgrade), then Open-Meteo, always on,
    /// then MET Norway.
    static func complete(_ rows: [Preferences.SourceRow]) -> [Preferences.SourceRow] {
        var seen = Set<String>()
        let unique = rows.filter { seen.insert($0.provider).inserted }
        let defaults = Preferences.defaultSources
        let head = unique.filter { movable.contains($0.provider) }
        let added = defaults.filter { movable.contains($0.provider) && !seen.contains($0.provider) }
        let unknown = unique.filter { info($0.provider) == nil }
        let met = unique.first { $0.provider == metNorway } ?? Preferences.SourceRow(provider: metNorway)
        return head + added + unknown + [Preferences.SourceRow(provider: openMeteo), met]
    }

    /// The rows of the old switches, in today's order (a saved list wins over these).
    /// A non-empty `models` turns the models row on; OpenWeather stays on, since it
    /// and Pirate Weather serve only with a key, as before it existed.
    static func migrated(nws nwsOn: Bool, models modelList: [String], metNorway met: Bool)
        -> [Preferences.SourceRow]
    {
        [
            Preferences.SourceRow(provider: nws, enabled: nwsOn),
            Preferences.SourceRow(provider: models, enabled: !modelList.isEmpty),
            Preferences.SourceRow(provider: openWeather),
            Preferences.SourceRow(provider: pirateWeather),
            Preferences.SourceRow(provider: openMeteo),
            Preferences.SourceRow(provider: metNorway, enabled: met),
        ]
    }

    /// The movable rows in the order shown (those this build knows).
    static func orderable(_ rows: [Preferences.SourceRow]) -> [Preferences.SourceRow] {
        rows.filter { movable.contains($0.provider) }
    }

    /// Moves movable rows as a list of those only would; the rest keep their place.
    static func move(_ rows: inout [Preferences.SourceRow], fromOffsets source: IndexSet, toOffset destination: Int) {
        var head = orderable(rows)
        head.relocate(fromOffsets: source, toOffset: destination)
        rows = head + rows.filter { !movable.contains($0.provider) }
    }

    /// What a status line depends on.
    struct Facts {
        var hasKey: [String: Bool] = [:]
        var oneCall = false
        var callsToday: UInt32 = 0
        /// A keyed provider's month count in the core's words ("12 of 9000 calls used this month").
        var monthUsage: [String: String] = [:]
        var dailyCalls = 0
        var models: [String] = []
        /// The preset the models match, if any.
        var presetName: String?
        /// Whether `apiContact()` gave one (MET Norway needs it).
        var hasContact = false
    }

    /// Whether `row` would serve at all.
    static func serves(_ row: Preferences.SourceRow, _ facts: Facts) -> Bool {
        guard row.enabled else { return false }
        switch row.provider {
        case models: return !facts.models.isEmpty
        case openWeather: return facts.hasKey[openWeather] == true
        case pirateWeather: return facts.hasKey[pirateWeather] == true
        case metNorway: return facts.hasContact
        default: return info(row.provider) != nil
        }
    }

    /// The line under a row's name: why it serves or does not.
    static func status(_ row: Preferences.SourceRow, _ facts: Facts) -> String {
        switch row.provider {
        case openMeteo: return "Always on, and always last"
        case metNorway:
            if !facts.hasContact { return "Needs the app's contact address" }
            return row.enabled ? "On, after Open-Meteo" : "Off"
        default: break
        }
        guard row.enabled else { return "Off" }
        switch row.provider {
        case nws: return "On for US places"
        case models:
            if facts.models.isEmpty { return "No models chosen: Open-Meteo's best match serves" }
            return facts.presetName ?? facts.models.joined(separator: " → ")
        case openWeather:
            if facts.hasKey[openWeather] != true { return "Needs a key" }
            if facts.oneCall {
                return oneCallUsageText(used: facts.callsToday, limit: UInt32(clamping: facts.dailyCalls))
            }
            return "Key saved"
        case pirateWeather:
            if facts.hasKey[pirateWeather] != true { return "Needs a key" }
            return facts.monthUsage[pirateWeather] ?? "Key saved"
        default: return "Unknown source"
        }
    }

    /// The order in words, for Settings' footer: the sources that would serve, in turn.
    static func summary(_ rows: [Preferences.SourceRow], _ facts: Facts) -> String {
        var names = orderable(rows).filter { serves($0, facts) }.compactMap { info($0.provider)?.name }
        names.append(info(openMeteo)?.name ?? "Open-Meteo")
        if let met = rows.first(where: { $0.provider == metNorway }), serves(met, facts) {
            names.append(info(metNorway)?.name ?? "MET Norway")
        }
        return names.joined(separator: " → ")
    }
}

extension Preferences {
    /// One source in the order, and whether it may serve.
    struct SourceRow: Codable, Hashable, Identifiable {
        var provider: String
        var enabled = true
        var id: String { provider }
    }

    /// Today's order (`defaultSources()` from the core): the NWS off, the rest on.
    static var defaultSources: [SourceRow] {
        WhetherKit.defaultSources().map { SourceRow(provider: $0.provider, enabled: $0.enabled) }
    }

    /// The saved list `complete`d; see `Sources.complete`.
    static func complete(_ rows: [SourceRow]) -> [SourceRow] { Sources.complete(rows) }

    /// The list for preferences saved before it (see `Sources.migrated`).
    static func migratedSources(nws: Bool, models: [String], metNorway: Bool) -> [SourceRow] {
        Sources.migrated(nws: nws, models: models, metNorway: metNorway)
    }

    /// Chooses the forecast models. A list turns the models row on and none
    /// ("Best match") turns it off, as the models page does and a desktop
    /// `config.toml` import must (the core ignores models while the row is off).
    mutating func setModels(_ list: [String]) {
        models = list
        set(Sources.models, enabled: !list.isEmpty)
    }

    /// Switches One Call, which needs a saved key (`hasKey`). Turning it on also
    /// turns the OpenWeather row on, as saving a first key does: One Call is
    /// ignored while that source is off.
    mutating func setOneCall(_ on: Bool, hasKey: Bool) {
        oneCall = on && hasKey
        if oneCall { set(Sources.openWeather, enabled: true) }
    }

    /// US forecasts from the National Weather Service first: its row.
    var nws: Bool {
        get { enabled(Sources.nws) }
        set { set(Sources.nws, enabled: newValue) }
    }

    /// MET Norway as the last resort when Open-Meteo fails (needs `apiContact()`): its row.
    var metNorway: Bool {
        get { enabled(Sources.metNorway) }
        set { set(Sources.metNorway, enabled: newValue) }
    }

    /// What the status lines and the summary depend on, as things stand (reads the
    /// Keychain for each keyed provider).
    func sourceFacts(callsToday: UInt32 = 0) -> Sources.Facts {
        var facts = Sources.Facts()
        for provider in Sources.keyedProviders { facts.hasKey[provider] = Keychain.key(for: provider) != nil }
        facts.oneCall = oneCall
        facts.callsToday = callsToday
        facts.dailyCalls = dailyCalls
        facts.models = models
        facts.presetName = modelPresets().first { $0.models == models }?.name
        facts.hasContact = apiContact() != nil
        return facts
    }

    /// Whether the row for `provider` is on.
    func enabled(_ provider: String) -> Bool {
        sources.first { $0.provider == provider }?.enabled ?? false
    }

    /// Switches the row for `provider` (added at its place when the list lacks it).
    mutating func set(_ provider: String, enabled: Bool) {
        if let i = sources.firstIndex(where: { $0.provider == provider }) {
            sources[i].enabled = enabled
        } else {
            sources = Sources.complete(sources + [SourceRow(provider: provider, enabled: enabled)])
        }
    }
}

extension Settings {
    /// These settings for a fetch made while the app is not in use (a widget, a background
    /// refresh): the foreground-only sources are switched off.
    func forBackground() -> Settings {
        var copy = self
        copy.metNorway = false
        copy.sources = sources.map {
            Sources.foregroundOnly.contains($0.provider)
                ? SourceChoice(provider: $0.provider, enabled: false) : $0
        }
        return copy
    }
}
