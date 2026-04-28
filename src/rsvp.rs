use std::io::{self, Write};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::{cursor, execute, queue, style, terminal};

use crate::orp;
use crate::tts::{Engine, Speaker};

const WPM_STEP: u32 = 25;
const WPM_MIN: u32 = 60;
const WPM_MAX: u32 = 2000;
const JUMP_WORDS: i32 = 10;
const MIN_COLS: u16 = 24;
const MIN_ROWS: u16 = 6;

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

struct State {
    idx: usize,
    paused: bool,
    wpm: u32,
    show_help: bool,
    finished: bool,
}

fn run<W: Write>(
    stdout: &mut W,
    words: &[&str],
    start_wpm: u32,
    start_paused: bool,
    engine: Option<Engine>,
) -> io::Result<()> {
    let mut st = State {
        idx: 0,
        paused: start_paused,
        wpm: start_wpm.clamp(WPM_MIN, WPM_MAX),
        show_help: false,
        finished: false,
    };
    let mut speaker = engine.map(Speaker::new);

    draw(stdout, words, &st, speaker.is_some())?;
    if !st.paused {
        start_narration(speaker.as_mut(), words, st.idx, st.wpm);
    }

    loop {
        let blocking = st.finished || st.paused || st.show_help;
        let tick_budget = if blocking {
            Duration::from_secs(3600)
        } else {
            frame_budget(st.wpm, words[st.idx])
        };

        match wait_for_tick(tick_budget, blocking)? {
            Tick::Quit => break,
            Tick::Resize => draw(stdout, words, &st, speaker.is_some())?,
            Tick::ToggleHelp => {
                st.show_help = !st.show_help;
                if let Some(s) = speaker.as_mut() {
                    if st.show_help && !st.paused {
                        s.pause();
                    } else if !st.show_help && !st.paused {
                        s.resume();
                    }
                }
                draw(stdout, words, &st, speaker.is_some())?;
            }
            Tick::TogglePause => {
                if st.finished {
                    st.idx = 0;
                    st.finished = false;
                    st.paused = false;
                    if !st.paused {
                        start_narration(speaker.as_mut(), words, st.idx, st.wpm);
                    }
                } else {
                    st.paused = !st.paused;
                    if let Some(s) = speaker.as_mut() {
                        if st.paused {
                            s.pause();
                        } else if s.is_idle() {
                            let _ = s.start(&words[st.idx..], st.wpm);
                        } else {
                            s.resume();
                        }
                    }
                }
                draw(stdout, words, &st, speaker.is_some())?;
            }
            Tick::Skip(delta) => {
                st.idx = clamp_skip(st.idx, words.len(), delta);
                st.finished = false;
                if !st.paused {
                    start_narration(speaker.as_mut(), words, st.idx, st.wpm);
                } else if let Some(s) = speaker.as_mut() {
                    s.stop();
                }
                draw(stdout, words, &st, speaker.is_some())?;
            }
            Tick::JumpStart => {
                st.idx = 0;
                st.finished = false;
                if !st.paused {
                    start_narration(speaker.as_mut(), words, st.idx, st.wpm);
                }
                draw(stdout, words, &st, speaker.is_some())?;
            }
            Tick::AdjustWpm(delta) => {
                st.wpm = adjust_wpm(st.wpm, delta);
                if !st.paused && !st.finished {
                    start_narration(speaker.as_mut(), words, st.idx, st.wpm);
                }
                draw(stdout, words, &st, speaker.is_some())?;
            }
            Tick::Advance => {
                if st.idx + 1 >= words.len() {
                    st.finished = true;
                    if let Some(s) = speaker.as_mut() {
                        s.stop();
                    }
                } else {
                    st.idx += 1;
                }
                draw(stdout, words, &st, speaker.is_some())?;
            }
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

// Heavier punctuation slows the next frame so the brain gets a beat to
// close the sentence. Multipliers scale the per-word base.
fn pause_multiplier(word: &str) -> u32 {
    let trailing = word.trim_end_matches(|c: char| !c.is_alphanumeric());
    let tail = &word[trailing.len()..];
    if tail.contains('.') || tail.contains('!') || tail.contains('?') {
        3
    } else if tail.contains(',') || tail.contains(';') || tail.contains(':') {
        2
    } else {
        1
    }
}

fn frame_budget(wpm: u32, word: &str) -> Duration {
    per_word(wpm) * pause_multiplier(word)
}

enum Tick {
    Advance,
    TogglePause,
    Skip(i32),
    JumpStart,
    AdjustWpm(i32),
    ToggleHelp,
    Quit,
    Resize,
}

fn wait_for_tick(timeout: Duration, blocking: bool) -> io::Result<Tick> {
    if blocking {
        loop {
            if let Some(tick) = read_event(timeout)? {
                return Ok(tick);
            }
        }
    } else {
        let deadline = Instant::now() + timeout;
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
        Event::Key(k) if k.kind != KeyEventKind::Release => {
            let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
            match k.code {
                KeyCode::Char(' ') => Ok(Some(Tick::TogglePause)),
                KeyCode::Char('q') | KeyCode::Esc => Ok(Some(Tick::Quit)),
                KeyCode::Char('c') if ctrl => Ok(Some(Tick::Quit)),
                KeyCode::Char('?') | KeyCode::Char('h') => Ok(Some(Tick::ToggleHelp)),
                KeyCode::Char('r') => Ok(Some(Tick::JumpStart)),
                KeyCode::Home => Ok(Some(Tick::JumpStart)),
                KeyCode::Left => Ok(Some(Tick::Skip(-1))),
                KeyCode::Right => Ok(Some(Tick::Skip(1))),
                KeyCode::Char('b') => Ok(Some(Tick::Skip(-JUMP_WORDS))),
                KeyCode::Char('f') => Ok(Some(Tick::Skip(JUMP_WORDS))),
                KeyCode::PageUp => Ok(Some(Tick::Skip(-JUMP_WORDS * 5))),
                KeyCode::PageDown => Ok(Some(Tick::Skip(JUMP_WORDS * 5))),
                KeyCode::Up => Ok(Some(Tick::AdjustWpm(WPM_STEP as i32))),
                KeyCode::Down => Ok(Some(Tick::AdjustWpm(-(WPM_STEP as i32)))),
                _ => Ok(None),
            }
        }
        _ => Ok(None),
    }
}

fn draw<W: Write>(stdout: &mut W, words: &[&str], st: &State, narrating: bool) -> io::Result<()> {
    let (cols_u16, rows_u16) = terminal::size()?;

    queue!(stdout, terminal::Clear(terminal::ClearType::All))?;

    if cols_u16 < MIN_COLS || rows_u16 < MIN_ROWS {
        let msg = format!("terminal too small ({cols_u16}x{rows_u16}) — resize and try again");
        queue!(stdout, cursor::MoveTo(0, 0), style::Print(msg))?;
        return stdout.flush();
    }

    let cols = cols_u16 as usize;
    let rows = rows_u16 as usize;

    draw_header(stdout, words, st, narrating, cols)?;
    draw_word(stdout, words, st, cols, rows)?;
    draw_progress(stdout, words.len(), st, cols, rows)?;
    draw_footer(stdout, cols, rows)?;

    if st.show_help {
        draw_help_overlay(stdout, cols_u16, rows_u16)?;
    }
    stdout.flush()
}

fn draw_header<W: Write>(
    stdout: &mut W,
    words: &[&str],
    st: &State,
    narrating: bool,
    cols: usize,
) -> io::Result<()> {
    let state = if st.finished {
        "DONE"
    } else if st.paused {
        "PAUSED"
    } else {
        "PLAYING"
    };
    let voice = if narrating { " · TTS" } else { "" };
    let pct = if words.is_empty() {
        100
    } else {
        ((st.idx + 1) * 100) / words.len()
    };
    let header = format!(
        " echo · {wpm} wpm · {state} · {cur}/{total} ({pct}%){voice} · ? help ",
        wpm = st.wpm,
        cur = st.idx + 1,
        total = words.len(),
    );
    queue!(
        stdout,
        cursor::MoveTo(0, 0),
        style::SetAttribute(style::Attribute::Reverse),
        style::Print(pad_right(&header, cols)),
        style::SetAttribute(style::Attribute::Reset),
    )
}

fn draw_word<W: Write>(
    stdout: &mut W,
    words: &[&str],
    st: &State,
    cols: usize,
    rows: usize,
) -> io::Result<()> {
    let word_row: u16 = (rows / 2) as u16;
    let anchor_col: usize = cols / 2;

    let guide_row = word_row.saturating_sub(1);
    queue!(
        stdout,
        cursor::MoveTo(anchor_col as u16, guide_row),
        style::SetForegroundColor(style::Color::DarkGrey),
        style::Print("▼"),
        style::SetForegroundColor(style::Color::Reset),
    )?;

    let word = words[st.idx];
    let chars: Vec<char> = word.chars().collect();
    let a = orp::anchor_index(chars.len()).min(chars.len().saturating_sub(1));
    let left: String = chars[..a].iter().collect();
    let anchor_ch: String = chars.get(a).map(|c| c.to_string()).unwrap_or_default();
    let right: String = chars[a.saturating_add(1).min(chars.len())..]
        .iter()
        .collect();

    let start_col = anchor_col.saturating_sub(a);
    let color = if st.finished {
        style::Color::DarkGrey
    } else {
        style::Color::Red
    };
    queue!(
        stdout,
        cursor::MoveTo(start_col as u16, word_row),
        style::SetForegroundColor(if st.finished {
            style::Color::DarkGrey
        } else {
            style::Color::Reset
        }),
        style::Print(&left),
        style::SetForegroundColor(color),
        style::SetAttribute(style::Attribute::Bold),
        style::Print(&anchor_ch),
        style::SetAttribute(style::Attribute::Reset),
        style::SetForegroundColor(if st.finished {
            style::Color::DarkGrey
        } else {
            style::Color::Reset
        }),
        style::Print(&right),
        style::SetForegroundColor(style::Color::Reset),
    )?;

    if st.finished {
        let note = "— end — press space to restart, q to quit —";
        let col = cols.saturating_sub(note.chars().count()) / 2;
        queue!(
            stdout,
            cursor::MoveTo(col as u16, word_row + 2),
            style::SetAttribute(style::Attribute::Dim),
            style::Print(note),
            style::SetAttribute(style::Attribute::Reset),
        )?;
    }
    Ok(())
}

fn draw_progress<W: Write>(
    stdout: &mut W,
    total: usize,
    st: &State,
    cols: usize,
    rows: usize,
) -> io::Result<()> {
    let bar_row = (rows.saturating_sub(3)) as u16;
    queue!(
        stdout,
        cursor::MoveTo(0, bar_row),
        style::Print(progress_bar(st.idx + 1, total, cols)),
    )
}

fn draw_footer<W: Write>(stdout: &mut W, cols: usize, rows: usize) -> io::Result<()> {
    let footer = " space play/pause · ← → step · b f jump · r restart · ↑ ↓ wpm · ? help · q quit ";
    queue!(
        stdout,
        cursor::MoveTo(0, (rows.saturating_sub(1)) as u16),
        style::SetAttribute(style::Attribute::Dim),
        style::Print(pad_right(footer, cols)),
        style::SetAttribute(style::Attribute::Reset),
    )
}

fn draw_help_overlay<W: Write>(stdout: &mut W, cols: u16, rows: u16) -> io::Result<()> {
    let lines: &[&str] = &[
        "echo — keyboard reference",
        "",
        "space        play / pause (when done: restart)",
        "← / →        previous / next word",
        "b / f        jump back / forward 10 words",
        "PgUp/PgDn    jump back / forward 50 words",
        "r / Home     restart from first word",
        "↑ / ↓        increase / decrease wpm by 25",
        "? or h       toggle this help",
        "q / Esc      quit",
    ];
    let width = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) + 4;
    let height = lines.len() + 2;
    let width = (width as u16).min(cols.saturating_sub(2));
    let height = (height as u16).min(rows.saturating_sub(2));
    let x = cols.saturating_sub(width) / 2;
    let y = rows.saturating_sub(height) / 2;

    for row in 0..height {
        queue!(
            stdout,
            cursor::MoveTo(x, y + row),
            style::SetAttribute(style::Attribute::Reverse),
            style::Print(" ".repeat(width as usize)),
            style::SetAttribute(style::Attribute::Reset),
        )?;
    }
    for (i, line) in lines.iter().enumerate() {
        if (i as u16) + 1 >= height.saturating_sub(1) {
            break;
        }
        queue!(
            stdout,
            cursor::MoveTo(x + 2, y + 1 + i as u16),
            style::SetAttribute(style::Attribute::Reverse),
            style::Print(line),
            style::SetAttribute(style::Attribute::Reset),
        )?;
    }
    Ok(())
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

    #[test]
    fn pause_multiplier_table() {
        assert_eq!(pause_multiplier("plain"), 1);
        assert_eq!(pause_multiplier("end."), 3);
        assert_eq!(pause_multiplier("what?"), 3);
        assert_eq!(pause_multiplier("wait!"), 3);
        assert_eq!(pause_multiplier("list,"), 2);
        assert_eq!(pause_multiplier("clause;"), 2);
        assert_eq!(pause_multiplier("colon:"), 2);
        // Leading punctuation doesn't count as an end-of-word pause.
        assert_eq!(pause_multiplier("(aside"), 1);
    }
}
