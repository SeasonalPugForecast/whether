import Foundation

/// SF Symbols for the core's icon names (`IconId`, `UiIcon`), and the condition
/// groups the timeline colors by.
public enum WeatherSymbols {
    /// The SF Symbol for an icon name such as `partly_cloudy_day`.
    public static func symbol(_ icon: String) -> String {
        if icon.hasPrefix("moon_") { return moon(icon) }
        switch icon {
        case "clear_day": return "sun.max.fill"
        case "clear_night": return "moon.stars.fill"
        case "partly_cloudy_day": return "cloud.sun.fill"
        case "partly_cloudy_night": return "cloud.moon.fill"
        case "cloudy", "overcast_day", "overcast_night": return "cloud.fill"
        case "fog_day", "fog_night": return "cloud.fog.fill"
        case "haze": return "sun.haze.fill"
        case "smoke": return "smoke.fill"
        case "dust": return "sun.dust.fill"
        case "drizzle": return "cloud.drizzle.fill"
        case "rain": return "cloud.rain.fill"
        case "extreme_rain": return "cloud.heavyrain.fill"
        case "showers_day": return "cloud.sun.rain.fill"
        case "showers_night": return "cloud.moon.rain.fill"
        case "sleet": return "cloud.sleet.fill"
        case "snow", "snow_showers_day", "snow_showers_night": return "cloud.snow.fill"
        case "extreme_snow": return "snowflake"
        case "thunderstorms_day": return "cloud.sun.bolt.fill"
        case "thunderstorms_night": return "cloud.moon.bolt.fill"
        case "thunderstorms_rain": return "cloud.bolt.rain.fill"
        case "hail": return "cloud.hail.fill"
        case "wind": return "wind"
        case "tornado": return "tornado"
        default: return "questionmark.circle"
        }
    }

    private static func moon(_ icon: String) -> String {
        switch icon.dropFirst("moon_".count) {
        case "new": return "moonphase.new.moon"
        case "waxing_crescent": return "moonphase.waxing.crescent"
        case "first_quarter": return "moonphase.first.quarter"
        case "waxing_gibbous": return "moonphase.waxing.gibbous"
        case "full": return "moonphase.full.moon"
        case "waning_gibbous": return "moonphase.waning.gibbous"
        case "last_quarter": return "moonphase.last.quarter"
        case "waning_crescent": return "moonphase.waning.crescent"
        default: return "moon"
        }
    }

    /// Dark Sky's timeline groups.
    public enum Group: String, Sendable, CaseIterable {
        case clear, partlyCloudy, cloudy, fog, drizzle, rain, heavyRain, snow, storm, other
    }

    /// The timeline group of an icon name.
    public static func group(_ icon: String) -> Group {
        switch icon {
        case "clear_day", "clear_night": return .clear
        case "partly_cloudy_day", "partly_cloudy_night": return .partlyCloudy
        case "cloudy", "overcast_day", "overcast_night": return .cloudy
        case "fog_day", "fog_night", "haze", "smoke", "dust": return .fog
        case "drizzle": return .drizzle
        case "rain", "showers_day", "showers_night", "sleet": return .rain
        case "extreme_rain", "hail": return .heavyRain
        case "snow", "extreme_snow", "snow_showers_day", "snow_showers_night": return .snow
        case "thunderstorms_day", "thunderstorms_night", "thunderstorms_rain", "tornado":
            return .storm
        default: return .other
        }
    }
}
