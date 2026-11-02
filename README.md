# Whether

A calm weather app with pluggable displays. One Rust core fetches and formats the forecast; each
display only draws it.

| Display | What it is |
|---|---|
| `whether gui` | Desktop window (Slint), with a wide layout for large monitors and a compact mode |
| `whether tui` | Terminal dashboard (ratatui) |
| `whether line` / `whether json` | One line or JSON for status bars such as waybar |
| `whether daemon` | Fetches in the background; displays attach to it over a socket |
| ESP32 panel | Firmware for small SPI displays, as a terminal-style or graphical panel (`firmware/`) |
| iPhone app | **Pug Forecast**, a SwiftUI app on the same core (`ios/`) |

It shows the feels-like temperature first, rain minute by minute, hourly charts, radar, history
(normals and records) and a trip planner built on past years of weather. There is no account, no
ads and no tracking.

## Data

| Source | Used for |
|---|---|
| [Open-Meteo](https://open-meteo.com/) | forecasts, air quality, history; the always-available fallback |
| [OpenWeather](https://openweathermap.org/) | forecasts and alerts, with your own API key |
| [MET Norway](https://api.met.no/) | backup forecast |
| [National Weather Service](https://www.weather.gov/) | US alerts and forecasts |
| [Pirate Weather](https://pirateweather.net/) | forecasts, with your own API key |
| [NOAA ACIS](https://www.rcc-acis.org/) | US climate normals and records |

Providers are sans-IO: they build requests and parse bytes, and only the runtime does network I/O.
Each source's data stays under its own license; the graphical and terminal displays credit them.
Keys come from the environment (for example `OPENWEATHER_API_KEY`) or, on iPhone, the Keychain,
and are never written to logs.

## Build and run

You need the Rust toolchain pinned in `rust-toolchain.toml` (rustup picks it up).

```sh
cargo run -p whether -- gui          # or: tui, line, json
cargo run -p whether -- locate 10001 --zip --country US
cargo run -p whether -- config check
```

`cargo xtask ci` runs the whole gate: formatting, clippy with warnings denied, tests, the
`no_std` builds and `cargo-deny`. See [CONTRIBUTING.md](CONTRIBUTING.md).

- **iPhone app:** [ios/README.md](ios/README.md) (needs a Mac with Xcode).
- **ESP32 firmware:** [firmware/README.md](firmware/README.md); build only through
  `cargo xtask fw build --board <board>`.
- **Packages and releases:** [packaging/README.md](packaging/README.md).
- **C++ plugin SDK:** `sdk/cpp`. External displays speak the plugin protocol over a socket
  (`sdk/schema`).

## Layout

| Path | Contents |
|---|---|
| `crates/whether-core` | weather types, derived values, the `ViewModel` and all unit conversion and formatting (`no_std`) |
| `crates/whether-providers` | the provider implementations (`no_std`) |
| `crates/whether-config` | the configuration schema |
| `crates/whether-runtime` | scheduling, caching and network I/O |
| `crates/whether-{gui,tui,line,panel}` | the displays |
| `crates/whether-ffi` | the UniFFI bindings the iPhone app uses |
| `apps/` | the `whether` binary and a firmware screen simulator |
| `firmware/` | ESP32 firmware (its own workspace) |
| `ios/` | the iPhone app and its Swift package |
| `xtask/` | build, test, packaging and remote-build automation |

## Privacy

The iPhone app's policy is [PRIVACY.md](PRIVACY.md), published at
<https://seasonalpugforecast.github.io/whether/privacy/>. For help see the
[support page](https://seasonalpugforecast.github.io/whether/support/); for security reports see
[SECURITY.md](SECURITY.md).

## License

MIT, see [LICENSE](LICENSE). The desktop GUI links [Slint](https://slint.dev/) under its
royalty-free license and shows the required attribution in its About dialog. Bundled fonts and
icons keep their own licenses (Inter: OFL; Meteocons: MIT).
