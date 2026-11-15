import SwiftUI
import WhetherKit

/// A significant storm coming within days, on the Now tab under the alerts: the core's
/// title, its headline and how well the models agree, with an edge in the level's color
/// and the level's symbol (a hurricane's symbol for a hurricane). Tap to open the Storms
/// screen, framed on the hurricane when it is one.
struct StormAheadCard: View {
    let card: StormAhead
    let cards: Bool
    let open: () -> Void
    @Environment(\.colorTheme) private var colors

    var body: some View {
        let color = colors.level(card.level)
        Button(action: open) {
            HStack(alignment: .top, spacing: 12) {
                Capsule()
                    .fill(color)
                    .frame(width: 5)
                VStack(alignment: .leading, spacing: 4) {
                    HStack(alignment: .firstTextBaseline, spacing: 6) {
                        Image(systemName: StormAheadRules.symbol(for: card))
                            .foregroundStyle(color)
                        Text(card.title)
                            .font(.headline)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    Text(card.line)
                        .font(.callout)
                        .fixedSize(horizontal: false, vertical: true)
                    if let agreement = card.agreement {
                        Text(agreement)
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                    }
                }
                Spacer(minLength: 0)
                Image(systemName: "chevron.right")
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(.tertiary)
                    .frame(maxHeight: .infinity)
            }
            .fixedSize(horizontal: false, vertical: true)
            .contentShape(.rect)
            .glassCard(cards)
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .combine)
        .accessibilityHint("Opens the storms")
    }
}
