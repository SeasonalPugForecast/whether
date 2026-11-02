import SwiftUI
import UniformTypeIdentifiers

/// Which of the two measure lists an editor or a menu changes.
enum MeasureKind {
    /// The Hourly tab's graph: the first measure is the filled one.
    case graph
    /// The Now tab's 24 hours: a line per measure, in order, the first largest.
    case rows

    /// The words for putting one first.
    var leadTitle: String { self == .graph ? "Fill this one" : "Lead with this one" }
    /// What the first one is called in the editor.
    var leadNote: String { self == .graph ? "Filled" : "Leads" }
    var screenTitle: String { self == .graph ? "Hourly graph" : "24-hour rows" }
    var earlier: String { self == .graph ? "Move left" : "Move up" }
    var later: String { self == .graph ? "Move right" : "Move down" }
    var earlierSymbol: String { self == .graph ? "arrow.left" : "arrow.up" }
    var laterSymbol: String { self == .graph ? "arrow.right" : "arrow.down" }
}

/// A measure dragged from one legend chip to another.
nonisolated struct MeasureDrag: Codable, Transferable {
    let id: String

    static var transferRepresentation: some TransferRepresentation {
        CodableRepresentation(contentType: .data)
    }
}

/// What touch and hold on a legend chip or dot offers: put it first, move it,
/// hide it, or open the editor.
struct MeasureActions: View {
    let measure: GraphMeasure
    let kind: MeasureKind
    @Binding var measures: [String]
    var customize: () -> Void

    private var list: [String] { GraphMeasure.chosen(measures).map(\.rawValue) }
    private var index: Int { list.firstIndex(of: measure.rawValue) ?? 0 }

    private func set(_ new: [String]) {
        withAnimation(.snappy) { measures = new }
    }

    var body: some View {
        Button(kind.leadTitle, systemImage: "star") {
            set(MeasureList.fill(measure.rawValue, in: list))
        }
        .disabled(index == 0)
        Button(kind.earlier, systemImage: kind.earlierSymbol) {
            set(MeasureList.move(measure.rawValue, by: -1, in: list))
        }
        .disabled(index == 0)
        Button(kind.later, systemImage: kind.laterSymbol) {
            set(MeasureList.move(measure.rawValue, by: 1, in: list))
        }
        .disabled(index == list.count - 1)
        Button("Hide", systemImage: "eye.slash") {
            set(MeasureList.hide(measure.rawValue, in: list))
        }
        .disabled(list.count < 2)
        Divider()
        Button("Customize…", systemImage: "slider.horizontal.3", action: customize)
    }
}

/// The measures of a graph or of the 24 hours as a sheet (touch and hold,
/// the measures menu).
struct MeasuresEditorSheet: View {
    let kind: MeasureKind
    @Binding var measures: [String]
    var options: Binding<GraphOptions>?
    let feelsLeads: Bool
    @Environment(\.dismiss) private var dismiss
    @State private var detent = PresentationDetent.medium

    var body: some View {
        NavigationStack {
            MeasuresEditor(kind: kind, measures: $measures, options: options, feelsLeads: feelsLeads)
                .toolbar {
                    ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } }
                }
        }
        .presentationDetents([.medium, .large], selection: $detent)
        .onAppear {
            #if DEBUG
                // Screenshots: `-editmeasures` and `-editrows` open it full height.
                let args = ProcessInfo.processInfo.arguments
                if args.contains("-editmeasures") || args.contains("-editrows") { detent = .large }
            #endif
        }
    }
}

/// Which measures show and in what order: switches with drag handles, the
/// first one filled (graph) or largest (rows); for the graph also how often
/// its values are labelled and whether like measures share a scale.
struct MeasuresEditor: View {
    @Environment(\.colorTheme) private var colors
    let kind: MeasureKind
    @Binding var measures: [String]
    /// The graph's options (`nil` for the rows, which print every hour's value).
    var options: Binding<GraphOptions>?
    let feelsLeads: Bool

    private var shown: [String] { GraphMeasure.chosen(measures).map(\.rawValue) }
    private var offered: [String] { GraphMeasure.offered(feelsLeads: feelsLeads).map(\.rawValue) }

    var body: some View {
        let rows = MeasureList.editorRows(shown: shown, offered: offered)
        List {
            Section {
                ForEach(rows, id: \.id) { row in
                    if let measure = GraphMeasure(rawValue: row.id) {
                        Toggle(isOn: binding(row.id)) {
                            HStack(spacing: 10) {
                                Capsule()
                                    .fill(row.id == shown.first ? Color.primary : measure.color(colors))
                                    .frame(width: 16, height: 4)
                                Text(measure.title(feelsLeads: feelsLeads))
                                if row.id == shown.first {
                                    Text(kind.leadNote).font(.caption).foregroundStyle(.secondary)
                                }
                            }
                        }
                        // One measure always stays.
                        .disabled(row.shown && shown.count < 2)
                    }
                }
                .onMove { source, destination in
                    withAnimation(.snappy) {
                        measures = MeasureList.moved(
                            shown: shown, offered: offered, from: source, to: destination)
                    }
                }
            } footer: {
                Text(
                    kind == .graph
                        ? "Drag the handles to reorder the graph; the first shown measure is filled. One measure always stays."
                        : "Drag the handles to reorder the 24 hours; the first shown measure is the largest line. One measure always stays."
                )
            }
            if let options {
                Section {
                    Picker("Show values every…", selection: options.valueEvery) {
                        ForEach(GraphOptions.everyChoices, id: \.self) { hours in
                            Text(Self.everyTitle(hours)).tag(hours)
                        }
                    }
                    Picker("Scales", selection: options.sharedScales) {
                        Text("Shared for like units").tag(true)
                        Text("Each on its own scale").tag(false)
                    }
                } header: {
                    Text("Labels and scales")
                } footer: {
                    Text(
                        "Automatic spaces the values by how wide the hours are, at least every 3 hours. A shared scale puts feels-like and air temperatures on one scale, and wind with its gusts."
                    )
                }
            }
            Section {
                Button("Restore the defaults") {
                    withAnimation(.snappy) {
                        measures = [MeasureList.fallback]
                        options?.wrappedValue = GraphOptions()
                    }
                }
                .disabled(measures == [MeasureList.fallback] && (options?.wrappedValue ?? GraphOptions()) == GraphOptions())
            }
        }
        .environment(\.editMode, .constant(.active))
        // The app's white tint would leave the switches' on state unreadable.
        .tint(.green)
        .navigationTitle(kind.screenTitle)
        .navigationBarTitleDisplayMode(.inline)
    }

    private static func everyTitle(_ hours: Int) -> String {
        switch hours {
        case 0: "Automatic"
        case 1: "Every hour"
        default: "Every \(hours) hours"
        }
    }

    /// A measure's switch: on adds it after the shown ones, off removes it.
    private func binding(_ id: String) -> Binding<Bool> {
        Binding(
            get: { shown.contains(id) },
            set: { on in
                withAnimation(.snappy) {
                    measures = on ? MeasureList.show(id, in: shown) : MeasureList.hide(id, in: shown)
                }
            })
    }
}
