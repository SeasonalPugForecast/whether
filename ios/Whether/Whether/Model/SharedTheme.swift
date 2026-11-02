import Foundation
import WhetherKit

/// Why a shared theme can't be used. `userMessage` shows `friendlyMessage` as the headline.
nonisolated enum SharedThemeError: LocalizedError, FriendlyError, Equatable, Sendable {
    /// Its `v` isn't one this app reads: a newer version of the app made it.
    case needsNewerVersion
    /// Not a theme (or far too large to be one).
    case unreadable
    /// The file couldn't be read at all (not a matter of what it holds).
    case unopenable

    var friendlyMessage: String {
        switch self {
        case .needsNewerVersion: "These colors need a newer version of the app."
        case .unreadable: "That isn't a color theme this app can read."
        case .unopenable: "That file couldn't be opened."
        }
    }

    var errorDescription: String? { friendlyMessage }
}

/// A custom color theme as it travels between people: a `.whethertheme` file.
/// Version 1 is JSON with `v`, `name`, `base` (the preset it was made from), `colors`
/// (only the roles that differ from the base, as a custom theme saves them:
/// `{"accent": "#FF66AA"}` or `{"light": …, "dark": …}`) and `patterns`. Not in it:
/// the theme's id, which each phone gives its own.
///
/// Reading one is careful: a newer version is refused, unknown roles and colors that
/// don't parse are dropped, every color is plain sRGB hex (no system colors, which only
/// the Whether preset paints), an unknown base becomes Whether, and the name is
/// trimmed and capped.
nonisolated struct SharedTheme: Hashable, Sendable {
    /// The payload version this app reads and writes.
    static let version = 1
    /// The file's extension.
    static let fileExtension = "whethertheme"
    /// The longest name kept from a file.
    static let longestName = 60
    /// A file larger than this isn't a theme (every role twice is a few kilobytes).
    static let largestFile = 64 * 1024

    var name: String
    /// The preset that fills the roles it doesn't set.
    var base: String
    /// Only the roles that differ from the base.
    var colors: [ColorRole: ThemeColor]
    /// Patterns and symbols always (Graphite and themes made from it).
    var patterns: Bool

    init(name: String, base: String, colors: [ColorRole: ThemeColor], patterns: Bool = false) {
        self.name = Self.cleanName(name)
        self.base = ColorPresets.ids.contains(base) ? base : ColorPresets.defaultID
        self.colors = colors.mapValues(Self.plain)
        self.patterns = patterns
    }

    /// A custom theme, to send: its name, base, saved colors and patterns.
    init(_ theme: ColorTheme) {
        self.init(name: theme.name, base: theme.base, colors: theme.colors, patterns: theme.patterns)
    }

    /// The headline of the sheet that receives it.
    static let headline = "Colors were shared with you"

    // MARK: Receiving

    /// The base preset as it paints in `month` (Seasons follows the month).
    func basePreset(month: Int) -> ColorTheme {
        ColorPresets.preset(base, month: month) ?? ColorPresets.whether
    }

    /// The theme to add to this phone: a new custom id, a name no other custom theme
    /// has, and only the colors that differ from the base preset.
    func theme(
        among existing: [ColorTheme], month: Int,
        id: String = CustomThemes.idPrefix + UUID().uuidString.lowercased()
    ) -> ColorTheme {
        ColorTheme(
            id: id, name: CustomThemes.uniqueName(name, among: existing), base: base, colors: colors,
            patterns: patterns
        )
        .trimmed(against: basePreset(month: month))
    }

    /// A custom theme already here that paints just like this one (same base, colors and
    /// patterns, whatever its name), or `nil`.
    func match(among existing: [ColorTheme], month: Int) -> ColorTheme? {
        let mine = theme(among: [], month: month, id: "")
        return existing.first { other in
            other.base == mine.base && other.patterns == mine.patterns
                && other.trimmed(against: basePreset(month: month)).colors == mine.colors
        }
    }

    // MARK: File

    /// The file a theme travels as: its name made safe (as a plan's), and `.whethertheme`.
    @MainActor var fileName: String {
        let stem = SharedPlan.fileStem(name)
        return (stem == "Plan" && name != "Plan" ? "Colors" : stem) + "." + Self.fileExtension
    }

    /// Whether `url` is a theme file, by its extension.
    static func isThemeFile(_ url: URL) -> Bool {
        url.pathExtension.lowercased() == fileExtension
    }

    // MARK: Data

    /// The theme as the JSON that is shared: sorted keys, so the same theme is the same file.
    func data() throws -> Data {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        return try encoder.encode(self)
    }

    /// A theme from shared JSON. Throws `SharedThemeError`: a newer version, or
    /// something that isn't a theme.
    init(data: Data) throws {
        guard data.count <= Self.largestFile else { throw SharedThemeError.unreadable }
        do {
            self = try JSONDecoder().decode(Self.self, from: data)
        } catch let error as SharedThemeError {
            throw error
        } catch {
            throw SharedThemeError.unreadable
        }
    }

    /// `color` as plain sRGB: each component clamped and rounded to the 256 steps a
    /// theme saves, and no system color.
    private static func plain(_ color: ThemeColor) -> ThemeColor {
        func srgb(_ c: RGBA) -> RGBA { RGBA(hex: c.clamped.hex) ?? c.clamped }
        return ThemeColor(light: srgb(color.light), dark: color.dark.map(srgb))
    }

    /// A name from a file: trimmed, at most `longestName` characters, "Shared colors"
    /// when nothing is left.
    private static func cleanName(_ text: String) -> String {
        let blanks = CharacterSet.controlCharacters.union(.newlines)
        let flat = String(String.UnicodeScalarView(text.unicodeScalars.map { blanks.contains($0) ? " " : $0 }))
        let name = CustomThemes.cleanName(String(flat.prefix(longestName)))
        return name ?? "Shared colors"
    }
}

