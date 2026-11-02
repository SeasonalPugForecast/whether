//! Physical quantities in canonical units and conversions to display units.
//!
//! Every quantity is stored in one canonical unit (°C, m/s, hPa, mm, m, %, °).
//! Conversion to the user's units happens only when presenting.

macro_rules! quantity {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
        #[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize), serde(transparent))]
        #[repr(transparent)]
        pub struct $name(pub f32);
    };
}

quantity!(
    /// Temperature in degrees Celsius.
    Celsius
);
quantity!(
    /// Speed in metres per second.
    Mps
);
quantity!(
    /// Pressure in hectopascals.
    Hpa
);
quantity!(
    /// Precipitation depth in millimetres.
    Mm
);
quantity!(
    /// Distance in metres.
    Meters
);
quantity!(
    /// A percentage in `0.0..=100.0`.
    Percent
);
quantity!(
    /// An angle in degrees; for wind, the direction the wind blows *from*.
    Degrees
);

/// Temperature display unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "lowercase")
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum TemperatureUnit {
    /// Degrees Celsius.
    Celsius,
    /// Degrees Fahrenheit.
    Fahrenheit,
    /// Kelvin.
    Kelvin,
}

impl TemperatureUnit {
    /// Every unit, in the order a picker lists them.
    pub const ALL: [Self; 3] = [Self::Celsius, Self::Fahrenheit, Self::Kelvin];

    /// Its symbol: "°C", "°F" or "K".
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Celsius => "°C",
            Self::Fahrenheit => "°F",
            Self::Kelvin => "K",
        }
    }
}

/// Wind speed display unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum WindUnit {
    /// Metres per second.
    #[cfg_attr(feature = "serde", serde(rename = "m/s"))]
    Mps,
    /// Kilometres per hour.
    #[cfg_attr(feature = "serde", serde(rename = "km/h"))]
    Kmh,
    /// Miles per hour.
    #[cfg_attr(feature = "serde", serde(rename = "mph"))]
    Mph,
    /// Knots.
    #[cfg_attr(feature = "serde", serde(rename = "kn"))]
    Knots,
    /// Beaufort force number.
    #[cfg_attr(feature = "serde", serde(rename = "bft"))]
    Beaufort,
}

impl WindUnit {
    /// Every unit, in the order a picker lists them.
    pub const ALL: [Self; 5] = [Self::Kmh, Self::Mph, Self::Mps, Self::Knots, Self::Beaufort];

    /// Its symbol: "km/h", "mph", "m/s", "kn" or "Bft".
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Mps => "m/s",
            Self::Kmh => "km/h",
            Self::Mph => "mph",
            Self::Knots => "kn",
            Self::Beaufort => "Bft",
        }
    }
}

/// Pressure display unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum PressureUnit {
    /// Hectopascals.
    #[cfg_attr(feature = "serde", serde(rename = "hPa"))]
    Hpa,
    /// Inches of mercury.
    #[cfg_attr(feature = "serde", serde(rename = "inHg"))]
    InHg,
    /// Millimetres of mercury.
    #[cfg_attr(feature = "serde", serde(rename = "mmHg"))]
    MmHg,
    /// Kilopascals.
    #[cfg_attr(feature = "serde", serde(rename = "kPa"))]
    Kpa,
}

impl PressureUnit {
    /// Every unit, in the order a picker lists them.
    pub const ALL: [Self; 4] = [Self::Hpa, Self::InHg, Self::MmHg, Self::Kpa];

    /// Its symbol: "hPa", "inHg", "mmHg" or "kPa".
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Hpa => "hPa",
            Self::InHg => "inHg",
            Self::MmHg => "mmHg",
            Self::Kpa => "kPa",
        }
    }
}

/// Precipitation display unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "lowercase")
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum PrecipUnit {
    /// Millimetres.
    Mm,
    /// Inches.
    In,
}

impl PrecipUnit {
    /// Every unit, in the order a picker lists them.
    pub const ALL: [Self; 2] = [Self::Mm, Self::In];

    /// Its symbol: "mm" or "in".
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Mm => "mm",
            Self::In => "in",
        }
    }
}

/// Distance display unit (visibility).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "lowercase")
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum DistanceUnit {
    /// Kilometres.
    Km,
    /// Miles.
    Mi,
}

