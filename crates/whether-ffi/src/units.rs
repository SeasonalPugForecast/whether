//! Units one by one: the overrides the phone applies over its unit
//! system, and the core's symbols for the picker's options.

use whether_core::units as core;

/// Temperature unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum TemperatureUnit {
    /// °C.
    Celsius,
    /// °F.
    Fahrenheit,
    /// K.
    Kelvin,
}

/// Wind speed unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum WindUnit {
    /// km/h.
    Kmh,
    /// mph.
    Mph,
    /// m/s.
    Mps,
    /// Knots.
    Knots,
    /// Beaufort force.
    Beaufort,
}

/// Pressure unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum PressureUnit {
    /// hPa.
    Hpa,
    /// inHg.
    InHg,
    /// mmHg.
    MmHg,
    /// kPa.
    Kpa,
}

/// Precipitation unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum PrecipitationUnit {
    /// mm.
    Mm,
    /// in.
    In,
}

/// Distance unit (visibility).
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum DistanceUnit {
    /// km.
    Km,
    /// mi.
    Mi,
}

macro_rules! mirror {
    ($ffi:ident <-> $core:ident { $($v:ident),+ $(,)? }) => {
        impl From<$ffi> for core::$core {
            fn from(unit: $ffi) -> Self {
                match unit { $($ffi::$v => Self::$v),+ }
            }
        }
        impl From<core::$core> for $ffi {
            fn from(unit: core::$core) -> Self {
                match unit { $(core::$core::$v => Self::$v),+ }
            }
        }
    };
}

mirror!(TemperatureUnit <-> TemperatureUnit { Celsius, Fahrenheit, Kelvin });
mirror!(WindUnit <-> WindUnit { Kmh, Mph, Mps, Knots, Beaufort });
mirror!(PressureUnit <-> PressureUnit { Hpa, InHg, MmHg, Kpa });
mirror!(DistanceUnit <-> DistanceUnit { Km, Mi });

impl From<PrecipitationUnit> for core::PrecipUnit {
    fn from(unit: PrecipitationUnit) -> Self {
        match unit {
            PrecipitationUnit::Mm => Self::Mm,
            PrecipitationUnit::In => Self::In,
        }
    }
}

impl From<core::PrecipUnit> for PrecipitationUnit {
    fn from(unit: core::PrecipUnit) -> Self {
        match unit {
            core::PrecipUnit::Mm => Self::Mm,
            core::PrecipUnit::In => Self::In,
        }
    }
}

/// Units that replace the chosen system's for one quantity each; `None` keeps
/// the system's own (the screen's "Automatic").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, uniffi::Record)]
pub struct UnitOverrides {
    /// Temperature.
    #[uniffi(default = None)]
    pub temperature: Option<TemperatureUnit>,
    /// Wind speed.
    #[uniffi(default = None)]
    pub wind: Option<WindUnit>,
    /// Pressure.
    #[uniffi(default = None)]
    pub pressure: Option<PressureUnit>,
    /// Precipitation depth.
    #[uniffi(default = None)]
    pub precipitation: Option<PrecipitationUnit>,
    /// Distance (visibility).
    #[uniffi(default = None)]
    pub distance: Option<DistanceUnit>,
}

impl UnitOverrides {
    /// The same as the config's `[units]` table.
    pub(crate) fn to_config(self) -> whether_config::UnitsOverride {
        whether_config::UnitsOverride {
            temperature: self.temperature.map(Into::into),
            wind: self.wind.map(Into::into),
            pressure: self.pressure.map(Into::into),
            precipitation: self.precipitation.map(Into::into),
            distance: self.distance.map(Into::into),
        }
    }

    /// From the config's `[units]` table.
    pub(crate) fn from_config(o: whether_config::UnitsOverride) -> Self {
        Self {
            temperature: o.temperature.map(Into::into),
            wind: o.wind.map(Into::into),
            pressure: o.pressure.map(Into::into),
            precipitation: o.precipitation.map(Into::into),
            distance: o.distance.map(Into::into),
        }
    }

    /// Whether nothing is overridden.
    pub(crate) fn is_empty(self) -> bool {
        self == Self::default()
    }
}

