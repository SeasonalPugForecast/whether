//! Categorical scales: severity levels, temperature bands, UV, air quality, compass
//! points, wind arrows and moon phases.

use crate::icon::UiIcon;

/// One severity scale that every display maps to colors (UV, air quality, alerts).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "snake_case")
)]
#[allow(missing_docs)]
pub enum Level {
    Good,
    Fair,
    Moderate,
    Poor,
    VeryPoor,
    Extreme,
}

/// Temperature band, computed from Celsius so displays never color by display units.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "snake_case")
)]
#[allow(missing_docs)]
pub enum TempLevel {
    Freezing,
    Cold,
    Cool,
    Mild,
    Warm,
    Hot,
}

impl TempLevel {
    /// Freezing < 0 °C ≤ Cold < 8 ≤ Cool < 16 ≤ Mild < 24 ≤ Warm < 30 ≤ Hot.
    pub fn from_celsius(c: f32) -> Self {
        match c {
            c if c < 0.0 => Self::Freezing,
            c if c < 8.0 => Self::Cold,
            c if c < 16.0 => Self::Cool,
            c if c < 24.0 => Self::Mild,
            c if c < 30.0 => Self::Warm,
            _ => Self::Hot,
        }
    }
}

/// WHO UV index category.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "snake_case")
)]
#[allow(missing_docs)]
pub enum UvLevel {
    Low,
    Moderate,
    High,
    VeryHigh,
    Extreme,
}

impl UvLevel {
    /// The shared severity level for this UV category.
    pub fn level(self) -> Level {
        match self {
            Self::Low => Level::Good,
            Self::Moderate => Level::Moderate,
            Self::High => Level::Poor,
            Self::VeryHigh => Level::VeryPoor,
            Self::Extreme => Level::Extreme,
        }
    }
}

/// The WHO category for a UV index (rounded to the nearest integer first).
pub fn uv_level(uv: f32) -> UvLevel {
    match libm::roundf(uv) {
        u if u < 3.0 => UvLevel::Low,
        u if u < 6.0 => UvLevel::Moderate,
        u if u < 8.0 => UvLevel::High,
        u if u < 11.0 => UvLevel::VeryHigh,
        _ => UvLevel::Extreme,
    }
}

/// Which air-quality index a value belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum AqiScale {
    /// US EPA AQI, 0–500.
    Us,
    /// European AQI, 0–100+.
    European,
    /// OpenWeather's 1–5 index.
    Owm,
}

/// The name of an air-quality category, across all three scales.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "snake_case")
)]
#[allow(missing_docs)]
pub enum AqiLabel {
    Good,
    Fair,
    Moderate,
    UnhealthyForSensitive,
    Unhealthy,
    VeryUnhealthy,
    Hazardous,
    Poor,
    VeryPoor,
    ExtremelyPoor,
}

/// An air-quality category: its name on the scale and its shared severity level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AqiCategory {
    /// The category name.
    pub label: AqiLabel,
    /// The shared severity level.
    pub level: Level,
}

const fn cat(label: AqiLabel, level: Level) -> AqiCategory {
    AqiCategory { label, level }
}

/// US AQI: 0–50 good, 51–100 moderate, 101–150 unhealthy for sensitive groups,
/// 151–200 unhealthy, 201–300 very unhealthy, 301+ hazardous.
pub fn us_aqi_category(aqi: u16) -> AqiCategory {
    match aqi {
        0..=50 => cat(AqiLabel::Good, Level::Good),
        51..=100 => cat(AqiLabel::Moderate, Level::Fair),
        101..=150 => cat(AqiLabel::UnhealthyForSensitive, Level::Moderate),
        151..=200 => cat(AqiLabel::Unhealthy, Level::Poor),
        201..=300 => cat(AqiLabel::VeryUnhealthy, Level::VeryPoor),
        _ => cat(AqiLabel::Hazardous, Level::Extreme),
    }
}

/// European AQI: ≤20 good, ≤40 fair, ≤60 moderate, ≤80 poor, ≤100 very poor,
/// above that extremely poor.
pub fn eu_aqi_category(aqi: u16) -> AqiCategory {
    match aqi {
        0..=20 => cat(AqiLabel::Good, Level::Good),
        21..=40 => cat(AqiLabel::Fair, Level::Fair),
        41..=60 => cat(AqiLabel::Moderate, Level::Moderate),
        61..=80 => cat(AqiLabel::Poor, Level::Poor),
        81..=100 => cat(AqiLabel::VeryPoor, Level::VeryPoor),
        _ => cat(AqiLabel::ExtremelyPoor, Level::Extreme),
    }
}

/// OpenWeather's index: 1 good, 2 fair, 3 moderate, 4 poor, 5 very poor.
pub fn owm_aqi_category(index: u8) -> Option<AqiCategory> {
    Some(match index {
        1 => cat(AqiLabel::Good, Level::Good),
        2 => cat(AqiLabel::Fair, Level::Fair),
        3 => cat(AqiLabel::Moderate, Level::Moderate),
        4 => cat(AqiLabel::Poor, Level::Poor),
        5 => cat(AqiLabel::VeryPoor, Level::VeryPoor),
        _ => return None,
    })
}

/// `deg` normalized into `0.0..360.0`.
fn normalize_degrees(deg: f32) -> f32 {
    let d = libm::fmodf(deg, 360.0);
    if d < 0.0 { d + 360.0 } else { d }
}

/// `index` into a ring of `n` sectors of `width` degrees centred on 0°.
fn sector(deg: f32, width: f32, n: usize) -> usize {
    let s = libm::floorf((normalize_degrees(deg) + width / 2.0) / width);
    // INVARIANT: s is in 0..=n, so the modulo keeps the index in range.
    (s as usize) % n
}

