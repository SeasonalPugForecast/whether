//! `cargo xtask ios remote …`: finish the iOS core on a Mac over SSH.
//!
//! Rust is built here (`cargo xtask ios build` cross-compiles every Apple slice);
//! the Mac only needs Xcode. The working tree is copied with rsync, honouring
//! `.gitignore` (so build output, `firmware/whether-fw.toml` and `.env` never
//! leave this machine), into a directory on the Mac that this command owns: it
//! carries a `.whether-remote` marker, and a directory without one is never
//! overwritten. The staged libraries and bindings follow into `.stage/`; the Mac
//! wraps them in the XCFramework with `xcodebuild` and runs `swift test`.
//!
//! The Mac comes from `--host`, else `WHETHER_MAC`, else `host` in the gitignored
//! `ios/remote.local.toml`; the directory likewise from `--dir`, `WHETHER_MAC_DIR`
//! or `dir`, relative to the Mac's home unless absolute (default `whether-build`);
//! Xcode from `xcode` (default `/Applications/Xcode.app`), used through
//! `DEVELOPER_DIR`, so the Mac's `xcode-select` setting does not matter.
//!
//! **Lanes** (see [`crate::lane`]): a linked git worktree gets `-<lane>` appended to a
//! directory that comes from the config file or the default (`--dir` and
//! `WHETHER_MAC_DIR` are used as given), and `app` and `shots` use a clone of the
//! simulator named `<simulator> (<lane>)`, made on first use. `device` and
//! `archive` sign with the Mac's login keychain, so they refuse to run in a lane. Each
//! run holds a lock on the directory (`<dir>.lock`), so two runs in one directory
//! queue. `store-shots` takes the App Store screenshots (six, light, on an iPhone Pro Max
//! simulator: `store_simulator`, default `iPhone 17 Pro Max`, made if the Mac has none) into
//! `target/ios-store-shots/`; `--simulator <name>` picks another device for any step that
//! uses one (an iPad, say). `clean --lane <name>` / `--all-lanes` removes lane directories (only those
//! with the marker) and their simulators. The config file is read from the worktree,
//! else from the main worktree.

use std::io::Write as _;
use std::process::{Command, Stdio};

use crate::ios::{SWIFT, SWIFT_DEST, XCFRAMEWORK, path, stage, xcframework_args};
use crate::lane::{self, Which};
use crate::workspace_root;

/// The marker that makes a directory on the Mac ours to replace.
const MARKER: &str = ".whether-remote";
/// The default directory on the Mac, under its home.
const DEFAULT_DIR: &str = "whether-build";
/// Where the staged Rust build goes on the Mac, inside the directory.
const STAGE: &str = ".stage";
/// The app's build products on the Mac, inside the directory.
const DERIVED: &str = ".derived";
/// The release archive on the Mac, inside the directory.
const ARCHIVE_DIR: &str = ".archive";
/// Screenshots on the Mac, inside the directory.
const SCREENS: &str = ".screens";
/// App Store screenshots on the Mac, inside the directory.
const STORE_SCREENS: &str = ".store-shots";
/// The simulator for `app` and `shots` when `simulator` is not set.
const DEFAULT_SIMULATOR: &str = "iPhone 17";
/// The simulator for `store-shots` when `store_simulator` is not set: a 6.9-inch
/// iPhone, which the App Store takes at 1320×2868.
const DEFAULT_STORE_SIMULATOR: &str = "iPhone 17 Pro Max";
/// The app's bundle identifier.
const BUNDLE_ID: &str = "dev.savery.whether";
/// Xcode when `xcode` is not set.
const DEFAULT_XCODE: &str = "/Applications/Xcode.app";

const USAGE: &str = "\
usage: cargo xtask ios remote <step> [--host user@mac] [--dir path] [--debug] [--dry-run]
                               [--no-weatherkit] [--iphone name] [--release] [--local]
                               [--simulator name]
       cargo xtask ios remote clean (--lane <name> | --all-lanes) [--host user@mac] [--dry-run]

steps:
  doctor   show the Mac's macOS, Xcode, SDKs and free disk space
  sync     copy the working tree to the Mac
  build    build the Rust here, sync, and make the XCFramework on the Mac
  test     build, then `swift test` in ios/WhetherKit on the Mac
  app      build, then build the app (ios/Whether) for the simulator
  shots    app, then run it in the simulator and copy screenshots (now,
           precipitation, days, details, and now in dark) to target/ios-screens/
  store-shots
           app, then the six App Store screenshots (now, hourly, days, radar, plan,
           widgets) in light on `store_simulator` (default iPhone 17 Pro Max, made
           when missing) to target/ios-store-shots/ (1320x2868)
  archive  build a Release .xcarchive of the app on the Mac and copy it to
           target/ios-archive/ (signing as `device` does). It never exports for
           the App Store or uploads: it prints the next steps
  device   build, sign and install the app on the iPhone named `iphone` in
           ios/remote.local.toml (paired with the Mac; team from `team`). Over
           SSH the Mac's login keychain is locked: it asks for the Mac
           account's password to unlock it (run it in a terminal), or takes
           it from `keychain_password_command` there. --no-weatherkit signs
           without the WeatherKit entitlement, before its profile exists
           (and without iCloud, so such a build has no live plan sharing).
           --iphone <name> installs on that phone instead (a new one is
           registered with the team on its first install).
           --release builds the app optimized, as the archive is (the
           Rust core is always release unless --debug)
  clean    delete a lane's directory and simulator on the Mac (--lane whether-lane-a),
           or every lane's (--all-lanes). Never the main checkout's

A linked git worktree is a lane: its directory on the Mac is `<dir>-<lane>` and its simulator
`<simulator> (<lane>)` (lane = the worktree directory's name; WHETHER_LANE overrides it,
empty switches it off). `device` and `archive` run only from the main checkout.

--simulator <name> uses that simulator instead of `simulator` / `store_simulator`
(for an iPad: store-shots --simulator iPad-name).

The Mac is --host, else WHETHER_MAC, else `host` in ios/remote.local.toml.
The Rust core is built on the build host of `cargo xtask remote` when one answers
(remote.local.toml), else here; --local always builds it here.";

/// What to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Doctor,
    Sync,
    Build,
    Test,
    App,
    Shots,
    StoreShots,
    Device,
    Archive,
    Clean,
}

impl Step {
    /// Whether the Rust core is built and packaged first.
    fn packages(self) -> bool {
        matches!(
            self,
            Self::Build
                | Self::Test
                | Self::App
                | Self::Shots
                | Self::StoreShots
                | Self::Device
                | Self::Archive
        )
    }
}

/// A parsed command line.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // command-line flags
struct Args {
    step: Step,
    host: Option<String>,
    dir: Option<String>,
    debug: bool,
    dry_run: bool,
    /// Sign the app without WeatherKit (`device`).
    no_weatherkit: bool,
    /// The phone to install on (`device`), overriding `iphone` in the config.
    iphone: Option<String>,
    /// Build the app (Swift) in Release for the phone (`device`).
    release: bool,
    /// Build the Rust core here even when a build host answers.
    local: bool,
    /// `clean`: the lane to delete.
    lane: Option<String>,
    /// `clean`: every lane.
    all_lanes: bool,
    /// The simulator to use instead of the configured one.
    simulator: Option<String>,
}

fn parse(args: &[&str]) -> Result<Args, String> {
    let mut iter = args.iter();
    let step = match iter.next() {
        Some(&"doctor") => Step::Doctor,
        Some(&"sync") => Step::Sync,
        Some(&"build") => Step::Build,
        Some(&"test") => Step::Test,
        Some(&"app") => Step::App,
        Some(&"shots") => Step::Shots,
        Some(&"store-shots") => Step::StoreShots,
        Some(&"device") => Step::Device,
        Some(&"archive") => Step::Archive,
        Some(&"clean") => Step::Clean,
        _ => return Err(USAGE.into()),
    };
    let mut out = Args {
        step,
        host: None,
        dir: None,
        debug: false,
        dry_run: false,
        no_weatherkit: false,
        iphone: None,
        release: false,
        local: false,
        lane: None,
        all_lanes: false,
        simulator: None,
    };
    while let Some(arg) = iter.next() {
        match *arg {
            "--host" => out.host = iter.next().map(|s| (*s).to_owned()),
            "--dir" => out.dir = iter.next().map(|s| (*s).to_owned()),
            "--debug" => out.debug = true,
            "--dry-run" => out.dry_run = true,
            "--no-weatherkit" => out.no_weatherkit = true,
            "--release" => out.release = true,
            "--local" => out.local = true,
            "--all-lanes" => out.all_lanes = true,
            "--lane" => match iter.next() {
                Some(name) => out.lane = Some((*name).to_owned()),
                None => return Err(format!("--lane needs a lane name\n\n{USAGE}")),
            },
            "--simulator" => match iter.next() {
                Some(name) if !name.is_empty() => out.simulator = Some((*name).to_owned()),
                _ => return Err(format!("--simulator needs a simulator name\n\n{USAGE}")),
            },
            "--iphone" => match iter.next() {
                Some(name) if !name.is_empty() => out.iphone = Some((*name).to_owned()),
                _ => return Err(format!("--iphone needs a phone name\n\n{USAGE}")),
            },
            other => return Err(format!("unknown argument `{other}`\n\n{USAGE}")),
        }
    }
    if step != Step::Clean && (out.lane.is_some() || out.all_lanes) {
        return Err(format!("--lane and --all-lanes belong to clean\n\n{USAGE}"));
    }
    if step == Step::Clean {
        lane::which(out.lane.as_deref(), out.all_lanes)?;
    }
    Ok(out)
}

/// `ios/remote.local.toml`: `host`, `dir` and `xcode`.
#[derive(Debug, Default)]
struct LocalFile {
    host: Option<String>,
    dir: Option<String>,
    xcode: Option<String>,
    simulator: Option<String>,
    store_simulator: Option<String>,
    team: Option<String>,
    iphone: Option<String>,
    keychain_password_command: Option<String>,
    /// The file these values came from, when there is one.
    path: Option<std::path::PathBuf>,
}

impl LocalFile {
    fn parse(text: &str) -> Result<Self, String> {
        let table: toml::Table = text.parse().map_err(|e: toml::de::Error| e.to_string())?;
        let mut out = Self::default();
        for (key, value) in table {
            let text = value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("`{key}` must be a string"))?;
            match key.as_str() {
                "host" => out.host = Some(text),
                "dir" => out.dir = Some(text),
                "xcode" => out.xcode = Some(text),
                "simulator" => out.simulator = Some(text),
                "store_simulator" => out.store_simulator = Some(text),
                "team" => out.team = Some(text),
                "iphone" => out.iphone = Some(text),
                "keychain_password_command" => out.keychain_password_command = Some(text),
                _ => {
                    return Err(format!(
                        "unknown key `{key}` (use host, dir, xcode, simulator, store_simulator, team, \
                         iphone and keychain_password_command)"
                    ));
                }
            }
        }
        Ok(out)
    }
}

