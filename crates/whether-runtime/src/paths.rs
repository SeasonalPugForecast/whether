//! XDG directories for the cache, state and logs, from an injected environment.

use std::path::{Path, PathBuf};

/// An absolute `var`, else `$HOME/<fallback>`.
fn xdg(env: &dyn Fn(&str) -> Option<String>, var: &str, fallback: &str) -> Option<PathBuf> {
    env(var)
        .filter(|v| Path::new(v).is_absolute())
        .map(PathBuf::from)
        .or_else(|| {
            env("HOME")
                .filter(|h| !h.is_empty())
                .map(|h| Path::new(&h).join(fallback))
        })
}

/// `$XDG_CACHE_HOME/whether/v1` (default `~/.cache/whether/v1`).
pub fn cache_dir(env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    xdg(env, "XDG_CACHE_HOME", ".cache").map(|d| d.join("whether/v1"))
}

/// `$XDG_STATE_HOME/whether` (default `~/.local/state/whether`): the call ledger and
/// the log, which must survive clearing the cache.
pub fn state_dir(env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    xdg(env, "XDG_STATE_HOME", ".local/state").map(|d| d.join("whether"))
}

/// `$XDG_RUNTIME_DIR/whether/whether.sock`, the daemon socket. There is no
/// fallback: a shared directory such as /tmp would let other users connect.
pub fn socket_path(env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    env("XDG_RUNTIME_DIR")
        .filter(|v| Path::new(v).is_absolute())
        .map(|d| Path::new(&d).join("whether/whether.sock"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |k| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| (*v).to_string())
        }
    }

    #[test]
    fn defaults_and_overrides() {
        let home = env_of(&[("HOME", "/home/u")]);
        assert_eq!(
            cache_dir(&home),
            Some(PathBuf::from("/home/u/.cache/whether/v1"))
        );
        assert_eq!(
            state_dir(&home),
            Some(PathBuf::from("/home/u/.local/state/whether"))
        );
        let xdg = env_of(&[
            ("HOME", "/home/u"),
            ("XDG_CACHE_HOME", "/c"),
            ("XDG_STATE_HOME", "rel"),
        ]);
        assert_eq!(cache_dir(&xdg), Some(PathBuf::from("/c/whether/v1")));
        assert_eq!(
            state_dir(&xdg),
            Some(PathBuf::from("/home/u/.local/state/whether")),
            "relative is ignored"
        );
        assert_eq!(cache_dir(&env_of(&[])), None);
    }

    #[test]
    fn the_socket_needs_a_runtime_dir() {
        let env = env_of(&[("XDG_RUNTIME_DIR", "/run/user/1000"), ("HOME", "/home/u")]);
        assert_eq!(
            socket_path(&env),
            Some(PathBuf::from("/run/user/1000/whether/whether.sock"))
        );
        assert_eq!(socket_path(&env_of(&[("HOME", "/home/u")])), None);
        assert_eq!(socket_path(&env_of(&[("XDG_RUNTIME_DIR", "run")])), None);
    }
}
