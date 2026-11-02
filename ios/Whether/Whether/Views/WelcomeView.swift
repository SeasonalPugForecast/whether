import SwiftUI

/// The first screen, until a place is added: where you are, or a search.
struct WelcomeView: View {
    @Bindable var store: WeatherStore
    @State private var locating = false
    @State private var searching = false
    @State private var problem: String?

    var body: some View {
        VStack(spacing: 20) {
            Spacer()
            Image(systemName: "cloud.sun.fill")
                .font(.system(size: 64))
                .symbolRenderingMode(.multicolor)
            Text("Welcome to Seasonal Pug Forecast")
                .font(.largeTitle.weight(.semibold))
                .multilineTextAlignment(.center)
                .padding(.horizontal, 24)
            Text("Forecasts, rain minute by minute, alerts and radar. Choose a place to start; add more any time.")
                .multilineTextAlignment(.center)
                .foregroundStyle(.secondary)
                .padding(.horizontal, 32)
            VStack(spacing: 12) {
                Button {
                    Task { await useLocation() }
                } label: {
                    Label(locating ? "Finding you…" : "Use my location", systemImage: "location.fill")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                .disabled(locating)
                Button {
                    searching = true
                } label: {
                    Label("Search for a place", systemImage: "magnifyingglass")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.bordered)
            }
            .controlSize(.large)
            .padding(.horizontal, 40)
            Text("Your location is read each time the app opens, and the forecast follows you.")
                .font(.footnote)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .padding(.horizontal, 32)
            if let problem {
                Text(problem)
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .padding(.horizontal, 32)
            }
            Spacer()
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(.systemBackground))
        .sheet(isPresented: $searching) { PlacesView(store: store) }
    }

    private func useLocation() async {
        locating = true
        defer { locating = false }
        do {
            await store.follow(try await CurrentLocation.place())
        } catch {
            problem = error.localizedDescription
        }
    }
}
