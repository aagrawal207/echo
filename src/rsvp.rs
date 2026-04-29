use std::io::{self, Write};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::{cursor, execute, queue, style, terminal};

use crate::orp;
use crate::picker::{self, Move as PickerMove, Picker};
use crate::theme::Theme;
use crate::tts::{Engine, Speaker};

const WPM_STEP: u32 = 25;
const WPM_MIN: u32 = 60;
const WPM_MAX: u32 = 2000;
const JUMP_WORDS: i32 = 10;
const MIN_COLS: u16 = 24;
const MIN_ROWS: u16 = 6;

pub fn play(
    text: &str,
    wpm: u32,
    start_paused: bool,
    engine: Option<Engine>,
    pauses: PauseLevel,
    focal: Focal,
    theme: Theme,
) -> io::Result<()> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return Ok(());
    }

    let mut stdout = io::stdout();

    terminal::enable_raw_mode()?;
    execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide)?;
    let result = run(
        &mut stdout,
        &words,
        wpm,
        start_paused,
        engine,
        pauses,
        focal,
        theme,
    );
    execute!(
        stdout,
        style::SetBackgroundColor(style::Color::Reset),
        style::SetForegroundColor(style::Color::Reset),
        cursor::Show,
        terminal::LeaveAlternateScreen,
    )?;
    terminal::disable_raw_mode()?;
    result
}

struct State {
    idx: usize,
    paused: bool,
    wpm: u32,
    focal: Focal,
    show_help: bool,
    finished: bool,
    picker: Option<Picker>,
}

