import SwiftUI
import UniformTypeIdentifiers

/// Tiles in a grid, edited like the home screen: touch and hold one and they
/// jiggle in place; drag to reorder, tap − to hide, + on a hidden one (in the
/// Add row) to bring it back. `editing` is ended by the caller's Done button.
struct JiggleGrid<Choice: GridChoice, Tile: View>: View {
    @Binding var choices: [Choice]
    @Binding var editing: Bool
    var columns = 2
    var spacing: CGFloat = 10
    /// Whether a tile has something to show now (others are left out).
    var available: (Choice) -> Bool = { _ in true }
    /// A name for VoiceOver.
    let title: (Choice) -> String
    @ViewBuilder let tile: (Choice) -> Tile

    @State private var dragging: Choice.ID?
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var grid: [GridItem] { Array(repeating: GridItem(.flexible(), spacing: spacing), count: columns) }
    private var shown: [Choice] { choices.filter { $0.shown && available($0) } }
    private var hidden: [Choice] { choices.filter { !$0.shown && available($0) } }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            LazyVGrid(columns: grid, alignment: .leading, spacing: spacing) {
                ForEach(shown) { choice in
                    cell(choice, adding: false)
                }
            }
            if editing {
                if !hidden.isEmpty {
                    Text("Add").font(.caption.weight(.semibold)).foregroundStyle(.secondary)
                    LazyVGrid(columns: grid, alignment: .leading, spacing: spacing) {
                        ForEach(hidden) { choice in
                            cell(choice, adding: true)
                        }
                    }
                }
                Text("Drag to reorder. Tap − to hide a card, + to add one back.")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
        }
        .sensoryFeedback(.impact(weight: .medium), trigger: editing)
    }

    /// Shows or hides `choice`: added back after the shown ones, hidden ones
    /// at the end.
    private func set(_ choice: Choice, shown on: Bool) {
        withAnimation(.snappy) { GridOrder.set(&choices, choice, shown: on) }
    }

    /// The home screen's jiggle: alternate tiles lean opposite ways, each a
    /// little out of step; still with Reduce Motion.
    private func lean(_ choice: Choice) -> Double {
        guard editing, !reduceMotion else { return 0 }
        let n = choice.id.description.count
        return n % 2 == 0 ? 1.3 : -1.3
    }

    @ViewBuilder private func cell(_ choice: Choice, adding: Bool) -> some View {
        let base = tile(choice)
            .opacity(adding ? 0.55 : 1)
            .overlay(alignment: .topLeading) {
                if editing {
                    Button {
                        set(choice, shown: adding)
                    } label: {
                        Image(systemName: adding ? "plus.circle.fill" : "minus.circle.fill")
                            .font(.title3)
                            .symbolRenderingMode(.palette)
                            .foregroundStyle(.white, adding ? .green : Color(white: 0.35))
                    }
                    .buttonStyle(.plain)
                    .offset(x: -8, y: -8)
                    .accessibilityLabel(adding ? "Add \(title(choice))" : "Hide \(title(choice))")
                }
            }
            .modifier(
                Jiggle(
                    active: editing && !adding && !reduceMotion, lean: lean(choice),
                    delay: Double(choice.id.description.count % 5) * 0.03))
            .contentShape(.rect)
        if editing && !adding {
            base
                .onDrag {
                    dragging = choice.id
                    return NSItemProvider(object: choice.id.description as NSString)
                }
                .onDrop(
                    of: [.text],
                    delegate: GridDrop(target: choice.id, choices: $choices, dragging: $dragging))
        } else if editing {
            base
        } else {
            base
                .onLongPressGesture(minimumDuration: 0.45) {
                    withAnimation(.snappy) { editing = true }
                }
                .accessibilityHint("Touch and hold to edit the cards")
        }
    }
}

/// The home screen's jiggle. It keeps its own state and starts when it
/// appears, as entering editing gives each tile a new identity (another
/// branch of `cell`), which a value-driven animation would never see change.
private struct Jiggle: ViewModifier {
    let active: Bool
    let lean: Double
    let delay: Double
    @State private var tilted = false

    func body(content: Content) -> some View {
        content
            .rotationEffect(.degrees(active ? (tilted ? lean : -lean) : 0))
            .onAppear(perform: start)
            .onChange(of: active) { start() }
    }

    private func start() {
        tilted = false
        guard active else { return }
        withAnimation(.easeInOut(duration: 0.13).repeatForever(autoreverses: true).delay(delay)) {
            tilted = true
        }
    }
}

/// Reorders the choices as a dragged tile passes over another.
private struct GridDrop<Choice: GridChoice>: DropDelegate {
    let target: Choice.ID
    @Binding var choices: [Choice]
    @Binding var dragging: Choice.ID?

    func dropEntered(info: DropInfo) {
        guard let dragging else { return }
        withAnimation(.snappy) { GridOrder.move(&choices, dragging: dragging, over: target) }
    }

    func dropUpdated(info: DropInfo) -> DropProposal? { DropProposal(operation: .move) }

    func performDrop(info: DropInfo) -> Bool {
        dragging = nil
        return true
    }
}

/// The Done button that ends editing.
struct DoneEditingButton: View {
    @Binding var editing: Bool

    var body: some View {
        if editing {
            Button {
                withAnimation(.snappy) { editing = false }
            } label: {
                Text("Done").font(.headline).padding(.horizontal, 10).padding(.vertical, 2)
            }
            .glassButton()
        }
    }
}
