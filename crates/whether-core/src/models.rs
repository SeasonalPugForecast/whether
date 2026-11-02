//! The weather models a user can choose from Open-Meteo: ids, names and
//! the area each covers. The view uses the short names to credit the models that
//! supplied a forecast; providers use the regions to drop models that cannot
//! forecast a place.

/// The area a model forecasts. Boxes live in `whether-providers::regions`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    /// The whole globe.
    Global,
    /// The lower 48 US states (NOAA's CONUS models).
    Conus,
    /// Canada and the northern US. The HRDPS grid is rotated, so its edge is
    /// irregular: it covers New York and Chicago but not Nashville or Denver.
    NorthAmerica,
}

/// One selectable model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelInfo {
    /// Open-Meteo's `models` value.
    pub id: &'static str,
    /// Full name, for lists.
    pub name: &'static str,
    /// Short name, for the status line.
    pub short: &'static str,
    /// Where it has data.
    pub region: Region,
    /// About how many hours ahead it forecasts.
    pub hours: u16,
}

const fn model(
    id: &'static str,
    name: &'static str,
    short: &'static str,
    region: Region,
    hours: u16,
) -> ModelInfo {
    ModelInfo {
        id,
        name,
        short,
        region,
        hours,
    }
}

/// Open-Meteo's own choice per place, used when no models are configured.
pub const BEST_MATCH: &str = "best_match";

/// The models offered, verified for ZIP 10001 on 2026-08-04. Hours are the
/// forecast lengths measured that day, rounded to the model's nominal range.
pub const CATALOGUE: &[ModelInfo] = &[
    model(
        BEST_MATCH,
        "Best match (Open-Meteo)",
        "Best match",
        Region::Global,
        384,
    ),
    model(
        "gfs_seamless",
        "NOAA GFS seamless",
        "GFS",
        Region::Global,
        384,
    ),
    model(
        "gfs_global",
        "NOAA GFS global",
        "GFS global",
        Region::Global,
        384,
    ),
    model(
        "ncep_hrrr_conus",
        "NOAA HRRR (3 km)",
        "HRRR",
        Region::Conus,
        48,
    ),
    model(
        "ncep_hrrr_conus_15min",
        "NOAA HRRR 15-minute",
        "HRRR 15 min",
        Region::Conus,
        18,
    ),
    model(
        "ncep_nbm_conus",
        "NOAA National Blend of Models",
        "NBM",
        Region::Conus,
        264,
    ),
    model(
        "ncep_nam_conus",
        "NOAA NAM (3 km)",
        "NAM",
        Region::Conus,
        60,
    ),
    model(
        "ncep_aigfs025",
        "NOAA AIGFS (AI)",
        "AIGFS",
        Region::Global,
        384,
    ),
    model(
        "ecmwf_ifs025",
        "ECMWF IFS 0.25°",
        "ECMWF",
        Region::Global,
        360,
    ),
    model(
        "ecmwf_ifs",
        "ECMWF IFS HRES (9 km)",
        "ECMWF HRES",
        Region::Global,
        360,
    ),
    model(
        "ecmwf_aifs025_single",
        "ECMWF AIFS (AI)",
        "ECMWF AI",
        Region::Global,
        360,
    ),
    model(
        "gem_seamless",
        "Canada GEM seamless",
        "GEM",
        Region::Global,
        240,
    ),
    model(
        "gem_hrdps_continental",
        "Canada HRDPS (2.5 km)",
        "HRDPS",
        Region::NorthAmerica,
        48,
    ),
    model(
        "icon_seamless",
        "DWD ICON seamless",
        "ICON",
        Region::Global,
        180,
    ),
    model(
        "ukmo_seamless",
        "UK Met Office seamless",
        "UKMO",
        Region::Global,
        168,
    ),
    model(
        "meteofrance_seamless",
        "Météo-France seamless",
        "Météo-France",
        Region::Global,
        96,
    ),
    model("jma_seamless", "JMA seamless", "JMA", Region::Global, 264),
];

/// A named model list the settings screens offer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Preset {
    /// What the screens call it.
    pub name: &'static str,
    /// The models in priority order; empty is Open-Meteo's best match.
    pub models: &'static [&'static str],
}

/// The presets, best match first.
pub const PRESETS: &[Preset] = &[
    Preset {
        name: "Best match",
        models: &[],
    },
    Preset {
        name: "NOAA blend (US)",
        models: &["ncep_nbm_conus"],
    },
    Preset {
        name: "US high resolution (HRRR → NBM → GFS)",
        models: &["ncep_hrrr_conus", "ncep_nbm_conus", "gfs_seamless"],
    },
    Preset {
        name: "ECMWF",
        models: &["ecmwf_ifs025"],
    },
    Preset {
        name: "ECMWF AI (AIFS → IFS)",
        models: &["ecmwf_aifs025_single", "ecmwf_ifs025"],
    },
    Preset {
        name: "Canada (GEM)",
        models: &["gem_seamless"],
    },
    Preset {
        name: "DWD ICON",
        models: &["icon_seamless"],
    },
];

/// The preset whose list is exactly `models`, if any.
pub fn preset_for<S: AsRef<str>>(models: &[S]) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| {
        p.models.len() == models.len() && p.models.iter().zip(models).all(|(a, b)| *a == b.as_ref())
    })
}

/// The catalogue entry for `id`.
pub fn lookup(id: &str) -> Option<&'static ModelInfo> {
    CATALOGUE.iter().find(|m| m.id == id)
}

/// The short name of `id`, or `id` itself for a model the catalogue lacks.
pub fn short_name(id: &str) -> &str {
    lookup(id).map_or(id, |m| m.short)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_lowercase() {
        for (i, m) in CATALOGUE.iter().enumerate() {
            assert!(
                CATALOGUE[i + 1..].iter().all(|other| other.id != m.id),
                "{}",
                m.id
            );
            assert!(
                m.id.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
                "{}",
                m.id
            );
        }
    }

    #[test]
    fn presets_use_known_models_and_are_found_by_list() {
        for preset in PRESETS {
            assert!(preset.models.len() <= 4, "{}", preset.name);
            for id in preset.models {
                assert!(lookup(id).is_some(), "{}: {id}", preset.name);
            }
        }
        assert_eq!(preset_for::<&str>(&[]).map(|p| p.name), Some("Best match"));
        assert_eq!(
            preset_for(&["ncep_nbm_conus"]).map(|p| p.name),
            Some("NOAA blend (US)")
        );
        assert_eq!(preset_for(&["ncep_nbm_conus", "gfs_seamless"]), None);
    }

    #[test]
    fn short_names_fall_back_to_the_id() {
        assert_eq!(short_name("ncep_nbm_conus"), "NBM");
        assert_eq!(short_name("knmi_seamless"), "knmi_seamless");
        assert_eq!(lookup(BEST_MATCH).map(|m| m.region), Some(Region::Global));
    }
}
