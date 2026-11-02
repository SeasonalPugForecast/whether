//! Radar imagery, sans-IO: which tiles show a location, their URLs and the
//! composite's valid time. The NOAA NEXRAD base reflectivity composite (n0q, public
//! domain) comes from the Iowa Environmental Mesonet's tile service in Web Mercator;
//! the base map from OpenStreetMap. Only the contiguous US is covered. Elsewhere a
//! user's own OpenWeather key unlocks OpenWeather's precipitation map layer (one
//! current frame, no loop).

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use serde::Deserialize;
use whether_core::time::{UnixTime, parse_rfc3339};

use crate::error::ProviderError;
use crate::http::{Endpoint, HttpRequest, Method};
use crate::url::QueryBuilder;

/// Pixels per tile side.
pub const TILE: u32 = 256;
/// Tiles per window side: the location's tile and one on each side.
pub const WINDOW: u32 = 3;
/// Minutes between composite frames.
pub const FRAME_STEP_MIN: u32 = 5;
/// Frames in a loop: the latest and five older ones (30 minutes).
pub const FRAMES: u32 = 6;
/// The supported zoom levels (6 shows several states, 10 a county).
pub const ZOOMS: core::ops::RangeInclusive<u8> = 6..=10;
/// The default zoom: about 360 km across at 40° N.
pub const DEFAULT_ZOOM: u8 = 8;

/// The required credit for radar views drawn on the OpenStreetMap base.
pub const ATTRIBUTION: &str = concat!(
    "Radar: NOAA NEXRAD via Iowa Environmental Mesonet",
    " · Map © OpenStreetMap contributors"
);

/// The radar's credit alone, for views on a map that credits itself (MapKit).
pub const RADAR_CREDIT: &str = "Radar: NOAA NEXRAD via Iowa Environmental Mesonet";

/// The credit for OpenWeather's precipitation layer (OpenWeather's terms also ask for
/// a link to openweathermap.org, which the app's data credits carry).
pub const OPENWEATHER_CREDIT: &str = "Radar: OpenWeather";

/// The highest zoom OpenWeather's map tiles are documented for (the FAQ's table of
/// tile numbers stops at 9); a map view stretches the level-9 tiles beyond it.
pub const OPENWEATHER_MAX_ZOOM: u8 = 9;

/// OpenWeather's weather-map tile service (Weather Maps 1.0, free for everyone).
const OPENWEATHER_TILES: &str = "https://tile.openweathermap.org/map";
/// Its precipitation layer.
const OPENWEATHER_LAYER: &str = "precipitation_new";

const IEM: &str = "https://mesonet.agron.iastate.edu";
const OSM: &str = "https://tile.openstreetmap.org";

/// Whether the n0q composite covers a point (the contiguous US).
pub fn covers(latitude: f64, longitude: f64) -> bool {
    (24.0..=50.0).contains(&latitude) && (-125.0..=-66.0).contains(&longitude)
}

/// Where a location's radar comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// The NEXRAD composite, with a loop of frames.
    Nexrad,
    /// OpenWeather's precipitation layer, with the user's key: the current frame only.
    OpenWeather,
    /// No radar: outside the composite and no key.
    None,
}

/// The radar source for a location: NEXRAD where it covers, else OpenWeather's layer
/// when the user has a key, else none.
pub fn source(latitude: f64, longitude: f64, has_key: bool) -> Source {
    if covers(latitude, longitude) {
        Source::Nexrad
    } else if has_key {
        Source::OpenWeather
    } else {
        Source::None
    }
}

/// A point in Web-Mercator tile coordinates at `zoom` (fractional tiles).
pub fn tile_position(latitude: f64, longitude: f64, zoom: u8) -> (f64, f64) {
    let n = f64::from(1_u32 << zoom);
    let lat = latitude.clamp(-85.051_128, 85.051_128).to_radians();
    let x = (longitude + 180.0) / 360.0 * n;
    let y =
        (1.0 - libm::log(libm::tan(lat) + 1.0 / libm::cos(lat)) / core::f64::consts::PI) / 2.0 * n;
    (x, y)
}

/// The tiles around a location: a [`WINDOW`]×[`WINDOW`] mosaic with the location's
/// tile in the middle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RadarWindow {
    /// Zoom level.
    pub zoom: u8,
    /// Column of the top-left tile.
    pub x0: u32,
    /// Row of the top-left tile.
    pub y0: u32,
    /// The location in mosaic pixels.
    pub marker: (u32, u32),
}

