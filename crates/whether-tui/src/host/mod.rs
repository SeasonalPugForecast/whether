//! The terminal host: the `tui` display plugin (crossterm through ratatui).

mod app;
mod input;

use std::io::IsTerminal;

use whether_config::{Issue, LinuxConfig, Severity};
use whether_display::{DisplayContext, DisplayError, DisplayFactory, DisplayInfo, DisplayPlugin};

use crate::config::{ColorMode, TuiConfig};
use crate::ui_state::ColorDepth;

const DESCRIPTION: &str = "interactive dashboard for terminals";

/// How many colors to use: none for `color = "never"`, or for `auto` when `NO_COLOR`
/// is set (to anything) or `TERM=dumb`; otherwise truecolor when `COLORTERM` says
/// so and the 16 ANSI colors when it does not.
pub fn color_depth(mode: ColorMode, env: &dyn Fn(&str) -> Option<String>) -> ColorDepth {
    let off = match mode {
        ColorMode::Never => true,
        ColorMode::Always => false,
        ColorMode::Auto => env("NO_COLOR").is_some() || env("TERM").as_deref() == Some("dumb"),
    };
    if off {
        return ColorDepth::Plain;
    }
    match env("COLORTERM").as_deref() {
        Some("truecolor" | "24bit") => ColorDepth::TrueColor,
        _ => ColorDepth::Ansi16,
    }
}

fn parse(section: Option<&toml::Table>) -> Result<TuiConfig, String> {
    match section {
        None => Ok(TuiConfig::default()),
        Some(t) => toml::Value::Table(t.clone())
            .try_into()
            .map_err(|e: toml::de::Error| e.message().trim().to_string()),
    }
}

fn validate(section: &toml::Table) -> Vec<Issue> {
    match parse(Some(section)) {
        Ok(cfg) => cfg.validate(),
        Err(message) => vec![Issue {
            severity: Severity::Error,
            path: "display.tui".into(),
            message,
        }],
    }
}

fn create(
    section: Option<&toml::Table>,
    _: &LinuxConfig,
) -> Result<Box<dyn DisplayPlugin>, DisplayError> {
    let cfg = parse(section)?;
    if let Some(issue) = cfg
        .validate()
        .into_iter()
        .find(|i| i.severity == Severity::Error)
    {
        return Err(issue.to_string().into());
    }
    Ok(Box::new(TuiPlugin { cfg }))
}

/// The registry entry for `tui`.
pub fn factory() -> DisplayFactory {
    DisplayFactory {
        name: "tui",
        description: DESCRIPTION,
        interactive: true,
        create,
        validate,
        schema: || schemars::schema_for!(TuiConfig),
    }
}

/// The `tui` plugin.
#[derive(Debug)]
pub struct TuiPlugin {
    cfg: TuiConfig,
}

impl DisplayPlugin for TuiPlugin {
    fn info(&self) -> DisplayInfo {
        DisplayInfo {
            name: "tui",
            description: DESCRIPTION,
            interactive: true,
            needs_tty: true,
        }
    }

    fn run(self: Box<Self>, ctx: DisplayContext) -> Result<(), DisplayError> {
        if !std::io::stdout().is_terminal() || !std::io::stdin().is_terminal() {
            return Err(
                "the tui needs a terminal; use `whether line` for scripts and status bars".into(),
            );
        }
        let colors = color_depth(self.cfg.color, &|k| std::env::var(k).ok());
        app::run(&self.cfg, &ctx, colors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRUECOLOR: &[(&str, &str)] = &[("TERM", "xterm-256color"), ("COLORTERM", "truecolor")];

    #[test]
    fn color_rules() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |k: &str| {
                pairs
                    .iter()
                    .find(|(n, _)| *n == k)
                    .map(|(_, v)| (*v).to_string())
            }
        };
        let depth = |mode, pairs| color_depth(mode, &env(pairs));
        assert_eq!(depth(ColorMode::Auto, TRUECOLOR), ColorDepth::TrueColor);
        assert_eq!(
            depth(ColorMode::Auto, &[("TERM", "xterm-256color")]),
            ColorDepth::Ansi16,
            "no COLORTERM"
        );
        assert_eq!(
            depth(ColorMode::Auto, &[("NO_COLOR", "")]),
            ColorDepth::Plain,
            "any value disables color"
        );
        assert_eq!(
            depth(ColorMode::Auto, &[("TERM", "dumb")]),
            ColorDepth::Plain
        );
        assert_eq!(
            depth(
                ColorMode::Always,
                &[("NO_COLOR", "1"), ("COLORTERM", "24bit")]
            ),
            ColorDepth::TrueColor
        );
        assert_eq!(
            depth(ColorMode::Never, &[("COLORTERM", "truecolor")]),
            ColorDepth::Plain
        );
    }

    #[test]
    fn factory_validates_tables() {
        let bad: toml::Table = "layout = \"custom\"".parse().unwrap();
        assert_eq!((factory().validate)(&bad)[0].path, "display.tui.rows");
        let unknown: toml::Table = "colour = 1".parse().unwrap();
        assert_eq!((factory().validate)(&unknown)[0].path, "display.tui");
        assert!(factory().interactive);
        let schema = (factory().schema)();
        assert!(
            schema
                .get("properties")
                .is_some_and(|p| p.get("hourly_columns").is_some())
        );
    }
}