impl DistanceUnit {
    /// Every unit, in the order a picker lists them.
    pub const ALL: [Self; 2] = [Self::Km, Self::Mi];

    /// Its symbol: "km" or "mi".
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Km => "km",
            Self::Mi => "mi",
        }
    }
}

/// The unit system selected in configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "lowercase")
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum UnitSystem {
    /// Chosen from the locale (resolved in `whether-config`).
    #[default]
    Auto,
    /// °C, km/h, hPa, mm, km.
    Metric,
    /// °F, mph, inHg, in, mi.
    Imperial,
    /// Per-quantity units from the `[units]` table.
    Custom,
}

/// Fully resolved display units, one per quantity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct UnitPrefs {
    /// Temperature unit.
    pub temperature: TemperatureUnit,
    /// Wind speed unit.
    pub wind: WindUnit,
    /// Pressure unit.
    pub pressure: PressureUnit,
    /// Precipitation unit.
    pub precipitation: PrecipUnit,
    /// Distance unit.
    pub distance: DistanceUnit,
}

impl UnitPrefs {
    /// °C, km/h, hPa, mm, km.
    pub const METRIC: Self = Self {
        temperature: TemperatureUnit::Celsius,
        wind: WindUnit::Kmh,
        pressure: PressureUnit::Hpa,
        precipitation: PrecipUnit::Mm,
        distance: DistanceUnit::Km,
    };

    /// °F, mph, inHg, in, mi.
    pub const IMPERIAL: Self = Self {
        temperature: TemperatureUnit::Fahrenheit,
        wind: WindUnit::Mph,
        pressure: PressureUnit::InHg,
        precipitation: PrecipUnit::In,
        distance: DistanceUnit::Mi,
    };

    /// The other standard system: metric becomes imperial; anything else becomes metric.
    #[must_use]
    pub fn toggled(self) -> Self {
        if self == Self::IMPERIAL {
            Self::METRIC
        } else {
            Self::IMPERIAL
        }
    }
}

impl Default for UnitPrefs {
    fn default() -> Self {
        Self::METRIC
    }
}

const MPS_TO_KMH: f32 = 3.6;
const MPS_TO_MPH: f32 = 2.236_936;
const MPS_TO_KN: f32 = 1.943_844;
const HPA_TO_INHG: f32 = 0.029_529_983;
const HPA_TO_MMHG: f32 = 0.750_061_7;
const HPA_TO_KPA: f32 = 0.1;
const MM_PER_IN: f32 = 25.4;
const M_PER_MI: f32 = 1_609.344;

impl Celsius {
    /// The value in `unit`.
    pub fn to(self, unit: TemperatureUnit) -> f32 {
        match unit {
            TemperatureUnit::Celsius => self.0,
            TemperatureUnit::Fahrenheit => self.0 * 9.0 / 5.0 + 32.0,
            TemperatureUnit::Kelvin => self.0 + 273.15,
        }
    }

    /// Builds a canonical value from a value expressed in `unit`.
    pub fn from_unit(value: f32, unit: TemperatureUnit) -> Self {
        Self(match unit {
            TemperatureUnit::Celsius => value,
            TemperatureUnit::Fahrenheit => (value - 32.0) * 5.0 / 9.0,
            TemperatureUnit::Kelvin => value - 273.15,
        })
    }
}

impl Mps {
    /// The value in `unit`; [`WindUnit::Beaufort`] yields the force number.
    pub fn to(self, unit: WindUnit) -> f32 {
        match unit {
            WindUnit::Mps => self.0,
            WindUnit::Kmh => self.0 * MPS_TO_KMH,
            WindUnit::Mph => self.0 * MPS_TO_MPH,
            WindUnit::Knots => self.0 * MPS_TO_KN,
            WindUnit::Beaufort => f32::from(beaufort(self.0)),
        }
    }

    /// Builds a canonical value from a value in `unit`, or `None` for Beaufort (not invertible).
    pub fn from_unit(value: f32, unit: WindUnit) -> Option<Self> {
        let mps = match unit {
            WindUnit::Mps => value,
            WindUnit::Kmh => value / MPS_TO_KMH,
            WindUnit::Mph => value / MPS_TO_MPH,
            WindUnit::Knots => value / MPS_TO_KN,
            WindUnit::Beaufort => return None,
        };
        Some(Self(mps))
    }
}

