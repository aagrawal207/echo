use std::io::{self, Write};
use std::process::{Child, Command, Stdio};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Say,
    Espeak,
}

pub fn detect() -> Option<Engine> {
    if which("say") {
        Some(Engine::Say)
    } else if which("espeak-ng") || which("espeak") {
        Some(Engine::Espeak)
    } else {
        None
    }
}

fn which(cmd: &str) -> bool {
    Command::new(cmd)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .status()
        .is_ok_and(|s| s.success() || s.code().is_some())
}

// macOS `say -r N` speaks noticeably faster than N visual-WPM because
// its rate model counts differently than our fixed per-word timer. This
// factor slows the voice down so it roughly tracks the visual.
const SAY_RATE_FACTOR: f64 = 0.82;

pub struct Speaker {
    engine: Engine,
    child: Option<Child>,
}

impl Speaker {
    pub fn new(engine: Engine) -> Self {
        Self {
            engine,
            child: None,
        }
    }

    pub fn start(&mut self, words: &[&str], wpm: u32) -> io::Result<()> {
        self.stop();
        if words.is_empty() {
            return Ok(());
        }

        let rate = match self.engine {
            Engine::Say => ((wpm as f64) * SAY_RATE_FACTOR) as u32,
            Engine::Espeak => wpm,
        };

        let mut cmd = match self.engine {
            Engine::Say => {
                let mut c = Command::new("say");
                c.arg("-r").arg(rate.to_string());
                c
            }
            Engine::Espeak => {
                let bin = if which("espeak-ng") {
                    "espeak-ng"
                } else {
                    "espeak"
                };
                let mut c = Command::new(bin);
                c.arg("-s").arg(rate.to_string());
                c
            }
        };
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        let mut child = cmd.spawn()?;
        if let Some(mut stdin) = child.stdin.take() {
            let text = words.join(" ");
            let _ = stdin.write_all(text.as_bytes());
        }
        self.child = Some(child);
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for Speaker {
    fn drop(&mut self) {
        self.stop();
    }
}
