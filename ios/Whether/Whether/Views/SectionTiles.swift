import SwiftUI

/// The Now tab in edit mode, home screen style: its sections as jiggling tiles
/// to drag into order (the system list scrolls while dragging), with a minus
/// to hide each and the hidden ones below to add back. Done returns to the
/// forecast. Settings → Sections edits the same list.
struct SectionTiles: View {
    @Bindable var store: WeatherStore
    let done: () -> Void
    @State private var jiggle = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var shown: [Preferences.SectionChoice] { store.preferences.sections.filter(\.shown) }
    private var hidden: [Preferences.SectionChoice] { store.preferences.sections.filter { !$0.shown } }

    var body: some View {
        List {
            Section {
                ForEach(shown) { choice in
                    tile(choice.section, adding: false)
                }
                .onMove { store.preferences.moveShown(from: $0, to: $1) }
            } header: {
                HStack {
                    Text("Drag to reorder").textCase(nil)
                    Spacer()
                    Button(action: done) {
                        Text("Done").font(.headline).padding(.horizontal, 10).padding(.vertical, 2)
                    }
                    .glassButton()
                    .textCase(nil)
                }
                .padding(.bottom, 4)
            }
            if !hidden.isEmpty {
                Section("Hidden") {
                    ForEach(hidden) { choice in
                        tile(choice.section, adding: true)
                    }
                }
            }
        }
        .scrollContentBackground(.hidden)
        .listRowSpacing(10)
        .environment(\.editMode, .constant(.active))
        .onAppear { jiggle = true }
    }

    private func tile(_ section: Preferences.OverviewSection, adding: Bool) -> some View {
        HStack(spacing: 14) {
            if adding {
                Button {
                    withAnimation(.snappy) { store.preferences.show(section) }
                } label: {
                    Image(systemName: "plus.circle.fill").font(.title2).foregroundStyle(.green)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Show \(section.title)")
            } else if Preferences.hideable(section) {
                Button {
                    withAnimation(.snappy) { store.preferences.hide(section) }
                } label: {
                    Image(systemName: "minus.circle.fill").font(.title2).foregroundStyle(.red)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Hide \(section.title)")
            } else {
                Image(systemName: "lock.fill").font(.callout).foregroundStyle(.secondary).frame(width: 26)
            }
            Image(systemName: section.symbol)
                .font(.title3)
                .frame(width: 44, height: 44)
                .background(.white.opacity(0.14), in: .rect(cornerRadius: 12))
            Text(section.title).font(.body.weight(.medium))
            Spacer()
        }
        .padding(.vertical, 6)
        .opacity(adding ? 0.65 : 1)
        // The home screen's jiggle, offset per tile; still with Reduce Motion.
        .rotationEffect(.degrees(jiggle && !adding && !reduceMotion ? 1.2 : 0))
        .animation(
            jiggle && !adding && !reduceMotion
                ? .easeInOut(duration: 0.14).repeatForever(autoreverses: true)
                    .delay(Double(section.title.count % 5) * 0.03)
                : .default,
            value: jiggle
        )
        .listRowSeparator(.hidden)
        .listRowBackground(
            RoundedRectangle(cornerRadius: 18, style: .continuous).fill(.white.opacity(0.1)))
        .moveDisabled(adding)
    }
}
