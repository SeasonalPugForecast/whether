import SwiftUI
import WidgetKit
import WhetherKit

/// The widgets: home screen (small, medium, large) and lock screen (rectangular,
/// circular, inline), laid out as the phone's own weather widgets are: the place
/// and the temperature first, the conditions and today's high and low beside or
/// below, then the hours and the days. Values come formatted from the core.
struct WeatherWidgetView: View {
    @Environment(\.colorTheme) private var colors
    @Environment(\.widgetRenderingMode) private var renderingMode
    @Environment(\.showsWidgetContainerBackground) private var showsBackground
    let entry: WeatherWidgetEntry
    let family: WidgetFamily

    var body: some View {
        if let view = entry.view, let now = entry.now {
            switch family {
            case .accessoryRectangular: rectangular(view, now)
            case .accessoryCircular: circular(now)
            case .accessoryInline: inline(now)
            default: onBackground(home(view, now))
            }
        } else {
            Text(entry.places == 0 ? "Open Pug Forecast to choose a place" : "Open Pug Forecast to load the forecast")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }

    @ViewBuilder private func home(_ view: ViewModel, _ now: WidgetNow) -> some View {
        switch family {
        case .systemMedium: medium(view, now)
        case .systemLarge: large(view, now)
        default: small(view, now)
        }
    }

    /// Whether the sky is behind: chosen, and drawn (not in StandBy, nor tinted).
    private var onSky: Bool {
        entry.backdrop == .sky && showsBackground && renderingMode == .fullColor
    }

    /// White text and the theme's colors for dark backgrounds on the sky (the
    /// skies are deep enough for both, light or dark mode); the secondary text a
    /// little stronger than usual, for the lighter day skies.
    @ViewBuilder private func onBackground(_ content: some View) -> some View {
        if onSky {
            content
                .foregroundStyle(.white, .white.opacity(0.8), .white.opacity(0.65))
                .environment(\.colorScheme, .dark)
        } else {
            content
        }
    }

    private func highLow(_ now: WidgetNow) -> String {
        "H \(now.high.text)  L \(now.low.text)"
    }

    /// The forecast's source (core text), and Apple's Weather mark when its minutes show.
    private func credit(_ view: ViewModel) -> some View {
        HStack(spacing: 4) {
            Text(view.status.provider)
                .lineLimit(1)
                .minimumScaleFactor(0.8)
            if view.nowcast?.stepMinutes == 1 {
                Text("·")
                AppleWeatherBadge()
            }
        }
        .font(.caption2)
        .foregroundStyle(.secondary)
    }

    /// Switches to the next saved place (interactive widgets).
    @ViewBuilder private var nextPlace: some View {
        // Only for "The app's": a widget set to one place stays there.
        if entry.followsApp, entry.places > 1 {
            Button(intent: NextPlaceIntent()) {
                Image(systemName: "arrow.right.circle.fill")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Next place")
        }
    }

    /// The place's name, with an arrow when it follows the phone.
    private func name(_ view: ViewModel, font: Font) -> some View {
        HStack(spacing: 3) {
            Text(view.location.name).lineLimit(1)
            if entry.followsPhone {
                Image(systemName: "location.fill")
                    .font(.system(size: 9, weight: .semibold))
                    .accessibilityLabel("Current location")
            }
        }
        .font(font)
    }

    /// The big number.
    private func temperature(_ now: WidgetNow, size: CGFloat) -> some View {
        Text(now.temperature.text)
            .font(.system(size: size, weight: .light))
            .lineLimit(1)
            .minimumScaleFactor(0.6)
    }

    // MARK: Small

    private func small(_ view: ViewModel, _ now: WidgetNow) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            name(view, font: .subheadline.weight(.semibold))
            temperature(now, size: 44)
            Spacer(minLength: 0)
            ConditionIcon(icon: now.icon, size: 18).padding(.bottom, 3)
            Text(now.conditionLabel).font(.caption.weight(.semibold)).lineLimit(1)
                .minimumScaleFactor(0.8)
            Text(highLow(now)).font(.caption).lineLimit(1).minimumScaleFactor(0.8)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
    }

    // MARK: Medium and large

    /// The place and the temperature, beside the conditions and today's high and low.
    private func header(_ view: ViewModel, _ now: WidgetNow, size: CGFloat) -> some View {
        HStack(alignment: .top, spacing: 8) {
            VStack(alignment: .leading, spacing: 0) {
                HStack(spacing: 6) {
                    name(view, font: .subheadline.weight(.semibold))
                    nextPlace
                }
                temperature(now, size: size)
            }
            Spacer(minLength: 0)
            VStack(alignment: .trailing, spacing: 1) {
                ConditionIcon(icon: now.icon, size: 18).padding(.bottom, 2)
                Text(now.conditionLabel).font(.caption.weight(.semibold))
                    .lineLimit(1).minimumScaleFactor(0.8)
                Text(highLow(now)).font(.caption).lineLimit(1).minimumScaleFactor(0.8)
            }
            .padding(.top, 2)
        }
    }

    private func medium(_ view: ViewModel, _ now: WidgetNow) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            header(view, now, size: 32)
            Spacer(minLength: 0)
            switch entry.content {
            case .overview, .nextHours:
                hourStrip(
                    view, count: entry.content.hours(large: false), chance: entry.content == .nextHours,
                    compact: true)
            case .nextHour:
                if let text = view.nowcast?.text {
                    Text(text).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                }
                nextHour(view, bars: 26)
            case .today:
                if let text = view.daily?.days.first?.outlook ?? view.summary {
                    Text(text).font(.caption2).foregroundStyle(.secondary).lineLimit(2)
                }
                if let day = view.daily?.days.first { facts(day, columns: 3) }
            }
            Spacer(minLength: 0)
            credit(view)
        }
    }

    /// Now, then what `entry.content` chooses for the rest of the widget.
    private func large(_ view: ViewModel, _ now: WidgetNow) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            header(view, now, size: 40)
            switch entry.content {
            case .overview:
                nowcastBlock(view, bars: 24)
                hourStrip(view, count: entry.content.hours(large: true))
                Divider()
                dayRows(view.daily?.days ?? [], count: entry.content.days)
            case .nextHour:
                nowcastBlock(view, bars: 70, alwaysText: true)
                // Without rain to draw, more hours fill the space.
                hourRows(view, count: view.nowcast?.wet == true ? 2 : 6)
            case .nextHours:
                nowcastBlock(view, bars: 0)
                let hours = entry.content.hours(large: true)
                hourStrip(view, count: hours / 2, chance: true)
                Divider()
                hourStrip(view, count: hours / 2, chance: true, skip: hours / 2)
            case .today:
                if let day = view.daily?.days.first {
                    if let text = day.outlook ?? view.summary {
                        Text(text).font(.caption).foregroundStyle(.secondary).lineLimit(3)
                    }
                    facts(day)
                }
                Divider()
                dayRows(Array((view.daily?.days ?? []).dropFirst()), count: entry.content.days - 1)
            }
            Spacer(minLength: 0)
            credit(view)
        }
    }

    /// One hour: its label, icon and temperature, and (`chance`) its chance of rain;
    /// `compact` in the medium widget.
    private func hourCell(_ hour: ViewModel.Hour, chance: Bool, compact: Bool) -> some View {
        VStack(spacing: compact ? 2 : 4) {
            Text(hour.label).font(.caption2.weight(.semibold)).foregroundStyle(.secondary)
                .lineLimit(1).minimumScaleFactor(0.7)
            ConditionIcon(icon: hour.icon, size: compact ? 16 : 17).frame(height: compact ? 18 : 20)
            Text(hour.temperature.text).font(.subheadline.weight(.medium))
            if chance {
                Text(hour.precipProbability.map { $0 >= 10 ? chanceText(percent: $0) : "" } ?? "")
                    .font(.caption2).foregroundStyle(colors.rain).lineLimit(1)
                    .frame(minHeight: 12)
            }
        }
        .frame(maxWidth: .infinity)
    }

    /// The hours after the current one, `count` of them across the widget, with
    /// (`chance`) their chance of rain when one of them has a chance worth showing.
    private func hourStrip(
        _ view: ViewModel, count: Int, chance: Bool = false, skip: Int = 0, compact: Bool = false
    ) -> some View {
        let hours = Array((view.hourly?.hours ?? []).dropFirst(1 + skip).prefix(count))
        let chance = chance && hours.contains { ($0.precipProbability ?? 0) >= 10 }
        return HStack(spacing: 0) {
            ForEach(hours, id: \.time) { hour in
                hourCell(hour, chance: chance, compact: compact)
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
            VStack(spacing: 4) { hourRows(view, count: 2) }
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

    /// Days as rows: the day, its weather, its low and high, and between them its
    /// span on the week's range (the core scales it to the days listed).
    private func dayRows(_ days: [ViewModel.Day], count: Int) -> some View {
        VStack(spacing: 6) {
            ForEach(Array(days.prefix(count)), id: \.dateShort) { day in
                HStack(spacing: 8) {
                    Text(day.label).font(.subheadline.weight(.medium)).lineLimit(1)
                        .minimumScaleFactor(0.7)
                        .frame(width: 76, alignment: .leading)
                    ConditionIcon(icon: day.icon, size: 16).frame(width: 24)
                    Text(day.precipProbability.map { $0 >= 10 ? chanceText(percent: $0) : "" } ?? "")
                        .font(.caption2).foregroundStyle(colors.rain).lineLimit(1)
                        .minimumScaleFactor(0.8)
                        .frame(width: 28, alignment: .leading)
                    Text(day.low.text).font(.subheadline).foregroundStyle(.secondary)
                        .frame(width: 36, alignment: .trailing)
                    if day.range.count == 2 {
                        RangeBar(low: day.range[0], high: day.range[1]).frame(height: 5)
                            // A faint track: the sky's stronger secondary text would brighten it.
                            .foregroundStyle(Color.primary)
                    } else {
                        Spacer(minLength: 0)
                    }
                    Text(day.high.text).font(.subheadline.weight(.medium))
                        .frame(width: 36, alignment: .leading)
                }
                .monospacedDigit()
            }
        }
    }

    // MARK: Lock Screen

    private func rectangular(_ view: ViewModel, _ now: WidgetNow) -> some View {
        HStack(spacing: 8) {
            Image(systemName: WeatherSymbols.symbol(now.icon)).font(.title2)
            VStack(alignment: .leading, spacing: 0) {
                Text("\(now.temperature.text) \(now.conditionLabel)")
                    .font(.headline).lineLimit(1)
                Text(highLow(now)).font(.caption)
                name(view, font: .caption2).foregroundStyle(.secondary)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// Today's low to high, with the temperature now on it.
    @ViewBuilder private func circular(_ now: WidgetNow) -> some View {
        if let value = now.temperature.value, let low = now.low.value, let high = now.high.value {
            let range = WidgetData.gaugeRange(value: value, low: low, high: high)
            Gauge(value: value, in: range) {
                Image(systemName: WeatherSymbols.symbol(now.icon))
            } currentValueLabel: {
                Text(now.temperature.text)
            } minimumValueLabel: {
                Text(now.low.text)
            } maximumValueLabel: {
                Text(now.high.text)
            }
            .gaugeStyle(.accessoryCircular)
        } else {
            ZStack {
                AccessoryWidgetBackground()
                VStack(spacing: 0) {
                    Image(systemName: WeatherSymbols.symbol(now.icon)).font(.caption)
                    Text(now.temperature.text).font(.title3.weight(.medium))
                }
            }
        }
    }

    private func inline(_ now: WidgetNow) -> some View {
        Label(
            "\(now.temperature.text) \(now.conditionLabel)",
            systemImage: WeatherSymbols.symbol(now.icon))
    }
}

/// Behind a Home Screen widget: the sky for what it leads with (still, without
/// the app's motion), a little deeper in dark mode, or the plain system background.
struct WidgetBackground: View {
    let entry: WeatherWidgetEntry
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        if entry.backdrop == .sky, let sky = entry.now?.sky {
            let (top, bottom) = entry.colorTheme.sky(sky)
            LinearGradient(colors: [top, bottom], startPoint: .top, endPoint: .bottom)
                .overlay(scheme == .dark ? Color.black.opacity(0.2) : .clear)
        } else {
            Color(.systemBackground)
        }
    }
}

/// Apple's Weather mark as the app saved it (`AppleWeatherMark`), or its name until then, and a
/// tap on it opens Apple's page of data sources: WeatherKit's credit in the widgets and the
/// Live Activity, which can't fetch the mark themselves.
struct AppleWeatherBadge: View {
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        if let page = AppleWeatherMark.legalPage {
            Link(destination: page) { mark }
        } else {
            mark
        }
    }

    @ViewBuilder private var mark: some View {
        let file = AppleWeatherMark.file(dark: scheme == .dark)
        if let image = UIImage(contentsOfFile: file.path(percentEncoded: false)) {
            Image(uiImage: image)
                .resizable()
                .scaledToFit()
                .frame(height: 10)
                .accessibilityLabel("Apple Weather")
        } else {
            Text("Apple Weather")
        }
    }
}
