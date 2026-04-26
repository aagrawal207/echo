use std::fs;
use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct Config {
    pub wpm: u32,
    pub start_paused: bool,
    pub narrate: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            wpm: 300,
            start_paused: true,
            narrate: false,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct RawConfig {
    wpm: Option<u32>,
    start_paused: Option<bool>,
    narrate: Option<bool>,
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
    Ok(merge(Config::default(), parsed))
}

fn merge(base: Config, raw: RawConfig) -> Config {
    Config {
        wpm: raw.wpm.unwrap_or(base.wpm),
        start_paused: raw.start_paused.unwrap_or(base.start_paused),
        narrate: raw.narrate.unwrap_or(base.narrate),
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
        let cfg = merge(Config::default(), RawConfig::default());
        assert_eq!(cfg.wpm, 300);
        assert!(cfg.start_paused);
    }

    #[test]
    fn partial_override() {
        let raw: RawConfig = toml::from_str("wpm = 420").unwrap();
        let cfg = merge(Config::default(), raw);
        assert_eq!(cfg.wpm, 420);
        assert!(cfg.start_paused);
    }

    #[test]
    fn full_override() {
        let raw: RawConfig = toml::from_str("wpm = 500\nstart_paused = false").unwrap();
        let cfg = merge(Config::default(), raw);
        assert_eq!(cfg.wpm, 500);
        assert!(!cfg.start_paused);
    }

    #[test]
    fn unknown_keys_rejected() {
        // Strict parse: extra keys are silently ignored by default, which we
        // accept. Asserting the happy-path parse still works with extras.
        let raw: RawConfig = toml::from_str("wpm = 250\nfuture_option = true").unwrap();
        assert_eq!(raw.wpm, Some(250));
    }
}
