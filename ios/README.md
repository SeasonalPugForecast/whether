# iOS app (personal use)



- `crates/whether-ffi`: the Rust core, exported to Swift with UniFFI.
- `ios/WhetherKit`: a Swift package over it: the bindings, the view model as
  Swift types, a `URLSession` host.
- `ios/Whether`: the SwiftUI app, Dark Sky style.

## On the Mac

1. Install Xcode and Rust, then the iOS targets:
   `rustup target add aarch64-apple-ios aarch64-apple-ios-sim`
2. From the repository root: `cargo xtask ios build`. It writes
   - `ios/WhetherKit/WhetherFFI.xcframework` (iPhone, simulator and Mac slices),
   - `ios/WhetherKit/Sources/WhetherKit/whether_ffi.swift` (generated bindings).
   Both are build output and not committed.
3. `cd ios/WhetherKit && swift test` runs the package tests on the Mac,
   including one live fetch from Open-Meteo.

On Linux the same command builds the iOS static libraries and the bindings
(useful as a check); only the XCFramework needs `xcodebuild`.

## Building on the Mac from Linux (over SSH)

Name the Mac once, in `ios/remote.local.toml` (gitignored, like every
`*.local.toml`):

```toml
host = "you@your-mac.local"   # anything `ssh` accepts, including a Host alias
# dir = "whether-build"       # on the Mac, relative to its home (the default)
```

or pass `--host`, or set `WHETHER_MAC`. Then, from the repository root:

| Command | Does |
|---|---|
| `cargo xtask ios remote doctor` | shows macOS, Xcode, the SDKs, Swift and free disk space |
| `cargo xtask ios remote sync` | copies the working tree to the Mac |
| `cargo xtask ios remote build` | builds the Rust here, syncs, makes the XCFramework on the Mac |
| `cargo xtask ios remote test` | build, then `swift test` in `ios/WhetherKit` |
| `cargo xtask ios remote app` | build, then the app (`ios/Whether`) for the simulator |
| `cargo xtask ios remote shots` | app, then run it in the simulator; light and dark screenshots land in `target/ios-screens/` |
| `cargo xtask ios remote store-shots [--simulator name]` | app, then the six App Store screenshots (now, hourly, days, radar, plan, widgets), light, on `store_simulator` (default `iPhone 17 Pro Max`, made when the Mac has none; 6.9-inch, 1320×2868) with a 9:41 status bar, into `target/ios-store-shots/`. `--simulator` picks another device (an iPad) and writes `target/ios-store-shots-<name>/` |
| `cargo xtask ios remote device [--iphone name] [--release]` | build, sign and install the app on your iPhone (or the named one; a new phone is registered with the team), then start it; `--release` builds it optimized (main checkout only) |
| `cargo xtask ios remote clean --lane name` / `--all-lanes` | delete a lane's directory and simulator on the Mac (see "Parallel lanes") |

Add `--dry-run` to see the commands without running them. The copy honours
`.gitignore`, so build output, `firmware/whether-fw.toml` and `.env` never leave
this machine; `.git` is not copied either. The directory on the Mac carries a
`.whether-remote` marker, and a directory without one is never overwritten. The
Mac keeps its own `target/`, so later builds are incremental.

All Rust is built on Linux (`rustup target add aarch64-apple-ios
aarch64-apple-ios-sim aarch64-apple-darwin` there); the Mac only wraps the staged
libraries with `xcodebuild` and runs Swift, so it needs Remote Login (System
Settings → General → Sharing), your SSH key, and Xcode 15.3 or newer (UniFFI's
Swift needs Swift 5.10). Xcode is used through `DEVELOPER_DIR`
(`xcode = "/Applications/Xcode.app"` in `ios/remote.local.toml` to change it).

### Parallel lanes (several worktrees at once)

Each linked git worktree is a **lane**, so several can build on the same Mac and build
host without wiping each other's trees. The lane is the worktree directory's name,
lower-cased and reduced to `[a-z0-9-]` (`../whether-lane-a` is `whether-lane-a`);
`WHETHER_LANE=name` overrides it and `WHETHER_LANE=` (empty) switches it off. The main
checkout has no lane and keeps every directory and simulator it always had.

```sh
git worktree add ../whether-lane-a -b lane-a
cd ../whether-lane-a
cargo xtask ios remote test --dry-run   # which config, lane, directories and simulator
cargo xtask ios remote test
```