/// The Mac, the directory there, and its Xcode.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Remote {
    host: String,
    /// The directory this run uses: `base_dir` with `-<lane>` in a lane, unless
    /// `--dir` or `WHETHER_MAC_DIR` named it.
    dir: String,
    /// The directory without a lane: the one the main checkout uses.
    base_dir: String,
    /// This worktree's lane, if it is a linked one.
    lane: Option<String>,
    xcode: String,
    /// The simulator this run uses: `base_simulator`, or its clone for the lane.
    simulator: String,
    /// The simulator the main checkout uses for this step: `--simulator`, else the
    /// configured one (`store_simulator` for `store-shots`, `simulator` otherwise).
    base_simulator: String,
    /// The configured simulator `store-shots` uses (`clean` removes its lane clones).
    store_simulator: String,
    /// The configured simulator `app` and `shots` use.
    config_simulator: String,
    /// The Apple development team for signing, if set here.
    team: Option<String>,
    /// The paired iPhone's name, for `device`.
    iphone: Option<String>,
    /// A command here that prints the Mac account's password, for `device` to
    /// unlock the keychain without asking (e.g. `secret-tool lookup …`).
    keychain_password_command: Option<String>,
}

/// `ios/remote.local.toml` of this worktree, else of the main worktree (a lane
/// has none of its own), or nothing when neither exists.
fn local_file() -> Result<LocalFile, String> {
    let Some(path) = lane::config_file("ios/remote.local.toml") else {
        return Ok(LocalFile::default());
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(LocalFile::default());
    };
    LocalFile::parse(&text)
        .map(|file| LocalFile {
            path: Some(path.clone()),
            ..file
        })
        .map_err(|e| {
            format!(
                "{}: {e}\n(values are quoted: host = \"user@mac\")",
                path.display()
            )
        })
}

fn remote(
    args: &Args,
    env: &dyn Fn(&str) -> Option<String>,
    file: LocalFile,
    lane: Option<String>,
) -> Result<Remote, String> {
    let nonempty = |v: Option<String>| v.filter(|s| !s.trim().is_empty());
    let host = nonempty(args.host.clone())
        .or_else(|| nonempty(env("WHETHER_MAC")))
        .or_else(|| nonempty(file.host))
        .ok_or_else(|| {
            "which Mac? pass --host user@mac, set WHETHER_MAC, or put `host = \"user@mac\"` \
             in ios/remote.local.toml"
                .to_string()
        })?;
    let explicit = nonempty(args.dir.clone()).or_else(|| nonempty(env("WHETHER_MAC_DIR")));
    // `clean --dir d` cleans the lanes of `d`; otherwise `d` is not a base at all.
    let named_base = explicit.clone().filter(|_| args.step == Step::Clean);
    let base_dir = named_base
        .or_else(|| nonempty(file.dir))
        .unwrap_or_else(|| DEFAULT_DIR.to_string());
    // A named directory is used as given; the file's or the default one is per lane.
    let dir = match (&explicit, &lane) {
        (Some(dir), _) => dir.clone(),
        (None, Some(lane)) => lane::lane_dir(&base_dir, lane),
        (None, None) => base_dir.clone(),
    };
    for checked in [&dir, &base_dir] {
        if checked.trim_matches('/').is_empty() || checked == "." || checked == "~" {
            return Err(format!(
                "`{checked}` is not a directory this command may replace"
            ));
        }
    }
    let xcode = nonempty(file.xcode).unwrap_or_else(|| DEFAULT_XCODE.to_string());
    let config_simulator =
        nonempty(file.simulator).unwrap_or_else(|| DEFAULT_SIMULATOR.to_string());
    let store_simulator =
        nonempty(file.store_simulator).unwrap_or_else(|| DEFAULT_STORE_SIMULATOR.to_string());
    let base_simulator = nonempty(args.simulator.clone()).unwrap_or_else(|| {
        if args.step == Step::StoreShots {
            store_simulator.clone()
        } else {
            config_simulator.clone()
        }
    });
    let simulator = lane_simulator(&base_simulator, lane.as_deref());
    Ok(Remote {
        host,
        dir,
        base_dir,
        lane,
        xcode,
        simulator,
        base_simulator,
        store_simulator,
        config_simulator,
        team: nonempty(file.team),
        iphone: nonempty(file.iphone),
        keychain_password_command: nonempty(file.keychain_password_command),
    })
}

/// The simulator a lane uses: `<simulator> (<lane>)`; the main checkout's is the
/// configured one.
fn lane_simulator(base: &str, lane: Option<&str>) -> String {
    lane.map_or_else(|| base.to_string(), |lane| format!("{base} ({lane})"))
}

/// `s` for a POSIX shell, in single quotes.
pub(crate) fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// The Mac's Xcode, whatever `xcode-select` says.
fn developer_dir(remote: &Remote) -> String {
    format!(
        "export DEVELOPER_DIR={}",
        quote(&format!("{}/Contents/Developer", remote.xcode))
    )
}

/// A shell line for the Mac: Xcode, then `body` in the directory.
fn in_dir(remote: &Remote, body: &str) -> String {
    format!(
        "set -e; {}; cd {}; {body}",
        developer_dir(remote),
        quote(&remote.dir)
    )
}

/// Refuses a directory without the marker, else creates both.
fn claim(remote: &Remote) -> String {
    let dir = quote(&remote.dir);
    format!(
        "if [ -e {dir} ] && [ ! -e {dir}/{MARKER} ]; then \
         echo {dir} exists and was not made by cargo xtask ios remote >&2; exit 3; fi; \
         mkdir -p {dir} && touch {dir}/{MARKER}"
    )
}

fn rsync_args(remote: &Remote) -> Vec<String> {
    vec![
        "-az".into(),
        "--delete".into(),
        // Every .gitignore applies, so build output and secrets stay here.
        "--filter=:- .gitignore".into(),
        format!("--filter=P /{MARKER}"),
        format!("--filter=P /{STAGE}"),
        format!("--filter=P /{DERIVED}"),
        format!("--filter=P /{SCREENS}"),
        format!("--filter=P /{ARCHIVE_DIR}"),
        "--exclude=/.git".into(),
    ]
    .into_iter()
    .chain(crate::remote::local_excludes())
    .chain(["./".into(), format!("{}:{}/", remote.host, remote.dir)])
    .collect()
}

/// Copies the staged build (`stage`, here) into the directory's `.stage/`.
fn rsync_stage(remote: &Remote, stage: &str) -> Vec<String> {
    vec![
        "-az".into(),
        "--delete".into(),
        format!("{stage}/"),
        format!("{}:{}/{STAGE}/", remote.host, remote.dir),
    ]
}

/// On the Mac: the bindings into the package and the XCFramework from `.stage`.
fn package(remote: &Remote) -> String {
    let args: Vec<String> = xcframework_args(STAGE, XCFRAMEWORK)
        .iter()
        .map(|a| quote(a))
        .collect();
    in_dir(
        remote,
        &format!(
            "cp {STAGE}/{SWIFT} {SWIFT_DEST}/{SWIFT}; rm -rf {XCFRAMEWORK}; \
             xcodebuild {} >/dev/null; echo wrote {XCFRAMEWORK}",
            args.join(" ")
        ),
    )
}

/// On the Mac: the app for the simulator, without signing.
fn app(remote: &Remote) -> String {
    let destination = quote(&format!("platform=iOS Simulator,name={}", remote.simulator));
    in_dir(
        remote,
        &format!(
            "plutil -lint ios/Whether/Whether/PrivacyInfo.xcprivacy \
             ios/Whether/WhetherWidget/PrivacyInfo.xcprivacy || exit 1; \
             xcodebuild -quiet -project ios/Whether/Whether.xcodeproj -scheme Whether \
             -destination {destination} -derivedDataPath {DERIVED} CODE_SIGNING_ALLOWED=NO build; \
             echo built the app"
        ),
    )
}

