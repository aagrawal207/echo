use std::io::{self, Write};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::{cursor, execute, queue, style, terminal};

use crate::orp;
use crate::tts::{Engine, Speaker};

const WPM_STEP: u32 = 25;
const WPM_MIN: u32 = 60;
const WPM_MAX: u32 = 2000;

pub fn play(text: &str, wpm: u32, start_paused: bool, engine: Option<Engine>) -> io::Result<()> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return Ok(());
    }

    let mut stdout = io::stdout();

    terminal::enable_raw_mode()?;
    execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide)?;
    let result = run(&mut stdout, &words, wpm, start_paused, engine);
    execute!(stdout, cursor::Show, terminal::LeaveAlternateScreen)?;
    terminal::disable_raw_mode()?;
    result
}

fn run<W: Write>(
    stdout: &mut W,
    words: &[&str],
    start_wpm: u32,
    start_paused: bool,
    engine: Option<Engine>,
) -> io::Result<()> {
    let mut paused = start_paused;
    let mut idx: usize = 0;
    let mut wpm = start_wpm.clamp(WPM_MIN, WPM_MAX);
    let mut speaker = engine.map(Speaker::new);

    draw(stdout, words, idx, paused, wpm, speaker.is_some())?;

    if !paused {
        start_narration(speaker.as_mut(), words, idx, wpm);
    }

    while idx < words.len() {
        match wait_for_tick(paused, per_word(wpm))? {
            Tick::Quit => {
                if let Some(s) = speaker.as_mut() {
                    s.stop();
                }
                return Ok(());
            }
            Tick::TogglePause => {
                paused = !paused;
                if let Some(s) = speaker.as_mut() {
                    if paused {
                        s.pause();
                    } else if s.is_idle() {
                        s.start(&words[idx..], wpm)?;
                    } else {
                        s.resume();
                    }
                }
                draw(stdout, words, idx, paused, wpm, speaker.is_some())?;
            }
            Tick::Skip(delta) => {
                idx = clamp_skip(idx, words.len(), delta);
                if !paused {
                    start_narration(speaker.as_mut(), words, idx, wpm);
                } else if let Some(s) = speaker.as_mut() {
                    s.stop();
                }
                draw(stdout, words, idx, paused, wpm, speaker.is_some())?;
            }
            Tick::AdjustWpm(delta) => {
                wpm = adjust_wpm(wpm, delta);
                if !paused {
                    start_narration(speaker.as_mut(), words, idx, wpm);
                }
                draw(stdout, words, idx, paused, wpm, speaker.is_some())?;
            }
            Tick::Advance => {
                idx += 1;
                if idx < words.len() {
                    draw(stdout, words, idx, paused, wpm, speaker.is_some())?;
                }
            }
            Tick::Resize => draw(stdout, words, idx, paused, wpm, speaker.is_some())?,
        }
    }
    if let Some(s) = speaker.as_mut() {
        s.stop();
    }
    Ok(())
}

fn start_narration(speaker: Option<&mut Speaker>, words: &[&str], idx: usize, wpm: u32) {
    if let Some(s) = speaker {
        let _ = s.start(&words[idx..], wpm);
    }
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
    Resize,
}