| | main checkout | lane `whether-lane-a` |
|---|---|---|
| Mac directory | `/Users/Shared/whether-build` | `/Users/Shared/whether-build-whether-lane-a` |
| build host directory | `whether-build` | `whether-build-whether-lane-a` |
| simulator (`app`, `shots`) | `iPhone 17` | `iPhone 17 (whether-lane-a)` |
| simulator (`store-shots`) | `iPhone 17 Pro Max` | `iPhone 17 Pro Max (whether-lane-a)` |
| lock | `<dir>.lock` | `<dir>.lock` |

- **Directories.** The suffix goes onto a directory from `remote.local.toml` /
  `ios/remote.local.toml` or the default. `--dir`, `WHETHER_MAC_DIR` and
  `WHETHER_BUILD_DIR` are used as given. `.derived` (DerivedData), `.stage` and `.screens`
  live inside the directory, so lanes don't share them. A lane directory carries the same
  `.whether-remote` marker, so a foreign directory is never overwritten.
- **Config.** The gitignored `remote.local.toml` and `ios/remote.local.toml` are read from the
  worktree, else from the main worktree. `--dry-run` prints which file it used.
- **Lock.** Every run holds `<dir>.lock` (a directory made with `mkdir`, with an `owner` file)
  until it ends, even on failure. A second run in the same directory prints
  `waiting for <dir> (held by …)` and retries every 10 s. A lock older than 3 hours is taken
  over with a warning, and so is one whose owner is a process on your machine that no longer
  exists (Ctrl-C and `kill` skip the release).
- **Simulator.** `app`, `shots` and `store-shots` clone the configured simulator the first time a lane
  needs it (`xcrun simctl clone`, which requires the source to be shut down, so creating a
  lane's simulator shuts `iPhone 17` down once: don't do it while the main checkout is
  taking screenshots). `test` runs `swift test` on the Mac itself and needs no simulator.
- **Phone and archive stay in the main checkout.** `ios remote device` and `archive` refuse to
  run in a lane: they sign with the Mac's login keychain and install on your phone.
- **Clean up.** `cargo xtask remote clean --lane whether-lane-a` and
  `cargo xtask ios remote clean --lane whether-lane-a` delete the lane's directory (and lock)
  on the build host and on the Mac, and the lane's simulator (`xcrun simctl delete`).
  `--all-lanes` does every lane. Only directories with the marker are removed, never the
  main one. `--dry-run` prints the commands. Then `git worktree remove ../whether-lane-a`.

