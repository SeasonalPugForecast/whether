import SwiftUI

/// The open-source notices: the project's own license, the weather data's
/// credits, and every crate in the Rust core with its license text. The crate
/// list is `Acknowledgements.json`, written by `cargo xtask ios licenses`.
struct AcknowledgementsView: View {
    /// Read once, not per view update.
    private static let loaded = AcknowledgementList.load()
    private var notices: AcknowledgementList { Self.loaded }

    /// The weather data's credits (the core's attribution says the same).
    private static let ccBy = URL(string: "https://creativecommons.org/licenses/by/4.0/")

    private static let credits: [(source: String, terms: String, license: URL?)] = [
        ("Open-Meteo.com", "Weather data under CC BY 4.0. Converted to your units and combined.", ccBy),
        (
            "ERA5 (history, normals)",
            "Contains modified Copernicus Climate Change Service information", nil
        ),
        (
            "MET Norway",
            "Backup forecasts. Based on data from the Norwegian Meteorological Institute under CC BY 4.0, converted to your units.",
            ccBy
        ),
        ("NOAA / National Weather Service", "Forecasts, alerts and observations: US government, public domain", nil),
        ("NOAA Regional Climate Centers (ACIS)", "US climate normals and history", nil),
        ("Iowa Environmental Mesonet", "NEXRAD radar tiles from NOAA", nil),
        ("OpenWeather", "Only with your own API key, under OpenWeather's terms", nil),
        (
            "Pirate Weather",
            "Only with your own API key, under Pirate Weather's terms (a blend of NOAA and other public weather models)",
            nil
        ),
        ("Apple Weather", "The next hour minute by minute", nil),
    ]

    var body: some View {
        List {
            Section {
                Text("Seasonal Pug Forecast is free software under the MIT License. Copyright (c) 2026 Sean Avery.")
                    .font(.callout)
            }
            Section("Data") {
                ForEach(Self.credits, id: \.source) { credit in
                    VStack(alignment: .leading, spacing: 2) {
                        Text(credit.source)
                        Text(credit.terms).font(.caption).foregroundStyle(.secondary)
                        if let license = credit.license {
                            Link("CC BY 4.0 license", destination: license).font(.caption)
                        }
                    }
                }
            }
            Section {
                ForEach(notices.crates) { crate in
                    NavigationLink {
                        ScrollView {
                            Text(notices.texts[crate.text])
                                .font(.system(.caption, design: .monospaced))
                                .padding()
                        }
                        .navigationTitle(crate.name)
                        .navigationBarTitleDisplayMode(.inline)
                    } label: {
                        VStack(alignment: .leading, spacing: 2) {
                            Text("\(crate.name) \(crate.version)")
                            Text(crate.license).font(.caption).foregroundStyle(.secondary)
                        }
                    }
                }
            } header: {
                Text("Libraries")
            } footer: {
                Text("The Rust crates in the app's core (whether).")
            }
        }
        .navigationTitle("Acknowledgements")
        .navigationBarTitleDisplayMode(.inline)
    }
}

/// `Acknowledgements.json`: crates pointing into a list of distinct texts.
struct AcknowledgementList: Decodable {
    struct Crate: Decodable, Identifiable {
        let name: String
        let version: String
        let license: String
        /// Index into `texts`.
        let text: Int
        var id: String { "\(name) \(version)" }
    }

    let crates: [Crate]
    let texts: [String]

    /// The bundled list; empty when it is missing or unreadable.
    static func load() -> AcknowledgementList {
        guard let url = Bundle.main.url(forResource: "Acknowledgements", withExtension: "json"),
            let data = try? Data(contentsOf: url),
            let list = try? JSONDecoder().decode(AcknowledgementList.self, from: data),
            list.crates.allSatisfy({ list.texts.indices.contains($0.text) })
        else { return AcknowledgementList(crates: [], texts: []) }
        return list
    }
}
