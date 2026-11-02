import SwiftUI
import WhetherKit

/// Sources, or forecast model presets, side by side for the place shown: now,
/// the next 24 hours every three hours, and the days. Three columns fill the
/// width; more scroll sideways beside the pinned row labels. Each source
/// serves alone (Open-Meteo only when it has no data there, which its column
/// says), so the differences show what each brings before reordering them; each
/// preset serves as it would when chosen.
struct CompareSourcesView: View {
    @Environment(\.colorTheme) private var colors
    /// What the columns are.
    enum Kind {
        /// Sources, by provider id.
        case sources
        /// Forecast model presets, by name.
        case models
    }

    let store: WeatherStore
    var kind = Kind.sources
    /// The sources compared, in column order.
    @State private var chosen: [String] = []
    /// Each source's forecast, or why there is none.
    @State private var results: [String: Result<ViewModel, Error>] = [:]
    /// The columns being fetched, so a column chosen again isn't fetched twice.
    @State private var loading: Set<String> = []

    /// The table's width, which three columns share; more scroll sideways.
    @State private var tableWidth: CGFloat = 0

    private static let labelWidth: CGFloat = 64
    private static let minColumnWidth: CGFloat = 100
    // Row heights, the same for the labels and every column.
    private static let headerHeight: CGFloat = 46
    private static let nowHeight: CGFloat = 46
    private static let hourHeight: CGFloat = 32
    private static let dayHeight: CGFloat = 46

    private var place: AppPlace? {
        store.places.indices.contains(store.active) ? store.places[store.active] : store.places.first
    }

    /// The forecast model presets, best match first.
    private static let presets = modelPresets()

    /// What can be compared, as (id, name): the sources that can serve now (a
    /// keyed one needs its key), or every model preset.
    private var available: [(id: String, name: String)] {
        switch kind {
        case .sources:
            let facts = store.preferences.sourceFacts()
            return Sources.catalog.filter { info in
                Sources.serves(Preferences.SourceRow(provider: info.id), facts)
            }
            .map { ($0.id, $0.name) }
        case .models:
            return Self.presets.map { ($0.name, $0.name) }
        }
    }

