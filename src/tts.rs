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

        let mut cmd = match self.engine {
            Engine::Say => {
                let mut c = Command::new("say");
                c.arg("-r").arg(wpm.to_string());
                c
            }
            Engine::Espeak => {
                let bin = if which("espeak-ng") {
                    "espeak-ng"
                } else {
                    "espeak"
                };
                let mut c = Command::new(bin);
                c.arg("-s").arg(wpm.to_string());
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

    pub fn is_idle(&mut self) -> bool {
        match self.child.as_mut() {
            None => true,
            Some(child) => matches!(child.try_wait(), Ok(Some(_))),
        }
    }

    pub fn pause(&mut self) {
        #[cfg(unix)]
        if let Some(child) = &self.child {
            unsafe { libc_kill(child.id() as i32, SIGSTOP) };
        }
    }

    pub fn resume(&mut self) {
        #[cfg(unix)]
        if let Some(child) = &self.child {
            unsafe { libc_kill(child.id() as i32, SIGCONT) };
        }
    }
}

impl Drop for Speaker {
    fn drop(&mut self) {
        self.stop();
    }
}

// Signal numbers differ by OS. Linux uses the "generic" set, macOS/BSD
// follows the historical BSD numbering. Get them wrong and `kill` hits
// a random signal.
#[cfg(target_os = "linux")]
const SIGSTOP: i32 = 19;
#[cfg(target_os = "linux")]
const SIGCONT: i32 = 18;

#[cfg(any(target_os = "macos", target_os = "freebsd", target_os = "openbsd"))]
const SIGSTOP: i32 = 17;
#[cfg(any(target_os = "macos", target_os = "freebsd", target_os = "openbsd"))]
const SIGCONT: i32 = 19;

#[cfg(unix)]
unsafe extern "C" {
    #[link_name = "kill"]
    fn libc_kill(pid: i32, sig: i32) -> i32;
}
