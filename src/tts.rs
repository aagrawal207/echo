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

fn espeak_bin() -> &'static str {
    if which("espeak-ng") {
        "espeak-ng"
    } else {
        "espeak"
    }
}

pub struct Speaker {
    engine: Engine,
    wpm: u32,
    // The currently speaking process (stdin already closed).
    active: Option<Child>,
    // A pre-spawned process waiting for text on stdin. Closing stdin
    // triggers it to speak immediately, cutting ~200ms of startup.
    warm: Option<WarmProcess>,
}

struct WarmProcess {
    child: Child,
    stdin: std::process::ChildStdin,
}

impl Speaker {
    pub fn with_wpm(engine: Engine, wpm: u32) -> Self {
        let mut s = Self {
            engine,
            wpm: 0,
            active: None,
            warm: None,
        };
        s.preheat(wpm);
        s
    }

    pub fn start(&mut self, words: &[&str], wpm: u32) -> io::Result<()> {
        self.stop();
        if words.is_empty() {
            return Ok(());
        }

        // If the warm process matches the current wpm, use it.
        // Otherwise spawn a fresh one.
        let text = words.join(" ");
        if self.wpm == wpm
            && let Some(mut wp) = self.warm.take()
        {
            let _ = wp.stdin.write_all(text.as_bytes());
            drop(wp.stdin);
            self.active = Some(wp.child);
            self.preheat(wpm);
            return Ok(());
        }

        // Cold start — kill the stale warm process, spawn directly.
        self.kill_warm();
        let child = spawn_speaking(self.engine, wpm, &text)?;
        self.active = Some(child);
        self.wpm = wpm;
        self.preheat(wpm);
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(mut child) = self.active.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    pub fn set_wpm(&mut self, wpm: u32) {
        if self.wpm != wpm {
            self.preheat(wpm);
        }
    }

    fn preheat(&mut self, wpm: u32) {
        self.kill_warm();
        self.wpm = wpm;
        if let Ok((child, stdin)) = spawn_warm(self.engine, wpm) {
            self.warm = Some(WarmProcess { child, stdin });
        }
    }

    fn kill_warm(&mut self) {
        if let Some(wp) = self.warm.take() {
            drop(wp.stdin);
            let mut child = wp.child;
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for Speaker {
    fn drop(&mut self) {
        self.stop();
        self.kill_warm();
    }
}

fn build_cmd(engine: Engine, wpm: u32) -> Command {
    match engine {
        Engine::Say => {
            let mut c = Command::new("say");
            c.arg("-r").arg(wpm.to_string());
            c
        }
        Engine::Espeak => {
            let mut c = Command::new(espeak_bin());
            c.arg("-s").arg(wpm.to_string());
            c
        }
    }
}

fn spawn_warm(engine: Engine, wpm: u32) -> io::Result<(Child, std::process::ChildStdin)> {
    let mut cmd = build_cmd(engine, wpm);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = cmd.spawn()?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| io::Error::other("failed to capture stdin"))?;
    Ok((child, stdin))
}

fn spawn_speaking(engine: Engine, wpm: u32, text: &str) -> io::Result<Child> {
    let (child, mut stdin) = spawn_warm(engine, wpm)?;
    let _ = stdin.write_all(text.as_bytes());
    drop(stdin);
    Ok(child)
}