/// Screenshots: a name and the app's debug launch arguments (`-scrollTo` a
/// section, `-show` a sheet, `-search` in the place list). The first is the top
/// of the screen.
const SHOTS: &[(&str, &str)] = &[
    ("now", "-seed -scrollTo now"),
    ("precipitation", "-seed -scrollTo precipitation"),
    ("hours", "-seed -scrollTo hours"),
    ("days", "-seed -scrollTo days"),
    ("details", "-seed -scrollTo details"),
    ("radar", "-seed -showradar -scrollTo radar"),
    ("radar-full", "-seed -showradar -scrollTo radar -show radar"),
    // Radar options, for the launch only: the hybrid map at 40 % on the
    // full screen; Apple's satellite and the state on the card; and the opacity
    // sheet and the options sheet over a standard map.
    (
        "radar-hybrid",
        "-seed -showradar -scrollTo radar -show radar -radarstyle hybrid -radaropacity 0.4",
    ),
    (
        "radar-section-satellite",
        "-seed -showradar -scrollTo radar -radarstyle satellite -radaropacity 0.5 -radarzoom state",
    ),
    (
        "radar-opacity",
        "-seed -showradar -scrollTo radar -show radar -radarstyle standard -radaropacity 0.4 -radarsheet opacity",
    ),
    (
        "radar-options",
        "-seed -showradar -scrollTo radar -show radar -radarstyle standard -radarspeed fast -radarsheet options",
    ),
    // Outside the US: OpenWeather's layer with a (fake, tile-less) key, and the
    // note without one.
    ("radar-abroad", "-seed -seedLondon -fakeRadarKey -tab radar"),
    ("radar-nokey", "-seed -seedLondon -tab radar"),
    (
        "radar-abroad-section",
        "-seed -seedLondon -fakeRadarKey -showradar -scrollTo radar",
    ),
    // An Apple Weather alert outside the US (a fake one: the simulator's build
    // is unsigned), in the list and opened.
    ("alerts", "-seed -seedLondon -fakeAppleAlert -scrollTo now"),
    (
        "alert-sheet",
        "-seed -seedLondon -fakeAppleAlert -scrollTo now -showAlert",
    ),
    ("history", "-seed -scrollTo history"),
    ("details-edit", "-seed -scrollTo details -editdetails"),
    ("days-tab", "-seed -tab days -expandDay 1"),
    ("days-scrub", "-seed -tab days -expandDay 1 -scrubHour 15"),
    // Facts under each day (air, humidity, wind, UV; for the launch only), then a
    // trimmed day: no chance or forecaster's line on the row, and an opened day
    // without the stretches and the normals. And the editor sheet.
    (
        "days-facts",
        "-seed -tab days -dayshow airTemps,humidity,wind,uv",
    ),
    (
        "days-trimmed",
        "-seed -tab days -expandDay 1 -dayhide precipChance,outlook -daydetailhide stretches,normals",
    ),
    ("days-edit", "-seed -tab days -editdays"),
    ("hourly-tab", "-seed -tab hourly"),
    ("hourly-picked", "-seed -tab hourly -pickHour 9"),
    // Dates: the needle's day and date over the graph, a midnight's pill, the
    // date style for the launch only.
    (
        "hourly-dates",
        "-seed -tab hourly -pickHour 12 -dates compact",
    ),
    (
        "days-dates",
        "-seed -tab days -expandDay 1 -scrubHour 15 -dates monthDayPadded",
    ),
    (
        "plan-picked",
        "-seed -tab plan -trip 2027-07-12:2027-07-19:30 -pickDate 2",
    ),
    (
        "hours-rows",
        "-seed -scrollTo hours -rows temperature,humidity,gusts -pickHour 9",
    ),
    ("sun", "-seed -tab hourly -scrollTo sun"),
    // Astronomical and nautical twilight hidden (for the launch only): the bar
    // starts at civil dawn. And the card's editor sheet.
    (
        "sun-hidden",
        "-seed -tab hourly -scrollTo sun -sunhide astronomical,nautical",
    ),
    ("sun-edit", "-seed -tab hourly -scrollTo sun -editsun"),
    (
        "hourly-wind",
        "-seed -tab hourly -metric temperature,wind,gusts",
    ),
    ("hourly-edit", "-seed -tab hourly -edittiles"),
    // The measures' editors: the Hourly graph's (with value spacing and scales)
    // and the Now tab's 24-hour rows.
    (
        "hourly-measures",
        "-seed -tab hourly -metric temperature,wind,gusts -editmeasures",
    ),
    (
        "hours-measures",
        "-seed -scrollTo hours -rows temperature,humidity,gusts -editrows",
    ),
    // The accessibility text size (the graph's labels scale with it).
    (
        "hourly-large",
        "-seed -tab hourly -UIPreferredContentSizeCategoryName UICTContentSizeCategoryAccessibilityL",
    ),
    // Tabs and their cards (the launch's choice, never saved): two tabs
    // hidden, the tabs reordered, a hidden tab falling back to Now, the tabs'
    // editor, a card hidden or moved on the Hourly and Days tabs, and their editors.
    ("tabs-hidden", "-seed -tabs hourly,plan"),
    (
        "tabs-reordered",
        "-seed -tabs plan,radar,days,hourly -tab hourly",
    ),
    ("tabs-fallback", "-seed -tabs hourly -tab radar"),
    ("tabs-edit", "-seed -show tabs"),
    ("hourly-cards", "-seed -tab hourly -hourlycards sun,hour"),
    ("hourly-no-sun", "-seed -tab hourly -hourlycards hour,graph"),
    ("hourly-cards-edit", "-seed -tab hourly -edithourly"),
    ("days-cards", "-seed -tab days -dayscards history"),
    (
        "days-cards-moved",
        "-seed -tab days -dayscards history,days",
    ),
    ("days-cards-edit", "-seed -tab days -editdaystab"),
    ("plan", "-seed -tab plan -trip 2027-07-12:2027-07-19:30"),
    (
        "plans",
        "-seed -tab plan -trip 2027-07-12:2027-07-19:30 -planlist",
    ),
    ("plan-forecast", "-seed -tab plan -trip +2:+8:30"),
    ("plan-under-way", "-seed -tab plan -trip -2:+4:30"),
    // A plan someone sent, with the Add / Decline sheet over the Plan tab (a built-in
    // sample, so the simulator needs no file).
    ("plan-received", "-seed -tab plan -receivePlan sample"),
    // Live shared plans, with made-up people and no iCloud: the owner's "Shared with"
    // card, the editor of a plan someone shared that may only be looked at (and one that may be
    // edited), the plan list with person badges, and the invitation's Add / Decline sheet.
    (
        "plan-shared-owner",
        "-seed -tab plan -trip 2027-07-12:2027-07-19:30 -sharedPlan owner",
    ),
    (
        "plan-shared-reader",
        "-seed -tab plan -trip 2027-07-12:2027-07-19:30 -sharedPlan reader",
    ),
    (
        "plan-shared-editor",
        "-seed -tab plan -trip 2027-07-12:2027-07-19:30 -sharedPlan editor",
    ),
    (
        "plans-shared",
        "-seed -tab plan -trip 2027-07-12:2027-07-19:30 -planlist -sharedPlan list",
    ),
    ("plan-invited", "-seed -tab plan -receiveShare sample"),
    // Live updates: the banner that says a shared plan changed (a made-up change by
    // Robin, kept up in these runs), and the line for an edit that iCloud hasn't taken yet.
    (
        "plan-changed-banner",
        "-seed -tab plan -planChangedBanner sample",
    ),
    (
        "plan-unsent",
        "-seed -tab plan -trip 2027-07-12:2027-07-19:30 -sharedPlan owner -unsentPlan",
    ),
    ("places", "-seed -show places -search Brooklyn"),
    ("settings", "-seed -show settings"),
    // Customize: every editor in one place; the question before "Restore all
    // layouts"; the list editors for the hour tiles and the detail cards; and the
    // page as the sheet a card's "Customize…" opens.
    ("customize", "-seed -show customize"),
    ("customize-restore", "-seed -show customize -restorelayouts"),
    (
        "hour-tiles-edit",
        "-seed -show customize -customize hourtiles",
    ),
    (
        "detail-cards-edit",
        "-seed -show customize -customize detailcards",
    ),
    ("customize-sheet", "-seed -show customize-sheet"),
    // The sources: the order, and one source's page (OpenWeather's key with a
    // passed test, faked).
    ("sources", "-seed -show sources"),
    (
        "source-openweather",
        "-seed -show source-openweather -keyStatus accepted",
    ),
    // Pirate Weather's page: the key test passed (faked), the monthly calls.
    (
        "source-pirate-weather",
        "-seed -show source-pirate-weather -keyStatus accepted",
    ),
    (
        "settings-forecast",
        "-seed -show settings -scrollTo forecast",
    ),
    ("settings-alerts", "-seed -show settings -scrollTo alerts"),
    ("settings-sources", "-seed -show settings -scrollTo sources"),
    ("compare-sources", "-seed -show compare"),
    ("compare-models", "-seed -show compare-models"),
    ("settings-units", "-seed -show units -units uk"),
    // °C with mph over the phone's region (imperial in the simulator).
    ("now-uk", "-seed -scrollTo now -units uk"),
    // The sun and moon crossing the top of the Now tab (the launch's clock only:
    // `-skyTime` is the share of the day from sunrise (0) to sunset (1), below 0
    // before sunrise and above 1 at night): the morning sun in the centered layout,
    // the afternoon's in Classic, noon on the plain background, and the moon at night.
    ("now-sun-arc", "-seed -scrollTo now -skyTime 0.3"),
    (
        "now-sun-arc-classic",
        "-seed -scrollTo now -darksky -skyTime 0.7",
    ),
    (
        "now-sun-arc-plain",
        "-seed -scrollTo now -plain -skyTime 0.5",
    ),
    ("now-moon-arc", "-seed -scrollTo now -skyTime 1.2"),
    // Colors: the preset gallery, and the Now tab in three presets for the
    // launch only (`-colors <id>`, not saved), Almanac on the plain light background.
    ("colors", "-seed -show colors"),
    ("now-aurora", "-seed -scrollTo now -colors aurora"),
    (
        "now-almanac-plain",
        "-seed -scrollTo now -plain -appearance light -colors almanac",
    ),
    ("now-neon", "-seed -scrollTo now -colors neon"),
    // Your own colors, for the launch only (not saved): "My Aurora"'s editor
    // (with the clear day's top made pale, so a reset button and a contrast warning
    // show), Whether's editor previewed as a deutan reader sees it, and the custom
    // themes listed under the presets.
    ("colors-editor", "-seed -show colors -editTheme aurora"),
    (
        "colors-editor-deutan",
        "-seed -show colors -editTheme whether -previewAs deutan",
    ),
    (
        "colors-custom",
        "-seed -show colors -customTheme aurora -scrollTo custom",
    ),
    // Colors someone sent: the Add / Decline sheet for a `.whethertheme` file (a
    // built-in sample, so the simulator needs no file).
    ("theme-received", "-seed -receiveTheme sample"),
    // Color vision and patterns, for the launch only: the Now tab with each
    // reader's ramps, rain turning to snow hatched (`-precipSample`), the severity word on
    // an alert, wet hours on the graph, the radar repainted with its stepped legend. The
    // `-sim` shots show the screen through the reader's simulation (`-simulate`), and
    // `radar-sim` NOAA's colors as a deutan reader sees them, for comparison.
    ("now-deutan", "-seed -scrollTo now -vision red_green"),
    ("now-tritan", "-seed -scrollTo now -vision blue_yellow"),
    ("now-nocolor", "-seed -scrollTo now -vision no_color"),
    (
        "precip-patterns",
        "-seed -scrollTo nexthours -precipSample -patterns on",
    ),
    (
        "alerts-patterns",
        "-seed -seedLondon -fakeAppleAlert -scrollTo now -patterns on",
    ),
    (
        "hourly-patterns",
        "-seed -tab hourly -precipSample -patterns on",
    ),
    (
        "radar-deutan",
        "-seed -showradar -scrollTo radar -show radar -vision red_green",
    ),
    (
        "now-deutan-sim",
        "-seed -scrollTo now -vision red_green -simulate deutan",
    ),
    (
        "now-tritan-sim",
        "-seed -scrollTo now -vision blue_yellow -simulate tritan",
    ),
    (
        "now-nocolor-sim",
        "-seed -scrollTo now -vision no_color -simulate achromat",
    ),
    (
        "precip-patterns-sim",
        "-seed -scrollTo nexthours -precipSample -vision red_green -patterns on -simulate deutan",
    ),
    (
        "radar-deutan-sim",
        "-seed -showradar -scrollTo radar -show radar -vision red_green -simulate deutan",
    ),
    (
        "radar-sim",
        "-seed -showradar -scrollTo radar -show radar -simulate deutan",
    ),
    // A preset with its own radar colors: the tiles are repainted in them and the
    // legend is the stepped one, so it describes the map.
    (
        "radar-aurora",
        "-seed -showradar -scrollTo radar -show radar -colors aurora",
    ),
    (
        "settings-advanced",
        "-seed -show settings -scrollTo advanced",
    ),
    ("widgets", "-seed -show widgets"),
    // Edit Widget's "Show" choices for the medium and large widgets.
    (
        "widgets-next-hour",
        "-seed -show widgets -widgetcontent nextHour",
    ),
    (
        "widgets-next-hours",
        "-seed -show widgets -widgetcontent nextHours",
    ),
    ("widgets-today", "-seed -show widgets -widgetcontent today"),
    ("acknowledgements", "-seed -show settings -ack"),
    ("offline", "-seed -offline -scrollTo footer"),
    ("updating", "-seed -slow"),
    // A place that follows the phone (the simulator has no location, so
    // `-fakeLocation` gives one: "lat:lon:name:detail", `_` for a space). `-reset` keeps these
    // runs from changing the saved places the others use; `welcome` resets again.
    (
        "places-follow",
        "-reset -seed -threePlaces -seedFollowing -fakeLocation 40.7580:-73.9855:Midtown:New York,_US -show places",
    ),
    // Pinned and favourite places: the third is pinned, the second is the
    // favourite (a star), the first follows the phone. `open-on-favourite` opens on it.
    (
        "places-pins",
        "-reset -seed -threePlaces -seedFollowing -seedPins -fakeLocation 40.7580:-73.9855:Midtown:New York,_US -show places",
    ),
    (
        "open-on-favourite",
        "-reset -seed -threePlaces -seedPins -openOn favourite",
    ),
    (
        "follow-off",
        "-reset -seed -seedFollowing -fakeLocation off -scrollTo footer",
    ),
    ("welcome", "-reset"),
];

