//! Turns TOML settings into the resolved values displays use.

use whether_core::presentation::PresentationConfig;
use whether_core::units::{UnitPrefs, UnitSystem};

use crate::types::{General, LocationConfig, PresentationToml, UnitsOverride};

/// Territories that measure in imperial units.
const IMPERIAL_TERRITORIES: &[&str] = &["US", "LR", "MM"];

/// The territory of a POSIX locale such as `en_US.UTF-8` or `es_US@euro`.
fn territory(locale: &str) -> Option<&str> {
    let rest = locale.split_once('_')?.1;
    let end = rest.find(['.', '@']).unwrap_or(rest.len());
    Some(&rest[..end])
}

/// The units for `general.units`, with `[units]` overrides on top. `locale` (the
/// measurement locale, e.g. `LC_MEASUREMENT`) matters only for `auto`.
pub fn resolve_units(general: &General, o: &UnitsOverride, locale: Option<&str>) -> UnitPrefs {
    let mut units = match general.units {
        UnitSystem::Imperial => UnitPrefs::IMPERIAL,
        UnitSystem::Metric | UnitSystem::Custom => UnitPrefs::METRIC,
        UnitSystem::Auto => {
            let imperial = locale
                .and_then(territory)
                .is_some_and(|t| IMPERIAL_TERRITORIES.contains(&t));
            if imperial {
                UnitPrefs::IMPERIAL
            } else {
                UnitPrefs::METRIC
            }
        }
    };
    if let Some(t) = o.temperature {
        units.temperature = t;
    }
    if let Some(w) = o.wind {
        units.wind = w;
    }
    if let Some(p) = o.pressure {
        units.pressure = p;
    }
    if let Some(p) = o.precipitation {
        units.precipitation = p;
    }
    if let Some(d) = o.distance {
        units.distance = d;
    }
    units
}

/// The presentation for a display: `p` over the built-in defaults, with units and
/// the clock style from `[general]` and `[units]`. Out-of-range numbers (reported
/// by validation) are clamped.
pub fn resolve_presentation(
    general: &General,
    o: &UnitsOverride,
    p: &PresentationToml,
    locale: Option<&str>,
) -> PresentationConfig {
    let base = PresentationConfig {
        units: resolve_units(general, o, locale),
        time_format: general.time_format,
        date_format: general.date_format,
        ..PresentationConfig::default()
    };
    overlay(base, p)
}

/// `base` with the keys `p` sets.
fn overlay(d: PresentationConfig, p: &PresentationToml) -> PresentationConfig {
    PresentationConfig {
        units: d.units,
        time_format: d.time_format,
        date_format: d.date_format,
        widgets: p.widgets.clone().unwrap_or(d.widgets),
        hourly_hours: p.hourly_hours.unwrap_or(d.hourly_hours).clamp(1, 48),
        hourly_metrics: p.hourly_metrics.clone().unwrap_or(d.hourly_metrics),
        daily_days: p.daily_days.unwrap_or(d.daily_days).clamp(1, 16),
        daily_icon: p.daily_icon.unwrap_or(d.daily_icon),
        precip_hours: p.precip_hours.unwrap_or(d.precip_hours).clamp(1, 48),
        history_days: p.history_days.unwrap_or(d.history_days).min(31),
        nowcast_hours: p.nowcast_hours.unwrap_or(d.nowcast_hours).clamp(1, 8),
        temperature_basis: p.temperature_basis.unwrap_or(d.temperature_basis),
        daily_hours: p.daily_hours.unwrap_or(d.daily_hours),
        details: p.details.clone().unwrap_or(d.details),
        aqi_scale: p.aqi_scale.unwrap_or(d.aqi_scale),
        precip_threshold: p.precip_threshold.unwrap_or(d.precip_threshold).min(100),
        temperature_decimals: p.temperature_decimals.unwrap_or(d.temperature_decimals),
        show_unit_letter: p.show_unit_letter.unwrap_or(d.show_unit_letter),
        missing: p.missing.clone().unwrap_or(d.missing),
        twilight_minutes: p.twilight_minutes.unwrap_or(d.twilight_minutes),
    }
}