**What a fresh worktree needs.** Nothing by hand. `ios/WhetherKit/WhetherFFI.xcframework` and
`ios/WhetherKit/Sources/WhetherKit/whether_ffi.swift` are generated and gitignored, and a new
worktree has neither, but `ios remote build|test|app|shots` stages the Rust core itself
(on the build host, in the lane's directory there), copies the stage to `.stage/` on the Mac
and makes both files there. The first run in a lane is a cold build: Rust on the build host
from an empty `target/`, then Swift in an empty `.derived`. `cargo xtask ios build` in the
worktree (for a Mac, or to look at the output) builds all three Apple targets into the
worktree's own `target/` and writes the Swift file locally. It needs
`rustup target add aarch64-apple-ios aarch64-apple-ios-sim aarch64-apple-darwin` once per
machine, like the main checkout. Disk: a lane's directory on the build host grows to about 29 GB
(a full `target/`; the main checkout's is the same size), on the Mac to about 0.4 GB, so
clean lanes up when you're done.

## Using it from Swift

```swift
import WhetherKit

let host = URLSessionHost(ledgerURL: ledgerFile)
let outcome = try await fetch(place: place, settings: settings,
                              cached: cached, now: now, host: host)
// Cache outcome.snapshotJson and outcome.lastModified, then:
let json = try viewModel(locations: entries, active: 0, settings: settings, now: now)
```

The view model JSON has the shape external plugins receive (protocol v1); every
value is already formatted. Each provider's key belongs in the Keychain (one item per
provider, `account` = the provider id) and is passed in `Settings.keys` (the OpenWeather
key also in the older `Settings.openweatherKey`). The order the sources are tried in is
`Settings.sources` (Settings → Sources).

## Building your own copy

If you cloned the repository and want the app on your own iPhone, you need a Mac with Xcode and
the repository (no SSH setup: `cargo xtask ios remote` is only for building from Linux).

1. `cargo xtask ios build` on the Mac stages the Rust core, then make the framework as in
   "On the Mac" above.
2. Create `ios/Whether/Signing.local.xcconfig` (gitignored) with your team and your own bundle ID,
   since `dev.savery.whether` belongs to the author's account:

   ```text
   DEVELOPMENT_TEAM = ABCDE12345
   WHETHER_BUNDLE_ID = dev.yourname.whether
   ```

   The widget follows as `<it>.widget`, and both share the App Group `group.<it>`.
3. Open `ios/Whether/Whether.xcodeproj` in Xcode, pick your iPhone and run. Xcode registers the
   device and creates the profiles (a free Apple ID works for 7 days at a time; WeatherKit needs
   a paid account, otherwise build with the WeatherKit entitlement removed as `--no-weatherkit`
   does).

## Running on your iPhone

Once, on the Mac:

1. Xcode → Settings → Accounts: sign in with your Apple ID (a free one works;
   its builds stop launching after 7 days until you install again).
2. Connect the iPhone by cable, unlock it and trust the Mac. Turn on
   Settings → Privacy & Security → Developer Mode on the phone (it restarts).
3. Find your team ID: open `~/whether-build/ios/Whether/Whether.xcodeproj`,
   select the Whether target → Signing & Capabilities → Team. The ID is also in
   Xcode → Settings → Accounts → your team.

Then add to `ios/remote.local.toml` here:

```toml
team = "ABCDE12345"        # your development team ID
iphone = "Your iPhone"     # the phone's name, as `xcrun devicectl list devices` shows it
```

and run `cargo xtask ios remote device`. It writes the team into
`ios/Whether/Signing.local.xcconfig` on the Mac (gitignored, never overwritten
by a sync), builds, installs and starts the app. If Apple says the bundle
identifier is taken, add `WHETHER_BUNDLE_ID = dev.<you>.whether` to that
file (and the widget follows as `<it>.widget`).

Signing needs the Mac's login keychain, and an SSH session finds it locked
(codesign then fails with `errSecInternalComponent`). So `device` first asks for
the Mac account's password (the SSH account's, typed into the
`security unlock-keychain` prompt; it stays on the Mac) and builds in the same
session. Run it from a terminal: without one the prompt cannot appear and the
step stops before building. The keychain stays unlocked for that SSH session
only.

To deploy without the prompt, keep the password in your desktop's keyring
(GNOME Keyring or KWallet, through `secret-tool`) and name the command that
prints it in `ios/remote.local.toml`:

```sh
secret-tool store --label="whether: Mac keychain" service whether account mac-dev
```

```toml
keychain_password_command = "secret-tool lookup service whether account mac-dev"
```

`device` runs it here and pipes the password into the Mac's
`security unlock-keychain` (never on a command line, never printed). It is
also the Mac account's login password, so anything running as you here can
read it while your keyring is unlocked. On the phone, the first launch may ask you to trust the developer:
Settings → General → VPN & Device Management.

Opening the project in Xcode on the Mac and pressing Run works too; the Team you
pick there is reset by the next sync, the xcconfig is not.

### When you use the Mac from a different account than the SSH one

Put the build directory where every account can reach it, in
`ios/remote.local.toml`:

```toml
dir = "/Users/Shared/whether-build"
```

and give the `staff` group (every local account) inherited access once:

```sh
chmod -R +a "group:staff allow list,add_file,search,add_subdirectory,delete_child,readattr,writeattr,readextattr,writeextattr,readsecurity,read,write,append,execute,delete,file_inherit,directory_inherit" /Users/Shared/whether-build
```

Signing certificates live in the keychain of the account that made the team, so
install on the phone from Xcode in that account (open
`/Users/Shared/whether-build/ios/Whether/Whether.xcodeproj`, choose the team,
press Run). Once a team is chosen, its ID is in the project file; copy it into
`ios/Whether/Signing.local.xcconfig` on the Mac (`DEVELOPMENT_TEAM = …`) and
syncs keep it. Xcode's per-user files (`xcuserdata/`, the workspace) are
gitignored, so syncs leave them alone.

## Sharing plans



- **Send a copy.** Share in the plan editor makes a `.whetherplan` file (a versioned `SharedPlan`
  JSON: name, place, dates, years) and opens the share sheet. The copy goes where you send it
  (Messages, Mail, AirDrop, Files); there is no server.
- **Receive.** Opening a `.whetherplan` file shows the plan with **Add** and **Decline**; nothing
  is saved until Add. If a plan with the same id is already there with different details, the
  choice is **Add as a copy** or **Replace mine**. A file whose payload version `v` is newer than
  the app reads is refused with "This plan needs a newer version of the app."