/// The App Store screenshots (`store-shots`): the file's name and the launch
/// arguments. The first is the top of the screen's Now tab.
const STORE_SHOTS: &[(&str, &str)] = &[
    ("now", "-seed -scrollTo now"),
    ("hourly", "-seed -tab hourly"),
    ("days", "-seed -tab days -expandDay 1"),
    ("radar", "-seed -tab radar"),
    ("plan", "-seed -tab plan -trip 2027-07-12:2027-07-19:30"),
    ("widgets", "-seed -show widgets"),
];

/// On the Mac: makes the simulator the step needs, unless it exists. `store-shots` makes
/// the configured simulator itself from its device type when the Mac has none (the
/// newest runtime). In a lane the lane's simulator is a clone of that one; `clone` needs
/// the source shut down, so the configured simulator is shut down first (once per
/// lane; the main checkout boots it again when it needs it). The main checkout has
/// nothing to make for the other steps.
fn ensure_simulator(remote: &Remote, step: Step) -> Option<String> {
    let make = step == Step::StoreShots;
    if remote.lane.is_none() && !make {
        return None;
    }
    let (sim, base) = (quote(&remote.simulator), quote(&remote.base_simulator));
    let mut parts = vec![developer_dir(remote)];
    if make {
        // `list` prints `    <name> (<udid>) (<state>)`; a lane's clone shares the name's start.
        let exists = quote(&format!(
            "^    {} \\([0-9A-Fa-f-]{{36}}\\)",
            ere_escape(&remote.base_simulator)
        ));
        let types = quote(&format!(
            "s/^{} (\\(com\\.apple\\.[^)]*\\))$/\\1/p",
            sed_escape(&remote.base_simulator)
        ));
        parts.push(format!(
            "if ! xcrun simctl list devices available | grep -qE {exists}; then \
             t=$(xcrun simctl list devicetypes | sed -n {types} | head -1); \
             if [ -z \"$t\" ]; then echo \"no simulator or device type named \"{base} >&2; exit 1; fi; \
             xcrun simctl create {base} \"$t\"; fi"
        ));
    }
    if remote.lane.is_some() {
        // The device list prints `    <name> (<udid>) (<state>)`: match the whole name.
        let listed = quote(&format!("    {} (", remote.simulator));
        parts.push(format!(
            "if xcrun simctl list devices available | grep -qF {listed}; \
             then echo simulator {sim} exists; \
             else xcrun simctl shutdown {base} 2>/dev/null || true; xcrun simctl clone {base} {sim}; fi"
        ));
    }
    Some(parts.join("; "))
}

/// `text` for an `grep -E` regular expression.
fn ere_escape(text: &str) -> String {
    text.chars()
        .flat_map(|c| {
            if "\\.[]*^$(){}+?|".contains(c) {
                vec!['\\', c]
            } else {
                vec![c]
            }
        })
        .collect()
}

/// `text` for a `sed` basic regular expression.
fn sed_escape(text: &str) -> String {
    text.chars()
        .flat_map(|c| {
            if "\\.[]*^$/".contains(c) {
                vec!['\\', c]
            } else {
                vec![c]
            }
        })
        .collect()
}

/// On the Mac: shuts down and deletes the simulators of the lanes named by `which`,
/// for every simulator a lane clones (`app` and `shots`'s, `store-shots`'s, and the
/// one `--simulator` named).
fn delete_simulators(remote: &Remote, which: &Which) -> String {
    let mut bases: Vec<&str> = Vec::new();
    for base in [
        &remote.config_simulator,
        &remote.store_simulator,
        &remote.base_simulator,
    ] {
        if !bases.contains(&base.as_str()) {
            bases.push(base);
        }
    }
    let mut parts = vec![developer_dir(remote)];
    for base in bases {
        match which {
            Which::One(lane) => {
                let sim = quote(&lane_simulator(base, Some(lane)));
                parts.push(format!(
                    "xcrun simctl shutdown {sim} 2>/dev/null || true; \
                     if xcrun simctl delete {sim} 2>/dev/null; then echo deleted simulator {sim}; \
                     else echo no simulator {sim}; fi"
                ));
            }
            // Lane clones are listed `    <simulator> (<lane>) (<udid>) (<state>)`; the
            // configured one has no lane in parentheses, so it never matches.
            Which::All => {
                let list = quote(&format!(
                    "s/^    {} (.*) (\\([0-9A-Fa-f-]\\{{36\\}}\\)) (.*$/\\1/p",
                    sed_escape(base)
                ));
                parts.push(format!(
                    "for u in $(xcrun simctl list devices | sed -n {list}); do \
                     xcrun simctl shutdown \"$u\" 2>/dev/null || true; \
                     xcrun simctl delete \"$u\" && echo deleted simulator \"$u\"; done"
                ));
            }
        }
    }
    parts.join("; ")
}

/// `device` and `archive` sign with the Mac's login keychain and install on the
/// phone, which only the main checkout does.
fn check_lane(step: Step, lane: Option<&str>) -> Result<(), String> {
    match (step, lane) {
        (Step::Device | Step::Archive, Some(lane)) => Err(format!(
            "`{}` signs with the Mac's login keychain and installs on the phone: \
             run {} from the main checkout (this is the lane `{lane}`)",
            if step == Step::Device {
                "device"
            } else {
                "archive"
            },
            if step == Step::Device {
                "device"
            } else {
                "archive"
            },
        )),
        _ => Ok(()),
    }
}

/// On the Mac: runs the app in the simulator and takes a light screenshot of each
/// of [`SHOTS`] and a dark one of the top.
fn shots(remote: &Remote) -> String {
    capture(remote, SHOTS, SCREENS, true)
}

/// On the Mac: the light App Store screenshots of [`STORE_SHOTS`] in
/// [`STORE_SCREENS`], with the status bar set to 9:41, full battery and signal.
fn store_shots(remote: &Remote) -> String {
    capture(remote, STORE_SHOTS, STORE_SCREENS, false)
}

/// A screenshot smaller than this is a blank (white, not yet drawn) screen: a blank
/// one is about 70 KB, the plainest real ones (the welcome screen, an editor list)
/// about 160 KB, and most well over that.
const BLANK_BYTES: u32 = 120_000;

/// One screenshot: (re)launches the app with `args`, waits `wait` seconds and
/// shoots `dir/name.png`. A busy Mac (other lanes building) can leave the app
/// undrawn after the wait: a blank white screen makes a small PNG, so a shot
/// under [`BLANK_BYTES`] is taken again after a further wait, up to three
/// times, and one still blank is reported rather than passed off. The size
/// comes from `wc -c`: the Mac's `stat` may be GNU's (Homebrew), whose `-f`
/// means something else.
fn launch_line(sim: &str, dir: &str, name: &str, args: &str, wait: u32) -> String {
    let file = format!("{dir}/{name}.png");
    format!(
        "xcrun simctl terminate {sim} {BUNDLE_ID} 2>/dev/null || true; \
         xcrun simctl launch {sim} {BUNDLE_ID} {args} >/dev/null; sleep {wait}; n=0; \
         while :; do xcrun simctl io {sim} screenshot {file} >/dev/null 2>&1; \
         size=$(wc -c < {file} 2>/dev/null) || size=0; [ ${{size:-0}} -ge {BLANK_BYTES} ] && break; \
         n=$((n+1)); if [ $n -ge 3 ]; then echo \"blank screenshot: {name}\" >&2; break; fi; \
         sleep 10; done"
    )
}

/// Runs the app in the simulator once per entry of `list`, taking light
/// screenshots into `dir`, and a dark one of the top when `dark`. With
/// `dark == false` the status bar shows the usual store values.
fn capture(remote: &Remote, list: &[(&str, &str)], dir: &str, dark: bool) -> String {
    let sim = quote(&remote.simulator);
    let app = format!("{DERIVED}/Build/Products/Debug-iphonesimulator/Whether.app");
    let launch = |name: &str, args: &str, wait: u32| launch_line(&sim, dir, name, args, wait);
    let mut steps = vec![format!(
        "rm -rf {dir}; mkdir -p {dir}; xcrun simctl boot {sim} 2>/dev/null || true; \
         xcrun simctl bootstatus {sim} -b >/dev/null; xcrun simctl ui {sim} appearance light; \
         xcrun simctl install {sim} {app}"
    )];
    if !dark {
        steps.push(format!(
            "xcrun simctl status_bar {sim} override --time 9:41 --batteryState charged \
             --batteryLevel 100 --wifiBars 3 --cellularMode active --cellularBars 4 \
             2>/dev/null || true"
        ));
    }
    for (i, (name, args)) in list.iter().enumerate() {
        // The first launch fetches; later ones start from the cache.
        steps.push(launch(name, args, if i == 0 { 12 } else { 8 }));
    }
    if dark {
        steps.push(format!(
            "xcrun simctl ui {sim} appearance dark; {}; xcrun simctl ui {sim} appearance light",
            launch("dark", "-seed -scrollTo now", 5)
        ));
    } else {
        steps.push(format!(
            "xcrun simctl status_bar {sim} clear 2>/dev/null || true"
        ));
    }
    in_dir(remote, &steps.join("; "))
}

/// On the Mac: the signed app for the iPhone, installed and started. Signing
/// uses the Apple ID signed in to Xcode (`-allowProvisioningUpdates`).
/// Signing needs the login keychain, which an SSH session finds locked:
/// `security` asks for the account's password on the terminal (`ssh -t`), or
/// reads it from standard input (`keychain_password_command`), which is then
/// closed so nothing after it reads the password.
const UNLOCK: &str = "security show-keychain-info login.keychain-db >/dev/null 2>&1 || \
     security unlock-keychain login.keychain-db || exit 1; exec </dev/null; ";

/// Writes `team` into `ios/Whether/Signing.local.xcconfig` on the Mac, keeping
/// the file's other lines (a `WHETHER_BUNDLE_ID` override).
fn team_line(remote: &Remote) -> String {
    remote.team.as_deref().map_or_else(String::new, |team| {
        let file = "ios/Whether/Signing.local.xcconfig";
        format!(
            "touch {file}; {{ grep -v '^DEVELOPMENT_TEAM' {file} || true; \
             printf 'DEVELOPMENT_TEAM = %s\\n' {}; }} > {file}.new; mv {file}.new {file}; ",
            quote(team)
        )
    })
}

