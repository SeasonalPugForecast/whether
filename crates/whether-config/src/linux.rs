//! Where configuration files live on Linux (XDG base directories).

use std::path::{Path, PathBuf};

/// A config file's path under an XDG config directory.
const FILE: &str = "whether/config.toml";

/// Candidate configuration files, lowest precedence first: every directory in
/// `XDG_CONFIG_DIRS` (default `/etc/xdg`, the first listed wins), then
/// `XDG_CONFIG_HOME` (default `~/.config`). Relative XDG paths are ignored, as the
/// specification requires.
pub fn config_layers(env: &dyn Fn(&str) -> Option<String>) -> Vec<PathBuf> {
    let dirs = env("XDG_CONFIG_DIRS")
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "/etc/xdg".to_string());
    let mut layers: Vec<PathBuf> = dirs
        .split(':')
        .filter(|d| Path::new(d).is_absolute())
        .map(|d| Path::new(d).join(FILE))
        .collect();
    layers.reverse();
    if let Some(home) = user_config_dir(env) {
        layers.push(home.join(FILE));
    }
    layers
}

/// `XDG_CONFIG_HOME`, else `$HOME/.config`.
pub fn user_config_dir(env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    env("XDG_CONFIG_HOME")
        .filter(|v| Path::new(v).is_absolute())
        .map(PathBuf::from)
        .or_else(|| {
            env("HOME")
                .filter(|h| !h.is_empty())
                .map(|h| Path::new(&h).join(".config"))
        })
}

/// The user's configuration file (where `config init` and `locate --add` write).
pub fn user_config_file(env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    user_config_dir(env).map(|d| d.join(FILE))
}

/// The measurement locale: `LC_ALL`, then `LC_MEASUREMENT`, then `LANG`.
pub fn measurement_locale(env: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    ["LC_ALL", "LC_MEASUREMENT", "LANG"]
        .into_iter()
        .find_map(|var| env(var).filter(|v| !v.is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        move |k| {
            pairs
                .iter()
                .find(|(name, _)| name == k)
                .map(|(_, v)| v.clone())
        }
    }

    #[test]
    fn default_layers() {
        let layers = config_layers(&env_of(&[("HOME", "/home/u")]));
        assert_eq!(
            layers,
            vec![
                PathBuf::from("/etc/xdg/whether/config.toml"),
                PathBuf::from("/home/u/.config/whether/config.toml")
            ]
        );
    }

    #[test]
    fn xdg_variables_take_effect() {
        let env = env_of(&[
            ("HOME", "/home/u"),
            ("XDG_CONFIG_HOME", "/cfg"),
            ("XDG_CONFIG_DIRS", "/first:relative:/second"),
        ]);
        assert_eq!(
            config_layers(&env),
            vec![
                PathBuf::from("/second/whether/config.toml"),
                PathBuf::from("/first/whether/config.toml"),
                PathBuf::from("/cfg/whether/config.toml"),
            ]
        );
    }

    #[test]
    fn relative_config_home_falls_back_to_home() {
        let env = env_of(&[("HOME", "/home/u"), ("XDG_CONFIG_HOME", "cfg")]);
        assert_eq!(
            user_config_file(&env),
            Some(PathBuf::from("/home/u/.config/whether/config.toml"))
        );
        assert_eq!(user_config_file(&env_of(&[])), None);
        assert_eq!(
            config_layers(&env_of(&[])).len(),
            1,
            "only the system layer"
        );
    }

    #[test]
    fn locale_precedence() {
        let all = env_of(&[
            ("LC_ALL", "de_DE.UTF-8"),
            ("LC_MEASUREMENT", "en_US.UTF-8"),
            ("LANG", "C"),
        ]);
        assert_eq!(measurement_locale(&all).as_deref(), Some("de_DE.UTF-8"));
        let measurement = env_of(&[
            ("LC_ALL", ""),
            ("LC_MEASUREMENT", "en_US.UTF-8"),
            ("LANG", "C"),
        ]);
        assert_eq!(
            measurement_locale(&measurement).as_deref(),
            Some("en_US.UTF-8")
        );
        assert_eq!(
            measurement_locale(&env_of(&[("LANG", "C")])).as_deref(),
            Some("C")
        );
        assert_eq!(measurement_locale(&env_of(&[])), None);
    }
}