impl RadarWindow {
    /// The window for a location at `zoom` (clamped to [`ZOOMS`]).
    pub fn new(latitude: f64, longitude: f64, zoom: u8) -> Self {
        let zoom = zoom.clamp(*ZOOMS.start(), *ZOOMS.end());
        let (x, y) = tile_position(latitude, longitude, zoom);
        let tiles = f64::from(1_u32 << zoom);
        let (x, y) = (x.clamp(0.0, tiles - 1e-9), y.clamp(0.0, tiles - 1e-9));
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (tx, ty) = (libm::floor(x) as u32, libm::floor(y) as u32);
        let x0 = tx.saturating_sub(1);
        let y0 = ty.saturating_sub(1);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let px =
            |v: f64, origin: u32| (libm::floor((v - f64::from(origin)) * f64::from(TILE))) as u32;
        Self {
            zoom,
            x0,
            y0,
            marker: (px(x, x0), px(y, y0)),
        }
    }

    /// Mosaic size in pixels.
    pub fn size(&self) -> (u32, u32) {
        (WINDOW * TILE, WINDOW * TILE)
    }

    /// The tiles row by row: `(column in the window, row, x, y)`.
    pub fn tiles(&self) -> Vec<(u32, u32, u32, u32)> {
        let mut out = Vec::new();
        for row in 0..WINDOW {
            for col in 0..WINDOW {
                out.push((col, row, self.x0 + col, self.y0 + row));
            }
        }
        out
    }

    /// The URL of a radar tile of the frame `minutes_ago` old (a multiple of 5).
    pub fn frame_url(&self, minutes_ago: u32, x: u32, y: u32) -> String {
        format!(
            "{IEM}/cache/tile.py/1.0.0/{}/{}/{x}/{y}.png",
            layer(minutes_ago),
            self.zoom
        )
    }

    /// The URL of a base-map tile.
    pub fn base_url(&self, x: u32, y: u32) -> String {
        format!("{OSM}/{}/{x}/{y}.png", self.zoom)
    }
}

/// The composite's tile layer for the frame `minutes_ago` old.
fn layer(minutes_ago: u32) -> String {
    if minutes_ago == 0 {
        String::from("nexrad-n0q-900913")
    } else {
        format!("nexrad-n0q-900913-m{minutes_ago:02}m")
    }
}

/// A `{z}/{x}/{y}` URL template of the frame `minutes_ago` old, for map views that
/// fetch tiles themselves (iOS `MKTileOverlay`).
pub fn tile_template(minutes_ago: u32) -> String {
    format!(
        "{IEM}/cache/tile.py/1.0.0/{}/{{z}}/{{x}}/{{y}}.png",
        layer(minutes_ago)
    )
}

/// OpenWeather's precipitation layer as a `{z}/{x}/{y}` URL template with the user's
/// `key` (`appid`). The template holds the key: log it only through [`redact_url`]
/// (`appid` is masked) and never store it.
///
/// [`redact_url`]: crate::url::redact_url
pub fn openweather_tile_template(key: &str) -> String {
    QueryBuilder::new(&format!(
        "{OPENWEATHER_TILES}/{OPENWEATHER_LAYER}/{{z}}/{{x}}/{{y}}.png"
    ))
    .param("appid", key.trim())
    .finish()
}

/// The OpenStreetMap base map as a `{z}/{x}/{y}` URL template.
pub fn base_template() -> String {
    format!("{OSM}/{{z}}/{{x}}/{{y}}.png")
}

/// The URL of the latest composite's metadata ([`parse_valid`] reads it).
pub fn meta_url() -> String {
    format!("{IEM}/data/gis/images/4326/USCOMP/n0q_0.json")
}

/// A GET for radar imagery or its metadata.
pub fn request(url: String) -> HttpRequest {
    HttpRequest {
        url,
        headers: Vec::new(),
        endpoint: Endpoint::Radar,
        required: false,
        metered: false,
        method: Method::Get,
        body: None,
    }
}

/// The request for the latest composite's metadata.
pub fn meta_request() -> HttpRequest {
    request(meta_url())
}

#[derive(Deserialize)]
struct Meta {
    meta: MetaInner,
}

#[derive(Deserialize)]
struct MetaInner {
    valid: String,
}

/// The latest composite's valid time from its metadata.
pub fn parse_valid(body: &[u8]) -> Result<UnixTime, ProviderError> {
    let meta: Meta =
        serde_json::from_slice(body).map_err(|e| ProviderError::Malformed(e.to_string()))?;
    parse_rfc3339(&meta.meta.valid)
        .ok_or_else(|| ProviderError::Malformed(format!("bad valid time `{}`", meta.meta.valid)))
}

