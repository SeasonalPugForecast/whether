//! Radar for the app's map: NOAA NEXRAD via the Iowa Environmental
//! Mesonet, contiguous US only. MapKit fetches the tiles from the templates.
//! Elsewhere the user's OpenWeather key unlocks OpenWeather's precipitation
//! layer: one current frame, no loop.

use whether_core::strings::Strings;
use whether_core::time::{UnixTime, UtcOffset};
use whether_providers::radar;

use crate::WhetherError;
use crate::settings::{Place, Settings};

/// One frame of the loop.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct RadarFrame {
    /// Minutes before the latest frame.
    pub minutes_ago: u32,
    /// When the composite was valid (Unix seconds).
    pub valid: i64,
    /// The frame's `{z}/{x}/{y}` tile URL template.
    pub tile_template: String,
}

/// Whether the composite covers a point.
#[uniffi::export]
pub fn radar_covers(latitude: f64, longitude: f64) -> bool {
    radar::covers(latitude, longitude)
}

/// Where a place's radar comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum RadarSource {
    /// The NEXRAD composite and its loop (the contiguous US).
    Nexrad,
    /// OpenWeather's precipitation layer: the current frame, with the user's key.
    OpenWeather,
    /// No radar: outside the composite and no key.
    None,
}

/// Whether any radar can be shown for a point: the composite covers it, or the
/// user has an OpenWeather key (`has_key`).
#[uniffi::export]
pub fn radar_covers_with(latitude: f64, longitude: f64, has_key: bool) -> bool {
    radar::source(latitude, longitude, has_key) != radar::Source::None
}

/// Where the radar for a point comes from.
#[uniffi::export]
pub fn radar_source(latitude: f64, longitude: f64, has_key: bool) -> RadarSource {
    match radar::source(latitude, longitude, has_key) {
        radar::Source::Nexrad => RadarSource::Nexrad,
        radar::Source::OpenWeather => RadarSource::OpenWeather,
        radar::Source::None => RadarSource::None,
    }
}

/// OpenWeather's precipitation layer as the only frame (`valid` 0: it is always the
/// current one), or no frames for an empty key. The template holds the key: never log
/// it.
#[uniffi::export]
pub fn radar_openweather_frames(key: &str) -> Vec<RadarFrame> {
    if key.trim().is_empty() {
        return Vec::new();
    }
    vec![RadarFrame {
        minutes_ago: 0,
        valid: 0,
        tile_template: radar::openweather_tile_template(key),
    }]
}

/// The credit for OpenWeather's layer.
#[uniffi::export]
pub fn radar_openweather_credit() -> String {
    radar::OPENWEATHER_CREDIT.to_string()
}

/// The highest zoom level OpenWeather's tiles exist at.
#[uniffi::export]
pub fn radar_openweather_max_zoom() -> u8 {
    radar::OPENWEATHER_MAX_ZOOM
}

/// The URL of the latest composite's metadata.
#[uniffi::export]
pub fn radar_meta_url() -> String {
    radar::meta_url()
}

/// The latest composite's valid time from the metadata body.
#[uniffi::export]
#[allow(clippy::needless_pass_by_value)] // UniFFI passes arguments by value
pub fn radar_latest_valid(body: Vec<u8>) -> Result<i64, WhetherError> {
    radar::parse_valid(&body)
        .map(|t| t.0)
        .map_err(|e| WhetherError::InvalidJson {
            what: "radar metadata".into(),
            message: e.to_string(),
        })
}

/// The loop ending at `latest`, oldest first.
#[uniffi::export]
pub fn radar_frames(latest: i64) -> Vec<RadarFrame> {
    radar::frames(UnixTime(latest))
        .into_iter()
        .map(|(minutes_ago, valid)| RadarFrame {
            minutes_ago,
            valid: valid.0,
            tile_template: radar::tile_template(minutes_ago),
        })
        .collect()
}