impl Hpa {
    /// The value in `unit`.
    pub fn to(self, unit: PressureUnit) -> f32 {
        match unit {
            PressureUnit::Hpa => self.0,
            PressureUnit::InHg => self.0 * HPA_TO_INHG,
            PressureUnit::MmHg => self.0 * HPA_TO_MMHG,
            PressureUnit::Kpa => self.0 * HPA_TO_KPA,
        }
    }

    /// Builds a canonical value from a value in `unit`.
    pub fn from_unit(value: f32, unit: PressureUnit) -> Self {
        Self(match unit {
            PressureUnit::Hpa => value,
            PressureUnit::InHg => value / HPA_TO_INHG,
            PressureUnit::MmHg => value / HPA_TO_MMHG,
            PressureUnit::Kpa => value / HPA_TO_KPA,
        })
    }
}

impl Mm {
    /// The value in `unit`.
    pub fn to(self, unit: PrecipUnit) -> f32 {
        match unit {
            PrecipUnit::Mm => self.0,
            PrecipUnit::In => self.0 / MM_PER_IN,
        }
    }

    /// Builds a canonical value from a value in `unit`.
    pub fn from_unit(value: f32, unit: PrecipUnit) -> Self {
        Self(match unit {
            PrecipUnit::Mm => value,
            PrecipUnit::In => value * MM_PER_IN,
        })
    }
}

impl Meters {
    /// The value in `unit` (kilometres or miles).
    pub fn to(self, unit: DistanceUnit) -> f32 {
        match unit {
            DistanceUnit::Km => self.0 / 1_000.0,
            DistanceUnit::Mi => self.0 / M_PER_MI,
        }
    }

    /// Builds a canonical value from kilometres or miles.
    pub fn from_unit(value: f32, unit: DistanceUnit) -> Self {
        Self(match unit {
            DistanceUnit::Km => value * 1_000.0,
            DistanceUnit::Mi => value * M_PER_MI,
        })
    }
}

/// Upper bounds (exclusive, m/s) of Beaufort forces 0 through 11; anything above is force 12.
const BEAUFORT_BOUNDS: [f32; 12] = [
    0.5, 1.6, 3.4, 5.5, 8.0, 10.8, 13.9, 17.2, 20.8, 24.5, 28.5, 32.7,
];

/// The Beaufort force (0–12) for a wind speed in m/s.
pub fn beaufort(mps: f32) -> u8 {
    let force = BEAUFORT_BOUNDS
        .iter()
        .take_while(|&&bound| mps >= bound)
        .count();
    // INVARIANT: at most 12 bounds are counted.
    u8::try_from(force).unwrap_or(12)
}

