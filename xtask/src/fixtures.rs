//! `cargo xtask fixtures --provider <id>`: records provider responses for tests.
//!
//! Requests come from the providers' own planners, so fixtures match what the app
//! sends. Downloads use `curl`. Keys come from the environment and are checked to be
//! absent from every written file.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use whether_core::builder::SnapshotBuilder;
use whether_core::model::ProviderId;
use whether_core::presentation::{DailyIcon, DetailKind, PresentationConfig, WidgetKind};
use whether_core::requirements::DataRequirements;
use whether_core::time::UnixTime;
use whether_providers::open_meteo::{OpenMeteo, OpenMeteoSettings};
use whether_providers::openweather::{DEFAULT_DAILY_CALL_LIMIT, OpenWeather, OpenWeatherSettings};
use whether_providers::pirate_weather::{PirateWeather, PirateWeatherSettings};
use whether_providers::{
    Coordinates, Endpoint, FetchContext, HttpRequest, HttpResponse, MAX_ALERT_DETAILS, Provider,
    UsageLedger,
};

/// ZIP 10001, New York, New York: the test location.
pub(crate) const TEST_LOCATION: Coordinates = Coordinates {
    latitude: 40.7143,
    longitude: -74.005_97,
};

const USER_AGENT: &str = "whether-fixtures/0.1";

pub(crate) fn run(args: &[&str]) -> Result<(), String> {
    match args {
        ["--provider", "openweather"] => openweather(),
        ["--provider", "open-meteo"] => open_meteo(),
        ["--provider", "open-meteo-models"] => open_meteo_models(),
        ["--provider", "nws"] => nws(),
        ["--provider", "nws-observation"] => nws_observation(),
        ["--provider", "acis"] => acis(),
        ["--provider", "open-meteo-history"] => open_meteo_history(),
        ["--provider", "climate"] => climate(),
        ["--provider", "openweather-onecall4"] => openweather_onecall4(),
        ["--provider", "met-norway"] => met_norway(),
        ["--provider", "openweather-geocode"] => openweather_geocode(),
        ["--provider", "pirate-weather"] => pirate_weather(),
        ["--provider", other] => Err(format!("fixtures for `{other}` are not implemented yet")),
        _ => Err(
            "usage: cargo xtask fixtures --provider openweather|openweather-onecall4|openweather-geocode|open-meteo|open-meteo-models|nws|nws-observation|acis|open-meteo-history|climate|met-norway|pirate-weather"
                .into(),
        ),
    }
}

fn fixture_dir(provider: &str) -> PathBuf {
    crate::workspace_root()
        .join("crates/whether-providers/tests/fixtures")
        .join(provider)
}

fn context() -> FetchContext {
    FetchContext {
        now: whether_core::time::UnixTime(0),
        user_agent: USER_AGENT.into(),
        utc_offset_hint: None,
        if_modified_since: None,
        language: "en".into(),
    }
}

/// Fetches `url`, returning `(status, body)`.
fn curl(url: &str) -> Result<(u16, Vec<u8>), String> {
    curl_with(url, &[])
}

/// Fetches `url` with the request's own headers (a `User-Agent` among them
/// replaces the default).
fn curl_with(url: &str, headers: &[(&str, String)]) -> Result<(u16, Vec<u8>), String> {
    curl_body(url, headers, None)
}

/// Performs a planned request: a GET, or a POST with its JSON body.
fn curl_request(request: &HttpRequest) -> Result<(u16, Vec<u8>), String> {
    curl_body(&request.url, &request.headers, request.body.as_deref())
}

