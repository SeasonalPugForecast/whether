import Foundation

/// An item of an ordered list the person using the app can reorder and switch off. The
/// raw values are saved, so they never change.
protocol ListedItem: Codable, Hashable, CaseIterable, Identifiable where ID == Self {
    /// What the editors call it.
    var title: String { get }
    /// Its SF Symbol.
    var symbol: String { get }
    /// Whether the item shows until it is switched off (also for one an
    /// update adds to a saved list).
    var shownByDefault: Bool { get }
}

extension ListedItem {
    nonisolated var id: Self { self }
}

/// One item's place in a list and whether it shows.
struct ItemChoice<Item: ListedItem>: Codable, Hashable, Identifiable {
    var item: Item
    var shown: Bool
    var id: Item { item }

    init(item: Item, shown: Bool? = nil) {
        self.item = item
        self.shown = shown ?? item.shownByDefault
    }

    /// Every item once, in the usual order, as it shows by default.
    static var defaults: [Self] { Item.allCases.map { Self(item: $0) } }

    /// `choices` with each item once; ones an update adds come at the end, as
    /// they show by default, and unknown ones are dropped (decoding already
    /// skips them).
    static func complete(_ choices: [Self]) -> [Self] {
        var seen = Set<Item>()
        var out = choices.filter { seen.insert($0.item).inserted }
        out += Item.allCases.filter { !seen.contains($0) }.map { Self(item: $0) }
        return out
    }
}

extension ItemChoice {
    /// A saved choice; one without `shown` is as the item shows by default.
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let item = try c.decode(Item.self, forKey: .item)
        self.init(item: item, shown: try? c.decodeIfPresent(Bool.self, forKey: .shown))
    }

    /// One saved choice, or nil for an item this version does not know, so a
    /// list saved by another version keeps the rest.
    struct Lossy: Decodable {
        var choice: ItemChoice<Item>?
        init(from decoder: Decoder) throws { choice = try? ItemChoice<Item>(from: decoder) }
    }
}

extension Array {
    /// Whether `item` shows.
    func isShown<I: ListedItem>(_ item: I) -> Bool where Element == ItemChoice<I> {
        first { $0.item == item }?.shown ?? item.shownByDefault
    }

    /// Shows or hides `item`, in place.
    mutating func set<I: ListedItem>(_ item: I, shown: Bool) where Element == ItemChoice<I> {
        if let i = firstIndex(where: { $0.item == item }) { self[i].shown = shown }
    }
}

extension ItemChoice where Item: RawRepresentable, Item.RawValue == String {
    /// The items named (by raw value) shown, in that order, and every other
    /// item hidden after them; unknown names are skipped. For a launch's
    /// arguments (screenshots), never saved.
    static func arranged(by names: [String]) -> [Self] {
        let named = names.compactMap { name in Item.allCases.first { $0.rawValue == name } }
        var seen = Set<Item>()
        let first = named.filter { seen.insert($0).inserted }.map { Self(item: $0, shown: true) }
        return first + Item.allCases.filter { !seen.contains($0) }.map { Self(item: $0, shown: false) }
    }
}
