import Foundation
import WhetherKit

/// The app's own settings, remembered between launches.
struct Preferences: Codable, Equatable {
    enum UnitChoice: String, Codable, CaseIterable, Identifiable {
        case automatic, imperial, metric
        var id: Self { self }
    }

    /// Units chosen one quantity at a time, over the bundle: each is the case name
    /// of the core's unit (`String(describing:)`, "celsius", "mph"), or nil for
    /// the bundle's own ("Automatic"). The core lists the units
    /// (`unitLabels()`); a name it no longer knows reads as Automatic.
    struct UnitPicks: Codable, Equatable {
        var temperature: String?
        var wind: String?
        var pressure: String?
        var precipitation: String?
        var distance: String?

        init() {}

        /// The picks for the core's overrides (a desktop config's `[units]`).
        init(_ o: UnitOverrides) {
            temperature = o.temperature.map { String(describing: $0) }
            wind = o.wind.map { String(describing: $0) }
            pressure = o.pressure.map { String(describing: $0) }
            precipitation = o.precipitation.map { String(describing: $0) }
            distance = o.distance.map { String(describing: $0) }
        }

        /// Saved picks; a quantity an older version lacks stays Automatic.
        init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            temperature = try? c.decodeIfPresent(String.self, forKey: .temperature)
            wind = try? c.decodeIfPresent(String.self, forKey: .wind)
            pressure = try? c.decodeIfPresent(String.self, forKey: .pressure)
            precipitation = try? c.decodeIfPresent(String.self, forKey: .precipitation)
            distance = try? c.decodeIfPresent(String.self, forKey: .distance)
        }

        /// Whether every quantity follows the bundle.
        var isAutomatic: Bool { self == UnitPicks() }

        /// The core's overrides for these picks.
        func overrides(_ labels: UnitLabels = unitLabels()) -> UnitOverrides {
            func unit<T>(_ pick: String?, _ options: [(unit: T, label: String)]) -> T? {
                options.first { String(describing: $0.unit) == pick }?.unit
            }
            return UnitOverrides(
                temperature: unit(temperature, labels.temperature.map { ($0.unit, $0.label) }),
                wind: unit(wind, labels.wind.map { ($0.unit, $0.label) }),
                pressure: unit(pressure, labels.pressure.map { ($0.unit, $0.label) }),
                precipitation: unit(precipitation, labels.precipitation.map { ($0.unit, $0.label) }),
                distance: unit(distance, labels.distance.map { ($0.unit, $0.label) }))
        }