    private func name(_ id: String) -> String {
        available.first { $0.id == id }?.name ?? Sources.info(id)?.name ?? id
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                picker
                if let place, !chosen.isEmpty {
                    Text(
                        kind == .sources
                            ? "For \(place.name). Each source alone, without filling its gaps from others."
                            : "For \(place.name). Each preset as it serves when chosen, Open-Meteo's best match filling what its models lack."
                    )
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    table
                        .glassCard(true)
                    credits
                }
            }
            .padding(16)
        }
        .navigationTitle(kind == .sources ? "Compare sources" : "Compare models")
        .navigationBarTitleDisplayMode(.inline)
        .task(id: chosen) { await load() }
        .onAppear {
            guard chosen.isEmpty else { return }
            var start: [String]
            switch kind {
            case .sources:
                // Your order's first sources that serve, and Open-Meteo to compare with.
                let facts = store.preferences.sourceFacts()
                let serving = store.preferences.sources.filter { Sources.serves($0, facts) }.map(\.provider)
                start = Array(serving.prefix(2))
                if !start.contains(Sources.openMeteo) { start.append(Sources.openMeteo) }
            case .models:
                // Your preset, if it is one, and the presets after best match.
                start = Self.presets.filter { $0.models == store.preferences.models }.map(\.name)
            }
            // Something to compare with even when only one serves here.
            for option in available where start.count < 2 && !start.contains(option.id) {
                start.append(option.id)
            }
            chosen = start
            #if DEBUG
                // `-compare nws,open-meteo` picks the columns; `-models 3` a model preset first.
                let args = ProcessInfo.processInfo.arguments
                if let i = args.firstIndex(of: "-models"), i + 1 < args.count, let n = Int(args[i + 1]),
                    modelPresets().indices.contains(n)
                {
                    store.preferences.models = modelPresets()[n].models
                }
                if let i = args.firstIndex(of: "-compare"), i + 1 < args.count {
                    chosen = args[i + 1].split(separator: ",").map(String.init)
                }
            #endif
        }
    }

    /// The sources to compare, as toggles.
    private var picker: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(available, id: \.id) { option in
                    let on = chosen.contains(option.id)
                    Button {
                        if on {
                            if chosen.count > 1 { chosen.removeAll { $0 == option.id } }
                        } else {
                            chosen.append(option.id)
                        }
                    } label: {
                        Label(option.name, systemImage: on ? "checkmark.circle.fill" : "circle")
                            .font(.callout)
                            .padding(.horizontal, 10)
                            .padding(.vertical, 6)
                            .background(.white.opacity(on ? 0.2 : 0.08), in: .capsule)
                    }
                    .buttonStyle(.plain)
                }
            }
        }
        .accessibilityLabel(kind == .sources ? "Sources to compare" : "Models to compare")
    }

    // MARK: The table

    private func view(_ id: String) -> ViewModel? {
        if case .success(let view) = results[id] { view } else { nil }
    }

    /// The hour rows: the most detailed column's next 24 hours, every three hours
    /// (a source with 3-hour steps, like OpenWeather's free forecast, matches the
    /// nearest of its hours).
    private var hours: [ViewModel.Hour] {
        let lists = chosen.compactMap { view($0)?.hourly?.hours }
        guard let start = lists.compactMap({ $0.first?.time }).min() else { return [] }
        let end = start + 24 * 3600
        let finest = lists.max { a, b in
            a.count { $0.time <= end } < b.count { $0.time <= end }
        } ?? []
        var rows: [ViewModel.Hour] = []
        for hour in finest where hour.time <= end {
            if let last = rows.last, hour.time - last.time < 3 * 3600 { continue }
            rows.append(hour)
        }
        return rows
    }

    /// The day rows: the first column's days, matched by date.
    private var days: [ViewModel.Day] {
        chosen.lazy.compactMap { view($0)?.daily?.days }.first ?? []
    }

    /// The column's width: three share the table, more keep their width and scroll.
    private var columnWidth: CGFloat {
        let room = tableWidth - Self.labelWidth - 8
        guard !chosen.isEmpty, room > 0 else { return Self.minColumnWidth }
        return max(Self.minColumnWidth, room / CGFloat(chosen.count))
    }

    /// The row labels, pinned, and the columns, which scroll sideways when they
    /// don't fit. Every row has a fixed height so the two line up.
    private var table: some View {
        HStack(alignment: .top, spacing: 8) {
            labels
                .frame(width: Self.labelWidth, alignment: .leading)
            ScrollView(.horizontal) {
                HStack(alignment: .top, spacing: 0) {
                    ForEach(chosen, id: \.self) { id in
                        column(id)
                            .frame(width: columnWidth, alignment: .leading)
                    }
                }
            }
            .scrollIndicators(chosen.count > 3 ? .visible : .hidden)
        }
        .monospacedDigit()
        .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { tableWidth = $0 }
    }

    private var labels: some View {
        VStack(alignment: .leading, spacing: 0) {
            Color.clear.frame(height: Self.headerHeight)
            Divider()
            Text("Now").font(.caption.weight(.semibold)).foregroundStyle(.secondary)
                .frame(height: Self.nowHeight)
            if !hours.isEmpty {
                Divider()
                ForEach(hours, id: \.time) { hour in
                    Text(hour.label).font(.caption).foregroundStyle(.secondary)
                        .frame(height: Self.hourHeight)
                }
            }
            if !days.isEmpty {
                Divider()
                ForEach(days, id: \.dateShort) { day in
                    VStack(alignment: .leading, spacing: 0) {
                        Text(day.label).font(.caption)
                        Text(day.dateShort).font(.caption2).foregroundStyle(.secondary)
                    }
                    .lineLimit(1)
                    .minimumScaleFactor(0.8)
                    .frame(height: Self.dayHeight)
                }
            }
        }
    }

    private func column(_ id: String) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            header(id).frame(height: Self.headerHeight, alignment: .topLeading)
            Divider()
            nowCell(id).frame(height: Self.nowHeight)
            if !hours.isEmpty {
                Divider()
                ForEach(hours, id: \.time) { hour in
                    hourCell(id, at: hour.time).frame(height: Self.hourHeight)
                }
            }
            if !days.isEmpty {
                Divider()
                ForEach(days, id: \.dateShort) { day in
                    dayCell(id, on: day.dateShort).frame(height: Self.dayHeight)
                }
            }
        }
        .padding(.trailing, 8)
    }

    /// The column's hour nearest `time`, within an hour and a half.
    private func hour(_ id: String, near time: Int64) -> ViewModel.Hour? {
        guard let hours = view(id)?.hourly?.hours,
            let nearest = hours.min(by: { abs($0.time - time) < abs($1.time - time) }),
            abs(nearest.time - time) <= 5400
        else { return nil }
        return nearest
    }

    /// The source's name, and the one that actually answered when that differs.
    private func header(_ id: String) -> some View {
        VStack(alignment: .leading, spacing: 1) {
            Text(name(id)).font(.caption.weight(.semibold)).lineLimit(1)
            switch results[id] {
            case .success(let view):
                Text(view.status.provider).font(.caption2).foregroundStyle(.secondary).lineLimit(2)
            case .failure(let error):
                Text(userMessage(error).title).font(.caption2).foregroundStyle(.orange).lineLimit(2)
            case nil:
                ProgressView().controlSize(.small)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func nowCell(_ id: String) -> some View {
        Group {
            if let current = view(id)?.current {
                HStack(spacing: 4) {
                    ConditionIcon(icon: current.icon, size: 18)
                    VStack(alignment: .leading, spacing: 0) {
                        Text(current.temperature.text).font(.callout)
                        Text(current.conditionLabel).font(.caption2).foregroundStyle(.secondary).lineLimit(1)
                    }
                }
            } else {
                Text("–").foregroundStyle(.secondary)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func hourCell(_ id: String, at time: Int64) -> some View {
        Group {
            if let hour = hour(id, near: time) {
                HStack(spacing: 4) {
                    ConditionIcon(icon: hour.icon, size: 15)
                    Text(hour.temperature.text).font(.callout)
                    if let chance = hour.precipProbability, chance > 0 {
                        Text(chanceText(percent: chance)).font(.caption2).foregroundStyle(colors.rain)
                    }
                }
            } else {
                Text("–").foregroundStyle(.secondary)
            }
        }
        .lineLimit(1)
        .minimumScaleFactor(0.7)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func dayCell(_ id: String, on date: String) -> some View {
        Group {
            if let day = view(id)?.daily?.days.first(where: { $0.dateShort == date }) {
                HStack(spacing: 4) {
                    ConditionIcon(icon: day.icon, size: 15)
                    VStack(alignment: .leading, spacing: 0) {
                        HStack(spacing: 3) {
                            Text(day.high.text).font(.callout)
                            Text(day.low.text).font(.caption).foregroundStyle(.secondary)
                        }
                        if let chance = day.precipProbability, chance > 0 {
                            Text(chanceText(percent: chance)).font(.caption2).foregroundStyle(colors.rain)
                        }
                    }
                }
            } else {
                Text("–").foregroundStyle(.secondary)
            }
        }
        .lineLimit(1)
        .minimumScaleFactor(0.7)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// Every column's data credit.
    private var credits: some View {
        VStack(alignment: .leading, spacing: 4) {
            ForEach(chosen, id: \.self) { id in
                if let view = view(id) {
                    Text("\(name(id)): \(view.attribution)")
                }
            }
        }
        .font(.caption2)
        .foregroundStyle(.secondary)
    }

    /// One column's forecast: a source alone, or a preset's models.
    private func forecast(_ id: String, at place: AppPlace) async throws -> ViewModel {
        switch kind {
        case .sources:
            return try await store.compareForecast(at: place, provider: id)
        case .models:
            let models = Self.presets.first { $0.name == id }?.models ?? []
            return try await store.compareModels(at: place, models: models)
        }
    }

    /// Fetches the chosen columns side by side; each column fills as it arrives.
    /// Each in a task of its own on the main actor, not a task group: handing a
    /// group child's result back crashed optimized builds (EXC_BAD_ACCESS in
    /// `TaskGroup.offer`).
    private func load() async {
        guard let place else { return }
        for id in chosen where results[id] == nil && !loading.contains(id) {
            loading.insert(id)
            Task {
                let result: Result<ViewModel, Error>
                do {
                    result = .success(try await forecast(id, at: place))
                } catch {
                    result = .failure(error)
                }
                results[id] = result
                loading.remove(id)
            }
        }
    }
}