/// A display's presentation `p` while `location` is shown: the location's unit
/// system and its `presentation` keys on top.
pub fn for_location(p: &PresentationConfig, location: &LocationConfig) -> PresentationConfig {
    let mut out = match &location.presentation {
        Some(over) => overlay(p.clone(), over),
        None => p.clone(),
    };
    match location.units {
        Some(UnitSystem::Imperial) => out.units = UnitPrefs::IMPERIAL,
        Some(UnitSystem::Metric) => out.units = UnitPrefs::METRIC,
        // Rejected by validation: they depend on the locale and `[units]`.
        Some(UnitSystem::Auto | UnitSystem::Custom) | None => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use whether_core::presentation::{DateFormat, TimeFormat, WidgetKind};
    use whether_core::units::{PressureUnit, TemperatureUnit, WindUnit};

    fn general(units: UnitSystem) -> General {
        General {
            units,
            ..Default::default()
        }
    }

    #[test]
    fn locations_override_units_and_keys() {
        let base = resolve_presentation(
            &general(UnitSystem::Imperial),
            &UnitsOverride::default(),
            &PresentationToml::default(),
            None,
        );
        assert_eq!(for_location(&base, &LocationConfig::default()), base);
        let abroad = LocationConfig {
            units: Some(UnitSystem::Metric),
            presentation: Some(PresentationToml {
                widgets: Some(vec![WidgetKind::Current, WidgetKind::Alerts]),
                daily_days: Some(40),
                ..PresentationToml::default()
            }),
            ..LocationConfig::default()
        };
        let p = for_location(&base, &abroad);
        assert_eq!(p.units, UnitPrefs::METRIC);
        assert_eq!(p.widgets, [WidgetKind::Current, WidgetKind::Alerts]);
        assert_eq!(p.daily_days, 16, "clamped");
        assert_eq!(p.hourly_hours, base.hourly_hours, "unset keys stay");
        assert_eq!(p.time_format, base.time_format);
    }

    #[test]
    fn territories() {
        assert_eq!(territory("en_US.UTF-8"), Some("US"));
        assert_eq!(territory("es_US@euro"), Some("US"));
        assert_eq!(territory("de_DE"), Some("DE"));
        assert_eq!(territory("C"), None);
        assert_eq!(territory("POSIX"), None);
    }

    #[test]
    fn auto_follows_the_locale() {
        let auto = general(UnitSystem::Auto);
        let none = UnitsOverride::default();
        for (locale, expected) in [
            (Some("en_US.UTF-8"), UnitPrefs::IMPERIAL),
            (Some("my_MM.UTF-8"), UnitPrefs::IMPERIAL),
            (Some("en_LR"), UnitPrefs::IMPERIAL),
            (Some("en_GB.UTF-8"), UnitPrefs::METRIC),
            (Some("C.UTF-8"), UnitPrefs::METRIC),
            (None, UnitPrefs::METRIC),
        ] {
            assert_eq!(resolve_units(&auto, &none, locale), expected, "{locale:?}");
        }
    }

    #[test]
    fn explicit_systems_ignore_the_locale() {
        let none = UnitsOverride::default();
        let us = Some("en_US.UTF-8");
        assert_eq!(
            resolve_units(&general(UnitSystem::Metric), &none, us),
            UnitPrefs::METRIC
        );
        assert_eq!(
            resolve_units(&general(UnitSystem::Imperial), &none, None),
            UnitPrefs::IMPERIAL
        );
        assert_eq!(
            resolve_units(&general(UnitSystem::Custom), &none, us),
            UnitPrefs::METRIC
        );
    }

    #[test]
    fn overrides_apply_on_top() {
        let o = UnitsOverride {
            temperature: Some(TemperatureUnit::Celsius),
            wind: Some(WindUnit::Knots),
            pressure: Some(PressureUnit::Hpa),
            ..Default::default()
        };
        let u = resolve_units(&general(UnitSystem::Imperial), &o, None);
        assert_eq!(u.temperature, TemperatureUnit::Celsius);
        assert_eq!(u.wind, WindUnit::Knots);
        assert_eq!(u.pressure, PressureUnit::Hpa);
        assert_eq!(u.distance, UnitPrefs::IMPERIAL.distance);
    }

    #[test]
    fn empty_presentation_is_the_default() {
        let p = resolve_presentation(
            &General::default(),
            &UnitsOverride::default(),
            &PresentationToml::default(),
            None,
        );
        assert_eq!(p, PresentationConfig::default());
    }

    #[test]
    fn presentation_overrides_and_clamps() {
        let g = General {
            time_format: TimeFormat::H12,
            date_format: DateFormat::DayMonthPadded,
            ..general(UnitSystem::Auto)
        };
        let t = PresentationToml {
            widgets: Some(vec![WidgetKind::Current]),
            hourly_hours: Some(99),
            daily_days: Some(0),
            precip_threshold: Some(250),
            missing: Some("n/a".into()),
            ..Default::default()
        };
        let p = resolve_presentation(&g, &UnitsOverride::default(), &t, Some("en_US.UTF-8"));
        assert_eq!(p.units, UnitPrefs::IMPERIAL);
        assert_eq!(p.time_format, TimeFormat::H12);
        assert_eq!(p.date_format, DateFormat::DayMonthPadded);
        assert_eq!(p.widgets, vec![WidgetKind::Current]);
        assert_eq!(
            (p.hourly_hours, p.daily_days, p.precip_threshold),
            (48, 1, 100)
        );
        assert_eq!(p.missing, "n/a");
        assert_eq!(p.details, PresentationConfig::default().details);
    }
}