- **Dates, not instants.** A plan stores calendar days (`2027-07-12`), so it does not shift a day
  between time zones.
- **Debug.** In debug builds `-receivePlan sample|<path>` opens the receive sheet for the sample
  plan or for a `.whetherplan` file; `cargo xtask ios remote shots` includes the `plan-received`
  shot.
- **Live sharing.** Plans that update for everyone (CloudKit) use the app's iCloud and push entitlements (in `Whether.entitlements`): the iCloud
  container `iCloud.<bundle id>` with CloudKit, `aps-environment`, and the
  `remote-notification` background mode. The portal side was done once (2026-10-13): the
  iCloud container `iCloud.dev.savery.whether`, and on the App ID `dev.savery.whether` iCloud
  (CloudKit, that container) and Push Notifications. After capabilities change, refresh the
  profile as for WeatherKit below (Signing & Capabilities in Xcode on the Mac, then run once
  on the phone); `--no-weatherkit` signs with the widget's entitlements, so without iCloud
  too. Testing needs two Apple IDs on two phones.

## Apple Weather (WeatherKit)

The app adds Apple's minute-by-minute next hour where Apple has one, with the
WeatherKit entitlement (`ios/Whether/Whether.entitlements`; the widget has its own
file without it). Its provisioning profile must include WeatherKit, and Xcode only
creates that with your Apple account signed in, so once (and whenever Apple asks
again), in a session on the Mac:

1. Xcode → Settings → Accounts: your Apple ID is signed in (sign in again if it
   says the details were rejected).
2. Open `/Users/Shared/whether-build/ios/Whether/Whether.xcodeproj`, select the
   Whether target → Signing & Capabilities, and let automatic signing update
   (Try Again if it shows an error). WeatherKit appears as a capability.
3. If it still says WeatherKit is not enabled: developer.apple.com → Certificates,
   Identifiers & Profiles → Identifiers → `dev.savery.whether` → enable WeatherKit
   on both the Capabilities and the App Services tabs, save, and try again
   (Apple can take a while to switch it on).
4. Run on the phone once from Xcode. After that `cargo xtask ios remote device`
   signs over SSH with the new profile.

Until then, `cargo xtask ios remote device --no-weatherkit` signs the app with
the widget's entitlements (the App Group only); the next hour stays in 15-minute
steps.

Settings → Sources turns Apple's minutes off (under "Besides the order") and, on the Forecast
models page, picks the forecast models: a
preset (best match, the NOAA blend, US high resolution, ECMWF, ECMWF AI, GEM,
ICON) or a list imported from the desktop config. An older "NOAA blend" switch
becomes the NOAA blend preset.

## Another iPhone in the household

`device` installs on the phone named by `iphone` in `ios/remote.local.toml`.
For another household phone:

1. Connect it to the Mac once with a cable (or pair it in Xcode → Window →
   Devices and Simulators), unlock it and tap Trust.
2. Let Xcode register it: automatic signing and `-allowProvisioningUpdates`
   (already set) add the device to the provisioning profile. A paid developer
   account registers up to 100 iPhones a year.