fn wait_for_tick(paused: bool, per_word: Duration) -> io::Result<Tick> {
    if paused {
        loop {
            if let Some(tick) = read_event(Duration::from_secs(3600))? {
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
            if let Some(tick) = read_event(deadline - now)? {
                return Ok(tick);
            }
        }
    }
}

fn read_event(timeout: Duration) -> io::Result<Option<Tick>> {
    if !event::poll(timeout)? {
        return Ok(None);
    }
    match event::read()? {
        Event::Resize(_, _) => Ok(Some(Tick::Resize)),
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

fn draw<W: Write>(
    stdout: &mut W,
    words: &[&str],
    idx: usize,
    paused: bool,
    wpm: u32,
    narrating: bool,
) -> io::Result<()> {
    let (cols, rows) = terminal::size()?;
    let cols = cols as usize;
    let rows = rows as usize;
    let word_row: u16 = (rows / 2) as u16;
    let anchor_col: usize = cols / 2;

    queue!(stdout, terminal::Clear(terminal::ClearType::All))?;

    let state = if paused { "PAUSED" } else { "PLAYING" };
    let voice = if narrating { " · TTS" } else { "" };
    let header = format!(
        " echo · {wpm} wpm · {state} · word {cur}/{total}{voice} ",
        cur = idx + 1,
        total = words.len()
    );
    queue!(
        stdout,
        cursor::MoveTo(0, 0),
        style::SetAttribute(style::Attribute::Reverse),
        style::Print(pad_right(&header, cols)),
        style::SetAttribute(style::Attribute::Reset),
    )?;

    let guide_row = word_row.saturating_sub(1);
    queue!(
        stdout,
        cursor::MoveTo(anchor_col as u16, guide_row),
        style::SetForegroundColor(style::Color::DarkGrey),
        style::Print("▼"),
        style::SetForegroundColor(style::Color::Reset),
    )?;

    let word = words[idx];
    let chars: Vec<char> = word.chars().collect();
    let a = orp::anchor_index(chars.len()).min(chars.len().saturating_sub(1));
    let left: String = chars[..a].iter().collect();
    let anchor_ch: String = chars.get(a).map(|c| c.to_string()).unwrap_or_default();
    let right: String = chars[a.saturating_add(1).min(chars.len())..]
        .iter()
        .collect();

    let start_col = anchor_col.saturating_sub(a);
    queue!(
        stdout,
        cursor::MoveTo(start_col as u16, word_row),
        style::Print(&left),
        style::SetForegroundColor(style::Color::Red),
        style::SetAttribute(style::Attribute::Bold),
        style::Print(&anchor_ch),
        style::SetAttribute(style::Attribute::Reset),
        style::SetForegroundColor(style::Color::Reset),
        style::Print(&right),
    )?;

    let bar_row = (rows.saturating_sub(3)) as u16;
    queue!(
        stdout,
        cursor::MoveTo(0, bar_row),
        style::Print(progress_bar(idx + 1, words.len(), cols)),
    )?;

    let footer = " space play/pause   ← → step word   ↑ ↓ wpm   q quit ";
    queue!(
        stdout,
        cursor::MoveTo(0, (rows.saturating_sub(1)) as u16),
        style::SetAttribute(style::Attribute::Dim),
        style::Print(pad_right(footer, cols)),
        style::SetAttribute(style::Attribute::Reset),
    )?;

    stdout.flush()
}

fn pad_right(s: &str, width: usize) -> String {
    let visible = s.chars().count();
    if visible >= width {
        s.chars().take(width).collect()
    } else {
        let mut out = String::from(s);
        out.extend(std::iter::repeat_n(' ', width - visible));
        out
    }
}

fn progress_bar(cur: usize, total: usize, width: usize) -> String {
    if width == 0 || total == 0 {
        return String::new();
    }
    let filled = (cur * width) / total;
    let mut bar = String::with_capacity(width);
    for i in 0..width {
        bar.push(if i < filled { '█' } else { '░' });
    }
    bar
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_bar_full_and_empty() {
        assert_eq!(progress_bar(0, 10, 5), "░░░░░");
        assert_eq!(progress_bar(10, 10, 5), "█████");
    }

    #[test]
    fn progress_bar_handles_zero_width() {
        assert_eq!(progress_bar(5, 10, 0), "");
    }

    #[test]
    fn clamp_skip_respects_bounds() {
        assert_eq!(clamp_skip(5, 10, -3), 2);
        assert_eq!(clamp_skip(5, 10, -100), 0);
        assert_eq!(clamp_skip(5, 10, 100), 9);
        assert_eq!(clamp_skip(0, 1, -1), 0);
    }

    #[test]
    fn adjust_wpm_respects_bounds() {
        assert_eq!(adjust_wpm(300, 25), 325);
        assert_eq!(adjust_wpm(WPM_MIN, -100), WPM_MIN);
        assert_eq!(adjust_wpm(WPM_MAX, 100), WPM_MAX);
    }
}
