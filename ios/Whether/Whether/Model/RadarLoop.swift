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
    /// When the frames were last asked for.
    private var loadedAt: Date?

    /// Asks for the frames again while the loop is on screen, after `load`: looking a
    /// minute at a time, once they are `RadarLoopRules.refreshAfter` old (IEM makes a
    /// composite every five minutes), so a loop left open, or the app opened again
    /// later, ends at the latest composite. Runs until its task is cancelled.
    func keepCurrent(place: AppPlace, settings: Settings) async {
        while !Task.isCancelled {
            try? await Task.sleep(for: .seconds(60))
            guard !Task.isCancelled else { return }
            if RadarLoopRules.isDue(loadedAt: loadedAt, now: .now) {
                await load(place: place, settings: settings)
            }
        }
    }

    /// Loads the frames for the place's radar: the NEXRAD loop ending at the latest
    /// composite, OpenWeather's current frame with a key, or none.
    func load(place: AppPlace, settings: Settings) async {
        loadedAt = .now
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
            let (data, _) = try await URLSession.noStore.data(from: url)
            let latest = try radarLatestValid(body: data)
            let frames = radarFrames(latest: latest)
            labels = radarFrameLabels(
                times: frames.map(\.valid), place: place.corePlace(), settings: settings)
            failure = nil
            // No new composite: the loop goes on where it is.
            guard frames != self.frames else { return }
            self.frames = frames
            index = frames.count - 1
        } catch {
            failure = userMessage(error).title
        }
    }

    /// Shows the latest frame.
    func showLatest() {
        index = max(frames.count - 1, 0)
    }

    /// Steps the loop, holding the latest frame a little longer.
    func tick(_ step: inout Int) {
        guard !frames.isEmpty else { return }
        (step, index) = RadarLoopRules.next(after: step, count: frames.count)
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