        /// The symbols of the units picked ("°C, mph"), in the screen's order; empty when none.
        func summary(_ labels: UnitLabels = unitLabels()) -> String {
            func label<T>(_ pick: String?, _ options: [(unit: T, label: String)]) -> String? {
                options.first { String(describing: $0.unit) == pick }?.label
            }
            return [
                label(temperature, labels.temperature.map { ($0.unit, $0.label) }),
                label(wind, labels.wind.map { ($0.unit, $0.label) }),
                label(pressure, labels.pressure.map { ($0.unit, $0.label) }),
                label(precipitation, labels.precipitation.map { ($0.unit, $0.label) }),
                label(distance, labels.distance.map { ($0.unit, $0.label) }),
            ].compactMap { $0 }.joined(separator: ", ")
        }
    }

    enum ClockChoice: String, Codable, CaseIterable, Identifiable {
        case automatic, twelve, twentyFour
        var id: Self { self }
    }

    /// How a date reads (Settings → Dates): the core's `date_format` styles.
    enum DateChoice: String, Codable, CaseIterable, Identifiable {
        case monthNameDay, dayMonthName, compact, monthDay, monthDayPadded
        case dayMonth, dayMonthPadded, dayMonthDots, iso
        var id: Self { self }

        /// How 1 November reads in it (English month names; the core writes
        /// them in the app's language).
        var example: String {
            switch self {
            case .monthNameDay: "Nov 1"
            case .dayMonthName: "1 Nov"
            case .compact: "NOV1"
            case .monthDay: "11/1"
            case .monthDayPadded: "11/01"
            case .dayMonth: "1/11"
            case .dayMonthPadded: "01/11"
            case .dayMonthDots: "01.11."
            case .iso: "2026-11-01"
            }
        }

        var style: DateStyle {
            switch self {
            case .monthNameDay: .monthNameDay
            case .dayMonthName: .dayMonthName
            case .compact: .compact
            case .monthDay: .monthDay
            case .monthDayPadded: .monthDayPadded
            case .dayMonth: .dayMonth
            case .dayMonthPadded: .dayMonthPadded
            case .dayMonthDots: .dayMonthDots
            case .iso: .iso
            }
        }

        init(_ style: DateStyle) {
            self = Self.allCases.first { $0.style == style } ?? .monthNameDay
        }
    }

    /// A section of the Now tab.
    enum OverviewSection: String, Codable, CaseIterable, Identifiable {
        case now, precipitation, alerts, hours, days, radar, details, sun, history
        var id: Self { self }

        /// Its symbol, for the editing tiles.
        var symbol: String {
            switch self {
            case .now: "thermometer.sun"
            case .precipitation: "cloud.rain"
            case .alerts: "exclamationmark.triangle"
            case .hours: "clock"
            case .days: "calendar"
            case .radar: "globe.americas"
            case .details: "square.grid.2x2"
            case .sun: "sun.horizon"
            case .history: "clock.arrow.circlepath"
            }
        }

        var title: String {
            switch self {
            case .now: "Current conditions"
            case .precipitation: "Precipitation"
            case .alerts: "Alerts"
            case .hours: "Next 24 hours"
            case .days: "Next days"
            case .radar: "Radar"
            case .details: "Details"
            case .sun: "Sun & moon"
            case .history: "History"
            }
        }
    }

    /// One section's place and whether it shows.
    struct SectionChoice: Codable, Hashable, Identifiable {
        var section: OverviewSection
        var shown = true
        var id: OverviewSection { section }
    }

    /// A tile of the hour card (Hourly tab).
    enum HourTile: String, Codable, CaseIterable, Identifiable {
        case feelsLike, rain, wind, gusts, humidity, dewPoint, uv, pressure, visibility, cloudCover
        var id: Self { self }

        var title: String {
            switch self {
            case .feelsLike: "Feels like"
            case .rain: "Rain"
            case .wind: "Wind"
            case .gusts: "Gusts"
            case .humidity: "Humidity"
            case .dewPoint: "Dew point"
            case .uv: "UV index"
            case .pressure: "Pressure"
            case .visibility: "Visibility"
            case .cloudCover: "Cloud cover"
            }
        }

        var symbol: String {
            switch self {
            case .feelsLike: "thermometer.medium"
            case .rain: "drop.fill"
            case .wind: "wind"
            case .gusts: "tornado"
            case .humidity: "humidity"
            case .dewPoint: "drop"
            case .uv: "sun.max"
            case .pressure: "gauge.with.dots.needle.33percent"
            case .visibility: "eye"
            case .cloudCover: "cloud"
            }
        }
    }

    /// One hour tile's place and whether it shows.
    struct HourTileChoice: Codable, Hashable, Identifiable {
        var tile: HourTile
        var shown = true
        var id: HourTile { tile }
    }

    static let defaultHourTiles = HourTile.allCases.map { HourTileChoice(tile: $0) }

    /// `tiles` with each once, new ones added at the end, shown.
    static func complete(_ tiles: [HourTileChoice]) -> [HourTileChoice] {
        var seen = Set<HourTile>()
        var out = tiles.filter { seen.insert($0.tile).inserted }
        out += HourTile.allCases.filter { !seen.contains($0) }.map { HourTileChoice(tile: $0) }
        return out
    }

    /// A Now → Details card, by the core's kind ("wind", "gusts" …).
    struct DetailChoice: Codable, Hashable, Identifiable {
        var kind: String
        var shown = true
        var id: String { kind }
    }

    /// The detail cards in their default order; feels like starts hidden.
    static let defaultDetails: [DetailChoice] =
        [
            "wind", "gusts", "humidity", "pressure", "uv", "visibility", "dew_point", "cloud_cover",
            "precipitation", "normals", "records",
        ].map { DetailChoice(kind: $0) } + [DetailChoice(kind: "feels_like", shown: false)]

    /// `details` with each once, new ones added from the defaults.
    static func complete(_ details: [DetailChoice]) -> [DetailChoice] {
        var seen = Set<String>()
        var out = details.filter { seen.insert($0.kind).inserted }
        out += defaultDetails.filter { !seen.contains($0.kind) }
        return out
    }

    /// The sections the Now tab offers. Sun & moon moved to the Hourly tab; its
    /// case stays so lists saved before still decode.
    static let offeredSections = OverviewSection.allCases.filter { $0 != .sun }

    /// The default order, every section shown but the radar (hidden, so at the end).
    static let defaultSections =
        offeredSections.filter { $0 != .radar }.map { SectionChoice(section: $0) }
        + [SectionChoice(section: .radar, shown: false)]

    /// `sections` with each offered section once, the ones it lacks (new in an
    /// update) added at the end, shown.
    static func complete(_ sections: [SectionChoice]) -> [SectionChoice] {
        var seen = Set<OverviewSection>()
        var out = sections.filter { offeredSections.contains($0.section) && seen.insert($0.section).inserted }
        out += offeredSections.filter { !seen.contains($0) }.map { SectionChoice(section: $0) }
        // Saved before alerts had to show.
        for i in out.indices where !hideable(out[i].section) {
            out[i].shown = true
        }
        return out
    }

    /// The home screen icon: the season's pug, or one season's always.
    enum AppIconChoice: String, Codable, CaseIterable, Identifiable {
        case seasonal, spring, summer, fall, winter
        var id: Self { self }

        var title: String {
            switch self {
            case .seasonal: "By the season"
            case .spring: "Spring"
            case .summer: "Summer"
            case .fall: "Fall"
            case .winter: "Winter"
            }
        }
    }

    enum LayoutChoice: String, Codable, CaseIterable, Identifiable {
        case darkSky, centered
        var id: Self { self }
    }

    enum BackgroundChoice: String, Codable, CaseIterable, Identifiable {
        case sky, plain
        var id: Self { self }
    }

    /// Patterns and symbols beside the colors that carry meaning (WCAG 1.4.1):
    /// following iOS's Differentiate Without Color, always, or never.
    enum PatternsChoice: String, Codable, CaseIterable, Identifiable {
        case automatic, on, off
        var id: Self { self }

        var title: String {
            switch self {
            case .automatic: "Automatic"
            case .on: "On"
            case .off: "Off"
            }
        }
    }

    /// Light or dark: by the sun at the place shown (Dark Sky's way), the
    /// phone's setting, or fixed.
    enum AppearanceChoice: String, Codable, CaseIterable, Identifiable {
        case sun, system, light, dark
        var id: Self { self }
    }

    var units = UnitChoice.automatic
    /// Units picked one by one, over the choice above (Settings → Custom units).
    var unitOverrides = UnitPicks()
    var clock = ClockChoice.automatic
    var dates = DateChoice.monthNameDay
    var appearance = AppearanceChoice.sun
    /// The Dark Sky layout, or the centered one with glass cards.
    var layout = LayoutChoice.centered
    /// The Now tab's sections, in order (Settings → Sections).
    var sections = Preferences.defaultSections
    /// The weather's sky behind the forecast, or a plain background.
    var background = BackgroundChoice.sky
    /// The sun and moon crossing the Now tab's top as they cross the sky.
    var skyBodies = true
    /// The colors (Settings → Display → Colors): a preset's id (`ColorPresets.ids`)
    /// or a custom theme's; an id that is neither paints Whether.
    var colorTheme = ColorPresets.defaultID
    /// How the reader sees color: replaces the informational colors of any theme.
    var colorVision = ColorVision.typical
    /// Patterns and symbols beside informational colors.
    var patterns = PatternsChoice.automatic
    /// The reader's own color themes, each saved with only the roles that
    /// differ from its base preset; the base fills the rest when it is resolved.
    var customThemes: [ColorTheme] = []
    /// A Live Activity (lock screen, Dynamic Island) while rain is near.
    var rainActivity = true
    var days = 8
    var precipHours = 12
    /// Hours of the next-hours rain graph: 1, 2, 4 or 8.
    var nowcastHours = 2
    /// Hours the Now tab's precipitation panel covers (up to 8 the graph,
    /// beyond the table).
    var rainSpan = 2
    var daylightIcons = true
    /// Lead with the "feels like" temperatures (highs and lows too).
    var feelsLike = true
    /// One Call 4.0 (metered: the phone's share of 1,000 calls a day).
    var oneCall = false
    var dailyCalls = Int(defaultSettings().dailyCallLimit)
    /// Pirate Weather calls this phone may make a month (the free key allows 10,000).
    var pirateWeatherCalls = Int(defaultSettings().pirateWeatherMonthlyCalls)
    var fillGaps = true
    /// The next hour minute by minute from Apple Weather (WeatherKit), where it has it.
    var appleMinutes = true
    /// Apple Weather's alerts (WeatherKit) for places outside the National Weather
    /// Service's coverage.
    var appleAlerts = true
    /// Open-Meteo weather models that forecast first, in priority order (a
    /// preset from `modelPresets()`); empty for none.
    var models: [String] = []
    /// Notify about new or worse weather alerts (they come with One Call).
    var alertNotifications = true
    /// Say when someone changes a plan shared with this person: a banner in the app, and a
    /// notification when it is in the background.
    var planChangeNotices = true
    /// The sources, in the order they are tried: a row per provider and
    /// whether it may serve. `open-meteo` and `met-norway` always sit last.
    /// `nws` and `metNorway` below read and write their rows.
    var sources = Preferences.defaultSources
    /// History: the last days against normal, today's normals and records, and
    /// normals on the days ahead (fetched once a day).
    var history = true
    /// The hour card's tiles, in order (long-press a tile to edit).
    var hourTiles = Preferences.defaultHourTiles
    /// The hourly graph's measures, the first filled.
    var graphMeasures = ["temperature"]
    /// The Now tab's 24 hours: the measures shown per hour, a row each
    /// (`GraphMeasure` raw values: temperature, humidity, wind, gusts …).
    var hourRows = ["temperature"]
    /// The Now tab's detail cards, in order (touch and hold one to edit).
    var detailCards = Preferences.defaultDetails
    /// The home screen icon.
    var appIcon = AppIconChoice.seasonal
    /// How often the forecast refreshes, in minutes (one of `refreshChoices`).
    var refreshMinutes = Preferences.defaultRefreshMinutes
    /// The Sun & moon card's items, in order (touch and hold the card to edit).
    var sunItems = SunItemChoice.defaults
    /// The favourite place (a star in the place list), by `AppPlace.id`; `nil` for none.
    var favouritePlaceID: String?
    /// Which place the app opens on (the widget shows the favourite when it has no choice of its own).
    var openOn = OpenOn.lastViewed
    /// What each day row shows, in order (touch and hold a day to edit).
    var dayRowItems = DayRowChoice.defaults
    /// What an opened day shows, in order.
    var dayDetailItems = DayDetailChoice.defaults
    /// How the Hourly graph labels its values and scales its lines.
    var graphOptions = GraphOptions()
    /// The radar's map style, loop speed, opacity and how far in it opens.
    var radar = RadarOptions()
    /// The tabs after Now, in order (Settings → Tabs).
    var tabs = TabChoice.defaults
    /// The Hourly tab's cards, in order (touch and hold one to edit).
    var hourlyCards = HourlyCardChoice.defaults
    /// The Days tab's cards, in order.
    var daysCards = DaysCardChoice.defaults

    static let refreshChoices = [15, 30, 60]
    static let defaultRefreshMinutes = 15

    /// Seconds after which the forecast shown is fetched again when the app opens (not when the core calls it out of date: that is twice the interval):
    /// two thirds of the refresh interval (10 minutes at the default 15).
    var fetchDueAfter: TimeInterval { Double(refreshMinutes) * 60 * 2 / 3 }

    /// The earliest the system should run the next background refresh, in seconds.
    var backgroundInterval: TimeInterval { Double(refreshMinutes) * 60 }

    /// Whether a section can be hidden: the current conditions always show,
    /// and so do alerts (since the Alerts tab gave way to the Plan tab, nowhere
    /// else shows them).
    static func hideable(_ section: OverviewSection) -> Bool {
        section != .now && section != .alerts
    }

    /// Hides `section` (unless it must show), after every other section, so
    /// the shown ones stay first.
    mutating func hide(_ section: OverviewSection) {
        guard Self.hideable(section), let i = sections.firstIndex(where: { $0.section == section }) else { return }
        var choice = sections.remove(at: i)
        choice.shown = false
        sections.append(choice)
    }

    /// Shows `section` again, after the shown ones.
    mutating func show(_ section: OverviewSection) {
        guard let i = sections.firstIndex(where: { $0.section == section }) else { return }
        var choice = sections.remove(at: i)
        choice.shown = true
        let last = sections.lastIndex(where: \.shown).map { $0 + 1 } ?? 0
        sections.insert(choice, at: last)
    }

    /// Moves shown sections as a list of the shown ones only would.
    mutating func moveShown(from source: IndexSet, to destination: Int) {
        var shown = sections.filter(\.shown)
        shown.relocate(fromOffsets: source, toOffset: destination)
        sections = shown + sections.filter { !$0.shown }
    }

    private static let key = "preferences"

    /// A saved custom theme, or `nil` when it doesn't decode.
    private struct LossyTheme: Decodable {
        var theme: ColorTheme?
        init(from decoder: Decoder) throws { theme = try? ColorTheme(from: decoder) }
    }

    /// Keys of fields older versions saved and this one replaced.
    private enum LegacyKeys: String, CodingKey {
        /// The "NOAA blend for US forecasts" switch, before model lists.
        case noaaBlend
        /// The switches that became rows of `sources`.
        case nws, metNorway
    }

    init() {}

    /// Saved preferences; a field saved by an older version that lacks it keeps
    /// its default (rather than losing every setting).
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let d = Preferences()
        func value<T: Decodable>(_ key: CodingKeys, _ fallback: T) -> T {
            (try? c.decodeIfPresent(T.self, forKey: key)) ?? fallback
        }
        units = value(.units, d.units)
        unitOverrides = value(.unitOverrides, d.unitOverrides)
        clock = value(.clock, d.clock)
        dates = value(.dates, d.dates)
        appearance = value(.appearance, d.appearance)
        layout = value(.layout, d.layout)
        sections = Self.complete(value(.sections, d.sections))
        background = value(.background, d.background)
        skyBodies = value(.skyBodies, d.skyBodies)
        colorTheme = value(.colorTheme, d.colorTheme)
        colorVision = value(.colorVision, d.colorVision)
        patterns = value(.patterns, d.patterns)
        // One theme that doesn't decode drops alone, not the whole list.
        customThemes = CustomThemes.loaded(value(.customThemes, [LossyTheme]()).compactMap(\.theme))
        rainActivity = value(.rainActivity, d.rainActivity)
        days = value(.days, d.days)
        precipHours = value(.precipHours, d.precipHours)
        nowcastHours = value(.nowcastHours, d.nowcastHours)
        rainSpan = value(.rainSpan, d.rainSpan)
        daylightIcons = value(.daylightIcons, d.daylightIcons)
        feelsLike = value(.feelsLike, d.feelsLike)
        oneCall = value(.oneCall, d.oneCall)
        dailyCalls = value(.dailyCalls, d.dailyCalls)
        pirateWeatherCalls = value(.pirateWeatherCalls, d.pirateWeatherCalls)
        fillGaps = value(.fillGaps, d.fillGaps)
        appleMinutes = value(.appleMinutes, d.appleMinutes)
        appleAlerts = value(.appleAlerts, d.appleAlerts)
        let legacy = try decoder.container(keyedBy: LegacyKeys.self)
        let blend = try? legacy.decodeIfPresent(Bool.self, forKey: .noaaBlend)
        models = value(.models, blend.map { modelsFromNoaaBlend(noaaBlend: $0) } ?? d.models)
        alertNotifications = value(.alertNotifications, d.alertNotifications)
        planChangeNotices = value(.planChangeNotices, d.planChangeNotices)
        // Saved before the ordered list: the old switches become its rows, so the
        // chain stays as it was. Once saved, the list is authoritative.
        let saved: [SourceRow] = value(.sources, [])
        sources =
            saved.isEmpty
            ? Self.migratedSources(
                nws: (try? legacy.decodeIfPresent(Bool.self, forKey: .nws)) ?? false,
                models: models,
                metNorway: (try? legacy.decodeIfPresent(Bool.self, forKey: .metNorway)) ?? true)
            : Self.complete(saved)
        history = value(.history, d.history)
        hourTiles = Self.complete(value(.hourTiles, d.hourTiles))
        graphMeasures = value(.graphMeasures, d.graphMeasures)
        hourRows = value(.hourRows, d.hourRows)
        detailCards = Self.complete(value(.detailCards, d.detailCards))
        appIcon = value(.appIcon, d.appIcon)
        refreshMinutes = value(.refreshMinutes, d.refreshMinutes)
        sunItems = SunItemChoice.complete(value(.sunItems, [SunItemChoice.Lossy]()).compactMap(\.choice))
        favouritePlaceID = value(.favouritePlaceID, d.favouritePlaceID)
        openOn = value(.openOn, d.openOn)
        dayRowItems = DayRowChoice.complete(value(.dayRowItems, [DayRowChoice.Lossy]()).compactMap(\.choice))
        dayDetailItems = DayDetailChoice.complete(
            value(.dayDetailItems, [DayDetailChoice.Lossy]()).compactMap(\.choice))
        graphOptions = value(.graphOptions, d.graphOptions)
        radar = value(.radar, d.radar)
        tabs = TabChoice.complete(value(.tabs, [TabChoice.Lossy]()).compactMap(\.choice))
        hourlyCards = HourlyCardChoice.complete(
            value(.hourlyCards, [HourlyCardChoice.Lossy]()).compactMap(\.choice))
        daysCards = DaysCardChoice.complete(value(.daysCards, [DaysCardChoice.Lossy]()).compactMap(\.choice))
    }

    static func load() -> Preferences {
        guard let data = SharedContainer.defaults.data(forKey: key),
            let prefs = try? JSONDecoder().decode(Preferences.self, from: data)
        else { return Preferences() }
        return prefs
    }

    func save() {
        #if DEBUG
            // Screenshots: `-units uk` is for one launch; saving it would carry
            // °C into every later shot.
            // `-skyTime` shots set the layout and background for one launch too, and
            // `-colors aurora` the colors, and `-vision red_green` and `-patterns on` the
            // color vision and patterns.
            let args = ProcessInfo.processInfo.arguments
            // `-editTheme aurora` and `-customTheme aurora` make a custom theme for one launch,
            // and `-dates compact` the dates.
            if ["-units", "-skyTime", "-colors", "-vision", "-patterns", "-editTheme", "-customTheme", "-dates"]
                .contains(where: args.contains)
            {
                return
            }
        #endif
        if let data = try? JSONEncoder().encode(self) {
            SharedContainer.defaults.set(data, forKey: Self.key)
        }
    }

    /// The core's settings: these choices over the phone's region, with the key
    /// from the Keychain (`withKey: false` skips reading it, to compare).
    func settings(locale: Locale = .current, withKey: Bool = true) -> Settings {
        var s = Settings.phone(locale: locale)
        switch units {
        case .automatic: break
        case .imperial: s.units = .imperial
        case .metric: s.units = .metric
        }
        s.unitOverrides = unitOverrides.overrides()
        switch clock {
        case .automatic: break
        case .twelve: s.clock24h = false
        case .twentyFour: s.clock24h = true
        }
        s.dateStyle = dates.style
        s.dailyDays = UInt8(clamping: days)
        s.precipHours = UInt8(clamping: precipHours)
        s.nowcastHours = UInt8(clamping: nowcastHours)
        s.daylightIcons = daylightIcons
        s.feelsLike = feelsLike
        // The core marks data out of date after twice the interval.
        s.refreshMinutes = UInt32(clamping: refreshMinutes)
        // The keys come from the Keychain, one per keyed provider; the legacy
        // `openweatherKey` carries the same OpenWeather key.
        s.keys = withKey ? Keychain.keys(for: Sources.keyedProviders) : [:]
        s.openweatherKey = s.keys[Sources.openWeather]
        s.oneCall = oneCall
        s.dailyCallLimit = UInt32(clamping: dailyCalls)
        s.pirateWeatherMonthlyCalls = UInt32(clamping: pirateWeatherCalls)
        s.fillGaps = fillGaps
        s.models = models
        s.sources = sources.map { SourceChoice(provider: $0.provider, enabled: $0.enabled) }
        // The old switches, which the core ignores while the list is not empty.
        s.nws = nws
        s.metNorway = metNorway
        s.history = history
        // History is one day here: yesterday against today's normals (the
        // Plan tab looks further back).
        s.historyDays = 1
        return s
    }
}

extension Preferences {
    /// The color theme to paint with: the chosen preset (Seasons in `now`'s
    /// month) or custom theme (completed from its base), Whether for an id it doesn't know, with the color vision's ramps in
    /// place of its informational colors. The one resolver the app, the widget and the
    /// Live Activity call.
    func colorTheme(now: Date = .now, calendar: Calendar = .current) -> ColorTheme {
        ColorPresets.resolve(
            id: colorTheme, custom: customThemes, vision: colorVision,
            month: calendar.component(.month, from: now))
    }

    /// Whether patterns and symbols show beside informational colors: always
    /// for a theme that wants them (Graphite) and with no color; otherwise as the
    /// setting says, Automatic following iOS's Differentiate Without Color.
    func showsPatterns(differentiateWithoutColor: Bool, theme: ColorTheme) -> Bool {
        if theme.patterns || colorVision == .noColor { return true }
        return switch patterns {
        case .automatic: differentiateWithoutColor
        case .on: true
        case .off: false
        }
    }
}
