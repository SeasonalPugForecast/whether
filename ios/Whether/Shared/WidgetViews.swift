import SwiftUI
import WidgetKit
import WhetherKit

/// The widgets: home screen (small, medium) and lock screen (rectangular,
/// circular, inline). Values come formatted from the core.
struct WeatherWidgetView: View {
    @Environment(\.colorTheme) private var colors
    let entry: WeatherWidgetEntry
    let family: WidgetFamily

    var body: some View {
        if let view = entry.view, let current = view.current {
            switch family {
            case .systemMedium: medium(view, current)
            case .systemLarge: large(view, current)
            case .accessoryRectangular: rectangular(view, current)
            case .accessoryCircular: circular(current)
            case .accessoryInline: inline(current)
            default: small(view, current)
            }
        } else {
            Text(entry.places == 0 ? "Open Pug Forecast to choose a place" : "Open Pug Forecast to load the forecast")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }

    private func highLow(_ current: ViewModel.Current) -> String {
        "H \(current.high.text)  L \(current.low.text)"
    }

    /// The forecast's source (core text), and Apple Weather when its minutes show.
    private func credit(_ view: ViewModel) -> some View {
        let apple = view.nowcast?.stepMinutes == 1
        return Text(apple ? "\(view.status.provider) · Apple Weather" : view.status.provider)
            .font(.caption2)
            .foregroundStyle(.secondary)
            .lineLimit(1)
            .minimumScaleFactor(0.8)
    }

    /// Switches to the next saved place (interactive widgets).
    @ViewBuilder private var nextPlace: some View {
        // Only for "The app's": a widget set to one place stays there.
        if entry.followsApp, entry.places > 1 {
            Button(intent: NextPlaceIntent()) {
                Image(systemName: "arrow.right.circle.fill")
                    .font(.body)
                    .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Next place")
        }
    }

    private func small(_ view: ViewModel, _ current: ViewModel.Current) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack {
                Text(view.location.name).font(.caption.weight(.semibold)).lineLimit(1)
                Spacer(minLength: 4)
                ConditionIcon(icon: current.icon, size: 20)
            }
            Spacer(minLength: 0)
            Text(current.temperature.text)
                .font(.system(size: 44, weight: .thin))
                .minimumScaleFactor(0.6)
            Text(current.conditionLabel).font(.caption).lineLimit(1)
            Text(highLow(current)).font(.caption2).foregroundStyle(.secondary)
        }
    }