/// `Some(x)` when `x` is finite; `None` for NaN and infinities (used by parsers).
pub fn finite(x: f32) -> Option<f32> {
    x.is_finite().then_some(x)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn close(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn symbols_name_every_unit_once() {
        let all: Vec<&str> = TemperatureUnit::ALL
            .iter()
            .map(|u| u.symbol())
            .chain(WindUnit::ALL.iter().map(|u| u.symbol()))
            .chain(PressureUnit::ALL.iter().map(|u| u.symbol()))
            .chain(PrecipUnit::ALL.iter().map(|u| u.symbol()))
            .chain(DistanceUnit::ALL.iter().map(|u| u.symbol()))
            .collect();
        assert_eq!(
            all,
            [
                "°C", "°F", "K", "km/h", "mph", "m/s", "kn", "Bft", "hPa", "inHg", "mmHg", "kPa",
                "mm", "in", "km", "mi"
            ]
        );
    }

    #[test]
    fn temperature_reference_points() {
        assert!(close(
            Celsius(0.0).to(TemperatureUnit::Fahrenheit),
            32.0,
            1e-4
        ));
        assert!(close(
            Celsius(100.0).to(TemperatureUnit::Fahrenheit),
            212.0,
            1e-4
        ));
        assert!(close(
            Celsius(-40.0).to(TemperatureUnit::Fahrenheit),
            -40.0,
            1e-4
        ));
        assert!(close(
            Celsius(0.0).to(TemperatureUnit::Kelvin),
            273.15,
            1e-4
        ));
    }

    #[test]
    fn wind_reference_points() {
        let w = Mps(10.0);
        assert!(close(w.to(WindUnit::Kmh), 36.0, 1e-3));
        assert!(close(w.to(WindUnit::Mph), 22.37, 5e-3));
        assert!(close(w.to(WindUnit::Knots), 19.44, 5e-3));
        assert!(close(w.to(WindUnit::Beaufort), 5.0, 0.0));
    }

    #[test]
    fn pressure_precip_distance_reference_points() {
        let p = Hpa(1013.25);
        assert!(close(p.to(PressureUnit::InHg), 29.92, 5e-3));
        assert!(close(p.to(PressureUnit::MmHg), 760.0, 0.05));
        assert!(close(p.to(PressureUnit::Kpa), 101.325, 1e-3));
        assert!(close(Mm(25.4).to(PrecipUnit::In), 1.0, 1e-6));
        assert!(close(Meters(1_609.344).to(DistanceUnit::Mi), 1.0, 1e-6));
        assert!(close(Meters(2_500.0).to(DistanceUnit::Km), 2.5, 1e-6));
    }

    #[test]
    fn beaufort_both_sides_of_every_boundary() {
        assert_eq!(beaufort(0.0), 0);
        for (force, &bound) in BEAUFORT_BOUNDS.iter().enumerate() {
            let force = u8::try_from(force).unwrap();
            assert_eq!(beaufort(bound - 0.01), force, "just below {bound}");
            assert_eq!(beaufort(bound), force + 1, "at {bound}");
        }
        assert_eq!(beaufort(60.0), 12);
    }

    #[test]
    fn beaufort_is_not_invertible() {
        assert_eq!(Mps::from_unit(4.0, WindUnit::Beaufort), None);
    }

    #[test]
    fn toggling_swaps_systems() {
        assert_eq!(UnitPrefs::METRIC.toggled(), UnitPrefs::IMPERIAL);
        assert_eq!(UnitPrefs::IMPERIAL.toggled(), UnitPrefs::METRIC);
        let custom = UnitPrefs {
            wind: WindUnit::Knots,
            ..UnitPrefs::METRIC
        };
        assert_eq!(custom.toggled(), UnitPrefs::IMPERIAL);
    }

    #[test]
    fn finite_rejects_nan_and_infinity() {
        assert_eq!(finite(1.5), Some(1.5));
        assert_eq!(finite(f32::NAN), None);
        assert_eq!(finite(f32::INFINITY), None);
        assert_eq!(finite(f32::NEG_INFINITY), None);
    }

    fn rel_close(a: f32, b: f32) -> bool {
        (a - b).abs() <= 1e-3 * b.abs().max(1.0)
    }

    proptest! {
        #[test]
        fn conversions_round_trip(v in -500.0f32..5_000.0) {
            for u in [TemperatureUnit::Celsius, TemperatureUnit::Fahrenheit, TemperatureUnit::Kelvin] {
                prop_assert!(rel_close(Celsius::from_unit(Celsius(v).to(u), u).0, v));
            }
            for u in [WindUnit::Mps, WindUnit::Kmh, WindUnit::Mph, WindUnit::Knots] {
                prop_assert!(rel_close(Mps::from_unit(Mps(v).to(u), u).unwrap().0, v));
            }
            for u in [PressureUnit::Hpa, PressureUnit::InHg, PressureUnit::MmHg, PressureUnit::Kpa] {
                prop_assert!(rel_close(Hpa::from_unit(Hpa(v).to(u), u).0, v));
            }
            for u in [PrecipUnit::Mm, PrecipUnit::In] {
                prop_assert!(rel_close(Mm::from_unit(Mm(v).to(u), u).0, v));
            }
            for u in [DistanceUnit::Km, DistanceUnit::Mi] {
                prop_assert!(rel_close(Meters::from_unit(Meters(v).to(u), u).0, v));
            }
        }
    }
}

#[cfg(all(test, feature = "schema"))]
mod schema_tests {
    use super::*;

    #[test]
    fn schemas_list_the_toml_names() {
        let wind = serde_json::to_string(&schemars::schema_for!(WindUnit)).unwrap();
        for name in ["m/s", "km/h", "mph", "kn", "bft"] {
            assert!(wind.contains(&alloc::format!("\"{name}\"")), "{wind}");
        }
        let system = serde_json::to_string(&schemars::schema_for!(UnitSystem)).unwrap();
        assert!(
            system.contains("\"auto\"") && system.contains("\"custom\""),
            "{system}"
        );
    }
}