#[allow(clippy::too_many_arguments)]
fn run<W: Write>(
    stdout: &mut W,
    words: &[&str],
    start_wpm: u32,
    start_paused: bool,
    engine: Option<Engine>,
    pauses: PauseLevel,
    focal: Focal,
    theme: Theme,
) -> io::Result<()> {
    let mut st = State {
        idx: 0,
        paused: start_paused,
        wpm: start_wpm.clamp(WPM_MIN, WPM_MAX),
        focal,
        show_help: false,
        finished: false,
        picker: None,
    };
    let mut speaker = engine.map(|e| Speaker::with_wpm(e, st.wpm));

    draw(stdout, words, &mut st, speaker.is_some(), &theme)?;
    if !st.paused {
        start_narration(speaker.as_mut(), words, st.idx, st.wpm);
    }

    loop {
        let blocking = st.finished || st.paused || st.show_help || st.picker.is_some();
        let tick_budget = if blocking {
            Duration::from_secs(3600)
        } else {
            frame_budget(st.wpm, words[st.idx], pauses)
        };

        let in_picker = st.picker.is_some();
        let tick = wait_for_tick(tick_budget, blocking, in_picker)?;

        match tick {
            Tick::Quit => break,
            Tick::Resize => draw(stdout, words, &mut st, speaker.is_some(), &theme)?,
            Tick::OpenPicker => {
                stop_narration(speaker.as_mut());
                st.picker = Some(Picker::new(st.idx));
                draw(stdout, words, &mut st, speaker.is_some(), &theme)?;
            }
            Tick::CancelPicker => {
                st.picker = None;
                if !st.paused {
                    start_narration(speaker.as_mut(), words, st.idx, st.wpm);
                }
                draw(stdout, words, &mut st, speaker.is_some(), &theme)?;
            }
            Tick::CommitPicker => {
                if let Some(p) = st.picker.take() {
                    st.idx = p.cursor;
                    st.finished = false;
                    st.paused = true;
                }
                draw(stdout, words, &mut st, speaker.is_some(), &theme)?;
            }
            Tick::MovePicker(m) => {
                if let Some(p) = st.picker.as_mut() {
                    p.move_cursor(m, words.len());
                }
                draw(stdout, words, &mut st, speaker.is_some(), &theme)?;
            }
            Tick::ToggleHelp => {
                st.show_help = !st.show_help;
                if st.show_help {
                    stop_narration(speaker.as_mut());
                } else if !st.paused {
                    start_narration(speaker.as_mut(), words, st.idx, st.wpm);
                }
                draw(stdout, words, &mut st, speaker.is_some(), &theme)?;
            }
            Tick::TogglePause => {
                if st.finished {
                    st.idx = 0;
                    st.finished = false;
                    st.paused = false;
                    start_narration(speaker.as_mut(), words, st.idx, st.wpm);
                } else {
                    st.paused = !st.paused;
                    if st.paused {
                        stop_narration(speaker.as_mut());
                    } else {
                        start_narration(speaker.as_mut(), words, st.idx, st.wpm);
                    }
                }
                draw(stdout, words, &mut st, speaker.is_some(), &theme)?;
            }
            Tick::Skip(delta) => {
                st.idx = clamp_skip(st.idx, words.len(), delta);
                st.finished = false;
                if !st.paused {
                    start_narration(speaker.as_mut(), words, st.idx, st.wpm);
                }
                draw(stdout, words, &mut st, speaker.is_some(), &theme)?;
            }
            Tick::JumpStart => {
                st.idx = 0;
                st.finished = false;
                if !st.paused {
                    start_narration(speaker.as_mut(), words, st.idx, st.wpm);
                }
                draw(stdout, words, &mut st, speaker.is_some(), &theme)?;
            }
            Tick::AdjustWpm(delta) => {
                st.wpm = adjust_wpm(st.wpm, delta);
                if let Some(s) = speaker.as_mut() {
                    if !st.paused && !st.finished {
                        let _ = s.start(&words[st.idx..], st.wpm);
                    } else {
                        s.set_wpm(st.wpm);
                    }
                }
                draw(stdout, words, &mut st, speaker.is_some(), &theme)?;
            }
            Tick::Advance => {
                if st.idx + 1 >= words.len() {
                    st.finished = true;
                    stop_narration(speaker.as_mut());
                } else {
                    st.idx += 1;
                }
                draw(stdout, words, &mut st, speaker.is_some(), &theme)?;
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

fn stop_narration(speaker: Option<&mut Speaker>) {
    if let Some(s) = speaker {
        s.stop();
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focal {
    Left,
    Middle,
    Right,
}

impl Focal {
    fn anchor_col(self, cols: usize) -> usize {
        match self {
            Focal::Left => cols / 4,
            Focal::Middle => cols / 2,
            Focal::Right => cols * 3 / 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseLevel {
    Off,
    Low,
    Medium,
    High,
}

impl PauseLevel {
    fn multiplier(self, word: &str) -> u32 {
        let trailing = word.trim_end_matches(|c: char| !c.is_alphanumeric());
        let tail = &word[trailing.len()..];
        let has_sentence = tail.contains('.') || tail.contains('!') || tail.contains('?');
        let has_clause = tail.contains(',') || tail.contains(';') || tail.contains(':');

        match self {
            PauseLevel::Off => 1,
            PauseLevel::Low => {
                if has_sentence {
                    2
                } else {
                    1
                }
            }
            PauseLevel::Medium => {
                if has_sentence {
                    3
                } else if has_clause {
                    2
                } else {
                    1
                }
            }
            PauseLevel::High => {
                if has_sentence {
                    5
                } else if has_clause {
                    3
                } else {
                    1
                }
            }
        }
    }
}

fn frame_budget(wpm: u32, word: &str, pauses: PauseLevel) -> Duration {
    per_word(wpm) * pauses.multiplier(word)
}

enum Tick {
    Advance,
    TogglePause,
    Skip(i32),
    JumpStart,
    AdjustWpm(i32),
    ToggleHelp,
    OpenPicker,
    CancelPicker,
    CommitPicker,
    MovePicker(PickerMove),
    Quit,
    Resize,
}

fn wait_for_tick(timeout: Duration, blocking: bool, in_picker: bool) -> io::Result<Tick> {
    if blocking {
        loop {
            if let Some(tick) = read_event(timeout, in_picker)? {
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
            if let Some(tick) = read_event(deadline - now, in_picker)? {
                return Ok(tick);
            }
        }
    }
}

fn read_event(timeout: Duration, in_picker: bool) -> io::Result<Option<Tick>> {
    if !event::poll(timeout)? {
        return Ok(None);
    }
    match event::read()? {
        Event::Resize(_, _) => Ok(Some(Tick::Resize)),
        Event::Key(k) if k.kind != KeyEventKind::Release => {
            let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
            if in_picker {
                return Ok(match k.code {
                    KeyCode::Esc => Some(Tick::CancelPicker),
                    KeyCode::Enter => Some(Tick::CommitPicker),
                    KeyCode::Char('c') if ctrl => Some(Tick::Quit),
                    KeyCode::Left => Some(Tick::MovePicker(PickerMove::Prev)),
                    KeyCode::Right => Some(Tick::MovePicker(PickerMove::Next)),
                    KeyCode::Up => Some(Tick::MovePicker(PickerMove::Up)),
                    KeyCode::Down => Some(Tick::MovePicker(PickerMove::Down)),
                    KeyCode::Char('b') => Some(Tick::MovePicker(PickerMove::JumpBack(10))),
                    KeyCode::Char('f') => Some(Tick::MovePicker(PickerMove::JumpForward(10))),
                    KeyCode::PageUp => Some(Tick::MovePicker(PickerMove::JumpBack(50))),
                    KeyCode::PageDown => Some(Tick::MovePicker(PickerMove::JumpForward(50))),
                    KeyCode::Home => Some(Tick::MovePicker(PickerMove::Start)),
                    KeyCode::End => Some(Tick::MovePicker(PickerMove::End)),
                    _ => None,
                });
            }
            match k.code {
                KeyCode::Char(' ') => Ok(Some(Tick::TogglePause)),
                KeyCode::Char('q') | KeyCode::Esc => Ok(Some(Tick::Quit)),
                KeyCode::Char('c') if ctrl => Ok(Some(Tick::Quit)),
                KeyCode::Char('?') | KeyCode::Char('h') => Ok(Some(Tick::ToggleHelp)),
                KeyCode::Char('/') => Ok(Some(Tick::OpenPicker)),
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

// Helper: reset fg/bg/attrs back to the theme baseline. Crossterm's
// Attribute::Reset nukes the background color, so we re-apply the
// theme bg immediately after.
fn reset_style<W: Write>(stdout: &mut W, theme: &Theme) -> io::Result<()> {
    queue!(
        stdout,
        style::SetAttribute(style::Attribute::Reset),
        style::SetForegroundColor(style::Color::Reset),
    )?;
    if let Some(bg) = theme.bg {
        queue!(stdout, style::SetBackgroundColor(bg))?;
    } else {
        queue!(stdout, style::SetBackgroundColor(style::Color::Reset))?;
    }
    Ok(())
}

fn draw<W: Write>(
    stdout: &mut W,
    words: &[&str],
    st: &mut State,
    narrating: bool,
    theme: &Theme,
) -> io::Result<()> {
    let (cols_u16, rows_u16) = terminal::size()?;

    // Paint the whole screen with the theme background.
    if let Some(bg) = theme.bg {
        queue!(stdout, style::SetBackgroundColor(bg))?;
    }
    queue!(stdout, terminal::Clear(terminal::ClearType::All))?;

    if cols_u16 < MIN_COLS || rows_u16 < MIN_ROWS {
        let msg = format!("terminal too small ({cols_u16}x{rows_u16}) — resize and try again");
        queue!(stdout, cursor::MoveTo(0, 0), style::Print(msg))?;
        return stdout.flush();
    }

    if let Some(p) = st.picker.as_mut() {
        picker::draw(stdout, p, words, cols_u16, rows_u16, theme)?;
        return stdout.flush();
    }

    let cols = cols_u16 as usize;
    let rows = rows_u16 as usize;

    draw_header(stdout, words, st, narrating, cols, theme)?;
    draw_word(stdout, words, st, cols, rows, theme)?;
    draw_progress(stdout, words.len(), st, cols, rows, theme)?;
    draw_footer(stdout, cols, rows, theme)?;

    if st.show_help {
        draw_help_overlay(stdout, cols_u16, rows_u16, theme)?;
    }
    stdout.flush()
}

fn draw_header<W: Write>(
    stdout: &mut W,
    words: &[&str],
    st: &State,
    narrating: bool,
    cols: usize,
    theme: &Theme,
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
        style::SetForegroundColor(theme.header_fg),
        style::SetBackgroundColor(theme.header_bg),
        style::Print(pad_right(&header, cols)),
    )?;
    reset_style(stdout, theme)
}

fn draw_word<W: Write>(
    stdout: &mut W,
    words: &[&str],
    st: &State,
    cols: usize,
    rows: usize,
    theme: &Theme,
) -> io::Result<()> {
    let word = words[st.idx];
    let chars: Vec<char> = word.chars().collect();
    let a = orp::anchor_index(chars.len()).min(chars.len().saturating_sub(1));

    let word_row: u16 = (rows / 2) as u16;
    let anchor_col: usize = st.focal.anchor_col(cols);

    // Horizontal rules above and below the word with a notch at the
    // anchor column — modelled after readrrr's ORP guide.
    let rule_half = 10usize;
    let rule_start = anchor_col.saturating_sub(rule_half);
    let rule_end = (anchor_col + rule_half + 1).min(cols);
    draw_rule(
        stdout,
        rule_start,
        rule_end,
        anchor_col,
        word_row.saturating_sub(1),
        true,
        theme,
    )?;
    draw_rule(
        stdout,
        rule_start,
        rule_end,
        anchor_col,
        word_row + 1,
        false,
        theme,
    )?;

    let left: String = chars[..a].iter().collect();
    let anchor_ch: String = chars.get(a).map(|c| c.to_string()).unwrap_or_default();
    let right: String = chars[a.saturating_add(1).min(chars.len())..]
        .iter()
        .collect();

    let start_col = anchor_col.saturating_sub(a);
    let anchor_color = if st.finished { theme.dim } else { theme.anchor };
    let word_color = if st.finished { theme.dim } else { theme.word };
    queue!(
        stdout,
        cursor::MoveTo(start_col as u16, word_row),
        style::SetForegroundColor(word_color),
        style::Print(&left),
        style::SetForegroundColor(anchor_color),
        style::SetAttribute(style::Attribute::Bold),
        style::Print(&anchor_ch),
    )?;
    reset_style(stdout, theme)?;
    queue!(
        stdout,
        style::SetForegroundColor(word_color),
        style::Print(&right),
    )?;
    reset_style(stdout, theme)?;

    if st.finished {
        let note = "— end — press space to restart, q to quit —";
        let col = cols.saturating_sub(note.chars().count()) / 2;
        let note_row = (rows.saturating_sub(4)) as u16;
        queue!(
            stdout,
            cursor::MoveTo(col as u16, note_row),
            style::SetForegroundColor(theme.dim),
            style::Print(note),
        )?;
        reset_style(stdout, theme)?;
    }
    Ok(())
}

// Draw a horizontal rule from `start` to `end` (exclusive) at `row`,
// with a tick mark at `anchor`. `above` = true draws ▼ at the anchor
// (rule above the word); false draws ▲ (rule below).
fn draw_rule<W: Write>(
    stdout: &mut W,
    start: usize,
    end: usize,
    anchor: usize,
    row: u16,
    above: bool,
    theme: &Theme,
) -> io::Result<()> {
    let tick = if above { '▼' } else { '▲' };
    queue!(
        stdout,
        cursor::MoveTo(start as u16, row),
        style::SetForegroundColor(theme.dim),
    )?;
    for col in start..end {
        if col == anchor {
            queue!(
                stdout,
                style::SetForegroundColor(theme.anchor),
                style::Print(tick),
                style::SetForegroundColor(theme.dim),
            )?;
        } else {
            queue!(stdout, style::Print('─'))?;
        }
    }
    reset_style(stdout, theme)
}

fn draw_progress<W: Write>(
    stdout: &mut W,
    total: usize,
    st: &State,
    cols: usize,
    rows: usize,
    theme: &Theme,
) -> io::Result<()> {
    let bar_row = (rows.saturating_sub(3)) as u16;
    let (filled_s, empty_s) = progress_bar_parts(st.idx + 1, total, cols);
    queue!(
        stdout,
        cursor::MoveTo(0, bar_row),
        style::SetForegroundColor(theme.progress_filled),
        style::Print(&filled_s),
        style::SetForegroundColor(theme.progress_empty),
        style::Print(&empty_s),
    )?;
    reset_style(stdout, theme)
}

fn draw_footer<W: Write>(
    stdout: &mut W,
    cols: usize,
    rows: usize,
    theme: &Theme,
) -> io::Result<()> {
    let footer = " space play/pause · ← → step · / pick · r restart · ↑ ↓ wpm · ? help · q quit ";
    queue!(
        stdout,
        cursor::MoveTo(0, (rows.saturating_sub(1)) as u16),
        style::SetForegroundColor(theme.dim),
        style::Print(pad_right(footer, cols)),
    )?;
    reset_style(stdout, theme)
}

fn draw_help_overlay<W: Write>(
    stdout: &mut W,
    cols: u16,
    rows: u16,
    theme: &Theme,
) -> io::Result<()> {
    let lines: &[&str] = &[
        "echo — keyboard reference",
        "",
        "space        play / pause (when done: restart)",
        "← / →        previous / next word",
        "b / f        jump back / forward 10 words",
        "PgUp/PgDn    jump back / forward 50 words",
        "r / Home     restart from first word",
        "↑ / ↓        increase / decrease wpm by 25",
        "/            open word picker",
        "? or h       toggle this help",
        "q / Esc      quit",
        "",
        "in picker:",
        "← → ↑ ↓      move cursor",
        "b / f        ± 10 words",
        "PgUp/PgDn    ± 50 words",
        "Home / End   first / last word",
        "Enter        jump playback to highlighted word",
        "Esc          cancel",
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
        )?;
        reset_style(stdout, theme)?;
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
        )?;
        reset_style(stdout, theme)?;
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

fn progress_bar_parts(cur: usize, total: usize, width: usize) -> (String, String) {
    if width == 0 || total == 0 {
        return (String::new(), String::new());
    }
    let filled = (cur * width) / total;
    let filled_s: String = "█".repeat(filled);
    let empty_s: String = "░".repeat(width - filled);
    (filled_s, empty_s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_bar_full_and_empty() {
        let (f, e) = progress_bar_parts(0, 10, 5);
        assert_eq!(format!("{f}{e}"), "░░░░░");
        let (f, e) = progress_bar_parts(10, 10, 5);
        assert_eq!(format!("{f}{e}"), "█████");
    }

    #[test]
    fn progress_bar_handles_zero_width() {
        let (f, e) = progress_bar_parts(5, 10, 0);
        assert!(f.is_empty() && e.is_empty());
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
    fn pause_levels_scale_correctly() {
        assert_eq!(PauseLevel::Off.multiplier("end."), 1);
        assert_eq!(PauseLevel::Off.multiplier("plain"), 1);

        assert_eq!(PauseLevel::Low.multiplier("end."), 2);
        assert_eq!(PauseLevel::Low.multiplier("list,"), 1);
        assert_eq!(PauseLevel::Low.multiplier("plain"), 1);

        assert_eq!(PauseLevel::Medium.multiplier("end."), 3);
        assert_eq!(PauseLevel::Medium.multiplier("what?"), 3);
        assert_eq!(PauseLevel::Medium.multiplier("wait!"), 3);
        assert_eq!(PauseLevel::Medium.multiplier("list,"), 2);
        assert_eq!(PauseLevel::Medium.multiplier("clause;"), 2);
        assert_eq!(PauseLevel::Medium.multiplier("colon:"), 2);
        assert_eq!(PauseLevel::Medium.multiplier("plain"), 1);

        assert_eq!(PauseLevel::High.multiplier("end."), 5);
        assert_eq!(PauseLevel::High.multiplier("list,"), 3);
        assert_eq!(PauseLevel::High.multiplier("plain"), 1);

        assert_eq!(PauseLevel::Medium.multiplier("(aside"), 1);
    }
}
