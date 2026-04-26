use std::io::{self, Write};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal;

pub fn play(text: &str, wpm: u32) -> io::Result<()> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return Ok(());
    }

    let per_word = Duration::from_millis(60_000 / u64::from(wpm));
    let mut stdout = io::stdout().lock();

    terminal::enable_raw_mode()?;
    let result = run(&mut stdout, &words, per_word);
    terminal::disable_raw_mode()?;
    writeln!(stdout)?;
    result
}

fn run<W: Write>(stdout: &mut W, words: &[&str], per_word: Duration) -> io::Result<()> {
    let mut paused = true;
    let mut idx = 0;

    render(stdout, words[idx], paused)?;

    while idx < words.len() {
        match wait_for_tick(paused, per_word)? {
            Tick::Quit => return Ok(()),
            Tick::TogglePause => {
                paused = !paused;
                render(stdout, words[idx], paused)?;
            }
            Tick::Advance => {
                idx += 1;
                if idx < words.len() {
                    render(stdout, words[idx], paused)?;
                }
            }
        }
    }
    Ok(())
}

enum Tick {
    Advance,
    TogglePause,
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
            _ => Ok(None),
        },
        _ => Ok(None),
    }
}

fn render<W: Write>(stdout: &mut W, word: &str, paused: bool) -> io::Result<()> {
    let marker = if paused { " [paused]" } else { "" };
    write!(stdout, "\r\x1b[K{word}{marker}")?;
    stdout.flush()
}
