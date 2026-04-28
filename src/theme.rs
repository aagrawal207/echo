use crossterm::style::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeName {
    Auto,
    Dark,
    Light,
    Solarized,
    Dracula,
}

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub bg: Option<Color>,
    pub word: Color,
    pub anchor: Color,
    pub dim: Color,
    pub header_fg: Color,
    pub header_bg: Color,
    pub progress_filled: Color,
    pub progress_empty: Color,
    pub picker_highlight_fg: Color,
    pub picker_highlight_bg: Color,
}

fn dark() -> Theme {
    Theme {
        bg: None,
        word: Color::White,
        anchor: Color::Red,
        dim: Color::DarkGrey,
        header_fg: Color::Black,
        header_bg: Color::White,
        progress_filled: Color::White,
        progress_empty: Color::DarkGrey,
        picker_highlight_fg: Color::Black,
        picker_highlight_bg: Color::Yellow,
    }
}

fn light() -> Theme {
    Theme {
        bg: Some(Color::Rgb {
            r: 250,
            g: 250,
            b: 250,
        }),
        word: Color::Rgb {
            r: 30,
            g: 30,
            b: 30,
        },
        anchor: Color::Rgb { r: 180, g: 0, b: 0 },
        dim: Color::Rgb {
            r: 140,
            g: 140,
            b: 140,
        },
        header_fg: Color::White,
        header_bg: Color::Rgb {
            r: 40,
            g: 40,
            b: 40,
        },
        progress_filled: Color::Black,
        progress_empty: Color::Rgb {
            r: 200,
            g: 200,
            b: 200,
        },
        picker_highlight_fg: Color::White,
        picker_highlight_bg: Color::Rgb {
            r: 0,
            g: 90,
            b: 180,
        },
    }
}

fn solarized() -> Theme {
    Theme {
        bg: Some(Color::Rgb { r: 0, g: 43, b: 54 }), // base03
        word: Color::Rgb {
            r: 131,
            g: 148,
            b: 150,
        }, // base0
        anchor: Color::Rgb {
            r: 203,
            g: 75,
            b: 22,
        }, // orange
        dim: Color::Rgb {
            r: 88,
            g: 110,
            b: 117,
        }, // base01
        header_fg: Color::Rgb { r: 0, g: 43, b: 54 }, // base03
        header_bg: Color::Rgb {
            r: 147,
            g: 161,
            b: 161,
        }, // base1
        progress_filled: Color::Rgb {
            r: 38,
            g: 139,
            b: 210,
        }, // blue
        progress_empty: Color::Rgb { r: 7, g: 54, b: 66 }, // base02
        picker_highlight_fg: Color::Rgb {
            r: 253,
            g: 246,
            b: 227,
        }, // base3
        picker_highlight_bg: Color::Rgb {
            r: 38,
            g: 139,
            b: 210,
        }, // blue
    }
}

fn dracula() -> Theme {
    Theme {
        bg: Some(Color::Rgb {
            r: 40,
            g: 42,
            b: 54,
        }), // background
        word: Color::Rgb {
            r: 248,
            g: 248,
            b: 242,
        }, // foreground
        anchor: Color::Rgb {
            r: 255,
            g: 121,
            b: 198,
        }, // pink
        dim: Color::Rgb {
            r: 98,
            g: 114,
            b: 164,
        }, // comment
        header_fg: Color::Rgb {
            r: 248,
            g: 248,
            b: 242,
        }, // foreground
        header_bg: Color::Rgb {
            r: 68,
            g: 71,
            b: 90,
        }, // current line
        progress_filled: Color::Rgb {
            r: 189,
            g: 147,
            b: 249,
        }, // purple
        progress_empty: Color::Rgb {
            r: 68,
            g: 71,
            b: 90,
        }, // current line
        picker_highlight_fg: Color::Rgb {
            r: 40,
            g: 42,
            b: 54,
        }, // background
        picker_highlight_bg: Color::Rgb {
            r: 80,
            g: 250,
            b: 123,
        }, // green
    }
}

fn detect_background() -> ThemeName {
    let bg_val = std::env::var("COLORFGBG")
        .ok()
        .and_then(|v| v.rsplit(';').next().map(str::to_owned))
        .and_then(|s| s.parse::<u32>().ok());
    match bg_val {
        Some(bg) if bg > 6 => ThemeName::Light,
        _ => ThemeName::Dark,
    }
}

impl ThemeName {
    pub fn resolve(self) -> Theme {
        match self {
            ThemeName::Auto => detect_background().resolve(),
            ThemeName::Dark => dark(),
            ThemeName::Light => light(),
            ThemeName::Solarized => solarized(),
            ThemeName::Dracula => dracula(),
        }
    }
}

pub fn parse_theme(s: &str) -> Result<ThemeName, String> {
    match s.to_ascii_lowercase().as_str() {
        "auto" => Ok(ThemeName::Auto),
        "dark" => Ok(ThemeName::Dark),
        "light" => Ok(ThemeName::Light),
        "solarized" | "solar" => Ok(ThemeName::Solarized),
        "dracula" => Ok(ThemeName::Dracula),
        other => Err(format!(
            "unknown theme `{other}` (expected auto, dark, light, solarized, or dracula)"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_valid_names() {
        assert_eq!(parse_theme("dark").unwrap(), ThemeName::Dark);
        assert_eq!(parse_theme("LIGHT").unwrap(), ThemeName::Light);
        assert_eq!(parse_theme("Auto").unwrap(), ThemeName::Auto);
        assert_eq!(parse_theme("solarized").unwrap(), ThemeName::Solarized);
        assert_eq!(parse_theme("solar").unwrap(), ThemeName::Solarized);
        assert_eq!(parse_theme("dracula").unwrap(), ThemeName::Dracula);
    }

    #[test]
    fn parse_rejects_unknown() {
        assert!(parse_theme("neon").is_err());
    }

    #[test]
    fn all_themes_resolve() {
        for name in [
            ThemeName::Dark,
            ThemeName::Light,
            ThemeName::Solarized,
            ThemeName::Dracula,
        ] {
            let t = name.resolve();
            // Smoke test: anchor color should differ from dim color.
            assert_ne!(format!("{:?}", t.anchor), format!("{:?}", t.dim));
        }
    }
}