/// Signs with the widget's entitlements (the App Group only) when WeatherKit's
/// profile is missing. Those have no iCloud either, so the app is also built with the
/// Swift condition `NO_ICLOUD` (`PlanCloud.isAvailable`): `CKContainer(identifier:)` raises
/// an Objective-C exception, which Swift can't catch, in a process without the entitlement,
/// so such a build must never offer or touch live sharing. The value is one shell word
/// (quoted for the remote shell); `$(inherited)` is for xcodebuild, which keeps `DEBUG`.
fn entitlements(no_weatherkit: bool) -> &'static str {
    if no_weatherkit {
        " CODE_SIGN_ENTITLEMENTS=WhetherWidget.entitlements \
         SWIFT_ACTIVE_COMPILATION_CONDITIONS='$(inherited) NO_ICLOUD'"
    } else {
        ""
    }
}

/// The build, install and launch on `iphone`; without WeatherKit, the app is
/// signed with the widget's entitlements (the App Group only), for while its
/// provisioning profile lacks WeatherKit (it falls back to 15-minute steps).
/// Built for that phone, so automatic signing registers a new one with the
/// team (`-allowProvisioningDeviceRegistration`); `release` optimizes the app.
fn device(remote: &Remote, iphone: &str, no_weatherkit: bool, release: bool) -> String {
    let entitlements = entitlements(no_weatherkit);
    let team = team_line(remote);
    let phone = quote(iphone);
    let destination = quote(&format!("platform=iOS,name={iphone}"));
    let configuration = if release { "Release" } else { "Debug" };
    in_dir(
        remote,
        &format!(
            "{team}{UNLOCK}xcodebuild -quiet -project ios/Whether/Whether.xcodeproj -scheme Whether \
             -configuration {configuration} -destination {destination} -derivedDataPath {DERIVED} \
             -allowProvisioningUpdates -allowProvisioningDeviceRegistration build{entitlements}; \
             xcrun devicectl device install app --device {phone} \
             {DERIVED}/Build/Products/{configuration}-iphoneos/Whether.app; \
             xcrun devicectl device process launch --device {phone} {BUNDLE_ID} >/dev/null 2>&1 \
             && echo 'installed and started' \
             || echo 'installed; it did not start (is the phone locked?), so open it on the phone'"
        ),
    )
}

/// On the Mac: a Release archive of the app (`.archive/Whether.xcarchive`),
/// signed like `device`. Nothing is exported or uploaded.
fn archive(remote: &Remote, no_weatherkit: bool) -> String {
    let entitlements = entitlements(no_weatherkit);
    let team = team_line(remote);
    in_dir(
        remote,
        &format!(
            "{team}{UNLOCK}rm -rf {ARCHIVE_DIR}; xcodebuild -quiet -project ios/Whether/Whether.xcodeproj \
             -scheme Whether -configuration Release -destination generic/platform=iOS \
             -derivedDataPath {DERIVED} -archivePath {ARCHIVE_DIR}/Whether.xcarchive \
             -allowProvisioningUpdates archive{entitlements} || exit 1; \
             du -sh {ARCHIVE_DIR}/Whether.xcarchive"
        ),
    )
}

/// Copies the archive back to `local`.
fn fetch_archive(remote: &Remote, local: &str) -> Vec<String> {
    vec![
        "-az".into(),
        "--delete".into(),
        format!(
            "{}:{}/{ARCHIVE_DIR}/Whether.xcarchive/",
            remote.host, remote.dir
        ),
        format!("{local}/Whether.xcarchive/"),
    ]
}

/// What to do with the archive; nothing here uploads it.
const ARCHIVE_NEXT_STEPS: &str = "\
archive copied to target/ios-archive/Whether.xcarchive. Nothing was exported or uploaded.
Next steps (yours): open the archive in Xcode (Window → Organizer → Archives, after copying
it to the Mac's ~/Library/Developer/Xcode/Archives or double-clicking it), then Distribute App →
App Store Connect, and fill in the listing, privacy answers and review notes there.";

/// Copies the screenshots in `dir` (on the Mac) back to `local`.
fn fetch_screens(remote: &Remote, dir: &str, local: &str, delete: bool) -> Vec<String> {
    let mut out = vec!["-az".to_string()];
    if delete {
        out.push("--delete".into());
    }
    out.push(format!("{}:{}/{dir}/", remote.host, remote.dir));
    out.push(format!("{local}/"));
    out
}

/// Where `store-shots` puts its screenshots here: `target/ios-store-shots`, or
/// `target/ios-store-shots-<simulator>` when `--simulator` named another device, so a
/// check on an iPad does not replace the iPhone's.
fn store_shots_dir(simulator: Option<&str>) -> String {
    let slug = simulator.map(|name| {
        let mut slug = String::new();
        for c in name.chars().flat_map(char::to_lowercase) {
            if c.is_ascii_alphanumeric() {
                slug.push(c);
            } else if !slug.ends_with('-') {
                slug.push('-');
            }
        }
        slug.trim_matches('-').to_string()
    });
    match slug.filter(|s| !s.is_empty()) {
        Some(slug) => format!("target/ios-store-shots-{slug}"),
        None => "target/ios-store-shots".to_string(),
    }
}

/// The commands for `args`, after the local build (`stage`: where it is staged):
/// `(program, arguments)`.
fn plan(args: &Args, remote: &Remote, stage: &str) -> Vec<(String, Vec<String>)> {
    let ssh = |line: String| ("ssh".to_string(), vec![remote.host.clone(), line]);
    if args.step == Step::Clean {
        // `parse` checked the lane arguments.
        let which = lane::which(args.lane.as_deref(), args.all_lanes).unwrap_or(Which::All);
        // Under `sh`: the Mac's login shell (zsh) errors on a glob that matches nothing.
        let sh = |line: String| ssh(crate::remote::posix(&line));
        return vec![
            sh(lane::remove_dirs(&remote.base_dir, &which, MARKER)),
            sh(delete_simulators(remote, &which)),
        ];
    }
    if args.step == Step::Doctor {
        return vec![ssh(format!(
            "{}; sw_vers; xcodebuild -version; \
             echo iOS SDK $(xcrun --sdk iphoneos --show-sdk-version); \
             echo simulator SDK $(xcrun --sdk iphonesimulator --show-sdk-version); \
             xcrun swift --version 2>&1 | head -1; df -h ~ | tail -1",
            developer_dir(remote)
        ))];
    }
    let mut steps = vec![
        ssh(claim(remote)),
        ("rsync".to_string(), rsync_args(remote)),
    ];
    if args.step.packages() {
        steps.push(("rsync".to_string(), rsync_stage(remote, stage)));
        steps.push(ssh(package(remote)));
    }
    if args.step == Step::Test {
        steps.push(ssh(in_dir(remote, "cd ios/WhetherKit; swift test")));
    }
    if matches!(args.step, Step::App | Step::Shots | Step::StoreShots) {
        // In a lane: the lane's own simulator, cloned on first use.
        steps.extend(ensure_simulator(remote, args.step).map(ssh));
        steps.push(ssh(app(remote)));
    }
    if args.step == Step::Device {
        match args.iphone.as_ref().or(remote.iphone.as_ref()) {
            // A terminal, so the keychain can ask for the password, unless
            // `run` pipes it in.
            Some(iphone) => {
                let mut ssh = vec![remote.host.clone(), device(remote, iphone, args.no_weatherkit, args.release)];
                if remote.keychain_password_command.is_none() {
                    ssh.insert(0, "-t".into());
                }
                steps.push(("ssh".to_string(), ssh));
            }
            None => steps.push(ssh(format!(
                "{}; echo 'name your iPhone in ios/remote.local.toml (iphone = \"…\"); paired devices:'; \
                 xcrun devicectl list devices; exit 2",
                developer_dir(remote)
            ))),
        }
    }
    if args.step == Step::Archive {
        let mut line = vec![remote.host.clone(), archive(remote, args.no_weatherkit)];
        if remote.keychain_password_command.is_none() {
            line.insert(0, "-t".into());
        }
        steps.push(("ssh".to_string(), line));
        let local = workspace_root().join("target/ios-archive");
        steps.push((
            "rsync".to_string(),
            fetch_archive(remote, &local.to_string_lossy()),
        ));
    }
    if args.step == Step::Shots {
        steps.push(ssh(shots(remote)));
        let local = workspace_root().join("target/ios-screens");
        steps.push((
            "rsync".to_string(),
            fetch_screens(remote, SCREENS, &local.to_string_lossy(), false),
        ));
    }
    if args.step == Step::StoreShots {
        steps.push(ssh(store_shots(remote)));
        let local = workspace_root().join(store_shots_dir(args.simulator.as_deref()));
        steps.push((
            "rsync".to_string(),
            fetch_screens(remote, STORE_SCREENS, &local.to_string_lossy(), true),
        ));
    }
    steps
}

/// The Mac account's password, from `get` (run by `sh` here). It is never
/// printed.
fn keychain_password(get: &str) -> Result<String, String> {
    let out = Command::new("sh")
        .args(["-c", get])
        .stderr(Stdio::inherit())
        .output()
        .map_err(|e| format!("keychain_password_command: {e}"))?;
    let text = String::from_utf8(out.stdout)
        .map_err(|_| "keychain_password_command printed something that is not UTF-8".to_string())?;
    let password = text.trim_end_matches(['\n', '\r']);
    if !out.status.success() || password.is_empty() {
        return Err(format!(
            "keychain_password_command (`{get}`) gave no password ({})",
            out.status
        ));
    }
    Ok(password.to_string())
}

/// Runs `command` with `input` and a newline on its standard input.
fn with_input(command: &mut Command, input: &str) -> std::io::Result<std::process::ExitStatus> {
    let mut child = command.stdin(Stdio::piped()).spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(format!("{input}\n").as_bytes())?;
    }
    child.wait()
}