/// One option of a picker: a unit and its symbol.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct TemperatureOption {
    /// The unit.
    pub unit: TemperatureUnit,
    /// Its symbol (`"°C"`).
    pub label: String,
}

/// One option of the wind picker.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct WindOption {
    /// The unit.
    pub unit: WindUnit,
    /// Its symbol (`"km/h"`).
    pub label: String,
}

/// One option of the pressure picker.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PressureOption {
    /// The unit.
    pub unit: PressureUnit,
    /// Its symbol (`"hPa"`).
    pub label: String,
}

/// One option of the precipitation picker.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PrecipitationOption {
    /// The unit.
    pub unit: PrecipitationUnit,
    /// Its symbol (`"mm"`).
    pub label: String,
}

/// One option of the distance picker.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct DistanceOption {
    /// The unit.
    pub unit: DistanceUnit,
    /// Its symbol (`"km"`).
    pub label: String,
}

/// Every unit the core can show, with its symbol, for the custom units screen.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct UnitLabels {
    /// Temperature units.
    pub temperature: Vec<TemperatureOption>,
    /// Wind speed units.
    pub wind: Vec<WindOption>,
    /// Pressure units.
    pub pressure: Vec<PressureOption>,
    /// Precipitation units.
    pub precipitation: Vec<PrecipitationOption>,
    /// Distance units.
    pub distance: Vec<DistanceOption>,
}

/// The units to offer per quantity, with the core's symbols (Swift writes none).
#[uniffi::export]
pub fn unit_labels() -> UnitLabels {
    UnitLabels {
        temperature: core::TemperatureUnit::ALL
            .into_iter()
            .map(|u| TemperatureOption {
                unit: u.into(),
                label: u.symbol().to_string(),
            })
            .collect(),
        wind: core::WindUnit::ALL
            .into_iter()
            .map(|u| WindOption {
                unit: u.into(),
                label: u.symbol().to_string(),
            })
            .collect(),
        pressure: core::PressureUnit::ALL
            .into_iter()
            .map(|u| PressureOption {
                unit: u.into(),
                label: u.symbol().to_string(),
            })
            .collect(),
        precipitation: core::PrecipUnit::ALL
            .into_iter()
            .map(|u| PrecipitationOption {
                unit: u.into(),
                label: u.symbol().to_string(),
            })
            .collect(),
        distance: core::DistanceUnit::ALL
            .into_iter()
            .map(|u| DistanceOption {
                unit: u.into(),
                label: u.symbol().to_string(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_labels_are_the_cores_symbols() {
        let l = unit_labels();
        assert_eq!(l.temperature.len(), 3);
        let wind: Vec<_> = l.wind.iter().map(|o| o.label.as_str()).collect();
        assert_eq!(wind, ["km/h", "mph", "m/s", "kn", "Bft"]);
        let pressure: Vec<_> = l.pressure.iter().map(|o| o.label.as_str()).collect();
        assert_eq!(pressure, ["hPa", "inHg", "mmHg", "kPa"]);
        assert_eq!(l.precipitation[1].label, "in");
        assert_eq!(l.distance[1].unit, DistanceUnit::Mi);
    }

    #[test]
    fn every_option_maps_to_its_core_unit_and_back() {
        let l = unit_labels();
        for o in &l.wind {
            let c: core::WindUnit = o.unit.into();
            assert_eq!(c.symbol(), o.label);
            assert_eq!(WindUnit::from(c), o.unit);
        }
        for o in &l.precipitation {
            let c: core::PrecipUnit = o.unit.into();
            assert_eq!(c.symbol(), o.label);
        }
    }

    #[test]
    fn overrides_round_trip_through_the_config_table() {
        let o = UnitOverrides {
            temperature: Some(TemperatureUnit::Kelvin),
            wind: Some(WindUnit::Beaufort),
            pressure: Some(PressureUnit::MmHg),
            precipitation: Some(PrecipitationUnit::In),
            distance: Some(DistanceUnit::Mi),
        };
        assert_eq!(UnitOverrides::from_config(o.to_config()), o);
        assert!(UnitOverrides::default().is_empty() && !o.is_empty());
    }
}
