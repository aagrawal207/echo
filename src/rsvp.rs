use std::io::{self, Write};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal;

const WPM_STEP: u32 = 25;
const WPM_MIN: u32 = 60;
const WPM_MAX: u32 = 2000;

pub fn play(text: &str, wpm: u32) -> io::Result<()> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return Ok(());
    }

    let mut stdout = io::stdout().lock();

    terminal::enable_raw_mode()?;
    let result = run(&mut stdout, &words, wpm);
    terminal::disable_raw_mode()?;
    writeln!(stdout)?;
    result
}

fn run<W: Write>(stdout: &mut W, words: &[&str], start_wpm: u32) -> io::Result<()> {
    let mut paused = true;
    let mut idx: usize = 0;
    let mut wpm = start_wpm.clamp(WPM_MIN, WPM_MAX);

    render(stdout, words[idx], paused, wpm)?;

    while idx < words.len() {
        match wait_for_tick(paused, per_word(wpm))? {
            Tick::Quit => return Ok(()),
            Tick::TogglePause => {
                paused = !paused;
                render(stdout, words[idx], paused, wpm)?;
            }
            Tick::Skip(delta) => {
                idx = clamp_skip(idx, words.len(), delta);
                render(stdout, words[idx], paused, wpm)?;
            }
            Tick::AdjustWpm(delta) => {
                wpm = adjust_wpm(wpm, delta);
                render(stdout, words[idx], paused, wpm)?;
            }
            Tick::Advance => {
                idx += 1;
                if idx < words.len() {
                    render(stdout, words[idx], paused, wpm)?;
                }
            }
        }
    }
    Ok(())
}

fn clamp_skip(idx: usize, len: usize, delta: i32) -> usize {
    let new = (idx as i64) + (delta as i64);
    new.clamp(0, (len - 1) as i64) as usize
}

fn adjust_wpm(wpm: u32, delta: i32) -> u32 {
    let next = (wpm as i32) + delta;
    (next.max(WPM_MIN as i32) as u32).min(WPM_MAX)
}

fn per_word(wpm: u32) -> Duration {
    Duration::from_millis(60_000 / u64::from(wpm))
}

enum Tick {
    Advance,
    TogglePause,
    Skip(i32),
    AdjustWpm(i32),
    Quit,
}

fn wait_for_tick(paused: bool, per_word: Duration) -> io::Result<Tick> {
    if paused {
        loop {
            if let Some(tick) = read_key(Duration::from_secs(3600))? {
                return Ok(tick);
            }
        }
    } else {
        let deadline = Instant::now() + per_word;
        loop {
            let now = Instant::now();
            if now >= deadline {
                return Ok(Tick::Advance);
            }
            if let Some(tick) = read_key(deadline - now)? {
                return Ok(tick);
            }
        }
    }
}

fn read_key(timeout: Duration) -> io::Result<Option<Tick>> {
    if !event::poll(timeout)? {
        return Ok(None);
    }
    match event::read()? {
        Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
            KeyCode::Char(' ') => Ok(Some(Tick::TogglePause)),
            KeyCode::Char('q') | KeyCode::Esc => Ok(Some(Tick::Quit)),
            KeyCode::Left => Ok(Some(Tick::Skip(-1))),
            KeyCode::Right => Ok(Some(Tick::Skip(1))),
            KeyCode::Up => Ok(Some(Tick::AdjustWpm(WPM_STEP as i32))),
            KeyCode::Down => Ok(Some(Tick::AdjustWpm(-(WPM_STEP as i32)))),
            _ => Ok(None),
        },
        _ => Ok(None),
    }
}

fn render<W: Write>(stdout: &mut W, word: &str, paused: bool, wpm: u32) -> io::Result<()> {
    let status = if paused {
        format!(" [paused · {wpm} wpm]")
    } else {
        format!(" [{wpm} wpm]")
    };
    write!(stdout, "\r\x1b[K{word}{status}")?;
    stdout.flush()
}