nonisolated extension SharedTheme: Codable {
    private enum Keys: String, CodingKey { case v, name, base, colors, patterns }

    /// One role's color, or `nil` when it doesn't parse (dropped, the base fills it).
    private struct LossyColor: Decodable {
        var color: ThemeColor?
        init(from decoder: Decoder) throws { color = try? ThemeColor(from: decoder) }
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        // First of all: a newer version may keep its fields elsewhere.
        guard try c.decode(Int.self, forKey: .v) == Self.version else {
            throw SharedThemeError.needsNewerVersion
        }
        let raw = try c.decodeIfPresent([String: LossyColor].self, forKey: .colors) ?? [:]
        var colors: [ColorRole: ThemeColor] = [:]
        for (key, value) in raw {
            guard let role = ColorRole(rawValue: key), let color = value.color else { continue }
            colors[role] = color
        }
        self.init(
            name: try c.decode(String.self, forKey: .name),
            base: try c.decodeIfPresent(String.self, forKey: .base) ?? ColorPresets.defaultID,
            colors: colors, patterns: try c.decodeIfPresent(Bool.self, forKey: .patterns) ?? false)
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        try c.encode(Self.version, forKey: .v)
        try c.encode(name, forKey: .name)
        try c.encode(base, forKey: .base)
        // Through `[String: ThemeColor]`, so the keys are the roles' names.
        try c.encode(
            Dictionary(uniqueKeysWithValues: colors.map { ($0.key.rawValue, $0.value) }), forKey: .colors)
        if patterns { try c.encode(patterns, forKey: .patterns) }
    }
}

/// Colors someone sent, waiting for Add or Decline: each opened file is its own, so
/// the same file opened twice shows the sheet again.
nonisolated struct IncomingTheme: Identifiable, Hashable, Sendable {
    let id: UUID
    let theme: SharedTheme

    init(id: UUID = UUID(), theme: SharedTheme) {
        self.id = id
        self.theme = theme
    }
}