/// The frames of a loop ending at `latest`, oldest first: `(minutes ago, valid time)`.
pub fn frames(latest: UnixTime) -> Vec<(u32, UnixTime)> {
    (0..FRAMES)
        .rev()
        .map(|i| {
            let ago = i * FRAME_STEP_MIN;
            (ago, UnixTime(latest.0 - i64::from(ago) * 60))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_york_tiles_at_zoom_8() {
        let w = RadarWindow::new(40.7143, -74.0060, 8);
        // Web Mercator puts New York at x 75.37, y 96.25 at zoom 8: tile 8/75/96.
        assert_eq!(
            (w.x0 + 1, w.y0 + 1),
            (75, 96),
            "the city's tile is in the middle"
        );
        let (mx, my) = w.marker;
        assert!(
            (256..512).contains(&mx) && (256..512).contains(&my),
            "{mx},{my}"
        );
        assert_eq!(w.size(), (768, 768));
        assert_eq!(w.tiles().len(), 9);
        assert_eq!(w.tiles()[4], (1, 1, 75, 96));
        assert_eq!(
            w.frame_url(0, 75, 96),
            "https://mesonet.agron.iastate.edu/cache/tile.py/1.0.0/nexrad-n0q-900913/8/75/96.png"
        );
        assert_eq!(
            w.frame_url(25, 75, 96),
            "https://mesonet.agron.iastate.edu/cache/tile.py/1.0.0/nexrad-n0q-900913-m25m/8/75/96.png"
        );
        assert_eq!(
            tile_template(10),
            "https://mesonet.agron.iastate.edu/cache/tile.py/1.0.0/nexrad-n0q-900913-m10m/{z}/{x}/{y}.png"
        );
        assert_eq!(
            base_template(),
            "https://tile.openstreetmap.org/{z}/{x}/{y}.png"
        );
        assert_eq!(
            w.base_url(75, 96),
            "https://tile.openstreetmap.org/8/75/96.png"
        );
        assert_eq!(
            RadarWindow::new(40.7, -74.0, 30).zoom,
            10,
            "zoom is clamped"
        );
    }

    #[test]
    fn coverage_is_the_contiguous_us() {
        assert!(covers(40.7143, -74.0060));
        assert!(!covers(40.614, -1.659), "Spain");
        assert!(!covers(61.2, -149.9), "Anchorage is outside the composite");
    }

    #[test]
    fn openweather_layer_and_redaction() {
        // The template shape is the one on openweathermap.org/api/weathermaps
        // (checked 2026-09-24): /map/{layer}/{z}/{x}/{y}.png?appid=…
        let t = openweather_tile_template("  abc123 ");
        assert_eq!(
            t,
            "https://tile.openweathermap.org/map/precipitation_new/{z}/{x}/{y}.png?appid=abc123"
        );
        let redacted = crate::url::redact_url(&t);
        assert_eq!(
            redacted,
            "https://tile.openweathermap.org/map/precipitation_new/{z}/{x}/{y}.png?appid=***"
        );
        assert!(!redacted.contains("abc123"));
        // A key with reserved characters cannot add parameters.
        assert!(openweather_tile_template("a&b=c").ends_with("?appid=a%26b%3Dc"));
        assert_eq!(OPENWEATHER_CREDIT, "Radar: OpenWeather");
    }

    #[test]
    fn the_source_follows_coverage_then_the_key() {
        let (ny, london) = ((40.7143, -74.0060), (51.5, -0.12));
        assert_eq!(source(ny.0, ny.1, false), Source::Nexrad);
        assert_eq!(source(ny.0, ny.1, true), Source::Nexrad, "NEXRAD first");
        assert_eq!(source(london.0, london.1, true), Source::OpenWeather);
        assert_eq!(source(london.0, london.1, false), Source::None);
        assert_eq!(source(61.2, -149.9, true), Source::OpenWeather, "Anchorage");
    }

    #[test]
    fn metadata_and_frames() {
        // Recorded from n0q_0.json on 2026-07-07.
        let body = br#"{"meta": {"vcp": null, "product": "N0Q", "site": "USCOMP", "valid": "2026-10-10T20:15:00Z", "processing_time_secs": 102, "radar_quorum": "143/147"}}"#;
        let valid = parse_valid(body).unwrap();
        assert_eq!(valid, UnixTime(1_791_663_300));
        let f = frames(valid);
        assert_eq!(f.len(), 6);
        assert_eq!(f[0], (25, UnixTime(valid.0 - 1500)), "oldest first");
        assert_eq!(f[5], (0, valid));
        assert!(parse_valid(b"{}").is_err());
        assert!(ATTRIBUTION.starts_with(RADAR_CREDIT));
        assert_eq!(meta_request().endpoint, Endpoint::Radar);
    }
}
