import Foundation
import Observation
import WhetherKit

/// The latest radar loop for a place: the frames' tile templates and labels.
@Observable
final class RadarLoop {
    private(set) var frames: [RadarFrame] = []
    private(set) var labels: [String] = []
    private(set) var failure: String?
    /// The frame shown.
    var index = 0

    /// Loads the frames for the place's radar: the NEXRAD loop ending at the latest
    /// composite, OpenWeather's current frame with a key, or none.
    func load(place: AppPlace, settings: Settings) async {
        switch settings.radarSource(for: place) {
        case .nexrad: await loadNexrad(place: place, settings: settings)
        case .openWeather: loadOpenWeather(settings: settings)
        case .none:
            frames = []
            labels = []
            failure = nil
            index = 0
        }
    }

    /// OpenWeather's precipitation layer: one frame, no loop and no time label.
    private func loadOpenWeather(settings: Settings) {
        var frames = radarOpenweatherFrames(key: settings.radarKey ?? "")
        #if DEBUG
            // The fake key loads no tiles: a template nothing can answer.
            if settings.radarKey == Settings.fakeRadarKey {
                frames = frames.map {
                    var frame = $0
                    frame.tileTemplate = "file:///nonexistent/{z}/{x}/{y}.png"
                    return frame
                }
            }
        #endif
        self.frames = frames
        labels = []
        failure = nil
        index = 0
    }

    private func loadNexrad(place: AppPlace, settings: Settings) async {
        do {
            guard let url = URL(string: radarMetaUrl()) else { return }
            let (data, _) = try await URLSession.shared.data(from: url)
            let latest = try radarLatestValid(body: data)
            let frames = radarFrames(latest: latest)
            labels = radarFrameLabels(
                times: frames.map(\.valid), place: place.corePlace(), settings: settings)
            self.frames = frames
            index = frames.count - 1
            failure = nil
        } catch {
            failure = userMessage(error).title
        }
    }

    /// Steps the loop, holding the latest frame a little longer.
    func tick(_ step: inout Int) {
        guard !frames.isEmpty else { return }
        step = (step + 1) % (frames.count + 3)
        index = min(step, frames.count - 1)
    }
}

#if DEBUG
    extension AppPlace {
        /// A place outside the US for the `radar-abroad` screenshot (`-seed -seedLondon`).
        static let london = AppPlace(
            id: "name:London", name: "London", detail: "England, GB", latitude: 51.5072,
            longitude: -0.1276, timeZone: "Europe/London")
    }
#endif
