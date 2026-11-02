//! The desktop `config.toml`, read for the app's settings and places.
//!
//! Only what the file sets is taken: a key the file leaves out keeps the phone's
//! choice. Secrets and the One Call budget never come across (the phone keeps
//! its own key in the Keychain and its own share of the calls).

use whether_config::OwmApi;
use whether_core::model::ProviderId;
use whether_core::units::UnitSystem;

use crate::WhetherError;
use crate::settings::{DateStyle, Units};
use crate::units::UnitOverrides;

/// A `[[location]]` from the file.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ImportedPlace {
    /// Its name.
    pub name: String,
    /// Latitude, when the file gives coordinates.
    pub latitude: Option<f64>,
    /// Longitude, when the file gives coordinates.
    pub longitude: Option<f64>,
    /// Otherwise what to search for: the postal code or the query.
    pub search: Option<String>,
    /// The postal code's country (`"US"`).
    pub country: Option<String>,
    /// IANA zone, when the file sets one.
    pub timezone: Option<String>,
}

/// What the file sets; `None` where it says nothing.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ImportedConfig {
    /// Its locations, in order.
    pub places: Vec<ImportedPlace>,
    /// `[general] units`; `custom` is metric with the `[units]` overrides.
    pub units: Option<Units>,
    /// `[units]`: the quantities the file sets one by one; `None` when the
    /// table sets none.
    pub unit_overrides: Option<UnitOverrides>,
    /// `[general] time_format`.
    pub clock_24h: Option<bool>,
    /// `[general] date_format`.
    pub date_style: Option<DateStyle>,
    /// `[providers.openweather] api`: One Call 4.0 or the free API.
    pub one_call: Option<bool>,
    /// `[providers] supplement`: whether anything fills gaps.
    pub fill_gaps: Option<bool>,
    /// `[providers.open-meteo] models`, in priority order.
    pub models: Option<Vec<String>>,
    /// `[providers] order` starts with `nws`: US forecasts from the NWS.
    pub nws: Option<bool>,
    /// `[presentation] daily_days`.
    pub daily_days: Option<u8>,
    /// `[presentation] precip_hours`.
    pub precip_hours: Option<u8>,
    /// `[presentation] daily_icon`: daylight icons.
    pub daylight_icons: Option<bool>,
    /// `[presentation] temperature_basis`: lead with "feels like".
    pub feels_like: Option<bool>,
    /// What was left out, one sentence each.
    pub notes: Vec<String>,
}

/// Whether `table` sets the dotted `path`.
fn sets(table: &toml::Table, path: &[&str]) -> bool {
    let mut value = None;
    let mut current = Some(table);
    for key in path {
        value = current.and_then(|t| t.get(*key));
        current = value.and_then(toml::Value::as_table);
    }
    value.is_some()
}