3. Run `cargo xtask ios remote device --iphone "<its name>"` (the build targets that
   phone, so `-allowProvisioningDeviceRegistration` adds it to the team's devices), the name as
   `xcrun devicectl list devices` shows it. `--no-weatherkit` works with it.

Development builds expire with their provisioning profile (about a year on a
paid account); reinstall then. TestFlight is the cable-free option for other
phones, but it needs an upload, which is your step.

## Importing the desktop config

Settings → Desktop → Import config.toml… reads the desktop's
`~/.config/whether/config.toml`: its `[[location]]` entries (added after the
places already on the phone) and the units, clock, days, precipitation hours,
day icons, One Call and gap filling it sets. Keys it leaves out keep the phone's
choice. The OpenWeather key and the call budget never come across: enter the key
in Settings → Sources → OpenWeather (Keychain), and the phone keeps its own daily call limit (800 by default).

To get the file onto the phone, copy it to the Mac and drag it into Finder →
the iPhone → Files → Whether (the app's Documents, also On My iPhone › Whether
in Files), or use iCloud Drive.

## Colors

.

- **Where.** Settings → Display → Colors. Everything the app paints comes from one `ColorTheme`
  (`ios/Whether/Shared/ColorTheme.swift`): a named role (`sky.clear_day.top`, `level.poor`,
  `radar.3`, …) mapped to a light color and an optional dark one. Roles are **decorative** (skies,
  cards, accent, icon parts) or **informational** (severity, temperature, precipitation type,
  radar steps, condition groups, twilight). The root view resolves the theme once from
  `Preferences.colorTheme(now:)` and puts it in the environment (`\.colorTheme`); the widget and
  the Live Activity resolve the same theme from the shared preferences.
- **Presets** (`Shared/ColorPresets.swift`, data only): Whether (the default, the colors from
  before themes, to the hex), Aurora, Almanac, Sorbet, Neon, Seasons (spring, summer, fall and
  winter looks by month), Graphite (gray, patterns always on) and High contrast (listed first when
  iOS Increase Contrast is on). Tap a card to use it.
- **Color vision** is its own setting and works with any preset: Typical, Red–green (protan and
  deutan), Blue–yellow (tritan) or No color. It replaces only the informational roles, with ramps
  tested to stay apart under the Machado et al. (2009) simulation (ΔE_OK ≥ 0.06); the decorative
  colors stay the preset's. No color also turns patterns on.
- **Patterns and symbols** (Automatic, On, Off; Automatic follows iOS Differentiate Without
  Color): frozen and mixed precipitation bars get a hatch and a snowflake, alerts show their
  severity word, the hourly graph marks rain and snow hours with a symbol, and the radar legend
  becomes five labeled steps. Timeline stretches always carry their labels.
- **Radar.** NOAA's NEXRAD tiles keep NOAA's colors only with the gradient legend: typical
  vision, patterns off and a theme whose radar colors are Whether's. Any other preset, a custom
  radar color, color vision or patterns repaints each tile in the theme's five steps
  (`RadarPalette`: IEM's n0q table read back to dBZ; steps at 20, 35, 45 and 55 dBZ) and shows the
  stepped legend with words, so the legend always describes the map. Repainted tiles are cached in
  memory only while fresh (at most five minutes, `RadarTileCache`). OpenWeather's layer abroad is
  never repainted.
- **Your own colors.** "Customize…" on a preset (or touch and hold its card) makes "My <Preset>"
  and opens the editor: every role grouped as Sky, Weather, Temperature, Severity, Precipitation,
  Radar, Sun & moon and Accent, with a `ColorPicker` (and a second one for dark where the role has
  one), a reset per changed role and "Restore the defaults". The sample card on top can be
  previewed as protan, deutan, tritan or no color, and the contrast lines warn (never block) when
  white text on a sky drops below 4.5:1, a mark below 3:1 on its card, or two neighbouring steps
  get too close for a color-vision reader. Custom themes save only the roles that differ from
  their preset and stay on this phone (no iCloud sync). Swipe or touch and hold one to rename,
  duplicate or delete it.
- **Sharing a theme.** Share in the editor (or in a custom theme's menu) makes a `.whethertheme`
  file: versioned JSON (`v` 1) with the theme's name, its preset, the colors that differ and the
  patterns flag; no id, no place, no settings. Opening one shows the sample with **Add** and
  **Decline**; a file from a newer app version is refused, colors are clamped to sRGB and unknown
  roles dropped. If the same colors are already here, the sheet offers to use them instead.
- **Debug** (debug builds): `-colors <id>`, `-vision <id>`, `-patterns automatic|on|off`,
  `-appearance light|dark|system|sun`, `-customTheme <preset>`, `-editTheme <preset>`,
  `-previewAs <vision>`, `-receiveTheme sample|<path>` and `-simulate
  protan|deutan|tritan|achromat` (redraws the whole window through the color-vision simulation).
  None of them is saved. `cargo xtask ios remote shots` includes `colors`, `now-aurora`,
  `now-almanac-plain`, `now-neon`, the color vision shots (`now-deutan`, `now-tritan`,
  `now-nocolor`, `precip-patterns`, `radar-deutan`, …), `colors-editor`,
  `colors-editor-deutan`, `colors-custom` and `theme-received`.

## Widgets

The widget extension (`ios/Whether/WhetherWidget`, views in `ios/Whether/Shared`)
reads the app's forecasts through the App Group `group.<bundle id>` in signed
builds, so it spends no API calls. Without the group (unsigned simulator builds)
or when the app's data is over two hours old, it fetches Open-Meteo itself and
never writes back. `cargo xtask ios remote shots` includes a gallery of every
size (`-show widgets`, debug builds).