/// The 16-point compass abbreviation for a bearing in degrees.
pub fn compass16(deg: f32) -> &'static str {
    const POINTS: [&str; 16] = [
        "N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE", "S", "SSW", "SW", "WSW", "W", "WNW",
        "NW", "NNW",
    ];
    POINTS[sector(deg, 22.5, 16)]
}

/// An arrow pointing where the wind blows *to*, given the direction it blows *from*.
pub fn wind_arrow(deg_from: f32) -> char {
    const ARROWS: [char; 8] = ['↑', '↗', '→', '↘', '↓', '↙', '←', '↖'];
    ARROWS[sector(deg_from + 180.0, 45.0, 8)]
}

/// The moon-phase icon for a phase fraction (0 = new, 0.5 = full), in eight bins.
pub fn moon_phase(fraction: f32) -> UiIcon {
    const PHASES: [UiIcon; 8] = [
        UiIcon::MoonNew,
        UiIcon::MoonWaxingCrescent,
        UiIcon::MoonFirstQuarter,
        UiIcon::MoonWaxingGibbous,
        UiIcon::MoonFull,
        UiIcon::MoonWaningGibbous,
        UiIcon::MoonLastQuarter,
        UiIcon::MoonWaningCrescent,
    ];
    PHASES[sector(fraction * 360.0, 45.0, 8)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temperature_bands() {
        assert_eq!(TempLevel::from_celsius(-0.1), TempLevel::Freezing);
        assert_eq!(TempLevel::from_celsius(0.0), TempLevel::Cold);
        assert_eq!(TempLevel::from_celsius(8.0), TempLevel::Cool);
        assert_eq!(TempLevel::from_celsius(16.0), TempLevel::Mild);
        assert_eq!(TempLevel::from_celsius(23.9), TempLevel::Mild);
        assert_eq!(TempLevel::from_celsius(24.0), TempLevel::Warm);
        assert_eq!(TempLevel::from_celsius(30.0), TempLevel::Hot);
    }

    #[test]
    fn uv_boundaries() {
        assert_eq!(uv_level(2.4), UvLevel::Low);
        assert_eq!(uv_level(2.5), UvLevel::Moderate);
        assert_eq!(uv_level(5.0), UvLevel::Moderate);
        assert_eq!(uv_level(6.0), UvLevel::High);
        assert_eq!(uv_level(7.0), UvLevel::High);
        assert_eq!(uv_level(8.0), UvLevel::VeryHigh);
        assert_eq!(uv_level(10.0), UvLevel::VeryHigh);
        assert_eq!(uv_level(11.0), UvLevel::Extreme);
        assert_eq!(uv_level(6.0).level(), Level::Poor);
    }

    #[test]
    fn us_aqi_boundaries() {
        let label = |v| us_aqi_category(v).label;
        assert_eq!(label(50), AqiLabel::Good);
        assert_eq!(label(51), AqiLabel::Moderate);
        assert_eq!(label(100), AqiLabel::Moderate);
        assert_eq!(label(101), AqiLabel::UnhealthyForSensitive);
        assert_eq!(label(151), AqiLabel::Unhealthy);
        assert_eq!(label(201), AqiLabel::VeryUnhealthy);
        assert_eq!(label(300), AqiLabel::VeryUnhealthy);
        assert_eq!(label(301), AqiLabel::Hazardous);
    }

    #[test]
    fn eu_aqi_boundaries() {
        let label = |v| eu_aqi_category(v).label;
        assert_eq!(label(20), AqiLabel::Good);
        assert_eq!(label(21), AqiLabel::Fair);
        assert_eq!(label(41), AqiLabel::Moderate);
        assert_eq!(label(61), AqiLabel::Poor);
        assert_eq!(label(100), AqiLabel::VeryPoor);
        assert_eq!(label(101), AqiLabel::ExtremelyPoor);
    }

    #[test]
    fn owm_index() {
        assert_eq!(owm_aqi_category(1).map(|c| c.label), Some(AqiLabel::Good));
        assert_eq!(owm_aqi_category(5).map(|c| c.level), Some(Level::VeryPoor));
        assert_eq!(owm_aqi_category(0), None);
        assert_eq!(owm_aqi_category(6), None);
    }

    #[test]
    fn compass_points() {
        assert_eq!(compass16(0.0), "N");
        assert_eq!(compass16(348.75), "N");
        assert_eq!(compass16(11.24), "N");
        assert_eq!(compass16(11.25), "NNE");
        assert_eq!(compass16(90.0), "E");
        assert_eq!(compass16(225.0), "SW");
        assert_eq!(compass16(-90.0), "W");
        assert_eq!(compass16(720.0), "N");
    }

    #[test]
    fn wind_arrows_point_downwind() {
        assert_eq!(wind_arrow(270.0), '→');
        assert_eq!(wind_arrow(0.0), '↓');
        assert_eq!(wind_arrow(180.0), '↑');
        assert_eq!(wind_arrow(45.0), '↙');
    }

    #[test]
    fn moon_phase_bins() {
        assert_eq!(moon_phase(0.0), UiIcon::MoonNew);
        assert_eq!(moon_phase(0.97), UiIcon::MoonNew);
        assert_eq!(moon_phase(0.25), UiIcon::MoonFirstQuarter);
        assert_eq!(moon_phase(0.5), UiIcon::MoonFull);
        assert_eq!(moon_phase(0.817), UiIcon::MoonWaningCrescent);
        assert_eq!(moon_phase(0.75), UiIcon::MoonLastQuarter);
    }
}