/// The settings and places in a desktop `config.toml` (validated as the desktop
/// does).
#[uniffi::export]
#[allow(clippy::needless_pass_by_value)] // UniFFI passes arguments by value
pub fn import_config(text: String) -> Result<ImportedConfig, WhetherError> {
    let invalid = |message: String| WhetherError::Config { message };
    let config = whether_config::parse(&text).map_err(|e| invalid(e.to_string()))?;
    let table: toml::Table = text
        .parse()
        .map_err(|e: toml::de::Error| invalid(e.to_string()))?;
    let common = &config.common;
    let mut notes = Vec::new();

    let mut places = Vec::new();
    for location in &common.location {
        let Some(name) = location.name.clone() else {
            continue;
        };
        let (latitude, longitude) = match (location.latitude, location.longitude) {
            (Some(lat), Some(lon)) => (Some(lat), Some(lon)),
            _ => (None, None),
        };
        let search = if latitude.is_some() {
            None
        } else {
            location.zip.clone().or_else(|| location.query.clone())
        };
        if latitude.is_none() && search.is_none() {
            notes.push(format!("{name} has no coordinates, postal code or query."));
            continue;
        }
        if location.units.is_some() || location.presentation.is_some() {
            notes.push(format!(
                "{name}'s own units and sections stay on the desktop."
            ));
        }
        places.push(ImportedPlace {
            name,
            latitude,
            longitude,
            search,
            country: location.country.clone(),
            timezone: location.timezone.clone(),
        });
    }

    let units = if sets(&table, &["general", "units"]) {
        match common.general.units {
            UnitSystem::Auto => Some(Units::Auto),
            UnitSystem::Imperial => Some(Units::Imperial),
            // As the desktop resolves it: metric, then the `[units]` overrides.
            UnitSystem::Metric | UnitSystem::Custom => Some(Units::Metric),
        }
    } else {
        None
    };
    let unit_overrides = Some(UnitOverrides::from_config(common.units)).filter(|o| !o.is_empty());
    let clock_24h = sets(&table, &["general", "time_format"])
        .then(|| common.general.time_format == whether_core::presentation::TimeFormat::H24);
    let date_style =
        sets(&table, &["general", "date_format"]).then(|| common.general.date_format.into());
    let openweather = common.providers.openweather.as_ref();
    let one_call = openweather
        .and_then(|o| o.api)
        .filter(|_| sets(&table, &["providers", "openweather", "api"]))
        .map(|api| api == OwmApi::OneCall4);
    if one_call.is_some() {
        notes.push(
            "The OpenWeather key is not imported: enter it in Settings (it stays in the Keychain)."
                .into(),
        );
    }
    if one_call == Some(true) {
        notes.push("The phone keeps its own One Call budget (set in Settings).".into());
    }
    let fill_gaps = sets(&table, &["providers", "supplement"])
        .then_some(!common.providers.supplement.is_empty());
    let models = common
        .providers
        .open_meteo
        .models
        .as_ref()
        .map(|m| m.ids().to_vec());
    let nws = sets(&table, &["providers", "order"])
        .then(|| common.providers.order.first() == Some(&ProviderId::Nws));
    let p = &common.presentation;
    Ok(ImportedConfig {
        places,
        units,
        unit_overrides,
        clock_24h,
        date_style,
        one_call,
        fill_gaps,
        models,
        nws,
        daily_days: p.daily_days,
        precip_hours: p.precip_hours,
        daylight_icons: p
            .daily_icon
            .map(|i| i == whether_core::presentation::DailyIcon::Daylight),
        feels_like: p
            .temperature_basis
            .map(|b| b == whether_core::presentation::TemperatureBasis::FeelsLike),
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_places_and_only_the_settings_the_file_sets() {
        let c = import_config(
            r#"
[[location]]
name = "Newark"
latitude = 40.73566
longitude = -74.17237

[[location]]
name = "Home"
zip = "10001"
country = "US"
units = "metric"

[providers.openweather]
api_key = { env = "OPENWEATHER_API_KEY" }
api = "onecall-4"

[providers.open-meteo]
models = "ncep_nbm_conus"
"#
            .into(),
        )
        .unwrap();
        assert_eq!(c.models, Some(vec!["ncep_nbm_conus".to_string()]));
        assert_eq!(c.places.len(), 2);
        assert_eq!(
            (c.places[0].latitude, c.places[0].search.as_deref()),
            (Some(40.73566), None)
        );
        assert_eq!(
            (
                c.places[1].search.as_deref(),
                c.places[1].country.as_deref()
            ),
            (Some("10001"), Some("US"))
        );
        assert_eq!(c.one_call, Some(true));
        assert_eq!(c.nws, None, "no order: the phone's choice");
        assert_eq!(
            (
                c.units,
                c.clock_24h,
                c.date_style,
                c.fill_gaps,
                c.daily_days,
                c.daylight_icons
            ),
            (None, None, None, None, None, None),
            "unset keys keep the phone's choice"
        );
        assert!(c.notes.iter().any(|n| n.contains("key is not imported")));
        assert!(c.notes.iter().any(|n| n.contains("Home's own units")));
    }

    #[test]
    fn reads_general_providers_and_presentation() {
        let c = import_config(
            r#"
[[location]]
name = "New York"
latitude = 40.7143
longitude = -74.00597

[general]
units = "imperial"
time_format = "12h"
date_format = "dd/mm"

[providers]
order = ["open-meteo"]
supplement = []

[presentation]
daily_days = 10
precip_hours = 24
daily_icon = "provider"
temperature_basis = "feels_like"
"#
            .into(),
        )
        .unwrap();
        assert_eq!(c.places.len(), 1);
        assert_eq!(c.date_style, Some(DateStyle::DayMonthPadded));
        assert_eq!(c.models, None, "the phone keeps its models");
        assert_eq!(c.nws, Some(false));
        assert_eq!(
            (c.units, c.clock_24h, c.fill_gaps, c.one_call),
            (Some(Units::Imperial), Some(false), Some(false), None)
        );
        assert_eq!(
            (c.daily_days, c.precip_hours, c.daylight_icons, c.feels_like),
            (Some(10), Some(24), Some(false), Some(true))
        );
        assert!(c.notes.is_empty(), "{:?}", c.notes);
    }

    #[test]
    fn the_units_table_comes_across_one_unit_at_a_time() {
        let uk = import_config(
            "[[location]]\nname = \"Home\"\nlatitude = 51.5\nlongitude = -0.13\n\
             [general]\nunits = \"custom\"\n[units]\ntemperature = \"celsius\"\nwind = \"mph\"\n\
             pressure = \"mmHg\"\n"
                .into(),
        )
        .unwrap();
        assert_eq!(uk.units, Some(Units::Metric), "custom starts from metric");
        assert_eq!(
            uk.unit_overrides,
            Some(UnitOverrides {
                temperature: Some(crate::units::TemperatureUnit::Celsius),
                wind: Some(crate::units::WindUnit::Mph),
                pressure: Some(crate::units::PressureUnit::MmHg),
                ..UnitOverrides::default()
            })
        );
        assert!(uk.notes.is_empty(), "{:?}", uk.notes);
        let none = import_config(
            "[[location]]\nname = \"Home\"\nlatitude = 51.5\nlongitude = -0.13\n\
             [general]\nunits = \"imperial\"\n"
                .into(),
        )
        .unwrap();
        assert_eq!(none.unit_overrides, None, "no table: the phone's choice");
        // Applied through the settings, the UK combination reads °C with mph.
        let mut s = crate::default_settings();
        s.units = Units::Metric;
        s.unit_overrides = uk.unit_overrides.unwrap();
        let p = s.presentation(&crate::Place {
            name: "Home".into(),
            latitude: 51.5,
            longitude: -0.13,
            units: None,
            utc_offset_seconds: None,
            offset_changes: Vec::new(),
        });
        assert_eq!(p.units.wind, whether_core::units::WindUnit::Mph);
    }

    #[test]
    fn the_nws_first_on_the_desktop_is_the_nws_on_the_phone() {
        let c = import_config(
            "[[location]]\nname = \"New York\"\nlatitude = 40.7143\nlongitude = -74.00597\n\
             [providers]\norder = [\"nws\", \"open-meteo\"]\n"
                .into(),
        )
        .unwrap();
        assert_eq!(c.nws, Some(true));
    }

    #[test]
    fn invalid_files_say_why() {
        let e = import_config("[general\nunits = 1".into()).unwrap_err();
        assert!(matches!(e, WhetherError::Config { .. }), "{e:?}");
        let e = import_config("[general]\nunits = \"kelvin\"".into()).unwrap_err();
        assert!(e.to_string().starts_with("config: "), "{e}");
    }
}
