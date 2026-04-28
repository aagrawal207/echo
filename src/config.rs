use std::fs;
use std::path::PathBuf;

use serde::Deserialize;

use crate::rsvp::{Focal, PauseLevel};
use crate::theme::{self, ThemeName};

#[derive(Debug, Clone)]
pub struct Config {
    pub wpm: u32,
    pub start_paused: bool,
    pub narrate: bool,
    pub pauses: PauseLevel,
    pub focal: Focal,
    pub theme: ThemeName,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            wpm: 300,
            start_paused: true,
            narrate: false,
            pauses: PauseLevel::Medium,
            focal: Focal::Middle,
            theme: ThemeName::Auto,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct RawConfig {
    wpm: Option<u32>,
    start_paused: Option<bool>,
    narrate: Option<bool>,
    pauses: Option<String>,
    focal: Option<String>,
    theme: Option<String>,
}

pub fn load() -> Config {
    match load_result() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("warning: ignoring config: {e}");
            Config::default()
        }
    }
}

fn load_result() -> Result<Config, String> {
    let Some(path) = config_path() else {
        return Ok(Config::default());
    };
    if !path.exists() {
        return Ok(Config::default());
    }
    let raw = fs::read_to_string(&path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    let parsed: RawConfig =
        toml::from_str(&raw).map_err(|e| format!("parsing {}: {e}", path.display()))?;
    merge(Config::default(), parsed).map_err(|e| format!("in {}: {e}", path.display()))
}

fn merge(base: Config, raw: RawConfig) -> Result<Config, String> {
    let pauses = match raw.pauses.as_deref() {
        None => base.pauses,
        Some(s) => parse_pause_level(s)?,
    };
    let focal = match raw.focal.as_deref() {
        None => base.focal,
        Some(s) => parse_focal(s)?,
    };
    let theme_name = match raw.theme.as_deref() {
        None => base.theme,
        Some(s) => theme::parse_theme(s)?,
    };
    Ok(Config {
        wpm: raw.wpm.unwrap_or(base.wpm),
        start_paused: raw.start_paused.unwrap_or(base.start_paused),
        narrate: raw.narrate.unwrap_or(base.narrate),
        pauses,
        focal,
        theme: theme_name,
    })
}

pub fn parse_focal(s: &str) -> Result<Focal, String> {
    match s.to_ascii_lowercase().as_str() {
        "left" | "l" => Ok(Focal::Left),
        "middle" | "center" | "m" => Ok(Focal::Middle),
        "right" | "r" => Ok(Focal::Right),
        other => Err(format!(
            "unknown focal value `{other}` (expected left, middle, or right)"
        )),
    }
}

pub fn parse_pause_level(s: &str) -> Result<PauseLevel, String> {
    match s.to_ascii_lowercase().as_str() {
        "off" | "none" | "0" => Ok(PauseLevel::Off),
        "low" | "small" => Ok(PauseLevel::Low),
        "medium" | "med" | "normal" => Ok(PauseLevel::Medium),
        "high" | "big" | "long" => Ok(PauseLevel::High),
        other => Err(format!(
            "unknown pauses value `{other}` (expected off, low, medium, or high)"
        )),
    }
}

fn config_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("ECHO_CONFIG") {
        return Some(PathBuf::from(p));
    }
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(xdg).join("echo/config.toml"));
    }
    if let Ok(home) = std::env::var("HOME") {
        return Some(PathBuf::from(home).join(".config/echo/config.toml"));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_when_empty() {
        let cfg = merge(Config::default(), RawConfig::default()).unwrap();
        assert_eq!(cfg.wpm, 300);
        assert!(cfg.start_paused);
        assert_eq!(cfg.pauses, PauseLevel::Medium);
    }

    #[test]
    fn partial_override() {
        let raw: RawConfig = toml::from_str("wpm = 420").unwrap();
        let cfg = merge(Config::default(), raw).unwrap();
        assert_eq!(cfg.wpm, 420);
        assert!(cfg.start_paused);
        assert_eq!(cfg.pauses, PauseLevel::Medium);
    }

    #[test]
    fn full_override() {
        let raw: RawConfig =
            toml::from_str("wpm = 500\nstart_paused = false\npauses = \"high\"").unwrap();
        let cfg = merge(Config::default(), raw).unwrap();
        assert_eq!(cfg.wpm, 500);
        assert!(!cfg.start_paused);
        assert_eq!(cfg.pauses, PauseLevel::High);
    }

    #[test]
    fn pauses_parses_case_insensitively() {
        assert_eq!(parse_pause_level("OFF").unwrap(), PauseLevel::Off);
        assert_eq!(parse_pause_level("Low").unwrap(), PauseLevel::Low);
        assert_eq!(parse_pause_level("medium").unwrap(), PauseLevel::Medium);
        assert_eq!(parse_pause_level("HIGH").unwrap(), PauseLevel::High);
    }

    #[test]
    fn bad_pauses_value_rejected() {
        let raw: RawConfig = toml::from_str("pauses = \"extreme\"").unwrap();
        let err = merge(Config::default(), raw).unwrap_err();
        assert!(err.contains("extreme"));
    }

    #[test]
    fn unknown_keys_rejected() {
        let raw: RawConfig = toml::from_str("wpm = 250\nfuture_option = true").unwrap();
        assert_eq!(raw.wpm, Some(250));
    }
}