    private func medium(_ view: ViewModel, _ current: ViewModel.Current) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            mediumBody(view, current)
            credit(view)
        }
    }

    private func mediumBody(_ view: ViewModel, _ current: ViewModel.Current) -> some View {
        HStack(spacing: 14) {
            small(view, current).frame(maxWidth: 130)
            VStack(alignment: .leading, spacing: 6) {
                switch entry.content {
                case .overview, .nextHours:
                    lead(view.summary)
                    hourStrip(view, count: entry.content.hours(large: false), chance: entry.content == .nextHours)
                case .nextHour:
                    lead(view.nowcast?.text ?? view.summary)
                    nextHour(view, bars: 34)
                case .today:
                    todayFigures(current)
                    lead(view.daily?.days.first?.outlook ?? view.summary, lines: 2, button: false)
                    if let day = view.daily?.days.first { facts(day, columns: 2) }
                }
            }
        }
    }

    /// A line of the core's text beside the place button.
    private func lead(_ text: String?, lines: Int = 2, button: Bool = true) -> some View {
        HStack(alignment: .top) {
            if let text {
                Text(text).font(.caption2).foregroundStyle(.secondary).lineLimit(lines)
            }
            Spacer(minLength: 0)
            if button { nextPlace }
        }
    }

    /// One hour: its label, icon and temperature, and (`chance`) its chance of rain.
    private func hourCell(
        _ hour: ViewModel.Hour, size: CGFloat = 16, spacing: CGFloat = 4, chance: Bool = false
    ) -> some View {
        VStack(spacing: spacing) {
            Text(hour.label).font(.caption2).foregroundStyle(.secondary)
                .lineLimit(1).minimumScaleFactor(0.7)
            ConditionIcon(icon: hour.icon, size: size)
            Text(hour.temperature.text).font(.caption)
            if chance {
                Text(hour.precipProbability.map { $0 >= 10 ? chanceText(percent: $0) : "" } ?? "")
                    .font(.caption2).foregroundStyle(colors.rain).lineLimit(1)
                    .frame(minHeight: 12)
            }
        }
        .frame(maxWidth: .infinity)
    }

    /// The hours after the current one, `count` of them in a row.
    private func hourStrip(
        _ view: ViewModel, count: Int, chance: Bool = false, large: Bool = false, skip: Int = 0
    ) -> some View {
        HStack(spacing: 0) {
            ForEach(Array((view.hourly?.hours ?? []).dropFirst(1 + skip).prefix(count)), id: \.time) { hour in
                hourCell(hour, size: large ? 15 : 16, spacing: large ? 3 : 4, chance: chance)
            }
        }
    }

    /// The next hour: the nowcast's bars when rain is coming, and the hour after now.
    @ViewBuilder private func nextHour(_ view: ViewModel, bars: CGFloat) -> some View {
        if let nowcast = view.nowcast, nowcast.wet {
            HStack(alignment: .bottom, spacing: 10) {
                RainBars(bars: nowcast.intensity).frame(height: bars)
                hourStrip(view, count: 1, chance: true).frame(width: 54)
            }
        } else {
            // Dry: the next two hours in words.
            VStack(spacing: 6) { hourRows(view, count: 2) }
        }
    }

    /// Today's high and low.
    private func todayFigures(_ current: ViewModel.Current) -> some View {
        HStack(spacing: 18) {
            VStack(alignment: .leading, spacing: 0) {
                Text("High").font(.caption2).foregroundStyle(.secondary)
                Text(current.high.text).font(.system(size: 26, weight: .light))
            }
            VStack(alignment: .leading, spacing: 0) {
                Text("Low").font(.caption2).foregroundStyle(.secondary)
                Text(current.low.text).font(.system(size: 26, weight: .light))
            }
            Spacer(minLength: 0)
            nextPlace
        }
    }

    /// Today's chance of rain, rain, wind and sun, as the core wrote them.
    private func facts(_ day: ViewModel.Day, columns: Int = 4) -> some View {
        // A value the forecast lacks reads "—": left out.
        let items: [(String, String)] = [
            day.precipProbability.map { ("drop.fill", chanceText(percent: $0)) },
            ("cloud.rain", day.precipSum.text),
            ("wind", day.windMax.text),
            day.sunrise.map { ("sunrise.fill", $0) },
            day.sunset.map { ("sunset.fill", $0) },
        ].compactMap { $0 }.filter { $0.1 != "—" }
        return LazyVGrid(
            columns: Array(repeating: GridItem(.flexible(), alignment: .leading), count: columns),
            alignment: .leading, spacing: 3
        ) {
            ForEach(items.indices, id: \.self) { i in
                Label(items[i].1, systemImage: items[i].0)
                    .font(.caption2).lineLimit(1).minimumScaleFactor(0.8)
            }
        }
    }

    /// Now, then what `entry.content` chooses for the rest of the widget.
    private func large(_ view: ViewModel, _ current: ViewModel.Current) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(alignment: .top) {
                VStack(alignment: .leading, spacing: 0) {
                    Text(view.location.name).font(.headline).lineLimit(1)
                    Text(current.temperature.text).font(.system(size: 44, weight: .thin))
                    Text("\(current.conditionLabel) · \(highLow(current))").font(.caption)
                }
                Spacer()
                VStack(alignment: .trailing, spacing: 8) {
                    nextPlace
                    ConditionIcon(icon: current.icon, size: 34)
                }
            }
            switch entry.content {
            case .overview:
                nowcastBlock(view, bars: 28)
                hourStrip(view, count: entry.content.hours(large: true), large: true)
                Divider()
                dayRows(view.daily?.days ?? [], count: 5)
            case .nextHour:
                nowcastBlock(view, bars: 70, alwaysText: true)
                // Without rain to draw, more hours fill the space.
                hourRows(view, count: view.nowcast?.wet == true ? 2 : 6)
            case .nextHours:
                nowcastBlock(view, bars: 0)
                let hours = entry.content.hours(large: true)
                hourStrip(view, count: hours / 2, chance: true, large: true)
                Divider()
                hourStrip(view, count: hours / 2, chance: true, large: true, skip: hours / 2)
            case .today:
                if let day = view.daily?.days.first {
                    if let text = day.outlook ?? view.summary {
                        Text(text).font(.caption).foregroundStyle(.secondary).lineLimit(3)
                    }
                    facts(day)
                }
                Divider()
                dayRows(Array((view.daily?.days ?? []).dropFirst()), count: 5)
            }
            Spacer(minLength: 0)
            credit(view)
        }
    }

    /// The nowcast's text, and its bars when rain is coming (`bars` 0 shows the text only).
    @ViewBuilder private func nowcastBlock(_ view: ViewModel, bars: CGFloat, alwaysText: Bool = false) -> some View {
        if let nowcast = view.nowcast {
            VStack(alignment: .leading, spacing: 4) {
                Text(nowcast.text).font(.caption).foregroundStyle(colors.rain).lineLimit(1)
                if nowcast.wet, bars > 0 {
                    RainBars(bars: nowcast.intensity).frame(height: bars)
                    if alwaysText, let first = nowcast.ticks.first, let last = nowcast.ticks.last {
                        HStack {
                            Text(first.label)
                            Spacer()
                            Text(last.label)
                        }
                        .font(.caption2).foregroundStyle(.secondary)
                    }
                }
            }
        }
    }

    /// The next hours as lines: when, the weather, the chance of rain and the temperature.
    private func hourRows(_ view: ViewModel, count: Int) -> some View {
        ForEach(Array((view.hourly?.hours ?? []).dropFirst().prefix(count)), id: \.time) { hour in
            HStack {
                Text(hour.label).font(.caption).frame(width: 52, alignment: .leading)
                ConditionIcon(icon: hour.icon, size: 15)
                Text(hour.conditionLabel).font(.caption).lineLimit(1)
                Spacer()
                Text(hour.precipProbability.map { $0 >= 10 ? chanceText(percent: $0) : "" } ?? "")
                    .font(.caption).foregroundStyle(colors.rain)
                Text(hour.temperature.text).font(.caption).frame(width: 40, alignment: .trailing)
            }
            .monospacedDigit()
        }
    }

    private func dayRows(_ days: [ViewModel.Day], count: Int) -> some View {
        ForEach(Array(days.prefix(count)), id: \.dateShort) { day in
            HStack {
                Text(day.label).font(.caption).frame(width: 74, alignment: .leading)
                ConditionIcon(icon: day.icon, size: 15)
                Spacer()
                Text(day.low.text).font(.caption).foregroundStyle(.secondary)
                Text(day.high.text).font(.caption).frame(width: 34, alignment: .trailing)
            }
            .monospacedDigit()
        }
    }

    private func rectangular(_ view: ViewModel, _ current: ViewModel.Current) -> some View {
        HStack(spacing: 8) {
            Image(systemName: WeatherSymbols.symbol(current.icon)).font(.title2)
            VStack(alignment: .leading, spacing: 0) {
                Text("\(current.temperature.text) \(current.conditionLabel)")
                    .font(.headline).lineLimit(1)
                Text(highLow(current)).font(.caption)
                Text(view.location.name).font(.caption2).foregroundStyle(.secondary).lineLimit(1)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func circular(_ current: ViewModel.Current) -> some View {
        ZStack {
            AccessoryWidgetBackground()
            VStack(spacing: 0) {
                Image(systemName: WeatherSymbols.symbol(current.icon)).font(.caption)
                Text(current.temperature.text).font(.title3.weight(.medium))
            }
        }
    }

    private func inline(_ current: ViewModel.Current) -> some View {
        Label(
            "\(current.temperature.text) \(current.conditionLabel)",
            systemImage: WeatherSymbols.symbol(current.icon))
    }
}