/// Labels for frames valid at `times` (Unix seconds): the place's local clock
/// time in the settings' style, `"10:15 AM"`.
#[uniffi::export]
#[allow(clippy::needless_pass_by_value)] // UniFFI passes arguments by value
pub fn radar_frame_labels(times: Vec<i64>, place: Place, settings: Settings) -> Vec<String> {
    let presentation = settings.presentation(&place);
    let strings = Strings::localized(&settings.language, &std::collections::BTreeMap::default()).0;
    let changes = place.core_offset_changes();
    let base = UtcOffset(place.utc_offset_seconds.unwrap_or(0));
    let offset = |t: UnixTime| {
        changes
            .iter()
            .rev()
            .find(|c| c.at.0 <= t.0)
            .map_or(base, |c| c.offset)
    };
    let times: Vec<UnixTime> = times.into_iter().map(UnixTime).collect();
    whether_core::view::radar_vm(&times, offset, "", &presentation, &strings).frames
}

/// The frame `minutes_ago` old as a `{z}/{x}/{y}` template.
#[uniffi::export]
pub fn radar_tile_template(minutes_ago: u32) -> String {
    radar::tile_template(minutes_ago)
}

/// The OpenStreetMap base map as a template; a view drawing it must show
/// [`radar_attribution`]'s OpenStreetMap credit too (MapKit's map credits itself).
#[uniffi::export]
pub fn radar_base_template() -> String {
    radar::base_template()
}

/// The credit radar views must show: the radar's alone with `on_apple_map`
/// (MapKit credits its map), else with the OpenStreetMap credit.
#[uniffi::export]
pub fn radar_attribution(on_apple_map: bool) -> String {
    if on_apple_map {
        radar::RADAR_CREDIT
    } else {
        radar::ATTRIBUTION
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openweather_outside_the_composite() {
        let london = (51.5, -0.12);
        assert!(!radar_covers_with(london.0, london.1, false));
        assert!(radar_covers_with(london.0, london.1, true));
        assert!(radar_covers_with(40.7143, -74.006, false));
        assert_eq!(
            radar_source(london.0, london.1, true),
            RadarSource::OpenWeather
        );
        assert_eq!(radar_source(london.0, london.1, false), RadarSource::None);
        assert_eq!(radar_source(40.7143, -74.006, true), RadarSource::Nexrad);
        let frames = radar_openweather_frames(" k3y ");
        assert_eq!(frames.len(), 1, "the current frame only");
        assert_eq!(
            frames[0].tile_template,
            "https://tile.openweathermap.org/map/precipitation_new/{z}/{x}/{y}.png?appid=k3y"
        );
        assert_eq!(radar_openweather_frames("  ").len(), 0);
        assert_eq!(radar_openweather_credit(), "Radar: OpenWeather");
        assert_eq!(radar_openweather_max_zoom(), 9);
    }

    #[test]
    fn frames_and_templates() {
        assert!(radar_covers(40.7143, -74.006));
        let body = br#"{"meta": {"valid": "2026-10-10T20:15:00Z"}}"#.to_vec();
        let latest = radar_latest_valid(body).unwrap();
        let frames = radar_frames(latest);
        assert_eq!(frames.len(), 6);
        assert_eq!(frames[5].valid, latest);
        assert!(frames[0].tile_template.contains("-m25m/{z}/{x}/{y}.png"));
        assert!(radar_latest_valid(b"{}".to_vec()).is_err());
        assert!(radar_attribution(true).contains("NEXRAD"));
        assert!(!radar_attribution(true).contains("OpenStreetMap"));
        assert!(radar_attribution(false).contains("OpenStreetMap"));
        let place = Place {
            name: "New York".into(),
            latitude: 40.7,
            longitude: -74.0,
            units: None,
            utc_offset_seconds: Some(-4 * 3600),
            offset_changes: Vec::new(),
        };
        let mut s = crate::default_settings();
        s.clock_24h = false;
        let labels = radar_frame_labels(vec![latest], place, s);
        assert_eq!(labels, ["4:15 PM"], "20:15 UTC in EDT");
    }
}
