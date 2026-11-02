import Foundation

extension Preferences {
    /// The saved keys of every order and visibility list: what "Restore all
    /// layouts" resets. Everything else (units, the clock, the background, the
    /// sources, the radar's options, the graph's value spacing, the favourite place)
    /// is not a layout and stays.
    static let layoutKeys: Set<String> = [
        "sections", "tabs", "hourlyCards", "daysCards", "sunItems", "graphMeasures", "hourRows",
        "hourTiles", "detailCards", "dayRowItems", "dayDetailItems", "graphOptions",
    ]

    /// These preferences with every order and visibility list back to its default:
    /// the Now tab's sections, the tabs, the Hourly and Days tabs' cards, the Sun &
    /// moon items, the graph's and the 24 hours' measures, the hour tiles, the
    /// detail cards, and the day rows and opened days' items. Nothing else changes.
    func restoringLayouts() -> Preferences {
        var p = self
        p.sections = Preferences.defaultSections
        p.tabs = TabChoice.defaults
        p.hourlyCards = HourlyCardChoice.defaults
        p.daysCards = DaysCardChoice.defaults
        p.sunItems = SunItemChoice.defaults
        p.graphMeasures = Preferences().graphMeasures
        p.hourRows = Preferences().hourRows
        p.graphOptions = GraphOptions()
        p.hourTiles = Preferences.defaultHourTiles
        p.detailCards = Preferences.defaultDetails
        p.dayRowItems = DayRowChoice.defaults
        p.dayDetailItems = DayDetailChoice.defaults
        return p
    }
}
