import ActivityKit
import Foundation
import WhetherKit

/// Starts the rain Live Activity when rain is near (within about two hours at
/// the place shown), keeps it current after each refresh, and ends it when the
/// rain has passed. iOS only lets the app start one while it is open.
nonisolated enum RainActivityController {
    /// The steps that count as near: two hours of quarter hours, or the hour of
    /// minutes.
    private static func near(_ nowcast: ViewModel.Nowcast) -> [Float] {
        Array(nowcast.intensity.prefix(nowcast.stepMinutes == 1 ? 60 : 8))
    }

    static func update(view: ViewModel?, placeID: String?, enabled: Bool) async {
        let running = Activity<RainAttributes>.activities
        guard enabled, let view, let placeID, let nowcast = view.nowcast,
            near(nowcast).contains(where: { $0 > 0.05 }),
            ActivityAuthorizationInfo().areActivitiesEnabled
        else {
            for activity in running {
                await activity.end(nil, dismissalPolicy: .immediate)
            }
            return
        }
        let state = RainAttributes.ContentState(
            text: nowcast.text, bars: near(nowcast),
            credit: nowcast.stepMinutes == 1 ? "Apple Weather" : nil)
        let content = ActivityContent(state: state, staleDate: .now.addingTimeInterval(30 * 60))
        let name = view.location.name
        let plan = RainActivityMatch.plan(
            running: running.map {
                .init(id: $0.id, placeID: $0.attributes.placeID, name: $0.attributes.place)
            }, placeID: placeID, name: name)
        for activity in running where plan.end.contains(activity.id) {
            await activity.end(nil, dismissalPolicy: .immediate)
        }
        if let keep = running.first(where: { $0.id == plan.keep }) {
            await keep.update(content)
        } else {
            _ = try? Activity.request(
                attributes: RainAttributes(place: name, placeID: placeID), content: content)
        }
    }
}
