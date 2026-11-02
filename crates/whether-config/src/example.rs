//! The commented example configuration, which doubles as the contract for defaults.

/// `docs/config/whether.example.toml`, written by `whether config init`.
pub const EXAMPLE: &str = include_str!("../../../docs/config/whether.example.toml");

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load::{LinuxConfig, LoadOptions, load};
    use crate::types::CommonConfig;
    use whether_core::presentation::PresentationConfig;

    fn load_example() -> LinuxConfig {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, EXAMPLE).unwrap();
        let env = |_: &str| None;
        load(&LoadOptions {
            explicit_path: Some(&path),
            env: &env,
            cli_overrides: vec![],
        })
        .unwrap()
    }

    #[test]
    fn example_validates() {
        let c = load_example();
        assert_eq!(c.issues, vec![], "the example has no warnings either");
    }

    #[test]
    fn example_matches_defaults() {
        let c = load_example();
        let defaults = CommonConfig::default();
        assert_eq!(c.common.version, defaults.version);
        assert_eq!(c.common.general, defaults.general);
        assert_eq!(c.common.units, defaults.units);
        assert_eq!(c.common.providers, defaults.providers);
        assert_eq!(c.common.strings, defaults.strings);
        // `units = "auto"` with no locale resolves to metric, the default.
        assert_eq!(
            c.presentation_for("tui").unwrap(),
            PresentationConfig::default()
        );
        assert_eq!(c.display.default.as_deref(), Some("tui"));
        assert_eq!(c.runtime, crate::load::RuntimeConfig::default());
        // Extended later: [display.line|tui|gui] equal each
        // plugin's defaults.
        for name in ["tui", "gui", "line"] {
            assert!(c.display.plugins.contains_key(name), "{name}");
        }
    }

    #[test]
    fn example_location_is_the_test_zip() {
        let c = load_example();
        let loc = &c.common.location[0];
        assert_eq!(
            (loc.zip.as_deref(), loc.country.as_deref()),
            (Some("10001"), Some("US"))
        );
    }

    /// The commented Pirate Weather section is valid once uncommented.
    #[test]
    fn example_pirate_weather_section_loads() {
        let section: String = EXAMPLE
            .lines()
            .skip_while(|l| !l.starts_with("# [providers.pirate-weather]"))
            .take_while(|l| !l.starts_with("# [providers.nws]"))
            .filter_map(|l| l.strip_prefix("# "))
            .filter(|l| l.starts_with('[') || l.contains(" = "))
            .map(|l| l.split(" #").next().unwrap_or(l).to_string() + "\n")
            .collect();
        assert!(
            section.starts_with("[providers.pirate-weather]"),
            "{section}"
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            format!(
                "[providers]\norder = [\"pirate-weather\", \"open-meteo\"]\n{section}\
                 [[location]]\nname = \"Home\"\nlatitude = 40.7\nlongitude = -74.0\n"
            ),
        )
        .unwrap();
        let env = |_: &str| None;
        let c = load(&LoadOptions {
            explicit_path: Some(&path),
            env: &env,
            cli_overrides: vec![],
        })
        .unwrap();
        assert_eq!(c.issues, vec![], "{section}");
        let p = c.common.providers.pirate_weather.unwrap();
        assert_eq!(p.monthly_call_limit, Some(9000));
        assert_eq!(p.exclude_models, Some(vec!["gfs".to_string()]));
    }

    #[test]
    fn example_points_editors_at_the_schema() {
        assert!(EXAMPLE.starts_with("#:schema ./whether.schema.json\n"));
    }
}
