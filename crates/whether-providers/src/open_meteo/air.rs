//! Parsing of the air-quality API (`/v1/air-quality`).

use alloc::string::ToString;

use serde::Deserialize;
use whether_core::builder::SnapshotBuilder;
use whether_core::model::AirQuality;
use whether_core::time::UnixTime;
use whether_core::units::finite;

use crate::error::ProviderError;

#[derive(Deserialize)]
struct Response {
    current: Option<CurrentBlock>,
}

#[derive(Deserialize)]
struct CurrentBlock {
    time: i64,
    us_aqi: Option<f32>,
    european_aqi: Option<f32>,
    pm10: Option<f32>,
    pm2_5: Option<f32>,
    carbon_monoxide: Option<f32>,
    nitrogen_dioxide: Option<f32>,
    sulphur_dioxide: Option<f32>,
    ozone: Option<f32>,
}

fn index(value: Option<f32>) -> Option<u16> {
    value
        .and_then(finite)
        .map(|v| libm::roundf(v).clamp(0.0, f32::from(u16::MAX)) as u16)
}

/// Parses an air-quality response into `out`.
pub(crate) fn parse(body: &[u8], out: &mut SnapshotBuilder) -> Result<(), ProviderError> {
    let r: Response =
        serde_json::from_slice(body).map_err(|e| ProviderError::Malformed(e.to_string()))?;
    let c = r
        .current
        .ok_or_else(|| ProviderError::Malformed("air quality has no current block".to_string()))?;
    out.set_air_quality(AirQuality {
        time: UnixTime(c.time),
        us_aqi: index(c.us_aqi),
        european_aqi: index(c.european_aqi),
        owm_index: None,
        pm2_5: c.pm2_5.and_then(finite),
        pm10: c.pm10.and_then(finite),
        o3: c.ozone.and_then(finite),
        no2: c.nitrogen_dioxide.and_then(finite),
        so2: c.sulphur_dioxide.and_then(finite),
        co: c.carbon_monoxide.and_then(finite),
    });
    Ok(())
}