pub(crate) fn run(args: &[&str]) -> Result<(), String> {
    let args = parse(args)?;
    let file = local_file()?;
    if args.dry_run {
        match &file.path {
            Some(path) => println!("config: {}", path.display()),
            None => {
                println!("config: none (no ios/remote.local.toml here or in the main worktree)");
            }
        }
    }
    let remote = remote(&args, &|k| std::env::var(k).ok(), file, lane::lane())?;
    check_lane(args.step, remote.lane.as_deref())?;
    if args.dry_run {
        match &remote.lane {
            Some(lane) => println!(
                "lane: {lane} (directory {}, simulator {})",
                remote.dir, remote.simulator
            ),
            None => println!(
                "lane: none (the main checkout; directory {}, simulator {})",
                remote.dir, remote.simulator
            ),
        }
    }
    let stage = if args.step.packages() && !args.dry_run {
        // The faster build host when one answers, else here.
        match (args.local, crate::remote::preferred()) {
            (false, Some(host)) => {
                crate::remote::stage_ios(&host, !args.debug)?;
                crate::ios::stage_dir()
            }
            _ => stage(!args.debug)?,
        }
    } else {
        crate::ios::stage_dir()
    };
    let stage = path(&stage)?;
    let steps = plan(&args, &remote, &stage);
    // Held until this function returns, whether the steps succeed or not.
    let _lock = if matches!(args.step, Step::Doctor | Step::Clean) {
        None
    } else {
        lane::lock(
            &remote.host,
            &remote.dir,
            remote.lane.as_deref(),
            args.dry_run,
        )?
    };
    if args.step == Step::Archive && !args.dry_run {
        let dir = workspace_root().join("target/ios-archive");
        std::fs::create_dir_all(&dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    }
    for (program, arguments) in steps {
        println!("$ {program} {}", arguments.join(" "));
        if args.dry_run {
            continue;
        }
        let mut command = Command::new(&program);
        command.args(&arguments).current_dir(workspace_root());
        let password = match &remote.keychain_password_command {
            Some(get)
                if matches!(args.step, Step::Device | Step::Archive)
                    && arguments
                        .last()
                        .is_some_and(|line| line.contains("unlock-keychain")) =>
            {
                Some(keychain_password(get)?)
            }
            _ => None,
        };
        let status = match password {
            Some(password) => with_input(&mut command, &password),
            None => command.status(),
        }
        .map_err(|e| format!("failed to start `{program}`: {e}"))?;
        if !status.success() {
            return Err(format!("`{program}` failed with {status}"));
        }
    }
    if args.step == Step::Archive && !args.dry_run {
        println!("{ARCHIVE_NEXT_STEPS}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(step: Step) -> Args {
        Args {
            step,
            host: Some("me@mac.local".into()),
            dir: None,
            debug: false,
            dry_run: false,
            no_weatherkit: false,
            iphone: None,
            release: false,
            local: false,
            lane: None,
            all_lanes: false,
            simulator: None,
        }
    }

    #[test]
    fn parses_steps_and_flags() {
        let a = parse(&[
            "test",
            "--host",
            "me@mac",
            "--dir",
            "/tmp/w",
            "--debug",
            "--dry-run",
            "--no-weatherkit",
            "--iphone",
            "Kid's iPhone",
            "--release",
            "--local",
        ])
        .unwrap();
        assert!(a.release && a.local);
        assert_eq!(a.iphone.as_deref(), Some("Kid's iPhone"));
        assert_eq!(a.step, Step::Test);
        assert_eq!(
            (a.host.as_deref(), a.dir.as_deref(), a.debug, a.dry_run),
            (Some("me@mac"), Some("/tmp/w"), true, true)
        );
        assert!(a.no_weatherkit);
        assert!(parse(&["device", "--iphone"]).is_err());
        assert!(parse(&[]).is_err());
        assert!(parse(&["build", "--fast"]).is_err());
    }

    #[test]
    fn the_host_comes_from_flags_then_the_environment() {
        let env = |k: &str| (k == "WHETHER_MAC").then(|| "env@mac".to_string());
        let mut a = args(Step::Build);
        assert_eq!(
            remote(&a, &env, LocalFile::default(), None).unwrap().host,
            "me@mac.local"
        );
        a.host = None;
        let r = remote(&a, &env, LocalFile::default(), None).unwrap();
        assert_eq!((r.host.as_str(), r.dir.as_str()), ("env@mac", DEFAULT_DIR));
        a.dir = Some("~".into());
        assert!(
            remote(&a, &env, LocalFile::default(), None).is_err(),
            "never the home directory itself"
        );
        a.dir = None;
        let file = LocalFile {
            host: Some("file@mac".into()),
            dir: Some("w".into()),
            xcode: None,
            simulator: None,
            store_simulator: None,
            team: None,
            iphone: None,
            keychain_password_command: None,
            path: None,
        };
        let r = remote(&a, &|_| None, file, None).unwrap();
        assert_eq!((r.host.as_str(), r.dir.as_str()), ("file@mac", "w"));
        assert_eq!(r.xcode, DEFAULT_XCODE);
    }

    #[test]
    fn the_local_file_names_host_and_dir() {
        let f = LocalFile::parse(
            "host = \"me@mac\"\ndir = \"src/whether\"\nxcode = \"/Applications/Xcode-15.app\"\n",
        )
        .unwrap();
        assert_eq!(f.xcode.as_deref(), Some("/Applications/Xcode-15.app"));
        assert_eq!(
            (f.host.as_deref(), f.dir.as_deref()),
            (Some("me@mac"), Some("src/whether"))
        );
        assert!(LocalFile::parse("hots = \"x\"").is_err());
        assert!(LocalFile::parse("host = 1").is_err());
    }

    #[test]
    fn device_installs_on_the_phone_named() {
        let r = Remote {
            host: "me@mac.local".into(),
            dir: "d".into(),
            base_dir: "d".into(),
            lane: None,
            xcode: DEFAULT_XCODE.into(),
            simulator: DEFAULT_SIMULATOR.into(),
            base_simulator: DEFAULT_SIMULATOR.into(),
            store_simulator: DEFAULT_STORE_SIMULATOR.into(),
            config_simulator: DEFAULT_SIMULATOR.into(),
            team: None,
            iphone: Some("Alex's iPhone".into()),
            keychain_password_command: None,
        };
        let other = Args {
            iphone: Some("Sam's iPhone".into()),
            no_weatherkit: true,
            ..args(Step::Device)
        };
        let other = plan(&other, &r, "s");
        let line = &other.last().unwrap().1[2];
        assert!(line.contains(r"--device 'Sam'\''s iPhone'"), "{line}");
        // Built for that phone, so a new one is registered with the team.
        assert!(
            line.contains(r"-destination 'platform=iOS,name=Sam'\''s iPhone'"),
            "{line}"
        );
        assert!(
            line.contains("-allowProvisioningDeviceRegistration"),
            "{line}"
        );
        assert!(
            line.contains("-configuration Debug") && line.contains("Debug-iphoneos/Whether.app")
        );
        assert!(!line.contains("Alex"), "{line}");
        assert!(line.contains("CODE_SIGN_ENTITLEMENTS=WhetherWidget.entitlements"));
        assert!(line.contains("NO_ICLOUD"), "{line}");
    }

    #[test]
    fn device_release_builds_the_optimized_app() {
        let r = Remote {
            host: "me@mac.local".into(),
            dir: "d".into(),
            base_dir: "d".into(),
            lane: None,
            xcode: DEFAULT_XCODE.into(),
            simulator: DEFAULT_SIMULATOR.into(),
            base_simulator: DEFAULT_SIMULATOR.into(),
            store_simulator: DEFAULT_STORE_SIMULATOR.into(),
            config_simulator: DEFAULT_SIMULATOR.into(),
            team: None,
            iphone: Some("Phone".into()),
            keychain_password_command: None,
        };
        let release = Args {
            release: true,
            ..args(Step::Device)
        };
        let release = plan(&release, &r, "s");
        let line = &release.last().unwrap().1[2];
        assert!(line.contains("-configuration Release"), "{line}");
        assert!(
            line.contains(".derived/Build/Products/Release-iphoneos/Whether.app"),
            "{line}"
        );
    }

    #[test]
    fn the_archive_is_a_signed_release_build_that_is_never_uploaded() {
        let r = Remote {
            host: "me@mac.local".into(),
            dir: "it's here".into(),
            base_dir: "it's here".into(),
            lane: None,
            xcode: DEFAULT_XCODE.into(),
            simulator: DEFAULT_SIMULATOR.into(),
            base_simulator: DEFAULT_SIMULATOR.into(),
            store_simulator: DEFAULT_STORE_SIMULATOR.into(),
            config_simulator: DEFAULT_SIMULATOR.into(),
            team: Some("ABCDE12345".into()),
            iphone: None,
            keychain_password_command: None,
        };
        let archive = plan(&args(Step::Archive), &r, "s");
        let programs: Vec<&str> = archive.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(programs, ["ssh", "rsync", "rsync", "ssh", "ssh", "rsync"]);
        assert_eq!(archive[4].1[0], "-t");
        let line = &archive[4].1[2];
        assert!(line.contains("-configuration Release"), "{line}");
        assert!(
            line.contains("-archivePath .archive/Whether.xcarchive"),
            "{line}"
        );
        assert!(line.contains("-allowProvisioningUpdates archive"), "{line}");
        // The team is added to the local signing file without dropping its other lines.
        assert!(line.contains("grep -v '^DEVELOPMENT_TEAM' ios/Whether/Signing.local.xcconfig"));
        assert!(
            line.contains("mv ios/Whether/Signing.local.xcconfig.new"),
            "{line}"
        );
        assert!(
            !line.contains("> ios/Whether/Signing.local.xcconfig;"),
            "{line}"
        );
        assert!(
            line.contains("security unlock-keychain login.keychain-db"),
            "{line}"
        );
        for forbidden in [
            "exportArchive",
            "altool",
            "notarytool",
            "upload",
            "Transporter",
        ] {
            assert!(!line.contains(forbidden), "{forbidden}: {line}");
        }
        assert!(
            archive[5]
                .1
                .last()
                .unwrap()
                .ends_with("target/ios-archive/Whether.xcarchive/"),
            "{:?}",
            archive[5].1
        );
        let plain = Args {
            no_weatherkit: true,
            ..args(Step::Archive)
        };
        assert!(plan(&plain, &r, "s")[4].1[2].contains(
            "archive CODE_SIGN_ENTITLEMENTS=WhetherWidget.entitlements \
             SWIFT_ACTIVE_COMPILATION_CONDITIONS='$(inherited) NO_ICLOUD'"
        ));
        assert!(!archive[4].1[2].contains("NO_ICLOUD"), "{:?}", archive[4].1);
    }

    fn lane_remote(lane: Option<&str>) -> Remote {
        let file = LocalFile {
            host: Some("me@mac".into()),
            dir: Some("/Users/Shared/whether-build".into()),
            ..LocalFile::default()
        };
        let a = Args {
            host: None,
            ..args(Step::App)
        };
        remote(&a, &|_| None, file, lane.map(str::to_owned)).unwrap()
    }

    #[test]
    fn a_lane_gets_its_own_directory_and_simulator_unless_one_is_named() {
        let main = lane_remote(None);
        assert_eq!(main.dir, "/Users/Shared/whether-build");
        assert_eq!(main.simulator, "iPhone 17");
        assert_eq!(main.lane, None);
        let lane = lane_remote(Some("whether-lane-a"));
        assert_eq!(lane.dir, "/Users/Shared/whether-build-whether-lane-a");
        assert_eq!(lane.base_dir, "/Users/Shared/whether-build");
        assert_eq!(lane.simulator, "iPhone 17 (whether-lane-a)");
        assert_eq!(lane.base_simulator, "iPhone 17");
        // --dir and WHETHER_MAC_DIR are used as given, but the simulator stays per lane.
        let named = Args {
            dir: Some("/Users/Shared/mine".into()),
            ..args(Step::App)
        };
        let r = remote(&named, &|_| None, LocalFile::default(), Some("a".into())).unwrap();
        assert_eq!(
            (r.dir.as_str(), r.simulator.as_str()),
            ("/Users/Shared/mine", "iPhone 17 (a)")
        );
        let env = |k: &str| (k == "WHETHER_MAC_DIR").then(|| "/m".to_string());
        let bare = Args {
            dir: None,
            ..args(Step::App)
        };
        assert_eq!(
            remote(&bare, &env, LocalFile::default(), Some("a".into()))
                .unwrap()
                .dir,
            "/m"
        );
        // The configured simulator is the base.
        let file = LocalFile {
            simulator: Some("iPhone 16".into()),
            ..LocalFile::default()
        };
        let r = remote(&bare, &|_| None, file, Some("b".into())).unwrap();
        assert_eq!(r.simulator, "iPhone 16 (b)");
    }

    #[test]
    fn a_lane_clones_the_simulator_for_app_and_shots_but_the_main_checkout_does_not() {
        let laned = lane_remote(Some("whether-lane-a"));
        let line = ensure_simulator(&laned, Step::App).unwrap();
        assert!(line.contains("DEVELOPER_DIR="), "{line}");
        assert!(
            line.contains("grep -qF '    iPhone 17 (whether-lane-a) ('"),
            "the whole name is matched: {line}"
        );
        assert!(
            line.contains("xcrun simctl shutdown 'iPhone 17' 2>/dev/null || true"),
            "{line}"
        );
        assert!(
            line.contains("xcrun simctl clone 'iPhone 17' 'iPhone 17 (whether-lane-a)'"),
            "{line}"
        );
        assert_eq!(ensure_simulator(&lane_remote(None), Step::Shots), None);
        // app and shots clone before building and use the lane's simulator.
        for step in [Step::App, Step::Shots] {
            let p = plan(&args(step), &laned, "s");
            let clone = p
                .iter()
                .position(|(_, a)| a.last().is_some_and(|l| l.contains("simctl clone")));
            let build = p
                .iter()
                .position(|(_, a)| a.last().is_some_and(|l| l.contains("xcodebuild -quiet")))
                .unwrap();
            assert!(clone.unwrap() < build, "{step:?}");
            let build = p[build].1.last().unwrap();
            assert!(
                build.contains(
                    "-destination 'platform=iOS Simulator,name=iPhone 17 (whether-lane-a)'"
                ),
                "{build}"
            );
        }
        let shots = plan(&args(Step::Shots), &laned, "s");
        let run = shots
            .iter()
            .find(|(_, a)| a.last().is_some_and(|l| l.contains("simctl launch")))
            .unwrap();
        assert!(
            run.1
                .last()
                .unwrap()
                .contains("simctl launch 'iPhone 17 (whether-lane-a)' dev.savery")
        );
        // The tests run on the Mac itself, and the main checkout is untouched.
        let test = plan(&args(Step::Test), &laned, "s");
        assert!(
            test.iter()
                .all(|(_, a)| !a.last().unwrap().contains("simctl"))
        );
        let main = plan(&args(Step::App), &lane_remote(None), "s");
        assert!(
            main.iter()
                .all(|(_, a)| !a.last().unwrap().contains("simctl clone"))
        );
    }

    /// Runs one shot's line in `sh`, with `xcrun`, `wc` and `sleep` stood in
    /// for: the screenshot is blank (small) for the first `blank` tries.
    fn run_shot(blank: u32) -> (u32, String) {
        let line = launch_line("'iPhone 17'", "shots", "now", "-seed", 8);
        let script = format!(
            "tries=0; xcrun() {{ case \"$1 $2\" in 'simctl io') tries=$((tries+1)); \
             mkdir -p shots; : > shots/now.png;; esac; }}; \
             wc() {{ if [ $tries -gt {blank} ]; then echo 1500000; else echo 70000; fi; }}; \
             sleep() {{ :; }}; {line}; echo $tries"
        );
        // The stand-in writes the screenshot file, so in a directory of its own.
        let dir = std::env::temp_dir().join(format!("whether-shot-{}-{blank}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let out = Command::new("sh")
            .args(["-c", &script])
            .current_dir(&dir)
            .output()
            .expect("sh runs");
        let _ = std::fs::remove_dir_all(&dir);
        let tries = String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse()
            .unwrap_or(0);
        (tries, String::from_utf8_lossy(&out.stderr).to_string())
    }

    #[test]
    fn a_blank_screenshot_is_taken_again_and_reported_if_it_stays_blank() {
        assert_eq!(run_shot(0), (1, String::new()), "drawn at once: one shot");
        assert_eq!(run_shot(2), (3, String::new()), "drawn on the third try");
        let (tries, err) = run_shot(9);
        assert_eq!(tries, 3, "three tries at most");
        assert!(err.contains("blank screenshot: now"), "{err}");
    }

    #[test]
    fn color_vision_shots_set_vision_patterns_and_simulation_for_the_launch() {
        let launch = |name: &str| SHOTS.iter().find(|(n, _)| *n == name).map(|(_, l)| *l);
        for (name, wanted) in [
            ("now-deutan", "-vision red_green"),
            ("now-tritan", "-vision blue_yellow"),
            ("now-nocolor", "-vision no_color"),
            ("precip-patterns", "-patterns on"),
            ("radar-deutan", "-vision red_green"),
            ("now-deutan-sim", "-simulate deutan"),
            ("now-tritan-sim", "-simulate tritan"),
            ("now-nocolor-sim", "-simulate achromat"),
            ("radar-deutan-sim", "-simulate deutan"),
        ] {
            let line = launch(name).unwrap_or_default();
            assert!(line.contains(wanted), "{name}: {line}");
        }
        assert!(launch("precip-patterns").is_some_and(|l| l.contains("-precipSample")));
        assert!(launch("radar-deutan").is_some_and(|l| l.contains("-show radar")));
        assert!(launch("radar-sim").is_some_and(|l| !l.contains("-vision")));
        assert!(launch("radar-aurora").is_some_and(|l| {
            l.contains("-show radar") && l.contains("-colors aurora") && !l.contains("-vision")
        }));
    }

    #[test]
    fn color_shots_open_the_gallery_and_pick_a_preset_for_the_launch() {
        let launch = |name: &str| SHOTS.iter().find(|(n, _)| *n == name).map(|(_, l)| *l);
        assert_eq!(launch("colors"), Some("-seed -show colors"));
        for (name, id) in [
            ("now-aurora", "aurora"),
            ("now-almanac-plain", "almanac"),
            ("now-neon", "neon"),
        ] {
            let line = launch(name).unwrap_or_default();
            assert!(line.contains(&format!("-colors {id}")), "{name}: {line}");
        }
        assert!(launch("now-almanac-plain").is_some_and(|l| l.contains("-plain")));
        // The theme editor opens over the gallery; the deutan one previews.
        for name in ["colors-editor", "colors-editor-deutan"] {
            let line = launch(name).unwrap_or_default();
            assert!(line.contains("-show colors -editTheme "), "{name}: {line}");
        }
        assert!(launch("colors-editor-deutan").is_some_and(|l| l.contains("-previewAs deutan")));
        assert!(launch("colors-custom").is_some_and(|l| l.contains("-customTheme aurora")));
        assert!(launch("theme-received").is_some_and(|l| l.contains("-receiveTheme sample")));
        // Each shot is a file: no name twice.
        let mut names: Vec<&str> = SHOTS.iter().map(|(n, _)| *n).collect();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count);
    }

    #[test]
    fn store_shots_take_six_light_shots_on_the_pro_max_simulator() {
        let a = parse(&["store-shots", "--simulator", "iPad (A16)", "--dry-run"]).unwrap();
        assert_eq!(a.step, Step::StoreShots);
        assert_eq!(a.simulator.as_deref(), Some("iPad (A16)"));
        assert!(parse(&["store-shots", "--simulator"]).is_err());
        assert!(Step::StoreShots.packages());
        // The six screens of the plan, in order, all with the seeded place.
        let names: Vec<&str> = STORE_SHOTS.iter().map(|(n, _)| *n).collect();
        assert_eq!(names, ["now", "hourly", "days", "radar", "plan", "widgets"]);
        assert!(STORE_SHOTS.iter().all(|(_, l)| l.starts_with("-seed")));
        // Without --simulator: the store simulator, not the configured one.
        let main = remote(
            &args(Step::StoreShots),
            &|_| None,
            LocalFile::default(),
            None,
        )
        .unwrap();
        assert_eq!(main.simulator, "iPhone 17 Pro Max");
        let shots = remote(&args(Step::Shots), &|_| None, LocalFile::default(), None).unwrap();
        assert_eq!(shots.simulator, "iPhone 17");
        let file = LocalFile {
            store_simulator: Some("iPhone 16 Pro Max".into()),
            ..LocalFile::default()
        };
        let r = remote(&args(Step::StoreShots), &|_| None, file, Some("c".into())).unwrap();
        assert_eq!(
            (r.simulator.as_str(), r.base_simulator.as_str()),
            ("iPhone 16 Pro Max (c)", "iPhone 16 Pro Max")
        );
        let ipad = Args {
            simulator: Some("iPad (A16)".into()),
            ..args(Step::StoreShots)
        };
        let r = remote(&ipad, &|_| None, LocalFile::default(), Some("c".into())).unwrap();
        assert_eq!(r.simulator, "iPad (A16) (c)");
        assert_eq!(
            r.store_simulator, "iPhone 17 Pro Max",
            "clean still knows it"
        );
        // --simulator also picks the device for shots.
        let on_ipad = Args {
            simulator: Some("iPad (A16)".into()),
            ..args(Step::Shots)
        };
        let r = remote(&on_ipad, &|_| None, LocalFile::default(), None).unwrap();
        assert_eq!(r.simulator, "iPad (A16)");
    }

    #[test]
    fn store_shots_commands_make_build_run_and_fetch() {
        let names: Vec<&str> = STORE_SHOTS.iter().map(|(n, _)| *n).collect();
        let file = || LocalFile {
            host: Some("me@mac".into()),
            dir: Some("/Users/Shared/whether-build".into()),
            ..LocalFile::default()
        };
        let store_remote = |lane: Option<&str>| {
            let a = Args {
                host: None,
                ..args(Step::StoreShots)
            };
            remote(&a, &|_| None, file(), lane.map(str::to_owned)).unwrap()
        };
        let main = store_remote(None);
        let p = plan(&args(Step::StoreShots), &main, "s");
        let lines: Vec<&String> = p.iter().map(|(_, a)| a.last().unwrap()).collect();
        let make = lines
            .iter()
            .position(|l| l.contains("simctl create"))
            .unwrap();
        let build = lines
            .iter()
            .position(|l| l.contains("xcodebuild -quiet"))
            .unwrap();
        assert!(make < build, "the device exists before the build");
        let create = lines[make];
        assert!(
            create.contains("grep -qE '^    iPhone 17 Pro Max \\([0-9A-Fa-f-]{36}\\)'"),
            "{create}"
        );
        assert!(
            create.contains("sed -n 's/^iPhone 17 Pro Max (\\(com\\.apple\\.[^)]*\\))$/\\1/p'"),
            "{create}"
        );
        assert!(
            create.contains("xcrun simctl create 'iPhone 17 Pro Max' \"$t\""),
            "{create}"
        );
        assert!(
            !create.contains("simctl clone"),
            "the main checkout does not clone"
        );
        assert!(
            lines[build].contains("-destination 'platform=iOS Simulator,name=iPhone 17 Pro Max'")
        );
        let run = p.iter().rev().nth(1).unwrap().1.last().unwrap();
        assert!(run.contains("appearance light"), "{run}");
        assert!(!run.contains("appearance dark"), "light only: {run}");
        assert!(
            run.contains("status_bar 'iPhone 17 Pro Max' override --time 9:41"),
            "{run}"
        );
        assert_eq!(run.matches("simctl launch").count(), 6, "{run}");
        for name in names {
            assert!(run.contains(&format!(".store-shots/{name}.png")), "{name}");
        }
        assert!(
            run.contains("simctl launch 'iPhone 17 Pro Max' dev.savery.whether -seed -tab radar")
        );
        let fetch = &p.last().unwrap().1;
        assert_eq!(fetch[0], "-az");
        assert!(fetch.contains(&"--delete".to_string()));
        assert_eq!(fetch[2], "me@mac:/Users/Shared/whether-build/.store-shots/");
        assert!(fetch[3].ends_with("/target/ios-store-shots/"), "{fetch:?}");
        // Another device goes to its own folder.
        assert_eq!(
            store_shots_dir(Some("iPad Air 11-inch (M4)")),
            "target/ios-store-shots-ipad-air-11-inch-m4"
        );
        assert_eq!(store_shots_dir(None), "target/ios-store-shots");
        // `shots` does not use the store folder, and keeps old files.
        let shots = plan(&args(Step::Shots), &main, "s");
        assert!(!shots.last().unwrap().1.contains(&"--delete".to_string()));
        // In a lane the device is made when missing, then cloned for the lane.
        let laned = lane_remote(Some("whether-lane-c"));
        let line = ensure_simulator(&laned, Step::StoreShots).unwrap();
        assert!(line.contains("xcrun simctl create"), "{line}");
        assert!(
            line.contains("xcrun simctl clone 'iPhone 17' 'iPhone 17 (whether-lane-c)'"),
            "the lane clones its own base: {line}"
        );
        let store = store_remote(Some("whether-lane-c"));
        let line = ensure_simulator(&store, Step::StoreShots).unwrap();
        assert!(
            line.contains(
                "xcrun simctl shutdown 'iPhone 17 Pro Max' 2>/dev/null || true; \
                 xcrun simctl clone 'iPhone 17 Pro Max' 'iPhone 17 Pro Max (whether-lane-c)'"
            ),
            "{line}"
        );
        // Other steps never make a simulator in the main checkout.
        assert_eq!(ensure_simulator(&lane_remote(None), Step::App), None);
        // clean removes the lane's store clone as well as its app one.
        let sims = delete_simulators(&main, &Which::One("whether-lane-c".into()));
        assert!(
            sims.contains("simctl delete 'iPhone 17 Pro Max (whether-lane-c)'"),
            "{sims}"
        );
        assert!(
            sims.contains("simctl delete 'iPhone 17 (whether-lane-c)'"),
            "{sims}"
        );
        let all = delete_simulators(&main, &Which::All);
        assert!(all.contains("s/^    iPhone 17 Pro Max (.*) ("), "{all}");
        assert_eq!(ere_escape("iPad (A16)."), "iPad \\(A16\\)\\.");
    }

    #[test]
    fn device_and_archive_are_refused_in_a_lane() {
        for step in [Step::Device, Step::Archive] {
            let e = check_lane(step, Some("whether-lane-a")).unwrap_err();
            assert!(
                e.contains("main checkout") && e.contains("whether-lane-a"),
                "{e}"
            );
            assert_eq!(check_lane(step, None), Ok(()), "{step:?}");
        }
        for step in [
            Step::Test,
            Step::App,
            Step::Shots,
            Step::Build,
            Step::Sync,
            Step::Clean,
        ] {
            assert_eq!(check_lane(step, Some("whether-lane-a")), Ok(()), "{step:?}");
        }
    }

    #[test]
    fn clean_removes_lane_directories_and_simulators_and_never_the_main_directory() {
        let main = lane_remote(None);
        let one = parse(&["clean", "--lane", "whether-lane-a"]).unwrap();
        let p = plan(&one, &main, "s");
        assert_eq!(p.len(), 2, "directory, simulator");
        let sh = crate::remote::posix;
        let one_dirs = lane::remove_dirs(
            "/Users/Shared/whether-build",
            &Which::One("whether-lane-a".into()),
            MARKER,
        );
        assert_eq!(p[0].1.last().unwrap(), &sh(&one_dirs));
        assert!(
            one_dirs.contains("'/Users/Shared/whether-build-whether-lane-a'/.whether-remote"),
            "{one_dirs}"
        );
        let one_sims = delete_simulators(&main, &Which::One("whether-lane-a".into()));
        assert_eq!(p[1].1.last().unwrap(), &sh(&one_sims));
        assert!(
            one_sims.contains("xcrun simctl delete 'iPhone 17 (whether-lane-a)'"),
            "{one_sims}"
        );
        assert!(!one_sims.contains("delete 'iPhone 17'"), "{one_sims}");
        let all = parse(&["clean", "--all-lanes"]).unwrap();
        let p = plan(&all, &main, "s");
        let all_dirs = lane::remove_dirs("/Users/Shared/whether-build", &Which::All, MARKER);
        assert_eq!(p[0].1.last().unwrap(), &sh(&all_dirs));
        assert!(all_dirs.contains("'/Users/Shared/whether-build'-*"));
        let sims = delete_simulators(&main, &Which::All);
        assert_eq!(p[1].1.last().unwrap(), &sh(&sims));
        assert!(
            sims.contains("sed -n 's/^    iPhone 17 (.*) (\\([0-9A-Fa-f-]\\{36\\}\\)) (.*$/\\1/p'"),
            "{sims}"
        );
        assert!(sims.contains("xcrun simctl delete \"$u\""), "{sims}");
        // Neither needs the build, and the same holds when run from inside a lane.
        assert!(
            plan(&all, &lane_remote(Some("whether-lane-b")), "s")
                .iter()
                .all(|(p, _)| p == "ssh")
        );
        assert!(parse(&["clean"]).is_err());
        assert!(parse(&["clean", "--lane", "../x"]).is_err());
        assert!(parse(&["test", "--lane", "a"]).is_err());
        assert_eq!(sed_escape("iPhone 17 (a.b)*"), "iPhone 17 (a\\.b)\\*");
    }

    #[test]
    fn plans_keep_secrets_home_and_claim_the_directory() {
        let r = Remote {
            host: "me@mac.local".into(),
            dir: "it's here".into(),
            base_dir: "it's here".into(),
            lane: None,
            xcode: DEFAULT_XCODE.into(),
            simulator: DEFAULT_SIMULATOR.into(),
            base_simulator: DEFAULT_SIMULATOR.into(),
            store_simulator: DEFAULT_STORE_SIMULATOR.into(),
            config_simulator: DEFAULT_SIMULATOR.into(),
            team: Some("ABCDE12345".into()),
            iphone: Some("Alex's iPhone".into()),
            keychain_password_command: None,
        };
        let p = plan(&args(Step::Test), &r, "/w/target/ios-stage");
        let programs: Vec<&str> = p.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(programs, ["ssh", "rsync", "rsync", "ssh", "ssh"]);
        assert!(p[0].1[1].contains(MARKER) && p[0].1[1].contains(r"'it'\''s here'"));
        let rsync = &p[1].1;
        assert!(rsync.contains(&"--filter=:- .gitignore".to_string()));
        assert!(rsync.contains(&"--exclude=/.git".to_string()));
        assert_eq!(rsync.last().unwrap(), "me@mac.local:it's here/");
        assert!(rsync.contains(&"--filter=P /.stage".to_string()));
        assert_eq!(
            p[2].1[2..],
            ["/w/target/ios-stage/", "me@mac.local:it's here/.stage/"]
        );
        let package = &p[3].1[1];
        assert!(package.contains("DEVELOPER_DIR='/Applications/Xcode.app/Contents/Developer'"));
        assert!(package.contains("xcodebuild '-create-xcframework'"));
        assert!(package.contains("cp .stage/whether_ffi.swift ios/WhetherKit/Sources/WhetherKit/"));
        assert!(p[4].1[1].ends_with("cd ios/WhetherKit; swift test"));
        let shots = plan(&args(Step::Shots), &r, "s");
        assert_eq!(shots.len(), 7, "sync, stage, package, app, run, fetch");
        assert!(shots[4].1[1].contains("'platform=iOS Simulator,name=iPhone 17'"));
        assert!(
            shots[5].1[1]
                .contains("simctl launch 'iPhone 17' dev.savery.whether -seed -scrollTo days")
        );
        assert!(shots[5].1[1].contains(".screens/dark.png"));
        assert!(shots[5].1[1].contains("-seed -show places -search Brooklyn"));
        assert!(shots[6].1[1].starts_with("me@mac.local:it's here/.screens/"));
        assert!(rsync.contains(&"--filter=P /.derived".to_string()));
        assert!(rsync.contains(&"--filter=P /.archive".to_string()));
        // Only the welcome shot starts without a place.
        for &(name, launch) in SHOTS {
            assert_eq!(launch.contains("-seed"), name != "welcome", "{name}");
        }
        let device = plan(&args(Step::Device), &r, "s");
        let line = &device.last().unwrap().1[2];
        assert_eq!(device.last().unwrap().1[0], "-t");
        assert!(line.contains(
            "|| security unlock-keychain login.keychain-db || exit 1; exec </dev/null; xcodebuild"
        ));
        assert!(
            line.contains("printf 'DEVELOPMENT_TEAM = %s\\n' 'ABCDE12345'"),
            "{line}"
        );
        assert!(line.contains("-allowProvisioningDeviceRegistration build;"));
        // A locked phone refuses the launch; the install still counts.
        assert!(
            line.contains("|| echo 'installed; it did not start"),
            "{line}"
        );
        let plain = Args {
            no_weatherkit: true,
            ..args(Step::Device)
        };
        let plain = plan(&plain, &r, "s");
        assert!(plain.last().unwrap().1[2].contains(
            "build CODE_SIGN_ENTITLEMENTS=WhetherWidget.entitlements \
             SWIFT_ACTIVE_COMPILATION_CONDITIONS='$(inherited) NO_ICLOUD';"
        ));
        // With the full entitlements iCloud works: live sharing stays on.
        assert!(!line.contains("NO_ICLOUD"), "{line}");
        assert!(line.contains(r"--device 'Alex'\''s iPhone'"), "{line}");
        let unnamed = Remote {
            iphone: None,
            ..r.clone()
        };
        let unnamed = plan(&args(Step::Device), &unnamed, "s");
        let line = &unnamed.last().unwrap().1[1];
        assert!(line.contains("devicectl list devices; exit 2"), "{line}");
        let piped = Remote {
            keychain_password_command: Some("printf pw".into()),
            ..r.clone()
        };
        let piped = plan(&args(Step::Device), &piped, "s");
        let ssh = &piped.last().unwrap().1;
        assert_eq!(
            ssh[0], "me@mac.local",
            "no terminal: the password is piped in"
        );
        assert!(ssh[1].contains("exit 1; exec </dev/null; xcodebuild"));
        assert_eq!(plan(&args(Step::Sync), &r, "s").len(), 2);
        assert_eq!(plan(&args(Step::Doctor), &r, "s").len(), 1);
    }
}