fn curl_body(
    url: &str,
    headers: &[(&str, String)],
    body: Option<&str>,
) -> Result<(u16, Vec<u8>), String> {
    let mut cmd = Command::new("curl");
    cmd.args(["-sS", "--compressed", "--max-time", "60"]);
    if let Some(body) = body {
        cmd.args([
            "-X",
            "POST",
            "-H",
            "Content-Type: application/json",
            "--data-raw",
            body,
        ]);
    }
    if !headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("user-agent"))
    {
        cmd.args(["-A", USER_AGENT]);
    }
    for (name, value) in headers {
        cmd.arg("-H").arg(format!("{name}: {value}"));
    }
    let out = cmd
        .args(["-w", "\n%{http_code}", url])
        .output()
        .map_err(|e| format!("failed to run curl: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "curl failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    split_status(&out.stdout)
}

/// Splits curl's `body\nSTATUS` output.
fn split_status(output: &[u8]) -> Result<(u16, Vec<u8>), String> {
    let pos = output
        .iter()
        .rposition(|&b| b == b'\n')
        .ok_or("no status line from curl")?;
    let status = std::str::from_utf8(&output[pos + 1..])
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .ok_or("unparsable status from curl")?;
    Ok((status, output[..pos].to_vec()))
}

/// Fails if `secret` occurs in `bytes`.
pub(crate) fn ensure_no_secret(path: &Path, bytes: &[u8], secret: &str) -> Result<(), String> {
    if !secret.is_empty() && bytes.windows(secret.len()).any(|w| w == secret.as_bytes()) {
        let _ = fs::remove_file(path);
        return Err(format!(
            "{} contained the API key and was deleted",
            path.display()
        ));
    }
    Ok(())
}

/// OpenWeather geocoding: ZIP 10001 and "New York" in the US, the queries the
/// synthetic `geocode_*` fixtures model. Geocoding calls are free and unmetered.
fn openweather_geocode() -> Result<(), String> {
    use whether_providers::GeocodeQuery;
    use whether_providers::openweather::DEFAULT_BASE_URL;
    use whether_providers::openweather::geocode::geocode_request;

    let key = std::env::var("OPENWEATHER_API_KEY")
        .map_err(|_| "set OPENWEATHER_API_KEY to record OpenWeather fixtures".to_string())?;
    let dir = fixture_dir("openweather");
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let queries = [
        (
            "geocode_zip_10001.json",
            GeocodeQuery::Zip {
                zip: "10001",
                country: "US",
            },
        ),
        (
            "geocode_direct_new_york.json",
            GeocodeQuery::Name {
                text: "New York",
                country: Some("US"),
            },
        ),
    ];
    for (name, query) in &queries {
        let request = geocode_request(DEFAULT_BASE_URL, &key, query, 5);
        let (status, body) = curl(&request.url)?;
        if status != 200 {
            return Err(format!("{name}: HTTP {status}"));
        }
        let path = dir.join(name);
        fs::write(&path, &body).map_err(|e| format!("{}: {e}", path.display()))?;
        ensure_no_secret(&path, &body, &key)?;
        println!("recorded {} ({} bytes)", path.display(), body.len());
    }
    Ok(())
}

fn openweather() -> Result<(), String> {
    let key = std::env::var("OPENWEATHER_API_KEY")
        .map_err(|_| "set OPENWEATHER_API_KEY to record OpenWeather fixtures".to_string())?;
    let provider = OpenWeather::new(OpenWeatherSettings::free(key.clone()));
    let dir = fixture_dir("openweather");
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;

    let profiles = [
        ("40", DataRequirements::full()),
        (
            "8",
            DataRequirements {
                hourly_hours: 24,
                daily_days: 1,
                ..DataRequirements::full()
            },
        ),
    ];
    for (i, (cnt, req)) in profiles.iter().enumerate() {
        let requests = provider
            .plan(&TEST_LOCATION, req, &context())
            .map_err(|e| e.to_string())?;
        for request in requests {
            let name = match request.endpoint {
                Endpoint::Current if i == 0 => "weather_10001.json".to_string(),
                Endpoint::Forecast => format!("forecast_10001_cnt{cnt}.json"),
                Endpoint::AirQuality if i == 0 => "air_pollution_10001.json".to_string(),
                _ => continue,
            };
            let (status, body) = curl(&request.url)?;
            if status == 401 {
                return Err("OpenWeather key not active yet (HTTP 401); try again later".into());
            }
            if status != 200 {
                return Err(format!("{name}: HTTP {status}"));
            }
            let path = dir.join(&name);
            fs::write(&path, &body).map_err(|e| format!("{}: {e}", path.display()))?;
            ensure_no_secret(&path, &body, &key)?;
            println!("recorded {} ({} bytes)", path.display(), body.len());
        }
    }
    Ok(())
}

/// Replaces every occurrence of `secret` with `***` (One Call timelines embed
/// `next`/`prev` URLs that carry the key).
pub(crate) fn scrub(bytes: &[u8], secret: &str) -> Vec<u8> {
    if secret.is_empty() {
        return bytes.to_vec();
    }
    let mut out = Vec::with_capacity(bytes.len());
    let mut rest = bytes;
    while !rest.is_empty() {
        if rest.starts_with(secret.as_bytes()) {
            out.extend_from_slice(b"***");
            rest = &rest[secret.len()..];
        } else {
            out.push(rest[0]);
            rest = &rest[1..];
        }
    }
    out
}

/// The app's call ledger, `$XDG_STATE_HOME/whether/usage.json` (state, not cache, so
/// clearing the cache never resets the budget).
fn usage_path() -> Result<PathBuf, String> {
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .ok_or("neither XDG_STATE_HOME nor HOME is set")?;
    Ok(state.join("whether/usage.json"))
}

/// Reads the ledger; a missing file is an empty ledger, a corrupt one an error (never
/// a silent reset).
fn load_ledger(path: &Path) -> Result<UsageLedger, String> {
    match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| {
            format!(
                "{} is corrupt ({e}); fix or delete it deliberately",
                path.display()
            )
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(UsageLedger::new()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

fn save_ledger(path: &Path, ledger: &UsageLedger) -> Result<(), String> {
    let dir = path.parent().ok_or("ledger path has no directory")?;
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string(ledger).map_err(|e| e.to_string())?;
    fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Records One Call 4.0 responses for ZIP 10001 (6 metered calls, plus one per
/// active alert, at most `MAX_ALERT_DETAILS`), counting each call in the app's
/// ledger before sending it.
fn openweather_onecall4() -> Result<(), String> {
    let key = std::env::var("OPENWEATHER_API_KEY")
        .map_err(|_| "set OPENWEATHER_API_KEY to record One Call fixtures".to_string())?;
    let provider = OpenWeather::new(OpenWeatherSettings::onecall4(
        key.clone(),
        DEFAULT_DAILY_CALL_LIMIT,
    ));
    let budget = provider.budget().ok_or("One Call has no budget")?;
    let limit = budget.limit;
    let now = UnixTime(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_secs()
            .try_into()
            .map_err(|_| "clock out of range")?,
    );
    let ctx = FetchContext { now, ..context() };
    let requests = provider
        .plan(&TEST_LOCATION, &DataRequirements::full(), &ctx)
        .map_err(|e| e.to_string())?;
    let ledger_path = usage_path()?;
    let mut ledger = load_ledger(&ledger_path)?;
    let dir = fixture_dir("openweather");
    let mut record = |url: &str, name: &str| -> Result<Vec<u8>, String> {
        ledger
            .try_spend(ProviderId::OpenWeather, budget, 1, now)
            .map_err(|e| format!("not sent: {e}"))?;
        save_ledger(&ledger_path, &ledger)?;
        let (status, body) = curl(url)?;
        if status == 401 {
            return Err(
                "One Call 4.0 answered 401: the subscription is not active for this key yet".into(),
            );
        }
        if status != 200 {
            return Err(format!("{name}: HTTP {status}"));
        }
        let body = scrub(&body, &key);
        let path = dir.join(name);
        fs::write(&path, &body).map_err(|e| format!("{}: {e}", path.display()))?;
        ensure_no_secret(&path, &body, &key)?;
        println!("recorded {} ({} bytes)", path.display(), body.len());
        Ok(body)
    };
    let mut page = 0;
    let mut alert_ids = Vec::new();
    for request in requests.iter().filter(|r| r.metered) {
        let name = match request.endpoint {
            Endpoint::Current => "onecall4_current_10001.json".to_string(),
            Endpoint::Hourly => {
                page += 1;
                format!("onecall4_1h_p{}_10001.json", page - 1)
            }
            Endpoint::Daily => "onecall4_1day_10001.json".to_string(),
            Endpoint::Nowcast => "onecall4_15min_10001.json".to_string(),
            _ => continue,
        };
        let body = record(&request.url, &name)?;
        alert_ids.extend(alerts_named(&provider, request.endpoint, &body, &ctx));
    }
    let mut seen = Vec::new();
    alert_ids.retain(|id| {
        let new = !seen.contains(id);
        seen.push(id.clone());
        new
    });
    for (n, id) in alert_ids.iter().take(MAX_ALERT_DETAILS).enumerate() {
        if let Some(request) = provider.alert_request(id, &ctx) {
            record(&request.url, &format!("onecall4_alert_{n}_10001.json"))?;
        }
    }
    let used = ledger.used(ProviderId::OpenWeather, now);
    println!(
        "{} active alerts; One Call calls today: {used} of {limit} (ledger {})",
        alert_ids.len(),
        ledger_path.display()
    );
    Ok(())
}

/// The alert ids a recorded One Call response names, in order.
fn alerts_named(
    provider: &OpenWeather,
    endpoint: Endpoint,
    body: &[u8],
    ctx: &FetchContext,
) -> Vec<String> {
    let mut builder = SnapshotBuilder::new(ProviderId::OpenWeather);
    let response = HttpResponse::new(200, body);
    match provider.parse(endpoint, &response, ctx, &mut builder) {
        Ok(()) => builder.take_alert_refs(),
        Err(_) => Vec::new(),
    }
}

/// The data an ESP32 panel shows: current, 12 hours, 5 days and four details.
fn esp32_requirements() -> DataRequirements {
    DataRequirements::from_presentation(&PresentationConfig {
        widgets: vec![
            WidgetKind::Current,
            WidgetKind::Hourly,
            WidgetKind::Daily,
            WidgetKind::Details,
        ],
        hourly_hours: 12,
        daily_days: 5,
        // As the firmware asks (its buffers are small).
        daily_icon: DailyIcon::Provider,
        details: vec![
            DetailKind::Wind,
            DetailKind::Humidity,
            DetailKind::Uv,
            DetailKind::Pressure,
        ],
        ..Default::default()
    })
}

/// Places with distinctive offsets, latitudes and hemispheres for the full profile.
const OPEN_METEO_PLACES: &[(&str, Coordinates)] = &[
    ("10001", TEST_LOCATION),
    (
        "tromso",
        Coordinates {
            latitude: 69.65,
            longitude: 18.96,
        },
    ),
    (
        "sydney",
        Coordinates {
            latitude: -33.87,
            longitude: 151.21,
        },
    ),
    (
        "honolulu",
        Coordinates {
            latitude: 21.31,
            longitude: -157.86,
        },
    ),
    (
        "reykjavik",
        Coordinates {
            latitude: 64.15,
            longitude: -21.94,
        },
    ),
];

/// Fetches `url` and writes the body to `dir/name` if the status is `expected`.
fn record(dir: &Path, name: &str, url: &str, expected: u16) -> Result<(), String> {
    let (status, body) = curl(url)?;
    if status != expected {
        return Err(format!("{name}: HTTP {status}, expected {expected}"));
    }
    let path = dir.join(name);
    fs::write(&path, &body).map_err(|e| format!("{}: {e}", path.display()))?;
    println!("recorded {} ({} bytes)", path.display(), body.len());
    Ok(())
}

/// MET Norway: the forecast for 10001 and Tromsø and today's sunrise for 10001.
/// The contact for the User-Agent comes from `WHETHER_MET_CONTACT` and is checked
/// to be absent from every written file.
fn met_norway() -> Result<(), String> {
    use whether_core::time::UtcOffset;
    use whether_providers::met_norway::{MetNorway, MetNorwaySettings};

    let contact = std::env::var("WHETHER_MET_CONTACT")
        .ok()
        .filter(|c| !c.trim().is_empty())
        .ok_or("set WHETHER_MET_CONTACT to an email or URL (MET Norway's terms require it)")?;
    let provider = MetNorway::new(MetNorwaySettings::new(contact.clone()));
    let dir = fixture_dir("met_norway");
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let now = UnixTime(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_secs()
            .try_into()
            .map_err(|_| "clock out of range")?,
    );
    let places = [
        ("10001", TEST_LOCATION, UtcOffset(-5 * 3600)),
        (
            "tromso",
            Coordinates {
                latitude: 69.6492,
                longitude: 18.9553,
            },
            UtcOffset(2 * 3600),
        ),
    ];
    for (place, loc, offset) in places {
        let ctx = FetchContext {
            now,
            user_agent: USER_AGENT.into(),
            utc_offset_hint: Some(offset),
            if_modified_since: None,
            language: "en".into(),
        };
        for request in provider
            .plan(&loc, &DataRequirements::full(), &ctx)
            .map_err(|e| e.to_string())?
        {
            let name = match request.endpoint {
                Endpoint::Forecast => format!("complete_{place}.json"),
                Endpoint::Sun if place == "10001" => "sun_10001.json".into(),
                _ => continue,
            };
            let (status, body) = curl_with(&request.url, &request.headers)?;
            if status != 200 {
                return Err(format!("{name}: HTTP {status}"));
            }
            let path = dir.join(&name);
            fs::write(&path, &body).map_err(|e| format!("{}: {e}", path.display()))?;
            ensure_no_secret(&path, &body, &contact)?;
            println!("recorded {} ({} bytes)", path.display(), body.len());
        }
    }
    Ok(())
}

/// Pirate Weather: the full forecast for 10001, and the error a rejected key gets
/// (sent with a made-up key, which the gateway refuses without counting a call). The
/// key comes from `PIRATE_WEATHER_API_KEY` (it goes in the `apikey` header), and is
/// checked to be absent from every written file. One forecast counts one call
/// against the key's 10,000 a month.
fn pirate_weather() -> Result<(), String> {
    let key = std::env::var("PIRATE_WEATHER_API_KEY")
        .map_err(|_| "set PIRATE_WEATHER_API_KEY to record Pirate Weather fixtures".to_string())?;
    let dir = fixture_dir("pirate_weather");
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let cases = [
        ("forecast_10001.json", key.as_str(), 200),
        ("error_401.json", "not-a-real-key", 401),
    ];
    for (name, api_key, expected) in cases {
        let provider = PirateWeather::new(PirateWeatherSettings::new(api_key));
        let request = provider
            .plan(&TEST_LOCATION, &DataRequirements::full(), &context())
            .map_err(|e| e.to_string())?
            .remove(0);
        let (status, body) = curl_request(&request)?;
        if status != expected {
            return Err(format!("{name}: HTTP {status}, expected {expected}"));
        }
        let path = dir.join(name);
        fs::write(&path, &body).map_err(|e| format!("{}: {e}", path.display()))?;
        ensure_no_secret(&path, &body, &key)?;
        println!("recorded {} ({} bytes)", path.display(), body.len());
    }
    Ok(())
}

fn open_meteo() -> Result<(), String> {
    let provider = OpenMeteo::default();
    let dir = fixture_dir("open_meteo");
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let plan = |loc: &Coordinates, req: &DataRequirements| {
        provider
            .plan(loc, req, &context())
            .map_err(|e| e.to_string())
    };

    let esp32 = plan(&TEST_LOCATION, &esp32_requirements())?;
    record(&dir, "forecast_esp32_10001.json", &esp32[0].url, 200)?;
    for (place, loc) in OPEN_METEO_PLACES {
        for request in plan(loc, &DataRequirements::full())? {
            match request.endpoint {
                Endpoint::Forecast => {
                    record(
                        &dir,
                        &format!("forecast_full_{place}.json"),
                        &request.url,
                        200,
                    )?;
                }
                Endpoint::AirQuality if *place == "10001" => {
                    record(&dir, "air_quality_10001.json", &request.url, 200)?;
                }
                _ => {}
            }
        }
    }
    let bad = Coordinates {
        latitude: 952.52,
        longitude: TEST_LOCATION.longitude,
    };
    let bad_url = plan(&bad, &esp32_requirements())?[0].url.clone();
    record(&dir, "error_400_latitude.json", &bad_url, 400)
}

/// The NWS forecast for ZIP 10001: the point lookup, then the raw
/// gridpoint and the twice-daily forecast it names, as the provider plans them;
/// and Open-Meteo's forecast at the same time.
fn nws() -> Result<(), String> {
    use whether_providers::nws::{DEFAULT_BASE_URL, Nws};

    let provider = Nws::forecaster(DEFAULT_BASE_URL);
    let dir = fixture_dir("nws");
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let save = |name: &str, request: &HttpRequest| -> Result<Vec<u8>, String> {
        let (status, body) = curl_with(&request.url, &request.headers)?;
        if status != 200 {
            return Err(format!("{name}: HTTP {status}"));
        }
        let path = dir.join(name);
        fs::write(&path, &body).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("recorded {} ({} bytes)", path.display(), body.len());
        Ok(body)
    };
    let requests = provider
        .plan(&TEST_LOCATION, &DataRequirements::full(), &context())
        .map_err(|e| e.to_string())?;
    let point = requests
        .iter()
        .find(|r| r.endpoint == Endpoint::Point)
        .ok_or("no point request")?;
    let body = save("points_10001.json", point)?;
    let follow = provider
        .follow_up(Endpoint::Point, &HttpResponse::new(200, &body), &context())
        .map_err(|e| e.to_string())?;
    for request in &follow {
        // `…/gridpoints/OKX/33,42[/forecast]` → `gridpoint_OKX_33_42.json`.
        let grid = request
            .url
            .split("/gridpoints/")
            .nth(1)
            .ok_or("unexpected follow-up")?;
        let (cell, kind) = match grid.strip_suffix("/forecast") {
            Some(cell) => (cell, "forecast"),
            None => (grid, "gridpoint"),
        };
        let name = format!("{kind}_{}.json", cell.replace(['/', ','], "_"));
        save(&name, request)?;
    }
    // Open-Meteo at the same time, to supplement the NWS forecast in tests.
    let open_meteo = OpenMeteo::default()
        .plan(&TEST_LOCATION, &DataRequirements::full(), &context())
        .map_err(|e| e.to_string())?;
    let forecast = open_meteo
        .iter()
        .find(|r| r.endpoint == Endpoint::Forecast)
        .ok_or("no Open-Meteo forecast")?;
    record(
        &fixture_dir("open_meteo"),
        "forecast_full_10001_with_nws.json",
        &forecast.url,
        200,
    )
}

/// ACIS history for ZIP 10001 today: the station list, then the chosen
/// station's days and normals, its chance of precipitation, and its and its
/// threaded record's records, as the provider plans them; and the same for
/// La Guardia airport configured as the station. Names:
/// `stnmeta_10001.json`, `stndata_<sid>.json`, `chance_<sid>.json`,
/// `records_<sid>.json`.
fn acis() -> Result<(), String> {
    use whether_core::requirements::HistoryRequirements;
    use whether_core::time::UtcOffset;
    use whether_providers::acis::{Acis, AcisSettings};
    use whether_providers::{HistoryContext, HistoryProvider};

    let now = UnixTime(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_secs()
            .try_into()
            .map_err(|_| "clock out of range")?,
    );
    // New York is on EST from 1 November.
    let offset = UtcOffset(-5 * 3600);
    let ctx = HistoryContext {
        now,
        today: now.local_date(offset),
        utc_offset: offset,
        user_agent: USER_AGENT.into(),
        normals: None,
    };
    let req = HistoryRequirements {
        past_days: 7,
        climate_days: 7,
        records: true,
        month_to_date: true,
    };
    let provider = Acis::default();
    let dir = fixture_dir("acis");
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let save = |name: &str, body: &[u8]| -> Result<(), String> {
        let path = dir.join(name);
        fs::write(&path, body).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("recorded {} ({} bytes)", path.display(), body.len());
        Ok(())
    };
    let meta = provider
        .plan(&TEST_LOCATION, &req, &ctx)
        .map_err(|e| e.to_string())?;
    let meta = meta.first().ok_or("no station list request")?;
    let (status, body) = curl_request(meta)?;
    if status != 200 {
        return Err(format!("StnMeta: HTTP {status}"));
    }
    save("stnmeta_10001.json", &body)?;
    let mut follow = provider
        .follow_up(
            Endpoint::Stations,
            &HttpResponse::new(200, &body),
            &TEST_LOCATION,
            &req,
            &ctx,
        )
        .map_err(|e| e.to_string())?;
    // And La Guardia airport, configured (`[history.acis] station = "LGA"`).
    let pinned = Acis::new(AcisSettings {
        station: Some("LGA".into()),
        ..AcisSettings::default()
    });
    follow.extend(
        pinned
            .plan(&TEST_LOCATION, &req, &ctx)
            .map_err(|e| e.to_string())?,
    );
    for request in &follow {
        let query: serde_json::Value =
            serde_json::from_str(request.body.as_deref().unwrap_or("{}"))
                .map_err(|e| e.to_string())?;
        let sid = query["sid"].as_str().unwrap_or("unknown");
        let text = request.body.as_deref().unwrap_or("");
        let kind = if text.contains("normal") {
            "stndata"
        } else if text.contains("cnt_ge") {
            "chance"
        } else {
            "records"
        };
        let (status, body) = curl_request(request)?;
        if status != 200 {
            return Err(format!("{kind} {sid}: HTTP {status}"));
        }
        save(&format!("{kind}_{sid}.json"), &body)?;
    }
    println!(
        "recorded for {} (today in New York)",
        ctx.today.year * 10_000 + i32::from(ctx.today.month) * 100 + i32::from(ctx.today.day)
    );
    Ok(())
}

/// The vacation planner's query for ZIP 10001: 12–19 July over
/// 1995–2024, from ACIS (the station chosen from the recorded station list)
/// and from the Open-Meteo archive.
fn climate() -> Result<(), String> {
    use whether_core::history::{ClimateQuery, YearRange};
    use whether_core::time::{LocalDate, UtcOffset};
    use whether_providers::acis::Acis;
    use whether_providers::open_meteo::history::OpenMeteoHistory;
    use whether_providers::{HistoryContext, HistoryProvider};

    let offset = UtcOffset(-5 * 3600);
    let now = UnixTime(1_793_637_000);
    let ctx = HistoryContext {
        now,
        today: now.local_date(offset),
        utc_offset: offset,
        user_agent: USER_AGENT.into(),
        normals: None,
    };
    let query = ClimateQuery::new(
        LocalDate {
            year: 2027,
            month: 7,
            day: 12,
        },
        LocalDate {
            year: 2027,
            month: 7,
            day: 19,
        },
        YearRange {
            first: 1995,
            last: 2024,
        },
    )
    .map_err(|e| e.to_string())?;
    let acis_dir = fixture_dir("acis");
    let stations = fs::read(acis_dir.join("stnmeta_10001.json"))
        .map_err(|e| format!("record the station list first (--provider acis): {e}"))?;
    let follow = Acis::default()
        .follow_up_climate(
            Endpoint::Stations,
            &HttpResponse::new(200, &stations),
            &TEST_LOCATION,
            &query,
            &ctx,
        )
        .map_err(|e| e.to_string())?;
    let request = follow.first().ok_or("no climate request")?;
    let body: serde_json::Value =
        serde_json::from_str(request.body.as_deref().unwrap_or("{}")).map_err(|e| e.to_string())?;
    let sid = body["sid"].as_str().unwrap_or("unknown").to_string();
    let (status, bytes) = curl_request(request)?;
    if status != 200 {
        return Err(format!("climate {sid}: HTTP {status}"));
    }
    let path = acis_dir.join(format!("climate_{sid}_0712_0719.json"));
    fs::write(&path, &bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    println!("recorded {} ({} bytes)", path.display(), bytes.len());
    let archive = OpenMeteoHistory::default()
        .plan_climate(&TEST_LOCATION, &query, &ctx)
        .map_err(|e| e.to_string())?;
    let archive = archive.first().ok_or("no archive request")?;
    record(
        &fixture_dir("open_meteo"),
        "climate_10001_0712_0719.json",
        &archive.url,
        200,
    )
}

/// Open-Meteo's past days for ZIP 10001: the 1st of the month to
/// yesterday, as `HistoryRequirements` with month-to-date totals plan them.
fn open_meteo_history() -> Result<(), String> {
    use whether_core::requirements::HistoryRequirements;
    use whether_core::time::UtcOffset;
    use whether_providers::open_meteo::history::OpenMeteoHistory;
    use whether_providers::{HistoryContext, HistoryProvider};

    let now = UnixTime(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_secs()
            .try_into()
            .map_err(|_| "clock out of range")?,
    );
    let offset = UtcOffset(-5 * 3600);
    let ctx = HistoryContext {
        now,
        today: now.local_date(offset),
        utc_offset: offset,
        user_agent: USER_AGENT.into(),
        normals: None,
    };
    let req = HistoryRequirements {
        past_days: 7,
        climate_days: 7,
        records: true,
        month_to_date: true,
    };
    let requests = OpenMeteoHistory::default()
        .plan(&TEST_LOCATION, &req, &ctx)
        .map_err(|e| e.to_string())?;
    let past = requests
        .iter()
        .find(|r| r.endpoint == Endpoint::Daily)
        .ok_or("no past-days request")?;
    record(
        &fixture_dir("open_meteo"),
        "history_past_10001.json",
        &past.url,
        200,
    )
}

/// The stations near ZIP 10001 and the nearest one's latest observation, from the
/// recorded point lookup (so the gridpoint fixtures stay as they are).
fn nws_observation() -> Result<(), String> {
    use whether_providers::nws::{DEFAULT_BASE_URL, Nws};

    let provider = Nws::forecaster(DEFAULT_BASE_URL);
    let dir = fixture_dir("nws");
    let point = fs::read(dir.join("points_10001.json"))
        .map_err(|e| format!("points_10001.json: {e}; run --provider nws first"))?;
    let stations = provider
        .follow_up(Endpoint::Point, &HttpResponse::new(200, &point), &context())
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|r| r.endpoint == Endpoint::Stations)
        .ok_or("no stations request")?;
    let (status, body) = curl_with(&stations.url, &stations.headers)?;
    if status != 200 {
        return Err(format!("stations: HTTP {status}"));
    }
    let path = dir.join("stations_OKX_33_42.json");
    fs::write(&path, &body).map_err(|e| format!("{}: {e}", path.display()))?;
    println!("recorded {} ({} bytes)", path.display(), body.len());
    let observation = provider
        .follow_up(
            Endpoint::Stations,
            &HttpResponse::new(200, &body),
            &context(),
        )
        .map_err(|e| e.to_string())?
        .into_iter()
        .next()
        .ok_or("no station listed")?;
    // `…/stations/KNYC/observations/latest` → `observation_KNYC.json`.
    let id = observation
        .url
        .split("/stations/")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .ok_or("unexpected observation URL")?;
    let (status, body) = curl_with(&observation.url, &observation.headers)?;
    if status != 200 {
        return Err(format!("observation: HTTP {status}"));
    }
    let path = dir.join(format!("observation_{id}.json"));
    fs::write(&path, &body).map_err(|e| format!("{}: {e}", path.display()))?;
    println!("recorded {} ({} bytes)", path.display(), body.len());
    Ok(())
}

/// Open-Meteo with an ordered model list: HRRR, then the NOAA blend,
/// then ECMWF, for ZIP 10001 with `DataRequirements::full()`.
fn open_meteo_models() -> Result<(), String> {
    let provider = OpenMeteo::new(OpenMeteoSettings {
        models: ["ncep_hrrr_conus", "ncep_nbm_conus", "ecmwf_ifs025"]
            .map(String::from)
            .to_vec(),
        ..OpenMeteoSettings::default()
    });
    let dir = fixture_dir("open_meteo");
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let requests = provider
        .plan(&TEST_LOCATION, &DataRequirements::full(), &context())
        .map_err(|e| e.to_string())?;
    let forecast = requests
        .iter()
        .find(|r| r.endpoint == Endpoint::Forecast)
        .ok_or("no forecast request")?;
    record(
        &dir,
        "forecast_models_hrrr_nbm_ecmwf.json",
        &forecast.url,
        200,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_curl_output() {
        assert_eq!(
            split_status(b"{\"a\":1}\n200").unwrap(),
            (200, b"{\"a\":1}".to_vec())
        );
        assert_eq!(split_status(b"\n401").unwrap(), (401, Vec::new()));
        assert!(split_status(b"no newline").is_err());
        assert!(split_status(b"body\nxyz").is_err());
    }

    #[test]
    fn detects_and_removes_files_containing_the_key() {
        let path = std::env::temp_dir().join(format!("xtask-fixture-{}", std::process::id()));
        fs::write(&path, b"x").unwrap();
        assert!(ensure_no_secret(&path, b"{\"k\":\"SECRET\"}", "SECRET").is_err());
        assert!(!path.exists());
        assert!(ensure_no_secret(&path, b"{\"k\":1}", "SECRET").is_ok());
    }

    #[test]
    fn rejects_unknown_providers() {
        assert!(run(&["--provider", "nope"]).is_err());
        assert!(run(&[]).is_err());
    }

    #[test]
    fn scrubs_every_occurrence() {
        let body =
            br#"{"next":"https://x/?appid=KEY123&start=1","prev":"https://x/?appid=KEY123"}"#;
        let clean = scrub(body, "KEY123");
        assert_eq!(
            clean,
            br#"{"next":"https://x/?appid=***&start=1","prev":"https://x/?appid=***"}"#.to_vec()
        );
        assert_eq!(scrub(b"abc", ""), b"abc".to_vec());
    }

    #[test]
    fn ledgers_round_trip_and_corruption_is_an_error() {
        let dir = std::env::temp_dir().join(format!("xtask-ledger-{}", std::process::id()));
        let path = dir.join("usage.json");
        assert_eq!(load_ledger(&path).unwrap(), UsageLedger::new());
        let mut ledger = UsageLedger::new();
        ledger
            .try_spend(
                ProviderId::OpenWeather,
                whether_providers::Budget::daily(5),
                1,
                UnixTime(1_791_217_800),
            )
            .unwrap();
        save_ledger(&path, &ledger).unwrap();
        assert_eq!(load_ledger(&path).unwrap(), ledger);
        fs::write(&path, "{").unwrap();
        assert!(load_ledger(&path).unwrap_err().contains("corrupt"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn esp32_profile_is_small() {
        let req = esp32_requirements();
        assert_eq!((req.hourly_hours, req.daily_days), (12, 5));
        assert!(!req.air_quality && req.minutely_hours == 0);
    }

    #[test]
    fn alert_ids_are_read_from_recorded_responses() {
        let provider = OpenWeather::new(OpenWeatherSettings::onecall4("K", 10));
        let body = include_bytes!(
            "../../crates/whether-providers/tests/fixtures/openweather/onecall4_current_alerts_10001.synthetic.json"
        );
        assert_eq!(
            alerts_named(&provider, Endpoint::Current, body, &context()),
            ["8B46C632-DCA7-44D7-8BDF-02445621BAFF"]
        );
        assert_eq!(
            alerts_named(&provider, Endpoint::Current, b"not json", &context()),
            [] as [String; 0]
        );
    }
}
