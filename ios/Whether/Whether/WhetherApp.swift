import SwiftUI

@main
struct WhetherApp: App {
    @State private var store: WeatherStore
    @Environment(\.scenePhase) private var phase
    /// Gives the window scene a delegate that hears CloudKit invitations.
    @UIApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate

    init() {
        // Made here, not on first use: iCloud's pushes and a tap on a notification reach the app
        // delegate before any view, and the system may launch the app in the background for
        // one (`PlanUpdates`).
        let store = WeatherStore()
        _store = State(initialValue: store)
        PlanUpdates.attach(store)
    }

    var body: some Scene {
        WindowGroup {
            ForecastView(store: store)
                .preferredColorScheme(store.colorScheme)
                .modifier(ColorSettings(preferences: store.preferences, theme: store.colorTheme))
                .task { await store.requestNotificationsIfUseful() }
                // A `.whetherplan` or `.whethertheme` opened from Messages, Mail, AirDrop or Files.
                .onOpenURL { store.receiveFile(from: $0) }
                // An invitation to a live shared plan, tapped in Messages or Mail.
                .task { IncomingShares.attach { store.receiveShare($0) } }
                #if DEBUG
                    // Screenshots: `-simulate deutan` shows the screen as that reader sees it.
                    .task { ColorVisionSimulation.startIfAsked() }
                #endif
        }
        .onChange(of: phase) {
            if phase == .active { updateIcon() }
            if phase == .background {
                store.savePendingPlans()
                store.scheduleRefresh()
            }
        }
        .onChange(of: store.preferences.appIcon) { updateIcon() }
        .backgroundTask(.appRefresh(WeatherStore.refreshTask)) {
            await store.backgroundRefresh()
        }
    }

    /// The season's (or the chosen) pug on the home screen.
    private func updateIcon() {
        #if DEBUG
            // Screenshot runs: iOS's "icon changed" alert would cover them.
            let args = ProcessInfo.processInfo.arguments
            if args.contains("-seed") || args.contains("-reset") { return }
        #endif
        applyAppIcon(store.preferences.appIcon)
    }
}

/// The colors for every view: the theme with the color vision's ramps, the
/// color vision itself (the radar repaints its tiles), and whether patterns and
/// symbols show, which by default follows iOS's Differentiate Without Color.
private struct ColorSettings: ViewModifier {
    let preferences: Preferences
    let theme: ColorTheme
    @Environment(\.accessibilityDifferentiateWithoutColor) private var differentiateWithoutColor

    func body(content: Content) -> some View {
        content
            .environment(\.colorTheme, theme)
            .environment(\.colorVision, preferences.colorVision)
            .environment(
                \.showsPatterns,
                preferences.showsPatterns(differentiateWithoutColor: differentiateWithoutColor, theme: theme))
    }
}

extension WeatherStore {
    /// The colors to paint with, from the settings.
    var colorTheme: ColorTheme { preferences.colorTheme() }

    /// Light or dark as the settings say: dark on the sky; by the sun, dark from
    /// sunset to sunrise at the place shown.
    var colorScheme: ColorScheme? {
        // Skies are deep enough for white text, day and night.
        if preferences.background == .sky { return .dark }
        return switch preferences.appearance {
        case .system: nil
        case .light: .light
        case .dark: .dark
        case .sun: view.map { $0.isDay ? .light : .dark }
        }
    }
}
